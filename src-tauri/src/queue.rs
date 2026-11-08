//! The app's state and the one thread that owns the engine: it runs the queue and unloads the
//! model when the app sits idle. A model download runs on a thread of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::audio::{self, SR};
use crate::engine::{Audio, Engine, EngineFailed, EngineInfo, EngineProcess, OutOfMemory, Recognized};
use crate::folder;
use crate::history;
use crate::models;
use crate::setup::{self, KeepAwake, ModelStatus, Paths, Settings};
use crate::transcript::{collapse_loops, find_loop, Segment, Transcript};

const MAX_CHUNK_S: f32 = 30.0;
const SEARCH_S: f32 = 8.0;
/// Free the GPU after this long with nothing to do.
const IDLE_UNLOAD: Duration = Duration::from_secs(10 * 60);
pub const MODEL_LABEL: &str = "Cohere Transcribe Arabic (07-2026)";
const ACTIVE_JOB: &[&str] = &["queued", "decoding", "loading", "transcribing"];

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
    /// Came from a folder: its transcript is also written next to the recording
    pub folder: bool,
}

/// What transcribing a folder found.
#[derive(Clone, Debug, Serialize)]
pub struct FolderScan {
    /// The jobs it started
    pub added: Vec<String>,
    /// Recordings with a finished transcript, or already in the queue
    pub already: usize,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct EngineView {
    /// missing (not installed) | off | starting | ready (model not loaded) | loading | loaded | error
    pub state: String,
    /// "gpu" | "cpu"
    pub device: Option<String>,
    pub gpu: Option<String>,
    pub vram_total_gb: Option<f64>,
    pub error: Option<String>,
    /// Why it runs on the CPU although a GPU was wanted: "gpu-too-small" | "gpu-failed"
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct DownloadView {
    pub active: bool,
    /// Which model, while it downloads
    pub model: Option<String>,
    pub done: u64,
    pub total: u64,
    /// after a dropped connection: seconds until it resumes, and the attempt number
    pub retry_wait: Option<u64>,
    pub retry_attempt: Option<u64>,
    /// All of it arrived, and it is being read back to check it
    pub verifying: bool,
    pub error: Option<String>,
}

/// One of the models the app can download.
#[derive(Clone, Debug, Serialize, Default)]
pub struct ModelView {
    pub id: String,
    pub file: String,
    pub size: u64,
    pub url: String,
    /// The whole file is there
    pub on_disk: bool,
    /// How much of a download that is not finished is there
    pub partial: u64,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SetupView {
    pub engine_found: bool,
    pub ffmpeg: Option<String>,
    pub model: ModelStatus,
    pub models: Vec<ModelView>,
    pub files_url: String,
    /// "auto" | "cpu"
    pub run_on: String,
    /// The interface language, when one was picked
    pub ui_lang: Option<String>,
    pub app_version: String,
    pub repo_url: String,
    /// Check for a newer version at startup
    pub check_updates: bool,
    /// What transcribing a folder writes next to each recording
    pub folder_formats: Vec<String>,
    pub download: DownloadView,
    pub data_dir: String,
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
    /// Stop the engine so the next one starts with new settings (another model, GPU or CPU)
    Restart,
    /// Stop the engine, which may be using this file, and delete it
    DeleteFile(PathBuf),
}

pub struct Core {
    app: AppHandle,
    pub paths: Paths,
    snap: Mutex<Snapshot>,
    live: Mutex<HashMap<String, Transcript>>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    download_stop: Arc<AtomicBool>,
    /// The GPU crashed or failed in this session: stay on the CPU until the app restarts
    gpu_failed: AtomicBool,
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
        let (tx, rx) = mpsc::channel();
        let settings = Settings::load(&paths);
        crate::i18n::set(settings.ui_lang.as_deref().unwrap_or("en"));
        let core = Arc::new(Core {
            app,
            paths,
            snap: Mutex::new(Snapshot::default()),
            live: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            download_stop: Arc::new(AtomicBool::new(false)),
            gpu_failed: AtomicBool::new(false),
            settings: Mutex::new(settings),
            tx: Mutex::new(tx),
            counter: AtomicU64::new(0),
        });
        core.refresh_setup();
        let c = core.clone();
        std::thread::Builder::new().name("engine".into()).spawn(move || engine_loop(c, rx))?;
        if core.snapshot().setup.engine_found {
            core.send(Msg::Probe);
        }
        // a model download that was running when the app closed carries on
        let resume = core.settings.lock().unwrap().resume_model.clone();
        if let Some(id) = resume {
            core.download_model(&id);
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

    /// Re-read what is installed (engine, tools, models) and tell the UI.
    pub fn refresh_setup(&self) {
        let engine_found = setup::engine_exe().is_some();
        let ffmpeg = setup::find_exe("ffmpeg").map(|p| p.display().to_string());
        let model = self.model_status();
        let (run_on, ui_lang, check_updates, folder_formats, dir) = {
            let st = self.settings.lock().unwrap();
            (st.run_on().to_string(), st.ui_lang.clone(), st.check_updates, st.folder_formats(), st.download_dir(&self.paths))
        };
        let models = models::VARIANTS
            .iter()
            .map(|v| {
                let dest = dir.join(v.file);
                ModelView {
                    id: v.id.into(),
                    file: v.file.into(),
                    size: v.size,
                    url: models::url(*v),
                    on_disk: models::check_file(&dest).is_ok() || model.variant == v.id,
                    partial: models::downloaded(&dest),
                    active: model.variant == v.id,
                }
            })
            .collect();
        self.update(|s| {
            let st = &mut s.setup;
            st.engine_found = engine_found;
            st.ffmpeg = ffmpeg;
            st.model = model;
            st.models = models;
            st.files_url = models::files_page();
            st.run_on = run_on;
            st.ui_lang = ui_lang;
            st.check_updates = check_updates;
            st.folder_formats = folder_formats;
            st.app_version = crate::updates::VERSION.into();
            st.repo_url = format!("https://github.com/{}", crate::updates::REPO);
            st.data_dir = self.paths.roaming.display().to_string();
            if !engine_found {
                s.engine.state = "missing".into();
            } else if s.engine.state == "missing" || s.engine.state.is_empty() {
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

    /// Use a model file the user already has, or one in the folder they point at.
    pub fn import_model(&self, path: &Path) -> Result<ModelStatus> {
        let file = if path.is_dir() {
            models::find_in_folder(path).ok_or_else(|| anyhow!(crate::tr!("That folder does not hold a model file (.gguf).")))?
        } else {
            models::check_file(path)?;
            path.to_path_buf()
        };
        self.change_settings(|s| s.model_file = Some(file.display().to_string()))?;
        self.send(Msg::Restart);
        Ok(self.model_status())
    }

    /// Switch to a model that is already on the disk.
    pub fn use_model(&self, id: &str) -> Result<()> {
        let v = models::variant(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
        let file = self.settings.lock().unwrap().download_dir(&self.paths).join(v.file);
        models::check_file(&file)?;
        self.change_settings(|s| s.model_file = Some(file.display().to_string()))?;
        self.send(Msg::Restart);
        Ok(())
    }

    /// Delete a downloaded model, and what is there of an unfinished download of it.
    pub fn delete_model(&self, id: &str) -> Result<()> {
        let v = models::variant(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
        let dir = self.settings.lock().unwrap().download_dir(&self.paths);
        let file = dir.join(v.file);
        models::discard(&file);
        if file.exists() {
            let in_use = self.settings.lock().unwrap().model_file.as_deref().map(PathBuf::from) == Some(file.clone());
            if in_use {
                self.change_settings(|s| s.model_file = None)?;
            }
            self.send(Msg::DeleteFile(file));
        } else {
            self.refresh_setup();
        }
        Ok(())
    }

    /// Change where downloads are saved. What is there of a paused download comes along, if it can.
    pub fn set_download_dir(&self, dir: &Path) -> Result<()> {
        let old = self.settings.lock().unwrap().download_dir(&self.paths);
        std::fs::create_dir_all(dir)?;
        for v in models::VARIANTS {
            models::move_unfinished(&old.join(v.file), &dir.join(v.file));
        }
        self.change_settings(|s| s.download_dir = Some(dir.display().to_string()))
    }

    /// The interface language, for the messages Rust and the worker show.
    pub fn set_ui_lang(&self, lang: &str) -> Result<()> {
        let lang = if crate::i18n::LANGUAGES.contains(&lang) { lang } else { "en" };
        crate::i18n::set(lang);
        self.change_settings(|s| s.ui_lang = Some(lang.to_string()))?;
        Ok(())
    }

    /// GPU or CPU: takes effect with a fresh engine.
    pub fn set_run_on(&self, value: &str) -> Result<()> {
        if !matches!(value, "auto" | "cpu") {
            return Err(anyhow!("unknown setting {value}"));
        }
        self.gpu_failed.store(false, Ordering::SeqCst); // the user asks for it: try the GPU again
        self.change_settings(|s| s.run_on = Some(value.to_string()))?;
        self.send(Msg::Restart);
        Ok(())
    }

    // ------------------------------------------------------------ model download

    /// Start or continue downloading a model. It stays wanted until it finishes or is paused,
    /// so closing the app does not stop it for good.
    pub fn download_model(self: &Arc<Self>, id: &str) {
        let Some(v) = models::variant(id) else { return };
        if self.snapshot().setup.download.active {
            return;
        }
        let dest = self.settings.lock().unwrap().download_dir(&self.paths).join(v.file);
        let _ = self.change_settings(|s| s.resume_model = Some(id.to_string()));
        self.download_stop.store(false, Ordering::SeqCst);
        self.update(|s| {
            s.setup.download = DownloadView {
                active: true,
                model: Some(id.to_string()),
                done: models::downloaded(&dest),
                total: v.size,
                ..Default::default()
            };
        });
        let core = self.clone();
        std::thread::spawn(move || {
            let _awake = KeepAwake::new();
            let job = models::Download { url: models::url(v), dest: dest.clone(), size: v.size, sha256: v.sha256.to_string() };
            let stop = core.download_stop.clone();
            let result = models::download(&job, &stop, &mut |event| {
                core.update(|s| {
                    let d = &mut s.setup.download;
                    match event {
                        models::Event::Progress { done } => {
                            d.done = done;
                            d.retry_wait = None;
                            d.retry_attempt = None;
                        }
                        models::Event::Retrying { wait_s, attempt } => {
                            d.retry_wait = Some(wait_s);
                            d.retry_attempt = Some(attempt);
                        }
                        models::Event::Verifying { .. } => {
                            d.done = v.size;
                            d.verifying = true;
                        }
                    }
                });
            });
            let finished = matches!(result, Ok(models::Finished::Complete));
            let error = result.err().map(|e| format!("{e:#}"));
            if finished {
                let _ = core.change_settings(|s| {
                    s.model_file = Some(dest.display().to_string());
                    s.resume_model = None;
                });
            } else if error.is_some() {
                let _ = core.change_settings(|s| s.resume_model = None);
            }
            core.update(|s| {
                let d = &mut s.setup.download;
                d.active = false;
                d.verifying = false;
                d.retry_wait = None;
                d.retry_attempt = None;
                d.error = error;
            });
            core.refresh_setup();
            if finished {
                core.send(Msg::Restart);
            }
        });
    }

    /// Pause: what arrived stays on the disk, and nothing resumes by itself.
    pub fn stop_download(&self) {
        let _ = self.change_settings(|s| s.resume_model = None);
        self.download_stop.store(true, Ordering::SeqCst);
    }

    // ------------------------------------------------------------ queue

    pub fn enqueue(&self, paths: Vec<String>, language: String) -> Vec<String> {
        self.add_jobs(paths, language, false)
    }

    /// Every recording under `dir` that has no finished transcript yet. One that has gets
    /// any file it is missing next to it, without being transcribed again.
    pub fn enqueue_folder(&self, dir: &std::path::Path, language: String) -> FolderScan {
        let finished: std::collections::HashMap<String, String> = history::list(&self.paths.history_dir())
            .into_iter()
            .filter(|h| !h.partial)
            .map(|h| (folder::same_file_key(&h.source_path), h.id))
            .collect();
        let active: std::collections::HashSet<String> = self
            .snapshot()
            .queue
            .iter()
            .filter(|j| ACTIVE_JOB.contains(&j.status.as_str()))
            .map(|j| folder::same_file_key(&j.path))
            .collect();
        let formats = self.settings.lock().unwrap().folder_formats();
        let mut todo = Vec::new();
        let mut already = 0;
        for rec in folder::recordings_in(dir) {
            let path = rec.display().to_string();
            let key = folder::same_file_key(&path);
            if active.contains(&key) {
                already += 1;
            } else if let Some(id) = finished.get(&key) {
                already += 1;
                let missing = folder::missing_outputs(&rec, &formats);
                if !missing.is_empty() {
                    if let Ok(t) = self.transcript(id) {
                        let _ = folder::write_outputs(&t, &missing);
                    }
                }
            } else {
                todo.push(path);
            }
        }
        FolderScan { added: self.add_jobs(todo, language, true), already }
    }

    fn add_jobs(&self, paths: Vec<String>, language: String, from_folder: bool) -> Vec<String> {
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
                folder: from_folder,
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

    pub fn cancel_all(&self) {
        let ids: Vec<String> = self.snapshot().queue.into_iter().filter(|j| ACTIVE_JOB.contains(&j.status.as_str())).map(|j| j.id).collect();
        for id in ids {
            self.cancel(&id);
        }
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

fn ensure_engine<'a>(core: &Arc<Core>, engine: &'a mut Option<EngineProcess>) -> Result<&'a mut EngineProcess> {
    if engine.is_none() {
        let exe = setup::engine_exe().ok_or_else(|| anyhow!(crate::tr!("The speech engine is missing. Please install Dawwin again.")))?;
        let model = core.model_status();
        let cpu_only = core.settings.lock().unwrap().run_on() == "cpu" || core.gpu_failed.load(Ordering::SeqCst);
        core.update(|s| {
            s.engine.state = "starting".into();
            s.engine.error = None;
        });
        *engine = Some(EngineProcess::spawn(&exe, &core.paths, &model.path, cpu_only)?);
    }
    Ok(engine.as_mut().unwrap())
}

/// The GPU let us down: from now on the engine stays off it, until the app restarts.
fn give_up_on_gpu(core: &Arc<Core>, engine: &mut Option<EngineProcess>) {
    core.gpu_failed.store(true, Ordering::SeqCst);
    if let Some(e) = engine.take() {
        e.shutdown();
    }
    core.update(|s| {
        s.engine.state = "off".into();
        s.engine.device = Some("cpu".into());
        s.engine.note = Some("gpu-failed".into());
    });
}

fn engine_loop(core: Arc<Core>, rx: Receiver<Msg>) {
    let mut engine: Option<EngineProcess> = None;
    let mut last_used = Instant::now();
    loop {
        let msg = match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => {
                if engine.is_some() && last_used.elapsed() > IDLE_UNLOAD {
                    engine.take().unwrap().shutdown();
                    core.update(|s| s.engine.state = "off".into());
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        last_used = Instant::now();
        match msg {
            Msg::Probe => match ensure_engine(&core, &mut engine).and_then(|e| e.probe()) {
                Ok(v) => core.update(|s| {
                    let e = &mut s.engine;
                    if e.state == "starting" || e.state == "off" {
                        e.state = "ready".into();
                    }
                    e.gpu = v.get("gpu").and_then(|x| x.as_str()).map(String::from);
                    e.vram_total_gb = v.get("vram_total_gb").and_then(|x| x.as_f64());
                    e.device = Some(if e.gpu.is_some() { "gpu".into() } else { "cpu".into() });
                }),
                Err(err) if err.is::<EngineFailed>() && !core.gpu_failed.load(Ordering::SeqCst) => {
                    // perhaps the GPU driver took it down: the engine can do without it
                    give_up_on_gpu(&core, &mut engine);
                    core.send(Msg::Probe);
                }
                Err(err) => {
                    engine = None;
                    core.update(|s| {
                        s.engine.state = "error".into();
                        s.engine.error = Some(format!("{err:#}"));
                    });
                }
            },
            Msg::Restart | Msg::DeleteFile(_) => {
                if let Some(e) = engine.take() {
                    e.shutdown();
                }
                core.update(|s| {
                    s.engine.state = "off".into();
                    s.engine.note = None;
                    s.engine.device = None;
                });
                if let Msg::DeleteFile(file) = &msg {
                    if let Err(err) = std::fs::remove_file(file) {
                        core.update(|s| s.setup.download.error = Some(format!("{}: {err}", file.display())));
                    }
                    core.refresh_setup();
                }
                if setup::engine_exe().is_some() {
                    core.send(Msg::Probe);
                }
            }
            Msg::Job(id) => {
                let res = run_job(&core, &mut engine, &id);
                if let Err(err) = res {
                    let engine_at_fault = err.is::<EngineFailed>() || err.is::<OutOfMemory>();
                    let on_gpu = core.snapshot().engine.device.as_deref() == Some("gpu");
                    let cancelled = core.cancels.lock().unwrap().get(&id).is_some_and(|c| c.load(Ordering::SeqCst));
                    if engine_at_fault && on_gpu && !cancelled && !core.gpu_failed.load(Ordering::SeqCst) {
                        // the same recording again, on the CPU: slower, but it gets done
                        give_up_on_gpu(&core, &mut engine);
                        core.update_job(&id, |j| {
                            j.status = "queued".into();
                            j.progress = 0.0;
                            j.eta_s = None;
                            j.saved = false;
                        });
                        let _ = core.app.emit("transcript-started", &id);
                        core.send(Msg::Job(id));
                        continue;
                    }
                    let msg = format!("{err:#}");
                    core.update_job(&id, |j| {
                        j.status = "failed".into();
                        j.error = Some(msg);
                        j.eta_s = None;
                    });
                    // a dead engine is restarted for the next job
                    if let Some(e) = engine.as_mut() {
                        if !e.alive() {
                            engine = None;
                            core.update(|s| s.engine.state = "off".into());
                        }
                    }
                }
                core.cancels.lock().unwrap().remove(&id);
            }
        }
    }
    if let Some(e) = engine.take() {
        e.shutdown();
    }
}
fn run_job(core: &Arc<Core>, engine: &mut Option<EngineProcess>, id: &str) -> Result<()> {
    let Some(job) = core.snapshot().queue.into_iter().find(|j| j.id == id) else { return Ok(()) };
    if job.status != "queued" {
        return Ok(());
    }
    let cancel = core.cancels.lock().unwrap().get(id).cloned().unwrap_or_default();
    let started = Instant::now();
    let _awake = KeepAwake::new();
    core.update_job(id, |j| j.status = "decoding".into());

    let model = core.model_status();
    if !model.found {
        return Err(anyhow!(crate::tr!("There is no model yet. Choose one in Settings.")));
    }
    let w = ensure_engine(core, engine)?;
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
        Some(g) if info.device == "gpu" => format!("GPU · {g}"),
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
        model: match model.variant.as_str() {
            "" => MODEL_LABEL.to_string(),
            v => format!("{MODEL_LABEL} · {}", v.to_uppercase()),
        },
        partial: false,
        segments: Vec::new(),
    };
    core.live.lock().unwrap().insert(id.to_string(), t.clone());
    core.update_job(id, |j| j.status = "transcribing".into());
    let _ = core.app.emit("transcript-started", id);

    let t_start = Instant::now();
    let mut batch = info.batch;
    let mut last_saved = Instant::now();
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
                for (r, got) in ranges.iter().zip(texts) {
                    match fix_loops(w, &audio, *r, got, &job.language, 0) {
                        Ok(s) => lines.extend(s.into_iter().filter(|s| !s.text.trim().is_empty())),
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    };
                }
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
                // closing the app in the middle of a long recording keeps what is written so far
                if last_saved.elapsed() > Duration::from_secs(20) && !t.segments.is_empty() {
                    t.partial = true;
                    t.elapsed_s = t_start.elapsed().as_secs_f64();
                    let _ = history::save(&core.paths.history_dir(), &t);
                    last_saved = Instant::now();
                }
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
    if job.folder && saved && !cancelled {
        let formats = core.settings.lock().unwrap().folder_formats();
        if let Err(e) = folder::write_outputs(&t, &formats) {
            core.update_job(id, |j| j.saved = saved);
            return Err(anyhow!(crate::tr!("Transcribed, but the files could not be saved next to the recording: {}", e)));
        }
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
fn show_loaded(core: &Arc<Core>, info: &EngineInfo) {
    core.update(|s| {
        s.engine.state = "loaded".into();
        s.engine.device = Some(info.device.clone());
        if info.gpu.is_some() {
            s.engine.gpu = info.gpu.clone();
            s.engine.vram_total_gb = info.vram_total_gb;
        }
        s.engine.note = info.fallback.clone().or_else(|| s.engine.note.take());
    });
}

/// The model sometimes repeats itself, or stops because of it, and the rest of the audio goes
/// unread. Transcribe such a chunk again in two halves; a half that still does it gets the
/// repetition cut and a "repeat" flag.
fn fix_loops(w: &mut dyn Engine, audio: &Audio, (a, b): (usize, usize), got: Recognized, lang: &str, depth: u32) -> Result<Vec<Segment>> {
    let seg = |a: usize, b: usize, text: String, flag: Option<&str>| Segment {
        start: a as f64 / SR as f64,
        end: b as f64 / SR as f64,
        text,
        flag: flag.map(String::from),
        edited: false,
        voice: audio::voice_profile(&audio.samples, a, b, 20),
        words: Vec::new(),
    };
    if !got.stopped && find_loop(&got.text).is_none() {
        return Ok(vec![seg(a, b, got.text, None)]);
    }
    if depth >= 2 || b - a < 4 * SR {
        return Ok(vec![seg(a, b, collapse_loops(&got.text), Some("repeat"))]);
    }
    let m = audio::split_middle(&audio.samples, a, b);
    let parts = w.transcribe(audio, &[(a, m), (m, b)], lang)?;
    let mut out = Vec::new();
    for (r, part) in [(a, m), (m, b)].into_iter().zip(parts) {
        out.extend(fix_loops(w, audio, r, part, lang, depth + 1)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Replays canned transcripts and records what it was asked.
    struct Fake {
        calls: Vec<Vec<(usize, usize)>>,
        replies: VecDeque<Vec<Recognized>>,
    }

    fn said(text: &str) -> Recognized {
        Recognized { text: text.to_string(), stopped: false }
    }

    impl Engine for Fake {
        fn start_loading(&mut self) -> Result<()> {
            Ok(())
        }
        fn ensure_loaded(&mut self) -> Result<EngineInfo> {
            Ok(EngineInfo::default())
        }
        fn transcribe(&mut self, _a: &Audio, ranges: &[(usize, usize)], _l: &str) -> Result<Vec<Recognized>> {
            self.calls.push(ranges.to_vec());
            Ok(self.replies.pop_front().expect("no more canned replies"))
        }
        fn forget(&mut self, _: &str) {}
    }

    fn looping() -> Recognized {
        said(&format!("ناجح 20 ما في {}س", "ست ".repeat(20)))
    }

    #[test]
    fn a_looping_chunk_is_transcribed_again_in_two_halves() {
        let audio = Audio { id: "x".into(), samples: vec![0.1; 20 * SR] };
        let mut e = Fake { calls: vec![], replies: VecDeque::from(vec![vec![said("أول نص"), said("ثاني نص")]]) };
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
        let segs = fix_loops(&mut e, &audio, (0, 20 * SR), said("طيب هلا مكتب الاختبارات"), "ar", 0).unwrap();
        assert!(e.calls.is_empty());
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn text_that_stopped_early_is_read_again_so_the_rest_of_the_audio_is_not_lost() {
        let audio = Audio { id: "x".into(), samples: vec![0.1; 20 * SR] };
        let mut e = Fake { calls: vec![], replies: VecDeque::from(vec![vec![said("أول نص"), said("ثاني نص")]]) };
        let stopped = Recognized { text: "أول".to_string(), stopped: true };
        let segs = fix_loops(&mut e, &audio, (0, 20 * SR), stopped, "ar", 0).unwrap();
        assert_eq!(e.calls.len(), 1);
        assert_eq!(segs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["أول نص", "ثاني نص"]);
    }

    #[test]
    fn a_short_piece_that_stopped_early_keeps_its_text_and_is_flagged() {
        let audio = Audio { id: "x".into(), samples: vec![0.1; 3 * SR] };
        let mut e = Fake { calls: vec![], replies: VecDeque::new() };
        let stopped = Recognized { text: "طيب".to_string(), stopped: true };
        let segs = fix_loops(&mut e, &audio, (0, 3 * SR), stopped, "ar", 0).unwrap();
        assert!(e.calls.is_empty());
        assert_eq!(segs[0].text, "طيب");
        assert_eq!(segs[0].flag.as_deref(), Some("repeat"));
    }
}
