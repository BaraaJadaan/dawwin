//! The app's state and the one thread that owns the engine: it runs the queue and unloads the
//! model when the app sits idle. A model download runs on a thread of its own.

use std::collections::{BTreeMap, HashMap};
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
use crate::models::{self, Family};
use crate::setup::{self, KeepAwake, ModelStatus, Paths, Settings};
use crate::tools;
use crate::transcript::{collapse_loops, find_loop, Segment, TranslatedLine, Transcript, Translation};
use crate::translate::TranslatorProcess;

mod minutes_job;

const MAX_CHUNK_S: f32 = 30.0;
const SEARCH_S: f32 = 8.0;
/// Free the GPU after this long with nothing to do.
const IDLE_UNLOAD: Duration = Duration::from_secs(10 * 60);
const ACTIVE_JOB: &[&str] = &["queued", "decoding", "loading", "transcribing", "translating", "writing", "saving_audio", "saving_video"];
/// Save a translation that is under way at least this often, so closing the app keeps it.
const TRANSLATION_SAVE: Duration = Duration::from_secs(10);

/// What to do with a recording, in this order: take the long silences out, save its sound,
/// transcribe it, translate the transcript, and add the subtitles to the video.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq)]
#[serde(default)]
pub struct Steps {
    pub remove_silence: bool,
    pub save_audio: bool,
    pub transcribe: bool,
    /// The language to translate into
    pub translate: Option<String>,
    pub video: bool,
    /// The video's subtitles burned into the picture, rather than added as tracks
    pub burn: bool,
    /// Write the minutes after the transcript
    pub minutes: bool,
}

impl Default for Steps {
    fn default() -> Self {
        Steps { remove_silence: false, save_audio: false, transcribe: true, translate: None, video: false, burn: false, minutes: false }
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct JobView {
    pub id: String,
    pub title: String,
    pub path: String,
    pub language: String,
    pub steps: Steps,
    /// The files it saved: the sound, the video with subtitles
    pub outputs: Vec<String>,
    /// "translate" for translating a finished transcript, "burn" for burning subtitles into its
    /// video, "minutes" for writing its minutes; empty for transcribing a recording
    pub kind: String,
    /// What a translation translates: the transcript's id, and the language it goes into
    pub transcript: Option<String>,
    pub target: Option<String>,
    /// Where a translation runs: "GPU · …" or "CPU"
    pub device: Option<String>,
    /// queued | decoding | loading | transcribing | translating | done | failed | cancelled
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
    /// Recordings left out because only ffmpeg reads them and there is none
    pub needs_ffmpeg: usize,
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
    /// Which model or tool, while it downloads
    pub model: Option<String>,
    /// "translator" while the translator downloads before the first translation model
    pub part: Option<String>,
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
    /// "cohere" | "canary" | "whisper"
    pub family: String,
    pub file: String,
    pub size: u64,
    pub url: String,
    /// Where to download it by hand
    pub page: String,
    /// The whole file is there
    pub on_disk: bool,
    /// How much of a download that is not finished is there
    pub partial: u64,
    pub active: bool,
    /// The file in use is the user's own copy outside the download folder: it is only forgotten,
    /// never deleted
    pub outside: bool,
}

/// One of the tools the app can download (see `tools`).
#[derive(Clone, Debug, Serialize, Default)]
pub struct ToolView {
    pub id: String,
    pub size: u64,
    pub on_disk: bool,
    /// How much of a download that is not finished is there
    pub partial: u64,
    /// The one the app uses
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SetupView {
    pub engine_found: bool,
    /// llama.cpp's server, which runs the translation models
    pub translator_found: bool,
    /// The ffmpeg in use, and the one that burns subtitles in, when there are
    pub ffmpeg: Option<String>,
    pub burner: Option<String>,
    pub tools: Vec<ToolView>,
    /// The model file in use of each family, by its key
    pub families: BTreeMap<String, ModelStatus>,
    pub models: Vec<ModelView>,
    /// Where downloads are saved
    pub download_dir: String,
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
    Translate(String),
    Minutes(String),
    Probe,
    /// Stop the engine so the next one starts with new settings (another model, GPU or CPU)
    Restart,
    /// Stop the engine, which may be using this file, and delete it
    DeleteFile(PathBuf),
    /// Stop the translator, and delete it
    RemoveTool(tools::Tool),
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
    /// The same, for the translator
    translator_gpu_failed: AtomicBool,
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
        // this is the one Dawwin of the data folder (see `instance`): what is left is nobody's
        paths.clear_leftover_audio();
        tools::refresh_dates(&paths);
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
            translator_gpu_failed: AtomicBool::new(false),
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
            let _ = core.download_model(&id);
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
        let translator_found = setup::translator_exe(&self.paths).is_some();
        let ffmpeg = setup::ffmpeg(&self.paths);
        let burner = setup::burner(&self.paths).map(|p| p.display().to_string());
        let tool_views = tools::TOOLS
            .iter()
            .map(|t| {
                let exe = tools::exe(&self.paths, t);
                ToolView {
                    id: t.id.into(),
                    size: t.size,
                    on_disk: exe.is_some(),
                    partial: tools::partial(&self.paths, t),
                    active: exe.is_some() && (t.id == tools::TRANSLATOR.id || exe == ffmpeg),
                }
            })
            .collect();
        let ffmpeg = ffmpeg.map(|p| p.display().to_string());
        let (families, run_on, ui_lang, check_updates, folder_formats, dir) = {
            let st = self.settings.lock().unwrap();
            let families: BTreeMap<String, ModelStatus> =
                Family::ALL.into_iter().map(|f| (f.key().to_string(), setup::model_status(&self.paths, &st, f))).collect();
            (families, st.run_on().to_string(), st.ui_lang.clone(), st.check_updates, st.folder_formats(), st.download_dir(&self.paths))
        };
        let models = models::VARIANTS
            .iter()
            .map(|v| {
                let dest = dir.join(v.file);
                let in_use = &families[v.family.key()];
                let active = in_use.variant == v.id;
                let outside = active && !self.in_dawwins_folders(&in_use.path, &dir, v.file);
                ModelView {
                    outside,
                    id: v.id.into(),
                    family: v.family.key().into(),
                    file: v.file.into(),
                    size: v.size,
                    url: models::url(*v),
                    page: models::files_page(*v),
                    on_disk: models::check_file(&dest).is_ok() || active,
                    partial: models::downloaded(&dest),
                    active,
                }
            })
            .collect();
        self.update(|s| {
            let st = &mut s.setup;
            st.engine_found = engine_found;
            st.translator_found = translator_found;
            st.ffmpeg = ffmpeg;
            st.burner = burner;
            st.tools = tool_views;
            st.families = families;
            st.models = models;
            st.download_dir = dir.display().to_string();
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

    /// Use a model file the user already has, or the ones in the folder they point at.
    pub fn import_model(&self, path: &Path) -> Result<()> {
        let files: Vec<(Family, PathBuf)> = if path.is_dir() {
            let found: Vec<_> = Family::ALL.into_iter().filter_map(|f| Some((f, models::find_in_folder(path, Some(f))?))).collect();
            if found.is_empty() {
                return Err(anyhow!(crate::tr!("That folder does not hold a model file (.gguf).")));
            }
            found
        } else {
            vec![(models::check_file(path)?, path.to_path_buf())]
        };
        self.change_settings(|s| {
            for (family, file) in &files {
                s.choose_model(*family, Some(file));
            }
        })?;
        self.send(Msg::Restart);
        Ok(())
    }

    /// Switch to a model that is already on the disk.
    pub fn use_model(&self, id: &str) -> Result<()> {
        let v = models::variant(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
        let file = self.settings.lock().unwrap().download_dir(&self.paths).join(v.file);
        models::check_file(&file)?;
        self.change_settings(|s| s.choose_model(v.family, Some(&file)))?;
        self.send(Msg::Restart);
        Ok(())
    }

    /// Delete a downloaded model or tool, and what is there of an unfinished download of it.
    pub fn delete_model(&self, id: &str) -> Result<()> {
        if let Some(t) = tools::tool(id) {
            if t == tools::TRANSLATOR {
                // it may be running: the engine thread stops it first
                self.send(Msg::RemoveTool(t));
            } else {
                tools::remove(&self.paths, &t)?;
                self.refresh_setup();
            }
            return Ok(());
        }
        let v = models::variant(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
        let dir = self.settings.lock().unwrap().download_dir(&self.paths);
        let in_use = setup::model_status(&self.paths, &self.settings.lock().unwrap(), v.family);
        let mut file = dir.join(v.file);
        if in_use.found && in_use.variant == v.id {
            if !self.in_dawwins_folders(&in_use.path, &dir, v.file) {
                // the user's own copy elsewhere: stop using it, and leave the file alone
                self.change_settings(|s| s.choose_model(v.family, None))?;
                self.send(Msg::Restart);
                return Ok(());
            }
            // the one in use, in the download folder or the one Dawwin used before it changed
            file = PathBuf::from(&in_use.path);
        }
        models::discard(&file);
        if file.exists() {
            let in_use = self.settings.lock().unwrap().chosen_model(v.family).as_ref() == Some(&file);
            if in_use {
                self.change_settings(|s| s.choose_model(v.family, None))?;
            }
            self.send(Msg::DeleteFile(file));
        } else {
            self.refresh_setup();
        }
        Ok(())
    }

    /// Whether a model file is one Dawwin downloaded: in the download folder, or the default one.
    fn in_dawwins_folders(&self, path: &str, download_dir: &Path, file: &str) -> bool {
        let key = folder::same_file_key(path);
        [download_dir.join(file), self.paths.default_model_dir().join(file)].iter().any(|p| folder::same_file_key(&p.display().to_string()) == key)
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
        // the user asks for it: try the GPU again
        self.gpu_failed.store(false, Ordering::SeqCst);
        self.translator_gpu_failed.store(false, Ordering::SeqCst);
        self.change_settings(|s| s.run_on = Some(value.to_string()))?;
        self.send(Msg::Restart);
        Ok(())
    }

    // ------------------------------------------------------------ model download

    /// Start or continue downloading a model or a tool. It stays wanted until it finishes or is
    /// paused, so closing the app does not stop it for good. The first translation model brings
    /// the translator along, before it.
    /// False when it did not start: one download at a time, and another one runs.
    pub fn download_model(self: &Arc<Self>, id: &str) -> bool {
        let tool = tools::tool(id);
        let variant = models::variant(id);
        if (tool.is_none() && variant.is_none()) || self.snapshot().setup.download.active {
            return false;
        }
        let dest = variant.map(|v| self.settings.lock().unwrap().download_dir(&self.paths).join(v.file));
        let translator_first = variant.is_some_and(|v| matches!(v.family, Family::Translator | Family::Minutes)) && setup::translator_exe(&self.paths).is_none();
        let _ = self.change_settings(|s| s.resume_model = Some(id.to_string()));
        self.download_stop.store(false, Ordering::SeqCst);
        let (done, total) = match (tool, variant, &dest) {
            (Some(t), _, _) => (tools::partial(&self.paths, &t), t.size),
            _ if translator_first => (tools::partial(&self.paths, &tools::TRANSLATOR), tools::TRANSLATOR.size),
            (_, Some(v), Some(dest)) => (models::downloaded(dest), v.size),
            _ => (0, 0),
        };
        self.update(|s| {
            s.setup.download = DownloadView {
                active: true,
                model: Some(id.to_string()),
                part: translator_first.then(|| tools::TRANSLATOR.id.to_string()),
                done,
                total,
                ..Default::default()
            };
        });
        let core = self.clone();
        std::thread::spawn(move || {
            let _awake = KeepAwake::new();
            let stop = core.download_stop.clone();
            let mut on_event = |event: models::Event| {
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
                        models::Event::Verifying { done } => {
                            d.done = done;
                            d.verifying = true;
                        }
                    }
                });
            };
            let result = (|| -> Result<models::Finished> {
                if let Some(t) = tool {
                    return tools::install(&core.paths, &t, &stop, &mut on_event);
                }
                let (v, dest) = (variant.unwrap(), dest.clone().unwrap());
                if translator_first && tools::install(&core.paths, &tools::TRANSLATOR, &stop, &mut on_event)? == models::Finished::Paused {
                    return Ok(models::Finished::Paused);
                }
                core.refresh_setup();
                core.update(|s| {
                    let d = &mut s.setup.download;
                    d.part = None;
                    d.verifying = false;
                    d.done = models::downloaded(&dest);
                    d.total = v.size;
                });
                models::download(&models::Download { url: models::url(v), dest, size: v.size, sha256: v.sha256.to_string() }, &stop, &mut on_event)
            })();
            let finished = matches!(result, Ok(models::Finished::Complete));
            let error = result.err().map(|e| format!("{e:#}"));
            if finished {
                let _ = core.change_settings(|s| {
                    if let (Some(v), Some(dest)) = (variant, &dest) {
                        s.choose_model(v.family, Some(dest));
                    }
                    s.resume_model = None;
                });
            } else if error.is_some() {
                let _ = core.change_settings(|s| s.resume_model = None);
            }
            core.update(|s| {
                let d = &mut s.setup.download;
                d.active = false;
                d.part = None;
                d.verifying = false;
                d.retry_wait = None;
                d.retry_attempt = None;
                d.error = error;
            });
            core.refresh_setup();
            if finished && variant.is_some() {
                core.send(Msg::Restart);
            }
        });
        true
    }

    /// Pause: what arrived stays on the disk, and nothing resumes by itself.
    pub fn stop_download(&self) {
        let _ = self.change_settings(|s| s.resume_model = None);
        self.download_stop.store(true, Ordering::SeqCst);
    }

    // ------------------------------------------------------------ queue

    pub fn enqueue(&self, paths: Vec<String>, language: String, steps: Steps) -> Vec<String> {
        self.add_jobs(paths, language, steps, false)
    }

    /// Every recording under `dir` that has no finished transcript yet. One that has gets
    /// any file it is missing next to it, without being transcribed again.
    pub fn enqueue_folder(&self, dir: &std::path::Path, language: String, steps: Steps) -> FolderScan {
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
        let have_ffmpeg = setup::ffmpeg(&self.paths).is_some();
        let mut todo = Vec::new();
        let (mut already, mut needs_ffmpeg) = (0, 0);
        for rec in folder::recordings_in(dir) {
            let path = rec.display().to_string();
            let key = folder::same_file_key(&path);
            if !have_ffmpeg && folder::needs_ffmpeg(&rec) && !finished.contains_key(&key) {
                // offered ffmpeg instead; a run after its download picks these up
                needs_ffmpeg += 1;
            } else if active.contains(&key) {
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
        FolderScan { added: self.add_jobs(todo, language, steps, true), already, needs_ffmpeg }
    }

    fn add_jobs(&self, paths: Vec<String>, language: String, steps: Steps, from_folder: bool) -> Vec<String> {
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
                steps: steps.clone(),
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

    /// Translate a finished transcript into another language, as a job in the queue. Lines that
    /// already have a translation of what they say now are kept, so this also brings a translation
    /// up to date after edits, or finishes one that was stopped.
    pub fn translate(&self, transcript: &str, language: &str) -> Result<String> {
        self.translate_then(transcript, language, false, false)
    }

    /// The same; with `video`, the video gets the subtitles in both languages after it, burned in
    /// with `burn`.
    pub fn translate_then(&self, transcript: &str, language: &str, video: bool, burn: bool) -> Result<String> {
        if !models::can_translate_into(language) {
            return Err(anyhow!(crate::tr!("Dawwin cannot translate into that language.")));
        }
        if self.is_live(transcript) {
            return Err(anyhow!(crate::tr!("This transcript is still being written. Translate it when it is done.")));
        }
        let t = history::load(&self.paths.history_dir(), transcript)?;
        if t.language == language {
            return Err(anyhow!(crate::tr!("The transcript is already in that language.")));
        }
        if !setup::model_status(&self.paths, &self.settings.lock().unwrap(), Family::Translator).found {
            return Err(anyhow!(crate::tr!("There is no translation model yet. Download one in Settings.")));
        }
        if setup::translator_exe(&self.paths).is_none() {
            return Err(anyhow!(crate::tr!("The translator is not downloaded. Download it in Settings.")));
        }
        let running = self.snapshot().queue.into_iter().find(|j| {
            j.kind == "translate" && j.transcript.as_deref() == Some(transcript) && j.target.as_deref() == Some(language) && ACTIVE_JOB.contains(&j.status.as_str())
        });
        if let Some(j) = running {
            return Ok(j.id);
        }
        // a translation that stopped before goes from the list: this one carries it on
        self.update(|s| {
            s.queue.retain(|j| !(j.kind == "translate" && j.transcript.as_deref() == Some(transcript) && j.target.as_deref() == Some(language)))
        });
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let id = format!("{}-{:03}", now_ms(), n % 1000);
        self.cancels.lock().unwrap().insert(id.clone(), Arc::new(AtomicBool::new(false)));
        let job = JobView {
            id: id.clone(),
            title: t.title.clone(),
            path: t.source_path.clone(),
            language: t.language.clone(),
            kind: "translate".into(),
            steps: Steps { transcribe: false, video, burn, ..Default::default() },
            transcript: Some(transcript.to_string()),
            target: Some(language.to_string()),
            status: "queued".into(),
            ..Default::default()
        };
        self.update(|s| s.queue.push(job));
        self.send(Msg::Translate(id.clone()));
        Ok(id)
    }

    /// Burn a transcript's subtitles (`view`: its own, its translation's, or both) into its video,
    /// saved as `dest`: a job in the list, which runs at once on a thread of its own, since it does
    /// not need the speech engine.
    pub fn burn(self: &Arc<Self>, transcript: &str, view: &crate::transcript::View, dest: PathBuf) -> Result<String> {
        let ffmpeg = setup::burner(&self.paths).ok_or_else(|| anyhow!(crate::tr!("Burning subtitles in needs the video ffmpeg. Download it in Settings.")))?;
        let t = self.transcript(transcript)?;
        let source = PathBuf::from(&t.source_path);
        if !source.is_file() {
            return Err(anyhow!(crate::tr!("The video is not at {} any more.", source.display())));
        }
        let srt = String::from_utf8(crate::transcript::export_view(&t, "srt", false, view)?)?;
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let id = format!("{}-{:03}", now_ms(), n % 1000);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels.lock().unwrap().insert(id.clone(), cancel.clone());
        let job = JobView {
            id: id.clone(),
            title: t.title.clone(),
            path: t.source_path.clone(),
            language: t.language.clone(),
            kind: "burn".into(),
            steps: Steps { transcribe: false, video: true, burn: true, ..Default::default() },
            transcript: Some(transcript.to_string()),
            status: "saving_video".into(),
            ..Default::default()
        };
        self.update(|s| s.queue.push(job));
        let core = self.clone();
        let job_id = id.clone();
        std::thread::spawn(move || {
            let _awake = KeepAwake::new();
            let started = Instant::now();
            let result = crate::video::burn_subtitles(&ffmpeg, &source, &srt, &dest, &core.paths.cache_dir(), &cancel, &mut |f| {
                core.update_job(&job_id, |j| {
                    let spent = started.elapsed().as_secs_f64();
                    j.progress = f;
                    j.elapsed_s = spent;
                    j.eta_s = (f > 0.02).then(|| spent / f * (1.0 - f));
                });
            });
            let cancelled = cancel.load(Ordering::SeqCst);
            core.update_job(&job_id, |j| {
                j.eta_s = None;
                j.elapsed_s = started.elapsed().as_secs_f64();
                match &result {
                    Ok(()) => {
                        j.status = "done".into();
                        j.outputs.push(dest.display().to_string());
                    }
                    Err(_) if cancelled => j.status = "cancelled".into(),
                    Err(e) => {
                        j.status = "failed".into();
                        j.error = Some(format!("{e:#}"));
                    }
                }
            });
            core.cancels.lock().unwrap().remove(&job_id);
        });
        Ok(id)
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

/// The engine, started if needed. With a `model` it is one that holds that model: a process holds
/// one, and quitting it gives all the memory back.
fn ensure_engine<'a>(core: &Arc<Core>, engine: &'a mut Option<EngineProcess>, model: Option<&str>) -> Result<&'a mut EngineProcess> {
    // an engine that died since its last job is not used
    if engine.as_mut().is_some_and(|e| !e.alive()) {
        *engine = None;
    }
    if let (Some(e), Some(m)) = (engine.as_mut(), model) {
        if !e.use_model(m) {
            engine.take().unwrap().shutdown();
        }
    }
    if engine.is_none() {
        let exe = setup::engine_exe().ok_or_else(|| anyhow!(crate::tr!("The speech engine is missing. Please install Dawwin again.")))?;
        let cpu_only = core.settings.lock().unwrap().run_on() == "cpu" || core.gpu_failed.load(Ordering::SeqCst);
        core.update(|s| {
            s.engine.state = "starting".into();
            s.engine.error = None;
        });
        *engine = Some(EngineProcess::spawn(&exe, &core.paths, model.unwrap_or_default(), cpu_only)?);
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
    // the speech engine and the translator take turns: both want the GPU's memory
    let mut translator: Option<TranslatorProcess> = None;
    let mut last_used = Instant::now();
    loop {
        let msg = match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => {
                if last_used.elapsed() > IDLE_UNLOAD {
                    if let Some(e) = engine.take() {
                        e.shutdown();
                        core.update(|s| s.engine.state = "off".into());
                    }
                    if let Some(t) = translator.take() {
                        t.shutdown();
                    }
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        last_used = Instant::now();
        match msg {
            Msg::Probe => match ensure_engine(&core, &mut engine, None).and_then(|e| e.probe()) {
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
            Msg::Restart | Msg::DeleteFile(_) | Msg::RemoveTool(_) => {
                if let Some(e) = engine.take() {
                    e.shutdown();
                }
                if let Some(t) = translator.take() {
                    t.shutdown();
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
                if let Msg::RemoveTool(tool) = &msg {
                    if let Err(err) = tools::remove(&core.paths, tool) {
                        core.update(|s| s.setup.download.error = Some(format!("{err:#}")));
                    }
                    core.refresh_setup();
                }
                if setup::engine_exe().is_some() {
                    core.send(Msg::Probe);
                }
            }
            Msg::Translate(id) => {
                if let Some(e) = engine.take() {
                    e.shutdown();
                    core.update(|s| s.engine.state = "off".into());
                }
                if let Err(err) = run_translation(&core, &mut translator, &id) {
                    let on_gpu = translator.as_ref().is_some_and(|t| !t.cpu_only);
                    let cancelled = core.cancels.lock().unwrap().get(&id).is_some_and(|c| c.load(Ordering::SeqCst));
                    if err.is::<EngineFailed>() && on_gpu && !cancelled && !core.translator_gpu_failed.swap(true, Ordering::SeqCst) {
                        // the rest of it again, on the CPU: slower, but it gets done
                        if let Some(t) = translator.take() {
                            t.shutdown();
                        }
                        core.update_job(&id, |j| {
                            j.status = "queued".into();
                            j.eta_s = None;
                        });
                        core.send(Msg::Translate(id));
                        continue;
                    }
                    end_in_error(&core, &id, err, cancelled);
                    if let Some(t) = translator.as_mut() {
                        if !t.alive() {
                            translator = None;
                        }
                    }
                }
                core.cancels.lock().unwrap().remove(&id);
            }
            Msg::Minutes(id) => {
                if let Some(e) = engine.take() {
                    e.shutdown();
                    core.update(|s| s.engine.state = "off".into());
                }
                if let Err(err) = minutes_job::run(&core, &mut translator, &id) {
                    let on_gpu = translator.as_mut().is_some_and(|t| !t.cpu_only && t.alive());
                    let cancelled = core.cancels.lock().unwrap().get(&id).is_some_and(|c| c.load(Ordering::SeqCst));
                    if err.is::<EngineFailed>() && on_gpu && !cancelled && !core.translator_gpu_failed.swap(true, Ordering::SeqCst) {
                        // the rest of it again, on the CPU: the notes done are kept
                        if let Some(t) = translator.take() {
                            t.shutdown();
                        }
                        core.update_job(&id, |j| {
                            j.status = "queued".into();
                            j.eta_s = None;
                        });
                        core.send(Msg::Minutes(id));
                        continue;
                    }
                    end_in_error(&core, &id, err, cancelled);
                    if let Some(t) = translator.as_mut() {
                        if !t.alive() {
                            translator = None;
                        }
                    }
                }
                core.cancels.lock().unwrap().remove(&id);
            }
            Msg::Job(id) => {
                if let Some(t) = translator.take() {
                    t.shutdown();
                }
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
                    end_in_error(&core, &id, err, cancelled);
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
        // idle from the end of the last job, not its start: a long job's model stays loaded after it
        last_used = Instant::now();
    }
    if let Some(e) = engine.take() {
        e.shutdown();
    }
    if let Some(t) = translator.take() {
        t.shutdown();
    }
}

/// A job that ended in an error: stopped, when the user had pressed stop (whatever the error says:
/// a stop while ffmpeg reads or the translator loads ends in one), else failed with the reason.
fn end_in_error(core: &Arc<Core>, id: &str, err: anyhow::Error, cancelled: bool) {
    let msg = format!("{err:#}");
    core.update_job(id, |j| {
        if cancelled {
            j.status = "cancelled".into();
        } else {
            j.status = "failed".into();
            j.error = Some(msg);
        }
        j.eta_s = None;
    });
}

/// The translator, started with this model if it is not running with it already, and loaded.
fn ensure_translator<'a>(core: &Arc<Core>, slot: &'a mut Option<TranslatorProcess>, model: &str, cancel: &AtomicBool) -> Result<&'a mut TranslatorProcess> {
    let cpu_only = core.settings.lock().unwrap().run_on() == "cpu" || core.translator_gpu_failed.load(Ordering::SeqCst);
    if let Some(t) = slot.as_mut() {
        if t.model != model || t.cpu_only != cpu_only || !t.alive() {
            slot.take().unwrap().shutdown();
        }
    }
    if slot.is_none() {
        let exe = setup::translator_exe(&core.paths).ok_or_else(|| anyhow!(crate::tr!("The translator is not downloaded. Download it in Settings.")))?;
        // the GPU the speech engine found best: a dedicated one before one in the processor
        let gpu = core.snapshot().engine.gpu;
        *slot = Some(TranslatorProcess::spawn(&exe, &core.paths, model, cpu_only, gpu.as_deref())?);
    }
    let t = slot.as_mut().unwrap();
    t.wait_ready(cancel)?;
    Ok(t)
}

/// The lines of a transcript to translate: those with text and without a translation of what they say now.
fn lines_to_translate(t: &Transcript, language: &str) -> Vec<usize> {
    let done = t.translation(language);
    (0..t.segments.len())
        .filter(|&i| !t.segments[i].text.trim().is_empty() && !done.is_some_and(|tr| tr.is_current(i, &t.segments[i])))
        .collect()
}

/// Write translated lines into the transcript's file. A line that was edited while it was being
/// translated keeps waiting: its translation is of words it no longer has.
fn keep_translated(dir: &Path, transcript: &str, language: &str, model: &str, lines: &mut Vec<(usize, String, String)>) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let taken = std::mem::take(lines);
    history::modify(dir, transcript, |t| {
        if t.translation(language).is_none() {
            t.translations.push(Translation::new(language, model));
        }
        let count = t.segments.len();
        let texts: Vec<String> = t.segments.iter().map(|s| s.text.clone()).collect();
        let tr = t.translations.iter_mut().find(|x| x.language == language).expect("just added");
        tr.model = model.to_string();
        tr.lines.resize(count, TranslatedLine::default());
        for (i, from, text) in taken {
            if texts.get(i) == Some(&from) {
                tr.lines[i] = TranslatedLine { text, from, edited: false };
            }
        }
    })?;
    Ok(())
}

fn run_translation(core: &Arc<Core>, translator: &mut Option<TranslatorProcess>, id: &str) -> Result<()> {
    let Some(job) = core.snapshot().queue.into_iter().find(|j| j.id == id) else { return Ok(()) };
    if job.status != "queued" {
        return Ok(());
    }
    let (Some(transcript), Some(language)) = (job.transcript.clone(), job.target.clone()) else { return Ok(()) };
    let cancel = core.cancels.lock().unwrap().get(id).cloned().unwrap_or_default();
    let started = Instant::now();
    let _awake = KeepAwake::new();
    core.update_job(id, |j| j.status = "loading".into());

    let model = setup::model_status(&core.paths, &core.settings.lock().unwrap(), Family::Translator);
    let variant = models::variant(&model.variant).filter(|v| model.found && v.family == Family::Translator);
    let Some(variant) = variant else {
        return Err(anyhow!(crate::tr!("There is no translation model yet. Download one in Settings.")));
    };
    let prompt = variant.prompt.expect("a translation model has a prompt");
    let t = history::load(&core.paths.history_dir(), &transcript)?;
    let todo = lines_to_translate(&t, &language);
    let w = ensure_translator(core, translator, &model.path, &cancel)?;
    let device = w.placement.label();
    core.update_job(id, |j| {
        j.status = "translating".into();
        j.progress = 0.0;
        j.device = Some(device);
    });
    let _ = core.app.emit("translation-started", serde_json::json!({ "id": transcript, "language": language }));

    let t_start = Instant::now();
    let mut pending: Vec<(usize, String, String)> = Vec::new();
    let mut last_saved = Instant::now();
    let mut failure = None;
    for (n, &i) in todo.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let source = t.segments[i].text.clone();
        let text = match w.translate(prompt, &source, &t.language, &language) {
            Ok(text) => text,
            Err(e) => {
                failure = Some(e);
                break;
            }
        };
        let _ = core.app.emit("translated", serde_json::json!({ "id": transcript, "language": language, "index": i, "text": text, "from": source }));
        pending.push((i, source, text));
        let done = n + 1;
        let spent = t_start.elapsed().as_secs_f64();
        let eta = Some(spent / done as f64 * (todo.len() - done) as f64);
        core.update_job(id, |j| {
            j.progress = done as f64 / todo.len() as f64;
            j.eta_s = eta;
            j.elapsed_s = started.elapsed().as_secs_f64();
        });
        if last_saved.elapsed() > TRANSLATION_SAVE {
            keep_translated(&core.paths.history_dir(), &transcript, &language, variant.label, &mut pending)?;
            last_saved = Instant::now();
        }
    }
    // what is done is kept, whatever stopped it
    keep_translated(&core.paths.history_dir(), &transcript, &language, variant.label, &mut pending)?;
    let _ = core.app.emit("history-changed", &transcript);
    if let Some(e) = failure {
        return Err(e);
    }
    let cancelled = cancel.load(Ordering::SeqCst);
    // the step after: the video, with the translation on and the transcript a click away
    let mut next_error = None;
    if job.steps.video && !cancelled {
        let t = history::load(&core.paths.history_dir(), &transcript)?;
        if crate::video::is_video(Path::new(&t.source_path)) {
            match subtitle_video(core, id, &t, &crate::transcript::View::Both(language.clone()), job.steps.burn) {
                Ok(file) => core.update_job(id, |j| j.outputs.push(file.display().to_string())),
                // stopped while it burned the subtitles in: that is not a failure
                Err(_) if cancel.load(Ordering::SeqCst) => {}
                Err(e) => next_error = Some(format!("{e:#}")),
            }
        }
    }
    let cancelled = cancel.load(Ordering::SeqCst);
    core.update_job(id, |j| {
        j.status = if next_error.is_some() { "failed".into() } else if cancelled { "cancelled".into() } else { "done".into() };
        j.eta_s = None;
        j.elapsed_s = started.elapsed().as_secs_f64();
        j.saved = true;
        j.error = next_error;
        if !cancelled {
            j.progress = 1.0;
        }
    });
    Ok(())
}

/// A line's times, from the shortened recording back to the original (see `audio::original_time`).
fn to_original(mut s: Segment, spans: &[(usize, usize)]) -> Segment {
    s.start = audio::original_time(spans, s.start, false);
    s.end = audio::original_time(spans, s.end, true);
    let last = s.voice.len().saturating_sub(1);
    for (k, v) in s.voice.iter_mut().enumerate() {
        *v = audio::original_time(spans, *v as f64, k == last) as f32;
    }
    for p in s.pauses.iter_mut() {
        p[1] = audio::original_time(spans, p[1] as f64, true) as f32;
        p[2] = audio::original_time(spans, p[2] as f64, false) as f32;
    }
    s
}

/// The video a transcript was made from, with its subtitles added as tracks, or burned into the
/// picture with `burn`, next to it.
fn subtitle_video(core: &Arc<Core>, id: &str, t: &Transcript, view: &crate::transcript::View, burn: bool) -> Result<PathBuf> {
    let tracks = crate::video::tracks(t, view).map_err(|e| anyhow!(e))?;
    let source = PathBuf::from(&t.source_path);
    core.update_job(id, |j| j.status = "saving_video".into());
    if burn {
        let ffmpeg = setup::burner(&core.paths).ok_or_else(|| anyhow!(crate::tr!("Burning subtitles in needs the video ffmpeg. Download it in Settings.")))?;
        let srt = String::from_utf8(crate::transcript::export_view(t, "srt", false, view)?)?;
        let dest = audio::unique_path(&crate::video::burned_dest(&source, &tracks));
        let cancel = core.cancels.lock().unwrap().get(id).cloned().unwrap_or_default();
        let started = Instant::now();
        core.update_job(id, |j| j.progress = 0.0);
        crate::video::burn_subtitles(&ffmpeg, &source, &srt, &dest, &core.paths.cache_dir(), &cancel, &mut |f| {
            core.update_job(id, |j| {
                let spent = started.elapsed().as_secs_f64();
                j.progress = f;
                j.eta_s = (f > 0.02).then(|| spent / f * (1.0 - f));
            });
        })?;
        return Ok(dest);
    }
    let ffmpeg = setup::ffmpeg(&core.paths).ok_or_else(|| anyhow!(crate::tr!("Adding subtitles to a video needs ffmpeg. Download it in Settings.")))?;
    let dest = audio::unique_path(&crate::video::default_dest(&source, &tracks, crate::video::output_ext(&ffmpeg, &source)));
    crate::video::add_subtitles(&ffmpeg, &source, &tracks, &dest, &core.paths.cache_dir())?;
    Ok(dest)
}

fn run_job(core: &Arc<Core>, engine: &mut Option<EngineProcess>, id: &str) -> Result<()> {
    let Some(job) = core.snapshot().queue.into_iter().find(|j| j.id == id) else { return Ok(()) };
    if job.status != "queued" {
        return Ok(());
    }
    let steps = job.steps.clone();
    let cancel = core.cancels.lock().unwrap().get(id).cloned().unwrap_or_default();
    let started = Instant::now();
    let _awake = KeepAwake::new();
    let source = PathBuf::from(&job.path);
    core.update_job(id, |j| j.status = "decoding".into());

    let mut model_in_use = None;
    if steps.transcribe {
        let found = setup::model_for(&core.paths, &core.settings.lock().unwrap(), &job.language);
        let Some((family, model)) = found else {
            return Err(anyhow!(crate::tr!("There is no model for this language yet. Choose one in Settings.")));
        };
        let w = ensure_engine(core, engine, Some(&model.path))?;
        w.start_loading()?; // the model loads while the audio decodes
        model_in_use = Some((family, model));
    }

    let ffmpeg = setup::ffmpeg(&core.paths);
    let samples = audio::decode(&source, ffmpeg.as_ref(), &cancel)?;
    let duration_s = samples.len() as f64 / SR as f64;
    core.update_job(id, |j| j.duration_s = duration_s);

    // the long silences out: from the sound saved, and from what the model has to read
    let spans = if steps.remove_silence {
        let spans = audio::speech_spans(&samples, 0.7, 0.3);
        if spans.is_empty() {
            return Err(anyhow!(crate::tr!("There is no speech in this recording.")));
        }
        Some(spans)
    } else {
        None
    };
    // a retry (on the CPU, after the GPU failed) does not save the sound twice
    if (steps.save_audio || (steps.remove_silence && !steps.transcribe)) && job.outputs.is_empty() {
        core.update_job(id, |j| j.status = "saving_audio".into());
        let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| job.title.clone());
        let name = if spans.is_some() { format!("{stem} ({}).m4a", crate::tr!("without silences")) } else { format!("{stem}.m4a") };
        let dest = source.with_file_name(name);
        let saved = audio::save_sound(&source, &dest, spans.as_deref(), &samples, ffmpeg.as_ref(), &cancel)?;
        core.update_job(id, |j| j.outputs.push(saved.display().to_string()));
    }
    let Some((family, model)) = model_in_use else {
        core.update_job(id, |j| {
            j.status = if cancel.load(Ordering::SeqCst) { "cancelled".into() } else { "done".into() };
            j.progress = 1.0;
            j.elapsed_s = started.elapsed().as_secs_f64();
        });
        return Ok(());
    };

    let heard = match &spans {
        Some(s) => audio::join_spans(&samples, s),
        None => samples,
    };
    let total = heard.len();
    let heard_s = total as f64 / SR as f64;
    let chunks = audio::split_points(&heard, MAX_CHUNK_S, SEARCH_S);
    let audio = Audio { id: id.to_string(), samples: heard };
    core.update_job(id, |j| j.status = "loading".into());
    let w = ensure_engine(core, engine, Some(&model.path))?;
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
        model: models::variant(&model.variant).map_or(family.label(), |v| v.label).to_string(),
        partial: false,
        segments: Vec::new(),
        translations: Vec::new(),
        minutes: None,
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
                for s in lines {
                    // the times of the original recording, whatever the model was given
                    let s = match &spans {
                        Some(sp) => to_original(s, sp),
                        None => s,
                    };
                    let _ = core.app.emit("segment", serde_json::json!({ "id": id, "segment": &s }));
                    t.segments.push(s.clone());
                    if let Some(lt) = core.live.lock().unwrap().get_mut(id) {
                        lt.segments.push(s);
                    }
                }
                if failure.is_some() {
                    break;
                }
                i += take.len();
                let done = chunks[i - 1].1;
                let done_s = done as f64 / SR as f64;
                let spent = t_start.elapsed().as_secs_f64();
                let eta = if done_s > 0.0 { Some(spent / done_s * (heard_s - done_s)) } else { None };
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
    // nothing the model could read: said on the job, which would otherwise end without a transcript
    if !saved && !cancelled {
        return Err(anyhow!(crate::tr!("There is no speech in this recording.")));
    }
    if job.folder && saved && !cancelled {
        let formats = core.settings.lock().unwrap().folder_formats();
        if let Err(e) = folder::write_outputs(&t, &formats) {
            core.update_job(id, |j| j.saved = saved);
            return Err(anyhow!(crate::tr!("Transcribed, but the files could not be saved next to the recording: {}", e)));
        }
    }
    // the next steps: a translation (which then does the video), or straight to the video
    let mut next_error = None;
    if saved && !cancelled {
        if let Some(language) = steps.translate.as_deref().filter(|l| *l != t.language) {
            if let Err(e) = core.translate_then(id, language, steps.video, steps.burn) {
                next_error = Some(format!("{e:#}"));
            }
        } else if steps.video && crate::video::is_video(&source) {
            match subtitle_video(core, id, &t, &crate::transcript::View::Original, steps.burn) {
                Ok(file) => core.update_job(id, |j| j.outputs.push(file.display().to_string())),
                Err(_) if cancel.load(Ordering::SeqCst) => {}
                Err(e) => next_error = Some(format!("{e:#}")),
            }
        }
        // the minutes, after the translation if there is one: the queue runs them in turn
        if steps.minutes && !cancel.load(Ordering::SeqCst) {
            if let Err(e) = core.write_minutes(id, None, job.folder) {
                next_error = next_error.or(Some(format!("{e:#}")));
            }
        }
    }
    let cancelled = cancel.load(Ordering::SeqCst);
    core.update_job(id, |j| {
        j.status = if next_error.is_some() { "failed".into() } else if cancelled { "cancelled".into() } else { "done".into() };
        j.error = next_error;
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
        pauses: audio::pauses(&audio.samples, a, b),
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

    fn line(text: &str) -> Segment {
        Segment { start: 0.0, end: 1.0, text: text.into(), flag: None, edited: false, voice: vec![], words: vec![], pauses: vec![] }
    }

    fn saved_transcript(name: &str, lines: &[&str]) -> (PathBuf, Transcript) {
        let dir = std::env::temp_dir().join(format!("dawwin-tr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let t = Transcript {
            id: "t1".into(),
            title: "اجتماع".into(),
            source_path: String::new(),
            created_ms: 0,
            duration_s: 3.0,
            language: "ar".into(),
            elapsed_s: 0.0,
            device: String::new(),
            model: String::new(),
            partial: false,
            segments: lines.iter().map(|l| line(l)).collect(),
            translations: vec![],
            minutes: None,
        };
        history::save(&dir, &t).unwrap();
        (dir, t)
    }

    #[test]
    fn a_translation_does_only_the_lines_it_has_not_done() {
        let (dir, t) = saved_transcript("todo", &["أول سطر", "  ", "ثالث سطر"]);
        assert_eq!(lines_to_translate(&t, "en"), [0, 2], "empty lines are not translated");
        let mut lines = vec![(0, "أول سطر".to_string(), "First line".to_string())];
        keep_translated(&dir, "t1", "en", "MiLMMT-46 1B · Q4", &mut lines).unwrap();
        assert!(lines.is_empty());
        let t = history::load(&dir, "t1").unwrap();
        let tr = t.translation("en").unwrap();
        assert_eq!(tr.lines.len(), 3, "one per line of the transcript");
        assert_eq!(tr.text(0), "First line");
        assert_eq!(lines_to_translate(&t, "en"), [2], "stopped half way: it carries on");
        assert_eq!(lines_to_translate(&t, "fr"), [0, 2], "each language on its own");
    }

    #[test]
    fn a_line_edited_while_it_was_translated_waits_for_another_turn() {
        let (dir, _) = saved_transcript("edited", &["أول سطر"]);
        history::modify(&dir, "t1", |t| t.segments[0].text = "أول سطر معدل".into()).unwrap();
        let mut lines = vec![(0, "أول سطر".to_string(), "First line".to_string())];
        keep_translated(&dir, "t1", "en", "m", &mut lines).unwrap();
        let t = history::load(&dir, "t1").unwrap();
        assert_eq!(t.translation("en").unwrap().text(0), "");
        assert_eq!(lines_to_translate(&t, "en"), [0]);
        // and a line edited after its translation is translated again
        let mut lines = vec![(0, "أول سطر معدل".to_string(), "First line, edited".to_string())];
        keep_translated(&dir, "t1", "en", "m", &mut lines).unwrap();
        let (t, _) = history::modify(&dir, "t1", |t| t.segments[0].text = "سطر آخر".into()).unwrap();
        assert_eq!(lines_to_translate(&t, "en"), [0]);
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
