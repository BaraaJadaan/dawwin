//! The app's state and the one thread that owns the engine: it runs the queue,
//! the model download, and unloads the model when the app sits idle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::audio::{self, SR};
use crate::engine::{Audio, Engine, EventSink, OutOfMemory, PythonWorker, WorkerOptions};
use crate::history;
use crate::setup::{self, ModelStatus, Paths, Settings};
use crate::transcript::{collapse_loops, find_loop, Segment, Transcript};

const MAX_CHUNK_S: f32 = 30.0;
const SEARCH_S: f32 = 8.0;
const BATCH: usize = 4;
/// Free the GPU after this long with nothing to do.
const IDLE_UNLOAD: Duration = Duration::from_secs(10 * 60);
pub const MODEL_LABEL: &str = "Cohere Transcribe Arabic (07-2026)";

#[derive(Clone, Debug, Serialize, Default)]
pub struct JobView {
    pub id: String,
    pub title: String,
    pub path: String,
    pub language: String,
    /// queued | decoding | loading | transcribing | done | failed | cancelled
    pub status: String,
    pub progress: f64,
    pub eta_s: Option<f64>,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub error: Option<String>,
    pub saved: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct EngineView {
    /// missing (no env) | off | starting | ready (model not loaded) | loading | loaded | error
    pub state: String,
    pub device: Option<String>,
    pub gpu: Option<String>,
    pub cuda: Option<bool>,
    pub torch: Option<String>,
    pub vram_total_gb: Option<f64>,
    pub vram_used_gb: Option<f64>,
    pub error: Option<String>,
    /// "bfloat16" | "int8", once loaded
    pub dtype: Option<String>,
    /// e.g. the GPU had too little memory, so it runs on the CPU
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct DownloadView {
    pub active: bool,
    pub done: u64,
    pub total: u64,
    /// after a dropped connection: seconds until it resumes, and the attempt number
    pub retry_wait: Option<u64>,
    pub retry_attempt: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SetupView {
    pub env_ready: bool,
    pub env_busy: bool,
    pub env_error: Option<String>,
    pub log: Vec<String>,
    pub uv: Option<String>,
    pub ffmpeg: Option<String>,
    pub model: ModelStatus,
    pub model_id: String,
    pub model_url: String,
    pub tokens_url: String,
    pub files_url: String,
    pub nvidia_driver: Option<String>,
    /// The PyTorch build the env has: "cu128", "cpu", …
    pub env_backend: Option<String>,
    /// "auto" | "cpu-fast" | "cpu-exact"
    pub run_on: String,
    /// The interface language, when one was picked
    pub ui_lang: Option<String>,
    pub app_version: String,
    pub repo_url: String,
    /// Check for a newer version at startup
    pub check_updates: bool,
    pub download: DownloadView,
    pub hf_home: String,
    pub data_dir: String,
    pub env_dir: String,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Snapshot {
    pub queue: Vec<JobView>,
    pub engine: EngineView,
    pub setup: SetupView,
}

enum Msg {
    Job(String),
    Probe,
    Download,
    /// Stop the worker so the next one starts with new settings (another model folder)
    Restart,
    /// Find word times for a saved transcript: all its lines, or the ones listed
    Align { id: String, lines: Option<Vec<usize>> },
}

pub struct Core {
    app: AppHandle,
    pub paths: Paths,
    snap: Mutex<Snapshot>,
    live: Mutex<HashMap<String, Transcript>>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    download_abort: Arc<AtomicBool>,
    pub settings: Mutex<Settings>,
    tx: Mutex<Sender<Msg>>,
    counter: AtomicU64,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Core {
    pub fn start(app: AppHandle) -> Result<Arc<Core>> {
        let paths = Paths::new()?;
        paths.install_worker_files()?;
        let (tx, rx) = mpsc::channel();
        let settings = Settings::load(&paths);
        crate::i18n::set(settings.ui_lang.as_deref().unwrap_or("en"));
        let core = Arc::new(Core {
            app,
            paths,
            snap: Mutex::new(Snapshot::default()),
            live: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            download_abort: Arc::new(AtomicBool::new(false)),
            settings: Mutex::new(settings),
            tx: Mutex::new(tx),
            counter: AtomicU64::new(0),
        });
        core.refresh_setup();
        let c = core.clone();
        std::thread::Builder::new().name("engine".into()).spawn(move || engine_loop(c, rx))?;
        let snap = core.snapshot();
        if snap.setup.env_ready {
            core.send(Msg::Probe);
            // a model download that was running when the app closed carries on
            if core.settings.lock().unwrap().download_wanted && !snap.setup.model.found {
                core.download_model();
            }
        }
        Ok(core)
    }

    fn send(&self, m: Msg) {
        let _ = self.tx.lock().unwrap().send(m);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snap.lock().unwrap().clone()
    }

    fn emit(&self) {
        let s = self.snapshot();
        let _ = self.app.emit("state", s);
    }

    fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        f(&mut *self.snap.lock().unwrap());
        self.emit();
    }

    fn update_job(&self, id: &str, f: impl FnOnce(&mut JobView)) {
        self.update(|s| {
            if let Some(j) = s.queue.iter_mut().find(|j| j.id == id) {
                f(j);
            }
        });
    }

    /// Re-read what is installed (env, tools, model) and tell the UI.
    pub fn refresh_setup(&self) {
        let tools = setup::tools();
        let env_ready = setup::env_ready(&self.paths);
        let env_backend = setup::env_backend(&self.paths);
        let model = self.model_status();
        let (run_on, ui_lang, dl_total, check_updates) = {
            let st = self.settings.lock().unwrap();
            (st.run_on().to_string(), st.ui_lang.clone(), st.download_total, st.check_updates)
        };
        let on_disk = if model.found { 0 } else { setup::downloaded_bytes(std::path::Path::new(&model.download_dir)) };
        self.update(|s| {
            let st = &mut s.setup;
            st.env_ready = env_ready;
            st.uv = tools.uv;
            st.ffmpeg = tools.ffmpeg;
            st.model = model;
            st.model_id = setup::MODEL_ID.into();
            st.model_url = setup::MODEL_URL.into();
            st.tokens_url = setup::TOKENS_URL.into();
            st.files_url = setup::FILES_URL.into();
            st.nvidia_driver = tools.nvidia_driver;
            st.env_backend = env_backend;
            st.run_on = run_on;
            st.ui_lang = ui_lang;
            st.check_updates = check_updates;
            st.app_version = crate::updates::VERSION.into();
            st.repo_url = format!("https://github.com/{}", crate::updates::REPO);
            // a paused or interrupted download: show how far it got, from the files on disk
            if !st.download.active && !st.model.found {
                st.download.done = on_disk;
                st.download.total = dl_total.max(on_disk);
            }
            st.hf_home = setup::hf_home().display().to_string();
            st.data_dir = self.paths.roaming.display().to_string();
            st.env_dir = self.paths.env_dir().display().to_string();
            if !env_ready && s.engine.state.is_empty() {
                s.engine.state = "missing".into();
            } else if env_ready && (s.engine.state == "missing" || s.engine.state.is_empty()) {
                s.engine.state = "off".into();
            }
        });
    }

    pub fn model_status(&self) -> ModelStatus {
        setup::model_status(&self.paths, &self.settings.lock().unwrap())
    }

    /// Change the settings, save them, and show what that changes.
    pub fn change_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<()> {
        {
            let mut st = self.settings.lock().unwrap();
            f(&mut *st);
            st.save(&self.paths)?;
        }
        self.refresh_setup();
        Ok(())
    }

    /// Use the model in `dir` (a folder with the model files, or a Hugging Face cache folder).
    pub fn import_model(&self, dir: &std::path::Path) -> Result<ModelStatus> {
        let (resolved, _) = setup::validate_model_dir(dir)
            .map_err(|e| anyhow!(crate::tr!("That folder does not hold the model: {}.", e)))?;
        self.change_settings(|s| s.model_dir = Some(resolved.display().to_string()))?;
        self.send(Msg::Restart);
        Ok(self.model_status())
    }

    /// The interface language, for the messages Rust and the worker show.
    pub fn set_ui_lang(&self, lang: &str) -> Result<()> {
        let lang = if crate::i18n::LANGUAGES.contains(&lang) { lang } else { "en" };
        crate::i18n::set(lang);
        self.change_settings(|s| s.ui_lang = Some(lang.to_string()))?;
        Ok(())
    }

    /// Ask for word times of a saved transcript (all lines, or some after an edit).
    pub fn request_align(&self, id: &str, lines: Option<Vec<usize>>) {
        self.send(Msg::Align { id: id.to_string(), lines });
    }

    /// GPU or CPU: takes effect with a fresh worker.
    pub fn set_run_on(&self, value: &str) -> Result<()> {
        if !matches!(value, "auto" | "cpu-fast" | "cpu-exact") {
            return Err(anyhow!("unknown setting {value}"));
        }
        self.change_settings(|s| s.run_on = Some(value.to_string()))?;
        self.send(Msg::Restart);
        Ok(())
    }

    // ------------------------------------------------------------ setup

    pub fn setup_engine(self: &Arc<Self>) {
        if self.snapshot().setup.env_busy {
            return;
        }
        self.update(|s| {
            s.setup.env_busy = true;
            s.setup.env_error = None;
            s.setup.log.clear();
        });
        let core = self.clone();
        std::thread::spawn(move || {
            let log = |line: String| {
                let _ = core.app.emit("setup-log", &line);
                let mut s = core.snap.lock().unwrap();
                s.setup.log.push(line);
                let n = s.setup.log.len();
                if n > 300 {
                    s.setup.log.drain(..n - 300);
                }
            };
            let res = setup::setup_env(&core.paths, &log);
            core.update(|s| {
                s.setup.env_busy = false;
                s.setup.env_error = res.as_ref().err().map(|e| format!("{e:#}"));
            });
            core.refresh_setup();
            if res.is_ok() {
                core.send(Msg::Probe);
            }
        });
    }

    pub fn download_model(&self) {
        let _ = self.change_settings(|s| s.download_wanted = true);
        self.download_abort.store(false, Ordering::SeqCst);
        // start from what is already on disk, so a resumed download shows its progress at once
        let (done, total) = (self.snapshot().setup.download.done, self.settings.lock().unwrap().download_total);
        self.update(|s| {
            s.setup.download = DownloadView { active: true, done, total: total.max(done), ..Default::default() };
        });
        self.send(Msg::Download);
    }

    pub fn stop_download(&self) {
        let _ = self.change_settings(|s| s.download_wanted = false);
        self.download_abort.store(true, Ordering::SeqCst);
    }

    // ------------------------------------------------------------ queue

    pub fn enqueue(&self, paths: Vec<String>, language: String) -> Vec<String> {
        let mut ids = Vec::new();
        for p in paths {
            let path = PathBuf::from(&p);
            if !path.is_file() {
                continue;
            }
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            let id = format!("{}-{:03}", now_ms(), n % 1000);
            let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
            self.cancels.lock().unwrap().insert(id.clone(), Arc::new(AtomicBool::new(false)));
            let job = JobView {
                id: id.clone(),
                title,
                path: p,
                language: language.clone(),
                status: "queued".into(),
                ..Default::default()
            };
            self.update(|s| s.queue.push(job));
            self.send(Msg::Job(id.clone()));
            ids.push(id);
        }
        ids
    }

    pub fn cancel(&self, id: &str) {
        if let Some(c) = self.cancels.lock().unwrap().get(id) {
            c.store(true, Ordering::SeqCst);
        }
        self.update_job(id, |j| {
            if j.status == "queued" {
                j.status = "cancelled".into();
            }
        });
    }

    pub fn dismiss(&self, id: &str) {
        self.update(|s| s.queue.retain(|j| j.id != id || !matches!(j.status.as_str(), "done" | "failed" | "cancelled")));
    }

    /// A transcript: the one being written if it is still running, else from history.
    pub fn transcript(&self, id: &str) -> Result<Transcript> {
        if let Some(t) = self.live.lock().unwrap().get(id) {
            return Ok(t.clone());
        }
        history::load(&self.paths.history_dir(), id)
    }

    pub fn is_live(&self, id: &str) -> bool {
        self.live.lock().unwrap().contains_key(id)
    }
}

// ---------------------------------------------------------------- engine thread

fn sink(core: &Arc<Core>) -> EventSink {
    let c = core.clone();
    Arc::new(move |v: &Value| match v.get("event").and_then(|e| e.as_str()) {
        Some("loading") => c.update(|s| s.engine.state = "loading".into()),
        Some("download") => {
            let done = v.get("done").and_then(|x| x.as_u64()).unwrap_or(0);
            let total = v.get("total").and_then(|x| x.as_u64()).unwrap_or(0);
            if total > 0 && c.settings.lock().unwrap().download_total != total {
                let _ = c.change_settings(|s| s.download_total = total);
            }
            c.update(|s| {
                s.setup.download.done = done;
                s.setup.download.total = total;
                s.setup.download.retry_wait = None;
                s.setup.download.retry_attempt = None;
            });
        }
        Some("download_retry") => {
            let wait = v.get("wait").and_then(|x| x.as_u64());
            let attempt = v.get("attempt").and_then(|x| x.as_u64());
            c.update(|s| {
                s.setup.download.retry_wait = wait;
                s.setup.download.retry_attempt = attempt;
            });
        }
        _ => {}
    })
}

fn ensure_worker<'a>(core: &Arc<Core>, worker: &'a mut Option<PythonWorker>) -> Result<&'a mut PythonWorker> {
    if worker.is_none() {
        if !setup::env_ready(&core.paths) {
            return Err(anyhow!(crate::tr!("The engine is not set up yet. Press “Set up engine” first.")));
        }
        let model = core.model_status();
        core.update(|s| {
            s.engine.state = "starting".into();
            s.engine.error = None;
        });
        let opts = WorkerOptions {
            offline: model.found,
            model_dir: model.found.then(|| PathBuf::from(&model.dir)),
            token: None,
            run_on: core.settings.lock().unwrap().run_on().to_string(),
        };
        *worker = Some(PythonWorker::spawn(&core.paths, opts, sink(core))?);
    }
    Ok(worker.as_mut().unwrap())
}

fn engine_loop(core: Arc<Core>, rx: Receiver<Msg>) {
    let mut worker: Option<PythonWorker> = None;
    let mut last_used = Instant::now();
    loop {
        let msg = match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => {
                if worker.is_some() && last_used.elapsed() > IDLE_UNLOAD {
                    worker.take().unwrap().shutdown();
                    core.update(|s| {
                        s.engine.state = "off".into();
                        s.engine.vram_used_gb = None;
                    });
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        last_used = Instant::now();
        match msg {
            Msg::Probe => {
                let res = ensure_worker(&core, &mut worker).and_then(|w| w.probe());
                match res {
                    Ok(v) => core.update(|s| {
                        let e = &mut s.engine;
                        if e.state == "starting" || e.state == "off" {
                            e.state = "ready".into();
                        }
                        e.cuda = v.get("cuda").and_then(|x| x.as_bool());
                        e.gpu = v.get("gpu").and_then(|x| x.as_str()).map(String::from);
                        e.torch = v.get("torch").and_then(|x| x.as_str()).map(String::from);
                        e.vram_total_gb = v.get("vram_total_gb").and_then(|x| x.as_f64());
                        let forced_cpu = v.get("force_cpu").and_then(|x| x.as_bool()) == Some(true);
                        e.device = Some(if e.cuda == Some(true) && !forced_cpu { "cuda".into() } else { "cpu".into() });
                    }),
                    Err(e) => {
                        worker = None;
                        core.update(|s| {
                            s.engine.state = "error".into();
                            s.engine.error = Some(format!("{e:#}"));
                        });
                    }
                }
            }
            Msg::Download => {
                // the running worker is offline; download in a fresh, online one
                if let Some(w) = worker.take() {
                    w.shutdown();
                }
                let res = (|| -> Result<()> {
                    if !setup::env_ready(&core.paths) {
                        return Err(anyhow!(crate::tr!("Set up the engine first.")));
                    }
                    let dest = PathBuf::from(core.model_status().download_dir);
                    let opts = WorkerOptions { offline: false, token: setup::app_token(&core.paths), ..Default::default() };
                    let mut w = PythonWorker::spawn(&core.paths, opts, sink(&core))?;
                    w.abort = Some(core.download_abort.clone());
                    let r = w.download(&dest);
                    w.shutdown();
                    r?;
                    setup::validate_model_dir(&dest).map_err(|e| anyhow!(crate::tr!("The download finished but the folder is not complete: {}", e)))?;
                    core.change_settings(|s| {
                        s.model_dir = Some(dest.display().to_string());
                        s.download_wanted = false;
                    })?;
                    Ok(())
                })();
                core.update(|s| {
                    s.setup.download.active = false;
                    s.setup.download.retry_wait = None;
                    s.setup.download.retry_attempt = None;
                    s.setup.download.error = res.as_ref().err().map(|e| format!("{e:#}"));
                    s.engine.state = "off".into();
                });
                core.refresh_setup();
                if res.is_ok() {
                    core.send(Msg::Probe);
                }
            }
            Msg::Restart => {
                if let Some(w) = worker.take() {
                    w.shutdown();
                }
                core.update(|s| {
                    s.engine.state = "off".into();
                    s.engine.vram_used_gb = None;
                    s.engine.dtype = None;
                    s.engine.note = None;
                    s.engine.device = None;
                });
                if setup::env_ready(&core.paths) {
                    core.send(Msg::Probe);
                }
            }
            Msg::Align { id, lines } => {
                if let Err(e) = align_saved(&core, &mut worker, &id, lines) {
                    eprintln!("word times for {id}: {e:#}");
                }
            }
            Msg::Job(id) => {
                let res = run_job(&core, &mut worker, &id);
                if let Err(e) = res {
                    let msg = format!("{e:#}");
                    core.update_job(&id, |j| {
                        j.status = "failed".into();
                        j.error = Some(msg);
                        j.eta_s = None;
                    });
                    // a dead worker is restarted for the next job
                    if let Some(w) = worker.as_mut() {
                        if !w.alive() {
                            worker = None;
                            core.update(|s| s.engine.state = "off".into());
                        }
                    }
                }
                core.cancels.lock().unwrap().remove(&id);
            }
        }
    }
    if let Some(w) = worker.take() {
        w.shutdown();
    }
}

fn run_job(core: &Arc<Core>, worker: &mut Option<PythonWorker>, id: &str) -> Result<()> {
    let Some(job) = core.snapshot().queue.into_iter().find(|j| j.id == id) else { return Ok(()) };
    if job.status != "queued" {
        return Ok(());
    }
    let cancel = core.cancels.lock().unwrap().get(id).cloned().unwrap_or_default();
    let started = Instant::now();
    core.update_job(id, |j| j.status = "decoding".into());

    if !core.model_status().found {
        return Err(anyhow!(crate::tr!("The model is not set up yet. Download or import it on the Engine and model page.")));
    }
    let w = ensure_worker(core, worker)?;
    w.start_loading()?; // the model loads while the audio decodes

    let ffmpeg = setup::find_exe("ffmpeg");
    let samples = audio::decode(PathBuf::from(&job.path).as_path(), ffmpeg.as_ref(), &cancel)?;
    let total = samples.len();
    let duration_s = total as f64 / SR as f64;
    let chunks = audio::split_points(&samples, MAX_CHUNK_S, SEARCH_S);
    let audio = Audio { id: id.to_string(), samples };
    core.update_job(id, |j| {
        j.duration_s = duration_s;
        j.status = "loading".into();
    });

    let info = w.ensure_loaded()?;
    show_loaded(core, &info);
    let device = match &info.gpu {
        Some(g) if info.device.starts_with("cuda") => format!("GPU · {g}"),
        _ if info.dtype.as_deref() == Some("int8") => "CPU (int8)".into(),
        _ => "CPU".into(),
    };

    let mut t = Transcript {
        id: id.to_string(),
        title: job.title.clone(),
        source_path: job.path.clone(),
        created_ms: now_ms(),
        duration_s,
        language: job.language.clone(),
        elapsed_s: 0.0,
        device,
        model: MODEL_LABEL.into(),
        partial: false,
        segments: Vec::new(),
    };
    core.live.lock().unwrap().insert(id.to_string(), t.clone());
    core.update_job(id, |j| j.status = "transcribing".into());
    let _ = core.app.emit("transcript-started", id);

    let t_start = Instant::now();
    let mut batch = BATCH;
    let mut i = 0;
    let mut failure: Option<anyhow::Error> = None;
    while i < chunks.len() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let take = &chunks[i..(i + batch).min(chunks.len())];
        let ranges: Vec<(usize, usize)> = take.iter().copied().filter(|(a, b)| !audio::is_silent(&audio.samples[*a..*b])).collect();
        let texts = if ranges.is_empty() { Ok(vec![]) } else { w.transcribe(&audio, &ranges, &job.language) };
        match texts {
            Ok(texts) => {
                let mut lines: Vec<Segment> = Vec::new();
                for (r, text) in ranges.iter().zip(texts) {
                    match fix_loops(w, &audio, *r, text, &job.language, 0) {
                        Ok(s) => lines.extend(s.into_iter().filter(|s| !s.text.trim().is_empty())),
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    };
                }
                attach_words(w, &audio, &mut lines, &job.language);
                {
                    for s in lines {
                        let _ = core.app.emit("segment", serde_json::json!({ "id": id, "segment": &s }));
                        t.segments.push(s.clone());
                        if let Some(lt) = core.live.lock().unwrap().get_mut(id) {
                            lt.segments.push(s);
                        }
                    }
                }
                if failure.is_some() {
                    break;
                }
                i += take.len();
                let done = chunks[i - 1].1;
                let done_s = done as f64 / SR as f64;
                let spent = t_start.elapsed().as_secs_f64();
                let eta = if done_s > 0.0 { Some(spent / done_s * (duration_s - done_s)) } else { None };
                core.update_job(id, |j| {
                    j.progress = done as f64 / total.max(1) as f64;
                    j.eta_s = eta;
                    j.elapsed_s = started.elapsed().as_secs_f64();
                });
            }
            Err(e) if e.is::<OutOfMemory>() && batch > 1 => {
                batch = 1; // retry the same chunks one at a time
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    w.forget(id);
    core.live.lock().unwrap().remove(id);

    let cancelled = cancel.load(Ordering::SeqCst);
    t.elapsed_s = t_start.elapsed().as_secs_f64();
    t.partial = cancelled || failure.is_some();
    let saved = !t.segments.is_empty();
    if saved {
        history::save(&core.paths.history_dir(), &t)?;
    }
    let _ = core.app.emit("history-changed", id);
    if let Some(e) = failure {
        core.update_job(id, |j| j.saved = saved);
        return Err(e);
    }
    core.update_job(id, |j| {
        j.status = if cancelled { "cancelled".into() } else { "done".into() };
        j.eta_s = None;
        j.elapsed_s = started.elapsed().as_secs_f64();
        if !cancelled {
            j.progress = 1.0;
        }
        j.saved = saved;
    });
    Ok(())
}

/// Tell the UI the model is in memory, and where.
fn show_loaded(core: &Arc<Core>, info: &crate::engine::EngineInfo) {
    core.update(|s| {
        s.engine.state = "loaded".into();
        s.engine.device = Some(if info.device.starts_with("cuda") { "cuda".into() } else { "cpu".into() });
        if info.gpu.is_some() {
            s.engine.gpu = info.gpu.clone();
        }
        s.engine.vram_used_gb = info.vram_used_gb;
        s.engine.dtype = info.dtype.clone();
        s.engine.note = info.fallback.clone();
    });
}

/// Word start times for `lines`, from the engine. A failure only costs the word times.
fn attach_words(w: &mut dyn Engine, audio: &Audio, lines: &mut [Segment], lang: &str) {
    if lines.is_empty() {
        return;
    }
    let n = audio.samples.len();
    let ranges: Vec<(usize, usize)> = lines
        .iter()
        .map(|s| (((s.start * SR as f64) as usize).min(n), ((s.end * SR as f64).round() as usize).min(n)))
        .collect();
    let texts: Vec<String> = lines.iter().map(|s| s.text.clone()).collect();
    match w.align(audio, &ranges, &texts, lang) {
        Ok(words) => {
            for (s, ws) in lines.iter_mut().zip(words) {
                s.words = ws;
            }
        }
        Err(e) => eprintln!("word times: {e:#}"),
    }
}

/// Word times for a transcript saved without them (from before word timing) or after an edit.
fn align_saved(core: &Arc<Core>, worker: &mut Option<PythonWorker>, id: &str, lines: Option<Vec<usize>>) -> Result<()> {
    let dir = core.paths.history_dir();
    let t = history::load(&dir, id)?;
    if !core.model_status().found || !setup::env_ready(&core.paths) || !PathBuf::from(&t.source_path).is_file() {
        return Ok(());
    }
    let wanted: Vec<usize> = lines.unwrap_or_else(|| (0..t.segments.len()).collect());
    let ffmpeg = setup::find_exe("ffmpeg");
    let stop = AtomicBool::new(false);
    let samples = audio::decode(PathBuf::from(&t.source_path).as_path(), ffmpeg.as_ref(), &stop)?;
    let audio = Audio { id: format!("align-{id}"), samples };
    let w = ensure_worker(core, worker)?;
    let info = w.ensure_loaded()?;
    show_loaded(core, &info);
    let mut found: Vec<(usize, String, Vec<(u32, f32)>)> = Vec::new();
    for part in wanted.chunks(8) {
        let mut segs: Vec<Segment> = part.iter().filter_map(|&i| t.segments.get(i).cloned()).collect();
        attach_words(w, &audio, &mut segs, &t.language);
        for (&i, s) in part.iter().zip(segs) {
            found.push((i, s.text, s.words));
        }
    }
    w.forget(&audio.id);
    // reload: keep edits made meanwhile, and only use times for text that did not change
    let mut cur = history::load(&dir, id)?;
    for (i, text, words) in found {
        if let Some(s) = cur.segments.get_mut(i) {
            if s.text == text {
                s.words = words;
            }
        }
    }
    history::save(&dir, &cur)?;
    let _ = core.app.emit("words-ready", id);
    Ok(())
}

/// The model sometimes repeats one word dozens of times. Transcribe such a chunk again in
/// two halves; a half that still loops gets the repetition cut and a "repeat" flag.
fn fix_loops(w: &mut dyn Engine, audio: &Audio, (a, b): (usize, usize), text: String, lang: &str, depth: u32) -> Result<Vec<Segment>> {
    let seg = |a: usize, b: usize, text: String, flag: Option<&str>| Segment {
        start: a as f64 / SR as f64,
        end: b as f64 / SR as f64,
        text,
        flag: flag.map(String::from),
        edited: false,
        voice: audio::voice_profile(&audio.samples, a, b, 20),
        words: Vec::new(),
    };
    if find_loop(&text).is_none() {
        return Ok(vec![seg(a, b, text, None)]);
    }
    if depth >= 2 || b - a < 4 * SR {
        return Ok(vec![seg(a, b, collapse_loops(&text), Some("repeat"))]);
    }
    let m = audio::split_middle(&audio.samples, a, b);
    let texts = w.transcribe(audio, &[(a, m), (m, b)], lang)?;
    let mut out = Vec::new();
    for (r, t) in [(a, m), (m, b)].into_iter().zip(texts) {
        out.extend(fix_loops(w, audio, r, t, lang, depth + 1)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineInfo;
    use std::collections::VecDeque;

    /// Replays canned transcripts and records what it was asked.
    struct Fake {
        calls: Vec<Vec<(usize, usize)>>,
        replies: VecDeque<Vec<String>>,
    }

    impl Engine for Fake {
        fn start_loading(&mut self) -> Result<()> {
            Ok(())
        }
        fn ensure_loaded(&mut self) -> Result<EngineInfo> {
            Ok(EngineInfo::default())
        }
        fn transcribe(&mut self, _a: &Audio, ranges: &[(usize, usize)], _l: &str) -> Result<Vec<String>> {
            self.calls.push(ranges.to_vec());
            Ok(self.replies.pop_front().expect("no more canned replies"))
        }
        fn forget(&mut self, _: &str) {}
    }

    fn looping() -> String {
        format!("ناجح 20 ما في {}س", "ست ".repeat(20))
    }

    #[test]
    fn a_looping_chunk_is_transcribed_again_in_two_halves() {
        let audio = Audio { id: "x".into(), samples: vec![0.1; 20 * SR] };
        let mut e = Fake { calls: vec![], replies: VecDeque::from(vec![vec!["أول نص".into(), "ثاني نص".into()]]) };
        let segs = fix_loops(&mut e, &audio, (0, 20 * SR), looping(), "ar", 0).unwrap();
        assert_eq!(e.calls.len(), 1);
        assert_eq!(e.calls[0].len(), 2, "both halves in one batch");
        assert_eq!(segs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["أول نص", "ثاني نص"]);
        assert!(segs.iter().all(|s| s.flag.is_none()));
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, segs[1].start);
        assert_eq!(segs[1].end, 20.0);
    }

    #[test]
    fn halves_that_keep_looping_are_cut_and_flagged() {
        let audio = Audio { id: "x".into(), samples: vec![0.1; 20 * SR] };
        let replies = (0..3).map(|_| vec![looping(), looping()]).collect();
        let mut e = Fake { calls: vec![], replies };
        let segs = fix_loops(&mut e, &audio, (0, 20 * SR), looping(), "ar", 0).unwrap();
        assert_eq!(e.calls.len(), 3, "split once, then each half once more");
        assert_eq!(segs.len(), 4);
        for s in &segs {
            assert_eq!(s.flag.as_deref(), Some("repeat"));
            assert_eq!(s.text, "ناجح 20 ما في ست ست …");
        }
    }

    #[test]
    fn ordinary_text_is_kept_as_one_line() {
        let audio = Audio { id: "x".into(), samples: vec![0.1; 20 * SR] };
        let mut e = Fake { calls: vec![], replies: VecDeque::new() };
        let segs = fix_loops(&mut e, &audio, (0, 20 * SR), "طيب هلا مكتب الاختبارات".into(), "ar", 0).unwrap();
        assert!(e.calls.is_empty());
        assert_eq!(segs.len(), 1);
    }
}
