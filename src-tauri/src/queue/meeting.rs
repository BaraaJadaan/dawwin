//! Recording a meeting, on the queue's side. The recorder (`crate::record`) runs on threads of its
//! own from Record to Stop and only records: nothing is transcribed meanwhile. Once Stop has made
//! the file whole, the recording waits beside its journal for what is to be done with it (the
//! choice the window offers): transcribed from the file, "me" and "them" apart, in time order
//! ("them" first on a tie), then the steps chosen; or only kept, as a transcript without text that
//! can be transcribed later; or deleted. A recording interrupted by a crash is found at the next
//! start by its journal and offered the same way.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::{detect_language, ensure_engine, find_turns, fix_loops, give_up_on_gpu, now_ms, show_loaded, Core, JobView, Msg, Steps};
use crate::audio;
use crate::engine::{Audio, Engine, EngineFailed, EngineProcess, OutOfMemory};
use crate::history;
use crate::models;
use crate::record::journal::{self, Journal};
use crate::record::{self, cutter::Cutter, echo, source, Piece, Recorder, Sources, Status, Track, SR};
use crate::setup::{self, Paths};
use crate::language;
use crate::models::Family;
use crate::speakers;
use crate::transcript::{Segment, Speaker, Transcript};

/// Where recordings are kept: the folder of one's own chosen in Settings ("Where files go"), like
/// every file Dawwin makes; without one, Documents\Dawwin, or, when Documents is in OneDrive (Windows
/// 11 often puts it there), Dawwin in the user's own folder, which OneDrive does not take: a
/// recording is not to leave the computer by itself. A chosen folder inside OneDrive is the user's
/// choice, and is said where Record is (`synced`).
pub fn recordings_dir(paths: &Paths, settings: &crate::setup::Settings) -> PathBuf {
    crate::saving::own_folder(settings).unwrap_or_else(|| default_dir(paths))
}

/// The folder recordings go to when none is chosen.
fn default_dir(paths: &Paths) -> PathBuf {
    if std::env::var_os("DAWWIN_HOME").is_some_and(|h| !h.is_empty()) {
        // everything in one folder, also for trying things out
        return paths.roaming.join("recordings");
    }
    let roots = onedrive_roots();
    match dirs::document_dir() {
        Some(d) if !synced(&d, &roots) => d.join("Dawwin"),
        _ => dirs::home_dir().unwrap_or_else(|| paths.roaming.clone()).join("Dawwin"),
    }
}

impl Core {
    /// Where this recording goes now.
    pub fn recordings_dir(&self) -> PathBuf {
        recordings_dir(&self.paths, &self.settings.lock().unwrap())
    }
}

/// The folders OneDrive keeps in step with the internet, as it tells programs.
fn onedrive_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(|v| std::env::var_os(v))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .collect();
    if cfg!(debug_assertions) {
        // for trying the warning on a computer without OneDrive
        if let Some(v) = std::env::var_os("DAWWIN_TEST_ONEDRIVE") {
            roots.push(PathBuf::from(v));
        }
    }
    roots
}

/// Whether `dir` is inside a folder OneDrive syncs: under one of its roots, or in a folder named
/// like one ("OneDrive", "OneDrive - Company").
pub fn synced(dir: &Path, roots: &[PathBuf]) -> bool {
    let low = |p: &Path| p.to_string_lossy().to_lowercase().replace('/', "\\");
    let d = low(dir);
    roots.iter().any(|r| {
        let r = low(r);
        let r = r.trim_end_matches('\\');
        !r.is_empty() && (d == r || d.starts_with(&format!("{r}\\")))
    }) || dir.components().any(|c| {
        let c = c.as_os_str().to_string_lossy().to_lowercase();
        c == "onedrive" || c.starts_with("onedrive - ")
    })
}

/// Whether recordings would go to a folder OneDrive uploads.
pub fn recordings_synced(core: &Core) -> bool {
    synced(&core.recordings_dir(), &onedrive_roots())
}

/// How recording is set up, in a file of its own beside the settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecordSettings {
    #[serde(default)]
    pub sources: Sources,
    /// The microphone chosen; none follows the Windows default
    #[serde(default)]
    pub mic: Option<String>,
    /// The notice about consent was shown
    #[serde(default)]
    pub notice_shown: bool,
}

fn settings_file(paths: &Paths) -> PathBuf {
    paths.roaming.join("recording.json")
}

pub fn load_settings(paths: &Paths) -> RecordSettings {
    std::fs::read(settings_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_settings(paths: &Paths, s: &RecordSettings) -> Result<()> {
    let tmp = settings_file(paths).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(&tmp, settings_file(paths))?;
    Ok(())
}

/// A recording: being made, or being transcribed or kept once it was stopped.
struct Session {
    id: String,
    title: String,
    /// The two tracks the engine hears: the recorder's WAV, or (a recording kept without its text)
    /// the tracks it kept, which ffmpeg reads
    wav: PathBuf,
    /// The sound kept for listening, when the source gives it
    hifi: Option<PathBuf>,
    /// None for a recording kept without its text: its transcript says what it is
    journal_path: Option<PathBuf>,
    journal: Mutex<Journal>,
    recorder: Mutex<Option<Recorder>>,
    /// The last word from the recorder
    status: Arc<Mutex<Status>>,
    /// A recording kept without its text, transcribed now: its files stay where they are
    saved: Option<Transcript>,
}

impl Session {
    fn save_journal(&self) {
        if let Some(p) = &self.journal_path {
            let _ = self.journal.lock().unwrap().save(p);
        }
    }
}

static SESSIONS: Mutex<Vec<Arc<Session>>> = Mutex::new(Vec::new());

fn session(id: &str) -> Option<Arc<Session>> {
    SESSIONS.lock().unwrap().iter().find(|s| s.id == id).cloned()
}

/// The recording being made now, if there is one: one that records, not one that was stopped and
/// is still writing its last second or its text (a new one may start meanwhile).
fn recording_now() -> Option<Arc<Session>> {
    SESSIONS.lock().unwrap().iter().find(|s| s.recorder.lock().unwrap().as_ref().is_some_and(|r| !r.stopping())).cloned()
}

pub fn is_recording() -> bool {
    recording_now().is_some()
}

/// Stop the recording `id` (the job's stop button): true when it was recording.
/// Stop the recording `id` if it is recording: true when it was. Once stopped, cancelling it is
/// stopping the writing of its text, even while it writes its last second.
pub(super) fn stop_if_recording(id: &str) -> bool {
    if let Some(s) = session(id) {
        if let Some(r) = s.recorder.lock().unwrap().as_ref().filter(|r| !r.stopping()) {
            r.stop();
            return true;
        }
    }
    false
}

#[derive(Clone, Debug, Serialize)]
pub struct RecordingView {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub sources: Sources,
}

fn view(s: &Session) -> RecordingView {
    RecordingView { id: s.id.clone(), title: s.title.clone(), status: s.status.lock().unwrap().clone(), sources: s.journal.lock().unwrap().sources }
}

/// Where the sound comes from: Windows, or made-up tracks in debug builds
/// (`DAWWIN_TEST_RECORD="me=<file>|them=<file>"`, files any decoder of Dawwin's reads; `|silent`
/// makes the default microphone send digital silence, as a muted one does).
fn sources(core: &Core) -> Result<Box<dyn source::Factory>> {
    #[cfg(debug_assertions)]
    if let Ok(spec) = std::env::var("DAWWIN_TEST_RECORD") {
        let ff = setup::ffmpeg(&core.paths);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut files = source::Files::new(None, None, false);
        for part in spec.split('|') {
            match part.split_once('=') {
                Some(("me", p)) => files.me = Some(Arc::new(audio::decode(Path::new(p), ff.as_ref(), &stop)?)),
                Some(("them", p)) => files.them = Some(Arc::new(audio::decode(Path::new(p), ff.as_ref(), &stop)?)),
                _ if part == "silent" => files.silent_default = true,
                _ => {}
            }
        }
        return Ok(Box::new(files));
    }
    let _ = core;
    #[cfg(windows)]
    return Ok(Box::new(record::windows::Wasapi));
    #[cfg(not(windows))]
    Err(anyhow!("recording needs Windows"))
}

#[derive(Clone, Debug, Serialize)]
pub struct Microphone {
    pub id: String,
    pub name: String,
    pub default: bool,
}

pub fn microphones() -> Vec<Microphone> {
    #[cfg(debug_assertions)]
    if std::env::var_os("DAWWIN_TEST_RECORD").is_some() {
        return vec![
            Microphone { id: "test-default".into(), name: "Test microphone".into(), default: true },
            Microphone { id: "test-other".into(), name: "Other test microphone".into(), default: false },
        ];
    }
    #[cfg(windows)]
    return record::windows::microphones().into_iter().map(|m| Microphone { id: m.id, name: m.name, default: m.default }).collect();
    #[cfg(not(windows))]
    Vec::new()
}

/// A recording beside its journal: stopped and waiting for what is to be done with it, or cut off
/// by a crash.
#[derive(Clone, Debug, Serialize)]
pub struct Interrupted {
    pub journal: String,
    pub id: String,
    pub title: String,
    pub started_ms: u64,
    pub seconds: f64,
    /// Stopped by Stop (or by itself): its file is whole, nothing was cut off
    pub stopped: bool,
    pub sources: Sources,
}

impl Core {
    /// Start recording: the job's id. It only records, so no model is needed, only the disk room;
    /// `language` and `steps` are kept in its journal until something else is chosen after Stop.
    pub fn start_recording(self: &Arc<Self>, language: String, steps: Steps) -> Result<String> {
        if is_recording() {
            return Err(anyhow!(crate::tr!("A recording is already running.")));
        }
        let dir = self.recordings_dir();
        std::fs::create_dir_all(&dir).map_err(|e| anyhow!(crate::tr!("Cannot make the folder for recordings, {}: {}", dir.display(), e)))?;
        if record::free_space(&dir).is_some_and(|f| f < record::DISK_STOP) {
            return Err(anyhow!(crate::tr!("The disk of {} is nearly full. Free some space to record.", dir.display())));
        }
        let rs = load_settings(&self.paths);
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let id = format!("{}-{:03}", now_ms(), n % 1000);
        let stamp = local_stamp();
        let title = crate::tr!("Recording {}", stamp);
        let wav = audio::unique_path(&dir.join(format!("{}.wav", title.replace(':', "-"))));
        let stem = wav.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let hifi = wav.with_file_name(format!("{stem}.listen.wav"));
        let journal_path = Journal::path_for(&wav);
        let journal = Journal {
            version: 1,
            id: id.clone(),
            title: title.clone(),
            started_ms: now_ms(),
            wav: wav.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            rate: SR as u32,
            channels: 2,
            left: "me".into(),
            right: "them".into(),
            sources: rs.sources,
            language: language.clone(),
            steps: steps.clone(),
            mic: rs.mic.clone(),
            state: "recording".into(),
            hifi: hifi.file_name().map(|n| n.to_string_lossy().into_owned()),
        };
        journal.save(&journal_path)?;
        let status = Arc::new(Mutex::new(Status::default()));
        let (st, app, rec_id) = (status.clone(), self.app.clone(), id.clone());
        let cfg = record::Config {
            sources: rs.sources,
            mic: rs.mic.clone(),
            wav: wav.clone(),
            hifi: Some(hifi.clone()),
            on_status: Box::new(move |s| {
                *st.lock().unwrap() = s.clone();
                let _ = app.emit("recording", serde_json::json!({ "id": rec_id, "status": s }));
                // it stopped by itself (the disk, an error): wrapped up as after Stop, on a thread
                // of its own, since this one is the recorder's, which is waited for
                if s.ended {
                    wrap_up_later(&app, &rec_id);
                }
            }),
        };
        let recorder = match Recorder::start(cfg, sources(self)?) {
            Ok(x) => x,
            Err(e) => {
                let _ = std::fs::remove_file(&journal_path);
                return Err(e);
            }
        };
        let hifi = hifi.is_file().then_some(hifi);
        let s = Arc::new(Session {
            id: id.clone(),
            title: title.clone(),
            wav: wav.clone(),
            hifi,
            journal_path: Some(journal_path),
            journal: Mutex::new(journal),
            recorder: Mutex::new(Some(recorder)),
            status,
            saved: None,
        });
        SESSIONS.lock().unwrap().push(s);
        let job = JobView {
            id: id.clone(),
            title,
            path: wav.display().to_string(),
            language,
            steps,
            kind: "record".into(),
            status: "recording".into(),
            ..Default::default()
        };
        self.update(|s| s.queue.push(job));
        Ok(id)
    }

    /// Stop the recording `id` (the one on screen), else the one recording now: its job leaves the
    /// queue, and once its file is whole the window is told, to ask what to do with it. A job
    /// still shown as recording with nothing recording under it says so, rather than leave a Stop
    /// that does nothing.
    pub fn stop_recording(&self, id: Option<&str>) {
        let Some(id) = id.map(str::to_string).or_else(|| recording_now().map(|s| s.id.clone())) else { return };
        if stop_if_recording(&id) || session(&id).is_some_and(|s| s.recorder.lock().unwrap().is_some()) {
            self.recording_stopped(&id);
            wrap_up_later(&self.app, &id);
        } else {
            self.update_job(&id, |j| {
                if j.status == "recording" {
                    j.status = "failed".into();
                    j.error = Some(crate::tr!("This recording had already stopped."));
                }
            });
        }
    }

    /// Stop the recording and wait until its file is whole (for quitting).
    pub fn stop_recording_and_wait(&self) {
        // one still writing its last second after Stop is waited for too
        let holding = SESSIONS.lock().unwrap().iter().find(|s| s.recorder.lock().unwrap().is_some()).cloned();
        if let Some(s) = holding {
            let r = s.recorder.lock().unwrap().take();
            if let Some(r) = r {
                r.stop();
                let _ = r.wait();
            }
            s.journal.lock().unwrap().state = "stopped".into();
            s.save_journal();
        }
    }

    /// The recording being made, if one is.
    pub fn recording(&self) -> Option<RecordingView> {
        recording_now().map(|s| view(&s))
    }

    pub fn record_settings(&self) -> RecordSettings {
        load_settings(&self.paths)
    }

    pub fn change_record_settings(&self, f: impl FnOnce(&mut RecordSettings)) -> Result<RecordSettings> {
        let mut s = load_settings(&self.paths);
        f(&mut s);
        save_settings(&self.paths, &s)?;
        Ok(s)
    }

    /// Record from this microphone (none: the Windows default), from now on, also in this recording.
    pub fn choose_microphone(&self, id: Option<String>) -> Result<RecordSettings> {
        let s = self.change_record_settings(|s| s.mic = id.clone())?;
        if let Some(r) = recording_now() {
            if let Some(rec) = r.recorder.lock().unwrap().as_ref() {
                rec.switch_mic(id.clone());
            }
            r.journal.lock().unwrap().mic = id;
        }
        Ok(s)
    }

    /// The recordings beside their journals: stopped and waiting for a choice, or cut off by a
    /// crash, the power or a forced close.
    pub fn interrupted_recordings(&self) -> Vec<Interrupted> {
        let busy: Vec<String> = SESSIONS.lock().unwrap().iter().map(|s| s.id.clone()).collect();
        let mut dirs = vec![self.recordings_dir(), default_dir(&self.paths)];
        dirs.dedup();
        dirs.iter()
            .flat_map(|d| journal::interrupted(d))
            .filter(|(_, j)| !busy.contains(&j.id))
            .filter_map(|(p, j)| {
                let wav = j.wav_path(&p)?;
                let c = record::wav::check(&wav).ok();
                Some(Interrupted {
                    journal: p.display().to_string(),
                    id: j.id.clone(),
                    title: j.title.clone(),
                    started_ms: j.started_ms,
                    seconds: c.map_or(0.0, |c| c.file_frames as f64 / SR as f64),
                    stopped: j.state == "stopped",
                    sources: j.sources,
                })
            })
            .collect()
    }

    /// A recording beside its journal: transcribed with its own language and steps (`keep`), the
    /// job's id; or deleted.
    pub fn recover_recording(self: &Arc<Self>, journal_file: &str, keep: bool) -> Result<Option<String>> {
        let path = self.journal_of(journal_file)?;
        if keep {
            let j = Journal::load(&path)?;
            return self.finish_recording(journal_file, j.language.clone(), Some(j.steps.clone())).map(Some);
        }
        let j = Journal::load(&path)?;
        if session(&j.id).is_some() {
            return Err(anyhow!(crate::tr!("This recording is being worked on.")));
        }
        let wav = j.wav_path(&path).ok_or_else(|| anyhow!("not a recording of Dawwin's"))?;
        let _ = std::fs::remove_file(&wav);
        if let Some(h) = j.hifi_path(&path).filter(|h| h.is_file()) {
            let _ = std::fs::remove_file(h);
        }
        std::fs::remove_file(&path)?;
        // what was written of its text before
        let _ = history::delete(&self.paths.history_dir(), &j.id);
        let _ = self.app.emit("history-changed", &j.id);
        self.update(|s| s.queue.retain(|x| x.id != j.id));
        Ok(None)
    }

    /// A journal of Dawwin's in a recordings folder, never another file.
    fn journal_of(&self, journal_file: &str) -> Result<PathBuf> {
        let path = PathBuf::from(journal_file);
        let dirs = [self.recordings_dir(), default_dir(&self.paths)];
        if !path.parent().is_some_and(|p| dirs.iter().any(|d| d == p)) || !path.to_string_lossy().ends_with(journal::SUFFIX) {
            return Err(anyhow!("not a recording of Dawwin's"));
        }
        Ok(path)
    }

    /// What was chosen for a recording beside its journal: transcribed in `language` with `steps`,
    /// or (`steps` None) only kept, as a transcript without text. The job's id.
    pub fn finish_recording(self: &Arc<Self>, journal_file: &str, language: String, steps: Option<Steps>) -> Result<String> {
        let path = self.journal_of(journal_file)?;
        let mut j = Journal::load(&path)?;
        if session(&j.id).is_some() {
            return Err(anyhow!(crate::tr!("This recording is being worked on.")));
        }
        let transcribe = steps.as_ref().is_some_and(|s| s.transcribe);
        if transcribe {
            self.can_transcribe(&language)?;
        }
        let wav = j.wav_path(&path).ok_or_else(|| anyhow!("not a recording of Dawwin's"))?;
        // only a file laid out the way the recorder writes it is repaired
        record::wav::check(&wav)?;
        record::wav::repair(&wav)?;
        // the sound to listen to, as much of it as was written; without it, the tracks are kept
        let hifi = j.hifi_path(&path).filter(|h| h.is_file());
        let hifi = hifi.filter(|h| record::wav::check(h).is_ok() && record::wav::repair_at(h, record::resample::HIFI as u32).is_ok());
        j.state = "stopped".into();
        if transcribe {
            j.language = language;
        }
        j.steps = steps.unwrap_or(Steps { transcribe: false, ..Default::default() });
        j.save(&path)?;
        let id = j.id.clone();
        let s = Arc::new(Session {
            id: id.clone(),
            title: j.title.clone(),
            wav: wav.clone(),
            hifi,
            journal_path: Some(path),
            journal: Mutex::new(j.clone()),
            recorder: Mutex::new(None),
            status: Default::default(),
            saved: None,
        });
        self.queue_session(s, &j, wav.display().to_string());
        Ok(id)
    }

    /// A recording kept without its text (`id`, its transcript), transcribed now in `language`
    /// with `steps`, from the tracks it kept. The job's id.
    pub fn transcribe_saved(self: &Arc<Self>, id: &str, language: String, steps: Steps) -> Result<String> {
        let t = history::load(&self.paths.history_dir(), id)?;
        let Some(sources) = t.sources else { return Err(anyhow!(crate::tr!("This transcript is not a recording kept without its text."))) };
        if session(id).is_some() || self.is_live(id) {
            return Err(anyhow!(crate::tr!("This recording is being worked on.")));
        }
        let tracks = tracks_of(&t);
        if !tracks.is_file() {
            return Err(anyhow!(crate::tr!("The recording is not at {} any more.", tracks.display())));
        }
        self.can_transcribe(&language)?;
        let steps = Steps { transcribe: true, ..steps };
        let j = Journal {
            version: 1,
            id: t.id.clone(),
            title: t.title.clone(),
            started_ms: t.created_ms,
            wav: String::new(),
            rate: SR as u32,
            channels: 2,
            left: "me".into(),
            right: "them".into(),
            sources,
            language,
            steps,
            mic: None,
            state: "stopped".into(),
            hifi: None,
        };
        let s = Arc::new(Session {
            id: t.id.clone(),
            title: t.title.clone(),
            wav: tracks,
            hifi: None,
            journal_path: None,
            journal: Mutex::new(j.clone()),
            recorder: Mutex::new(None),
            status: Default::default(),
            saved: Some(t.clone()),
        });
        self.queue_session(s, &j, t.source_path.clone());
        Ok(t.id)
    }

    /// The model for `language` is there (the language model, to detect it).
    fn can_transcribe(&self, language: &str) -> Result<()> {
        if language == language::AUTO {
            if !setup::model_status(&self.paths, &self.settings.lock().unwrap(), Family::Language).found {
                return Err(anyhow!(crate::tr!("Detecting the language needs its model. Download it in Settings.")));
            }
        } else if setup::model_for(&self.paths, &self.settings.lock().unwrap(), language).is_none() {
            return Err(anyhow!(crate::tr!("There is no model for this language yet. Choose one in Settings.")));
        }
        Ok(())
    }

    /// A stopped recording's job, in the queue: transcribed by the engine's worker, or only kept,
    /// which needs no engine, on a thread of its own.
    fn queue_session(self: &Arc<Self>, s: Arc<Session>, j: &Journal, path: String) {
        let id = s.id.clone();
        SESSIONS.lock().unwrap().push(s.clone());
        self.cancels.lock().unwrap().insert(id.clone(), Default::default());
        self.update(|st| {
            st.queue.retain(|x| x.id != id);
            st.queue.push(JobView {
                id: id.clone(),
                title: j.title.clone(),
                path,
                language: j.language.clone(),
                steps: j.steps.clone(),
                kind: "record".into(),
                status: "queued".into(),
                ..Default::default()
            })
        });
        if j.steps.transcribe {
            self.send(Msg::Record(id));
        } else {
            let core = self.clone();
            std::thread::spawn(move || {
                let _awake = setup::KeepAwake::new();
                let result = only_keep(&core, &s);
                SESSIONS.lock().unwrap().retain(|x| x.id != s.id);
                core.cancels.lock().unwrap().remove(&s.id);
                if let Err(err) = result {
                    core.update_job(&s.id, |x| {
                        x.status = "failed".into();
                        x.error = Some(format!("{err:#}"));
                    });
                }
            });
        }
    }
}

/// `wrap_up`, on a thread of its own: it waits for the recorder.
pub(super) fn wrap_up_later(app: &AppHandle, id: &str) {
    let (core, id) = (core_of(app), id.to_string());
    std::thread::spawn(move || wrap_up(&core, &id));
}

/// A recording stopped: once the recorder has made its file whole, its journal says so, its job
/// leaves the queue, and the window is told, to ask what to do with it. Called by Stop and by the
/// recorder when it stops by itself; whichever comes first does it.
fn wrap_up(core: &Arc<Core>, id: &str) {
    let Some(sess) = session(id) else { return };
    // taken first, then waited for: never with the lock held, which Stop and the levels need
    let r = sess.recorder.lock().unwrap().take();
    let Some(r) = r else { return };
    let _ = r.wait();
    sess.journal.lock().unwrap().state = "stopped".into();
    sess.save_journal();
    SESSIONS.lock().unwrap().retain(|s| s.id != sess.id);
    core.update(|s| s.queue.retain(|x| x.id != sess.id));
    let journal = sess.journal_path.as_ref().map(|p| p.display().to_string());
    let _ = core.app.emit("recorded", serde_json::json!({ "id": sess.id, "journal": journal }));
}

/// A recording only kept, with no text: the sound to listen to as the recording, the tracks beside
/// a transcript without lines that can be transcribed later, and the steps that need no text.
fn only_keep(core: &Arc<Core>, sess: &Arc<Session>) -> Result<()> {
    let id = sess.id.as_str();
    let started = Instant::now();
    let j = sess.journal.lock().unwrap().clone();
    let total_s = record::wav::check(&sess.wav).map(|c| c.file_frames).unwrap_or(0) as f64 / SR as f64;
    core.update_job(id, |x| {
        x.status = "saving_audio".into();
        x.duration_s = total_s;
    });
    let (kept, tracks) = keep(core, sess, total_s);
    let mut t = Transcript {
        id: id.to_string(),
        title: j.title.clone(),
        source_path: kept.display().to_string(),
        created_ms: j.started_ms,
        duration_s: total_s,
        language: j.language.clone(),
        speakers: meeting_speakers(j.sources),
        tracks: tracks.map(|p| p.display().to_string()),
        outputs: vec![kept.display().to_string()],
        outputs_listed: true,
        sources: Some(j.sources),
        ..Default::default()
    };
    let mut next_error = None;
    if j.steps.remove_silence {
        match without_silences(core, &mut t) {
            Ok(file) => core.update_job(id, |x| x.outputs.push(file.display().to_string())),
            Err(e) => next_error = Some(e),
        }
    }
    history::save(&core.paths.history_dir(), &t)?;
    if let Some(p) = &sess.journal_path {
        let _ = std::fs::remove_file(p);
    }
    let _ = core.app.emit("history-changed", id);
    core.update_job(id, |x| {
        x.status = if next_error.is_some() { "failed".into() } else { "done".into() };
        x.error = next_error;
        x.progress = 1.0;
        x.saved = true;
        x.elapsed_s = started.elapsed().as_secs_f64();
        x.outputs.insert(0, kept.display().to_string());
    });
    Ok(())
}

/// The recording's sound without its long silences, beside it: the file, also among the
/// transcript's outputs.
fn without_silences(core: &Arc<Core>, t: &mut Transcript) -> std::result::Result<PathBuf, String> {
    let source = PathBuf::from(&t.source_path);
    let ffmpeg = setup::ffmpeg(&core.paths);
    let ext = if ffmpeg.is_some() { "m4a" } else { "wav" };
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let dest = audio::unique_path(&source.with_file_name(format!("{stem} ({}).{ext}", crate::tr!("without silences"))));
    let file = crate::save_sound_of(t, true, &dest, ffmpeg)?;
    t.outputs.push(file.display().to_string());
    Ok(file)
}

/// "2026-11-12 14:03" in local time, for a recording's name.
fn local_stamp() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            year: u16,
            month: u16,
            weekday: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            ms: u16,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetLocalTime(t: *mut SystemTime);
        }
        let mut t = SystemTime::default();
        // SAFETY: fills the struct it is given.
        unsafe { GetLocalTime(&mut t) };
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
    }
    #[cfg(not(windows))]
    {
        now_ms().to_string()
    }
}

// ---------------------------------------------------------------- the lines

/// Lines as they come from both tracks. A "them" line is shown at once; a "me" line once "them"
/// has been heard past its end, so that what overlaps it is known, and not at all when it only
/// says what "them" said at the same time.
struct Lines {
    /// Up to where each track is transcribed, in seconds (a track not recorded: all of it)
    done: [f64; 2],
    held: Vec<Segment>,
    /// Every "them" line, to compare the "me" lines with
    them: Vec<(f64, f64, String)>,
}

fn idx(t: Track) -> usize {
    match t {
        Track::Me => 0,
        Track::Them => 1,
    }
}

impl Lines {
    fn new(sources: Sources) -> Self {
        let done = [Track::Me, Track::Them].map(|t| if sources.has(t) { 0.0 } else { f64::INFINITY });
        Lines { done, held: Vec::new(), them: Vec::new() }
    }

    fn add(&mut self, track: Track, upto_s: f64, lines: Vec<Segment>) {
        for l in &lines {
            if track == Track::Them {
                self.them.push((l.start, l.end, l.text.clone()));
            }
        }
        self.held.extend(lines);
        self.done[idx(track)] = self.done[idx(track)].max(upto_s);
    }

    /// The lines that can be shown now (all of them with `all`), in time order.
    fn release(&mut self, all: bool) -> Vec<Segment> {
        let them_done = self.done[idx(Track::Them)];
        let (mut ready, rest): (Vec<Segment>, Vec<Segment>) =
            self.held.drain(..).partition(|s| all || s.track.as_deref() != Some("me") || s.end <= them_done + 1e-6);
        self.held = rest;
        ready.sort_by(|a, b| a.start.total_cmp(&b.start));
        ready.retain(|s| {
            if s.track.as_deref() != Some("me") {
                return true;
            }
            let at_once: Vec<&str> = self.them.iter().filter(|(a, b, _)| *a < s.end + 0.5 && *b > s.start - 0.5).map(|(_, _, t)| t.as_str()).collect();
            !echo::doubled(&s.text, &at_once)
        });
        ready
    }
}

// ---------------------------------------------------------------- the job

/// The speech engine, for one piece. A GPU that fails costs the GPU: the piece is heard again on
/// the CPU. Pieces without speech never reach the model.
#[allow(clippy::too_many_arguments)]
fn hear(core: &Arc<Core>, engine: &mut Option<EngineProcess>, model: &str, id: &str, piece: &Piece, language: &str, gates: &mut [echo::Gate; 2], sources: Sources) -> Result<Vec<Segment>> {
    if !gates[idx(piece.track)].admit(&piece.samples) {
        return Ok(Vec::new());
    }
    let audio = Audio { id: format!("{id}-{}-{}", piece.track.key(), piece.start), samples: piece.samples.clone() };
    let len = audio.samples.len();
    let mut tries = 0;
    let segs = loop {
        let w = ensure_engine(core, engine, Some(model))?;
        let got = w.transcribe(&audio, &[(0, len)], language).and_then(|mut g| g.pop().ok_or_else(|| anyhow!("no text")));
        let r = got.and_then(|g| fix_loops(w, &audio, (0, len), g, language, 0));
        w.forget(&audio.id);
        match r {
            Ok(s) => break s,
            Err(e) if (e.is::<EngineFailed>() || e.is::<OutOfMemory>()) && tries == 0 => {
                tries += 1;
                if !core.gpu_failed.load(Ordering::SeqCst) && core.snapshot().engine.device.as_deref() == Some("gpu") {
                    give_up_on_gpu(core, engine);
                } else {
                    *engine = None;
                }
                let info = ensure_engine(core, engine, Some(model))?.ensure_loaded()?;
                show_loaded(core, &info);
            }
            Err(e) => return Err(e),
        }
    };
    let off = piece.start as f64 / SR as f64;
    Ok(segs
        .into_iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|mut s| {
            s.start += off;
            s.end += off;
            for v in s.voice.iter_mut() {
                *v += off as f32;
            }
            for p in s.pauses.iter_mut() {
                p[1] += off as f32;
                p[2] += off as f32;
            }
            s.track = Some(piece.track.key().into());
            s.speaker = Some(speaker_id(sources, piece.track));
            s.text = crate::words::apply(&s.text, &core.settings.lock().unwrap().words);
            // a piece can begin in a long silence (the others listening): the line starts where the speech does
            if let (Some(first), Some(last)) = (s.voice.first(), s.voice.last()) {
                s.start = s.start.max(*first as f64 - 0.2).min(s.end);
                s.end = s.end.min(*last as f64 + 0.2).max(s.start);
            }
            s
        })
        .collect())
}

/// The language of `samples`, heard by the language model: Ok(Ok(language)) when there is a model
/// for it, Ok(Err(what to say)) when there is none (and its model is offered), Err when the model
/// could not tell (too little speech).
fn heard_language(core: &Arc<Core>, engine: &mut Option<EngineProcess>, id: &str, samples: &[f32]) -> Result<std::result::Result<String, String>> {
    let d = detect_language(core, engine, id, samples)?;
    if d.has_model {
        return Ok(Ok(d.language));
    }
    let offer = models::models_for(&d.language).first().map(|v| v.id);
    let _ = core.app.emit("language-needs-model", serde_json::json!({ "id": id, "language": d.language, "name": d.name, "model": offer }));
    let name = if models::known_language(&d.language) { models::language_name(&d.language).to_string() } else { d.name.clone() };
    Ok(Err(crate::tr!("This recording is in {}, and there is no model for it yet. Download one in Settings.", name)))
}

/// The language heard, kept: on the job, the transcript (as heard) and the journal.
fn settle_language(core: &Arc<Core>, sess: &Session, t: &mut Transcript, language: &str) {
    t.language = language.to_string();
    t.detected = Some(language.to_string());
    if let Some(lt) = core.live.lock().unwrap().get_mut(&sess.id) {
        lt.language = language.to_string();
        lt.detected = Some(language.to_string());
    }
    core.update_job(&sess.id, |x| x.language = language.to_string());
    sess.journal.lock().unwrap().language = language.to_string();
    sess.save_journal();
}

/// The speaker a track is: "Me" first when the microphone is recorded, then "Them".
fn speaker_id(sources: Sources, track: Track) -> u32 {
    match track {
        Track::Them if sources.has(Track::Me) => 2,
        _ => 1,
    }
}

/// "Me" and "Them" as the speakers of a meeting, in the interface's language (names the user can
/// change like any other).
fn meeting_speakers(sources: Sources) -> Vec<Speaker> {
    sources
        .tracks()
        .into_iter()
        .rev()
        .map(|t| Speaker { id: speaker_id(sources, t), name: if t == Track::Me { crate::tr!("Me") } else { crate::tr!("Them") } })
        .collect()
}

/// A transcript of a recorded meeting: its lines come from the two tracks.
pub fn is_meeting(t: &Transcript) -> bool {
    t.segments.iter().any(|s| s.track.is_some())
}

/// The "them" lines told apart by voice, from `turns` the speaker model heard on the PC's track
/// alone; the "me" lines stay "Me". A "them" voice that was given a name keeps it.
pub fn split_them(t: &mut Transcript, turns: &[speakers::Turn]) {
    let me = t.segments.iter().find(|s| s.track.as_deref() == Some("me")).and_then(|s| s.speaker);
    let offset = u32::from(me.is_some());
    let me_speaker = me.and_then(|m| t.speakers.iter().find(|x| x.id == m).cloned());
    let (them, rest): (Vec<Segment>, Vec<Segment>) = t.segments.drain(..).partition(|s| s.track.as_deref() == Some("them"));
    let mut others = Transcript {
        speakers: t
            .speakers
            .iter()
            .filter(|x| Some(x.id) != me)
            // "Them" was only Dawwin's word for all of them: a name the user gave is kept
            .map(|x| Speaker { id: x.id.saturating_sub(offset), name: if x.name == crate::tr!("Them") { String::new() } else { x.name.clone() } })
            .collect(),
        segments: them.into_iter().map(|mut s| { s.speaker = s.speaker.map(|x| x.saturating_sub(offset)); s }).collect(),
        ..Default::default()
    };
    speakers::give_speakers(&mut others, turns);
    let first = offset + 1;
    let count = others.speakers.len();
    for s in others.segments.iter_mut() {
        // one voice, or a line in nobody's turn: "Them" as before
        s.speaker = Some(match s.speaker {
            Some(x) if count > 1 => x + offset,
            _ => first,
        });
    }
    let mut names: Vec<Speaker> = me_speaker.into_iter().collect();
    if count > 1 {
        names.extend(others.speakers.iter().map(|x| Speaker { id: x.id + offset, name: x.name.clone() }));
    } else {
        let name = others.speakers.first().map(|x| x.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| crate::tr!("Them"));
        names.push(Speaker { id: first, name });
    }
    t.speakers = names;
    t.segments = rest;
    t.segments.extend(others.segments);
    t.segments.sort_by(|a, b| a.start.total_cmp(&b.start));
}

/// Where a recorded meeting's two tracks are: kept apart from the recording (1.8.1 on), or in it.
pub fn tracks_of(t: &Transcript) -> PathBuf {
    t.tracks.as_deref().map(PathBuf::from).filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(&t.source_path))
}

/// One track of a recorded meeting, from its file (the WAV, or the M4A through ffmpeg): what the
/// timing pass and the speaker model hear, rather than both sides mixed.
pub fn channel(core: &Core, source: &Path, track: Track) -> Result<Vec<f32>> {
    if let Ok(c) = record::wav::check(source) {
        let (me, them) = record::wav::Reader::open(source, 0)?.next(c.file_frames as usize)?;
        return Ok(if track == Track::Me { me } else { them });
    }
    let ff = setup::ffmpeg(&core.paths).ok_or_else(|| anyhow!(crate::tr!("Reading this recording needs ffmpeg. Download it in Settings.")))?;
    // both channels as they are (no filter: Dawwin's small ffmpeg has few), then the one wanted
    let mut cmd = std::process::Command::new(&ff);
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"]).arg(source).args(["-ac", "2", "-ar", "16000", "-f", "f32le", "pipe:1"]);
    cmd.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    setup::no_window(&mut cmd);
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(anyhow!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let k = if track == Track::Me { 0 } else { 4 };
    Ok(out.stdout.chunks_exact(8).map(|f| f32::from_le_bytes([f[k], f[k + 1], f[k + 2], f[k + 3]])).collect())
}

/// A recorded meeting's two tracks, read from the start a stretch at a time: the recorder's WAV,
/// or the M4A they were kept as, through ffmpeg, never all of it in memory at once.
enum Tracks {
    Wav(record::wav::Reader),
    Pipe(std::process::Child, std::process::ChildStdout),
}

impl Tracks {
    fn open(core: &Core, file: &Path) -> Result<Tracks> {
        if record::wav::check(file).is_ok() {
            return Ok(Tracks::Wav(record::wav::Reader::open(file, 0)?));
        }
        let ff = setup::ffmpeg(&core.paths).ok_or_else(|| anyhow!(crate::tr!("Reading this recording needs ffmpeg. Download it in Settings.")))?;
        let mut cmd = std::process::Command::new(&ff);
        cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"]).arg(file).args(["-ac", "2", "-ar", "16000", "-f", "f32le", "pipe:1"]);
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
        setup::no_window(&mut cmd);
        let mut child = cmd.spawn()?;
        let out = child.stdout.take().ok_or_else(|| anyhow!("ffmpeg gave nothing"))?;
        Ok(Tracks::Pipe(child, out))
    }

    /// Up to `n` frames more, "me" and "them": none at the end.
    fn next(&mut self, n: usize) -> Result<(Vec<f32>, Vec<f32>)> {
        match self {
            Tracks::Wav(r) => Ok(r.next(n)?),
            Tracks::Pipe(child, out) => {
                use std::io::Read;
                let mut buf = vec![0u8; n * 8];
                let mut got = 0;
                while got < buf.len() {
                    match out.read(&mut buf[got..])? {
                        0 => break,
                        k => got += k,
                    }
                }
                if got == 0 && !child.wait()?.success() {
                    return Err(anyhow!(crate::tr!("ffmpeg could not read the recording.")));
                }
                let frames = &buf[..got / 8 * 8];
                let me = frames.chunks_exact(8).map(|f| f32::from_le_bytes([f[0], f[1], f[2], f[3]])).collect();
                let them = frames.chunks_exact(8).map(|f| f32::from_le_bytes([f[4], f[5], f[6], f[7]])).collect();
                Ok((me, them))
            }
        }
    }
}

impl Drop for Tracks {
    fn drop(&mut self) {
        if let Tracks::Pipe(child, _) = self {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The timing pass on a recorded meeting: each line heard again on its own track.
pub fn time_tracks(core: &Core, w: &mut dyn Engine, source: &Path, segments: &mut [Segment], language: &str, cancel: &std::sync::atomic::AtomicBool, mut progress: impl FnMut(f64)) -> Result<()> {
    let tracks: Vec<Track> = [Track::Me, Track::Them].into_iter().filter(|t| segments.iter().any(|s| s.track.as_deref() == Some(t.key()))).collect();
    for (k, tr) in tracks.iter().enumerate() {
        let audio = Audio { id: format!("timing-{}", tr.key()), samples: channel(core, source, *tr)? };
        let idx: Vec<usize> = (0..segments.len()).filter(|&i| segments[i].track.as_deref() == Some(tr.key())).collect();
        let mut mine: Vec<Segment> = idx.iter().map(|&i| segments[i].clone()).collect();
        let n = tracks.len() as f64;
        let r = crate::timing::pass(w, &audio, &mut mine, language, cancel, |f| progress((k as f64 + f) / n));
        w.forget(&audio.id);
        r?;
        for (i, s) in idx.into_iter().zip(mine) {
            segments[i] = s;
        }
    }
    Ok(())
}

/// The next piece to hear: the earliest, "them" first on a tie.
fn next_piece(pending: &mut Vec<Piece>) -> Option<Piece> {
    let i = (0..pending.len()).min_by_key(|&i| (pending[i].start, idx(pending[i].track) == 0))?;
    Some(pending.remove(i))
}

/// A line in its place in the transcript being written, and on the screen.
fn show(core: &Arc<Core>, t: &mut Transcript, id: &str, s: Segment) {
    let at = t.segments.partition_point(|x| x.start <= s.start);
    t.segments.insert(at, s.clone());
    if let Some(lt) = core.live.lock().unwrap().get_mut(id) {
        let at = lt.segments.partition_point(|x| x.start <= s.start);
        lt.segments.insert(at, s.clone());
    }
    let _ = core.app.emit("segment", serde_json::json!({ "id": id, "segment": &s }));
}

pub(super) fn run(core: &Arc<Core>, engine: &mut Option<EngineProcess>, id: &str) -> Result<()> {
    let Some(sess) = session(id) else { return Ok(()) };
    let result = run_session(core, engine, &sess);
    SESSIONS.lock().unwrap().retain(|s| s.id != sess.id);
    if result.is_err() {
        // its journal stays, so that it is offered again
        core.live.lock().unwrap().remove(id);
        let _ = core.app.emit("history-changed", id);
    }
    core.cancels.lock().unwrap().remove(id);
    result
}

/// The model for `language`, loaded: its file, where it runs, and its name.
fn prepare(core: &Arc<Core>, engine: &mut Option<EngineProcess>, language: &str) -> Result<(String, String, String)> {
    let Some((family, model)) = setup::model_for(&core.paths, &core.settings.lock().unwrap(), language) else {
        return Err(anyhow!(crate::tr!("There is no model for this language yet. Choose one in Settings.")));
    };
    let w = ensure_engine(core, engine, Some(&model.path))?;
    let info = w.ensure_loaded()?;
    show_loaded(core, &info);
    let words = core.settings.lock().unwrap().words.clone();
    w.set_vocabulary(if family == Family::Whisper { crate::words::hints(&words) } else { Vec::new() });
    let device = match &info.gpu {
        Some(g) if info.device == "gpu" => format!("GPU · {g}"),
        _ => "CPU".into(),
    };
    let label = models::variant(&model.variant).map_or(family.label(), |v| v.label).to_string();
    Ok((model.path.clone(), device, label))
}

/// A stopped recording transcribed from its file, each track cut where the recorder would have
/// cut it, then the steps chosen for it.
fn run_session(core: &Arc<Core>, engine: &mut Option<EngineProcess>, sess: &Arc<Session>) -> Result<()> {
    let id = sess.id.as_str();
    let started = Instant::now();
    let j = sess.journal.lock().unwrap().clone();
    let cancelled = || core.cancels.lock().unwrap().get(id).is_some_and(|c| c.load(Ordering::SeqCst));
    // cancelled while it waited its turn: it waits beside its journal again
    if cancelled() {
        core.update_job(id, |x| x.status = "cancelled".into());
        return Ok(());
    }
    let mut language = j.language.clone();
    core.update_job(id, |x| x.status = "loading".into());
    let saved = sess.saved.as_ref();
    let mut t = Transcript {
        id: id.to_string(),
        title: j.title.clone(),
        source_path: saved.map_or_else(|| sess.wav.display().to_string(), |s| s.source_path.clone()),
        created_ms: j.started_ms,
        language: language.clone(),
        partial: true,
        // "Me" and "Them" are its speakers from the first line
        speakers: meeting_speakers(j.sources),
        // one kept without its text stays what it is until its text is whole
        tracks: saved.and_then(|s| s.tracks.clone()),
        outputs: saved.map(|s| s.outputs.clone()).unwrap_or_default(),
        outputs_listed: saved.is_some(),
        sources: saved.map(|_| j.sources),
        ..Default::default()
    };
    let total = match saved {
        Some(s) => (s.duration_s * SR as f64) as usize,
        None => record::wav::check(&sess.wav).map(|c| c.file_frames).unwrap_or(0) as usize,
    };
    let total_s = total as f64 / SR as f64;
    t.duration_s = total_s;
    core.update_job(id, |x| x.duration_s = total_s);
    core.live.lock().unwrap().insert(id.to_string(), t.clone());
    let _ = core.app.emit("transcript-started", id);
    if language == language::AUTO {
        core.update_job(id, |x| x.status = "detecting".into());
        // the first three minutes, both sides together
        let (me, them) = Tracks::open(core, &sess.wav)?.next(SR * 180)?;
        let mix: Vec<f32> = me.iter().zip(&them).map(|(a, b)| a + b).collect();
        language = heard_language(core, engine, id, &mix)?.map_err(|m| anyhow!(m))?;
        settle_language(core, sess, &mut t, &language);
        core.update_job(id, |x| x.status = "loading".into());
    }
    let (model, device, model_label) = prepare(core, engine, &language)?;
    t.device = device;
    t.model = model_label;
    // stopped by the user: what is written is kept, and the recording is offered again
    let stop_here = |t: &mut Transcript| {
        t.partial = true;
        core.live.lock().unwrap().remove(id);
        let saved = !t.segments.is_empty() && history::save(&core.paths.history_dir(), t).is_ok();
        let _ = core.app.emit("history-changed", id);
        core.update_job(id, |x| {
            x.status = "cancelled".into();
            x.saved = saved;
            x.eta_s = None;
        });
    };
    core.update_job(id, |x| x.status = "transcribing".into());
    let progress = |core: &Arc<Core>, at: usize| {
        core.update_job(id, |x| {
            x.progress = (at as f64 / total.max(1) as f64).min(1.0);
            x.elapsed_s = started.elapsed().as_secs_f64();
        });
    };
    let mut lines = Lines::new(j.sources);
    let mut gates = [echo::Gate::default(), echo::Gate::pc()];
    let mut last_saved = Instant::now();
    let delay = test_delay();
    let mut reader = Tracks::open(core, &sess.wav)?;
    let mut cutters: Vec<(Track, Cutter)> = j.sources.tracks().into_iter().map(|tr| (tr, Cutter::new(record::PIECE_S, record::SEARCH_S, 0))).collect();
    let mut queue: Vec<Piece> = Vec::new();
    loop {
        let (me, them) = reader.next(SR * 10)?;
        let last = me.is_empty();
        for (tr, c) in cutters.iter_mut() {
            let samples = if *tr == Track::Me { &me } else { &them };
            for (start, s) in c.push(samples) {
                queue.push(Piece { track: *tr, start, samples: s });
            }
        }
        if last {
            for (tr, c) in cutters.drain(..) {
                for (start, s) in c.finish() {
                    queue.push(Piece { track: tr, start, samples: s });
                }
            }
        }
        while let Some(p) = next_piece(&mut queue) {
            if cancelled() {
                stop_here(&mut t);
                return Ok(());
            }
            if let Some(d) = delay {
                std::thread::sleep(d);
            }
            let end = p.start + p.samples.len();
            let segs = hear(core, engine, &model, id, &p, &language, &mut gates, j.sources)?;
            lines.add(p.track, end as f64 / SR as f64, segs);
            for s in lines.release(false) {
                show(core, &mut t, id, s);
            }
            progress(core, end);
            if last_saved.elapsed() > Duration::from_secs(20) && !t.segments.is_empty() {
                let _ = history::save(&core.paths.history_dir(), &t);
                last_saved = Instant::now();
            }
        }
        if last {
            break;
        }
    }
    drop(reader);
    for tr in [Track::Me, Track::Them] {
        lines.done[idx(tr)] = f64::INFINITY;
    }
    for s in lines.release(true) {
        show(core, &mut t, id, s);
    }
    let mut next_error = None;
    let them_lines = t.segments.iter().any(|s| s.track.as_deref() == Some("them"));
    if j.steps.speakers && them_lines {
        core.update_job(id, |x| {
            x.status = "speakers".into();
            x.eta_s = None;
        });
        let turns = channel(core, &sess.wav, Track::Them).and_then(|samples| {
            let audio = Audio { id: format!("{id}-them"), samples };
            let w = ensure_engine(core, engine, None)?;
            let turns = find_turns(core, w, &audio);
            w.forget(&audio.id);
            turns
        });
        match turns {
            Ok(turns) => split_them(&mut t, &turns),
            Err(e) => next_error = Some(crate::tr!("Transcribed, but the speakers could not be told apart: {}", format!("{e:#}"))),
        }
    }

    // kept: the sound to listen to as the recording, the tracks in Dawwin's folder; one kept
    // without its text keeps the files it has
    core.update_job(id, |x| x.status = "saving_audio".into());
    let kept = match saved {
        Some(s) => PathBuf::from(&s.source_path),
        None => {
            let (kept, tracks) = keep(core, sess, total_s);
            t.source_path = kept.display().to_string();
            t.tracks = tracks.map(|p| p.display().to_string());
            // the sound it kept is its own: a folder run never takes it for a recording
            t.outputs = vec![kept.display().to_string()];
            t.outputs_listed = true;
            kept
        }
    };
    let mut made = vec![kept.display().to_string()];
    if j.steps.remove_silence {
        match without_silences(core, &mut t) {
            Ok(file) => made.push(file.display().to_string()),
            Err(e) => next_error = next_error.or(Some(e)),
        }
    }
    t.sources = Some(j.sources);
    t.partial = false;
    t.elapsed_s = started.elapsed().as_secs_f64();
    core.live.lock().unwrap().remove(id);
    history::save(&core.paths.history_dir(), &t)?;
    if let Some(p) = &sess.journal_path {
        let _ = std::fs::remove_file(p);
    }
    let _ = core.app.emit("history-changed", id);
    // the steps chosen for it: a translation, then the minutes (the queue runs them in turn)
    if !t.segments.is_empty() && next_error.is_none() {
        if let Some(lang) = j.steps.translate.as_deref().filter(|l| *l != t.language) {
            if let Err(e) = core.translate_then(id, lang, false, false) {
                next_error = Some(format!("{e:#}"));
            }
        }
    }
    if j.steps.minutes && !t.segments.is_empty() {
        if let Err(e) = core.write_minutes(id, None, false) {
            next_error = next_error.or(Some(format!("{e:#}")));
        }
    }
    core.update_job(id, |x| {
        x.status = if next_error.is_some() { "failed".into() } else { "done".into() };
        x.error = next_error;
        x.progress = 1.0;
        x.eta_s = None;
        x.saved = true;
        x.elapsed_s = started.elapsed().as_secs_f64();
        x.outputs.extend(made);
    });
    Ok(())
}

/// Each piece takes this much longer, to try a slow computer (`DAWWIN_TEST_ENGINE_DELAY_MS`, debug builds).
fn test_delay() -> Option<Duration> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("DAWWIN_TEST_ENGINE_DELAY_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis)
}

/// What stays of a recording once its text is written: the recording itself, the sound to listen
/// to (the devices' 48 kHz, the PC's in stereo) as an M4A of 192 kb/s beside where it was
/// recorded; and the two tracks the engine hears, for "Find the speakers" and "Fix the timing",
/// in Dawwin's own folder beside the transcript (an M4A of 64 kb/s). Each M4A is used only once
/// ffmpeg reads it back as long as the recording; otherwise the WAV stays. Without the sound to
/// listen to (a recording of an earlier version, or a made-up source), the tracks are the
/// recording, as before.
fn keep(core: &Arc<Core>, sess: &Session, seconds: f64) -> (PathBuf, Option<PathBuf>) {
    let wav = sess.wav.as_path();
    let ff = setup::ffmpeg(&core.paths);
    let Some(hifi) = sess.hifi.as_deref().filter(|h| record::wav::check(h).is_ok_and(|c| c.file_frames > 0)) else {
        let kept = ff.as_deref().and_then(|ff| encode(ff, wav, &audio::unique_path(&wav.with_extension("m4a")), "96k", seconds)).unwrap_or_else(|| wav.to_path_buf());
        if kept != wav {
            let _ = std::fs::remove_file(wav);
        }
        return (kept, None);
    };
    let dir = core.paths.history_dir();
    let Some(ff) = ff else {
        // no ffmpeg: the tracks move to Dawwin's folder, the sound to listen to takes the recording's name
        let Ok(tracks) = history::tracks_file(&dir, &sess.id, "wav") else { return (wav.to_path_buf(), None) };
        if move_file(wav, &tracks).is_err() {
            let _ = std::fs::remove_file(hifi);
            return (wav.to_path_buf(), None);
        }
        return match std::fs::rename(hifi, wav) {
            Ok(()) => (wav.to_path_buf(), Some(tracks)),
            Err(_) => (hifi.to_path_buf(), Some(tracks)),
        };
    };
    let listen = encode(&ff, hifi, &audio::unique_path(&wav.with_extension("m4a")), "192k", seconds);
    let tracks = history::tracks_file(&dir, &sess.id, "m4a").ok().and_then(|t| encode(&ff, wav, &t, "64k", seconds));
    match (listen, tracks) {
        (Some(listen), Some(tracks)) => {
            let _ = std::fs::remove_file(hifi);
            let _ = std::fs::remove_file(wav);
            (listen, Some(tracks))
        }
        (Some(listen), None) => {
            // the tracks as a WAV, then: they are what Dawwin hears again
            let _ = std::fs::remove_file(hifi);
            match history::tracks_file(&dir, &sess.id, "wav").map(|t| (move_file(wav, &t), t)) {
                Ok((Ok(()), t)) => (listen, Some(t)),
                _ => {
                    let _ = std::fs::remove_file(&listen);
                    (wav.to_path_buf(), None)
                }
            }
        }
        (None, tracks) => {
            if let Some(t) = tracks {
                let _ = std::fs::remove_file(t);
            }
            let _ = std::fs::remove_file(hifi);
            (wav.to_path_buf(), None)
        }
    }
}

/// `from` as an AAC M4A at `to`, used only when ffmpeg reads it back as long as `seconds`.
fn encode(ff: &Path, from: &Path, to: &Path, bitrate: &str, seconds: f64) -> Option<PathBuf> {
    let mut cmd = std::process::Command::new(ff);
    cmd.args(["-hide_banner", "-nostdin", "-y", "-i"]).arg(from).args(["-c:a", "aac", "-b:a", bitrate, "-movflags", "+faststart"]).arg(to);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    setup::no_window(&mut cmd);
    let made = cmd.status().is_ok_and(|s| s.success());
    if made && duration_of(ff, to).is_some_and(|d| (d - seconds).abs() < 1.0) {
        return Some(to.to_path_buf());
    }
    let _ = std::fs::remove_file(to);
    None
}

/// A file moved, across drives too.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// How long ffmpeg says a file is.
fn duration_of(ff: &Path, file: &Path) -> Option<f64> {
    let mut cmd = std::process::Command::new(ff);
    cmd.args(["-hide_banner", "-nostdin", "-i"]).arg(file);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
    setup::no_window(&mut cmd);
    let out = cmd.output().ok()?;
    let err = String::from_utf8_lossy(&out.stderr);
    let d = err.split("Duration: ").nth(1)?.split(',').next()?.trim().to_string();
    let mut parts = d.split(':').map(|x| x.parse::<f64>().ok());
    Some(parts.next()?? * 3600.0 + parts.next()?? * 60.0 + parts.next()??)
}

// ---------------------------------------------------------------- the commands

type R<T> = std::result::Result<T, String>;

fn core_of(app: &AppHandle) -> Arc<Core> {
    app.state::<Arc<Core>>().inner().clone()
}

/// A language Dawwin knows, or Arabic.
fn known(language: Option<String>) -> String {
    language.filter(|l| models::known_language(l) || l == language::AUTO).unwrap_or_else(|| "ar".into())
}

#[tauri::command]
pub fn start_recording(app: AppHandle, language: Option<String>, steps: Option<Steps>) -> R<String> {
    core_of(&app).start_recording(known(language), steps.unwrap_or_default()).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn stop_recording(app: AppHandle, id: Option<String>) {
    core_of(&app).stop_recording(id.as_deref());
}

#[tauri::command]
pub fn recording_state(app: AppHandle) -> Option<RecordingView> {
    core_of(&app).recording()
}

#[derive(Serialize)]
pub struct RecordSetup {
    pub settings: RecordSettings,
    pub microphones: Vec<Microphone>,
    pub folder: String,
    /// The folder is inside OneDrive: recordings saved there are uploaded
    pub synced: bool,
}

#[tauri::command]
pub async fn record_setup(app: AppHandle) -> RecordSetup {
    let core = core_of(&app);
    let microphones = tauri::async_runtime::spawn_blocking(microphones).await.unwrap_or_default();
    RecordSetup { settings: core.record_settings(), microphones, folder: core.recordings_dir().display().to_string(), synced: recordings_synced(&core) }
}

#[tauri::command]
pub fn set_record_sources(app: AppHandle, sources: Sources) -> R<RecordSettings> {
    core_of(&app).change_record_settings(|s| s.sources = sources).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn choose_microphone(app: AppHandle, id: Option<String>) -> R<RecordSettings> {
    core_of(&app).choose_microphone(id).map_err(|e| format!("{e:#}"))
}

/// The notice shown once before the first recording: true when the user goes on.
#[tauri::command]
pub async fn recording_notice(app: AppHandle) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let core = core_of(&app);
    let synced = recordings_synced(&core);
    // shown once; again whenever the recordings would go to OneDrive, so that it is never untrue
    if core.record_settings().notice_shown && !synced {
        return true;
    }
    let text = if synced {
        crate::tr!(
            "Recording a call may need the consent of everyone on it. Tell the others before you start. The recordings folder, {}, is in OneDrive, so OneDrive will upload the recording.",
            core.recordings_dir().display()
        )
    } else {
        crate::tr!("Recording a call may need the consent of everyone on it. Tell the others before you start. The recording stays on this computer.")
    };
    let ok = app
        .dialog()
        .message(text)
        .title(crate::tr!("Before you record"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(crate::tr!("Record"), crate::tr!("Cancel")))
        .blocking_show();
    if ok {
        let _ = core.change_record_settings(|s| s.notice_shown = true);
    }
    ok
}

#[tauri::command]
pub fn interrupted_recordings(app: AppHandle) -> Vec<Interrupted> {
    core_of(&app).interrupted_recordings()
}

#[tauri::command]
pub fn recover_recording(app: AppHandle, journal: String, keep: bool) -> R<Option<String>> {
    core_of(&app).recover_recording(&journal, keep).map_err(|e| format!("{e:#}"))
}

/// What was chosen for a stopped recording: transcribed with `steps`, or (none) only kept.
#[tauri::command]
pub fn finish_recording(app: AppHandle, journal: String, language: Option<String>, steps: Option<Steps>) -> R<String> {
    core_of(&app).finish_recording(&journal, known(language), steps).map_err(|e| format!("{e:#}"))
}

/// A recording kept without its text, transcribed now.
#[tauri::command]
pub fn transcribe_saved(app: AppHandle, id: String, language: Option<String>, steps: Option<Steps>) -> R<String> {
    core_of(&app).transcribe_saved(&id, known(language), steps.unwrap_or_default()).map_err(|e| format!("{e:#}"))
}

/// Windows' page where desktop apps are allowed the microphone.
#[tauri::command]
pub fn open_microphone_privacy() {
    let _ = std::process::Command::new("explorer.exe").arg("ms-settings:privacy-microphone").spawn();
}

/// Closing the window while recording asks first; yes stops the recording, keeps its file whole,
/// and quits (the text is offered at the next start).
pub fn on_close(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    if !is_recording() {
        return;
    }
    api.prevent_close();
    ask_to_quit(window.app_handle().clone());
}

/// A recording runs and Dawwin is to quit (its window closed, or Quit in the notification area):
/// ask, and on yes stop the recording properly before quitting.
pub fn ask_to_quit(app: AppHandle) {
    std::thread::spawn(move || {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        let quit = app
            .dialog()
            .message(crate::tr!("A recording is running. Stop it and quit? The recording is kept, and at the next start Dawwin asks what to do with it."))
            .title(crate::tr!("Dawwin"))
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(crate::tr!("Stop and quit"), crate::tr!("Keep recording")))
            .blocking_show();
        if quit {
            core_of(&app).stop_recording_and_wait();
            crate::screen::app::quit_now(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(track: &str, start: f64, end: f64, text: &str) -> Segment {
        Segment { start, end, text: text.into(), track: Some(track.into()), ..Default::default() }
    }

    #[test]
    fn them_is_shown_at_once_and_me_once_them_is_heard_past_it() {
        let mut l = Lines::new(Sources::Both);
        l.add(Track::Me, 22.0, vec![line("me", 3.0, 21.0, "شكرا لكم")]);
        assert!(l.release(false).is_empty(), "them is not heard that far yet");
        l.add(Track::Them, 20.0, vec![line("them", 0.0, 20.0, "مرحبا بكم في الاجتماع")]);
        assert_eq!(l.release(false).iter().map(|s| s.start).collect::<Vec<_>>(), [0.0]);
        l.add(Track::Them, 45.0, vec![]);
        assert_eq!(l.release(false).iter().map(|s| s.start).collect::<Vec<_>>(), [3.0]);
    }

    #[test]
    fn a_me_line_that_only_repeats_them_is_left_out() {
        let mut l = Lines::new(Sources::Both);
        l.add(Track::Them, 25.0, vec![line("them", 1.0, 9.0, "the budget is approved for next year")]);
        l.add(Track::Me, 25.0, vec![line("me", 1.1, 9.1, "The budget is approved for next year."), line("me", 12.0, 14.0, "Great, thank you")]);
        let out = l.release(true);
        assert_eq!(out.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["the budget is approved for next year", "Great, thank you"]);
    }

    #[test]
    fn one_track_alone_is_not_held_back() {
        let mut l = Lines::new(Sources::Mic);
        l.add(Track::Me, 20.0, vec![line("me", 0.0, 5.0, "lecture")]);
        assert_eq!(l.release(false).len(), 1);
    }

    #[test]
    fn them_goes_first_on_a_tie() {
        let mut pending = vec![
            Piece { track: Track::Me, start: 0, samples: vec![] },
            Piece { track: Track::Them, start: 0, samples: vec![] },
            Piece { track: Track::Me, start: 5, samples: vec![] },
        ];
        assert_eq!(next_piece(&mut pending).unwrap().track, Track::Them);
        assert_eq!(next_piece(&mut pending).unwrap().track, Track::Me);
        assert_eq!(next_piece(&mut pending).unwrap().start, 5);
    }
}

#[cfg(test)]
mod meeting_tests {
    use super::*;

    fn line(track: &str, speaker: u32, start: f64, end: f64) -> Segment {
        Segment { start, end, text: format!("{track} at {start}"), track: Some(track.into()), speaker: Some(speaker), ..Default::default() }
    }

    #[test]
    fn the_others_are_told_apart_and_me_stays_me() {
        let mut t = Transcript {
            speakers: vec![Speaker { id: 1, name: "Baraa".into() }, Speaker { id: 2, name: crate::tr!("Them") }],
            segments: vec![line("them", 2, 0.0, 5.0), line("them", 2, 6.0, 12.0), line("me", 1, 13.0, 18.0), line("them", 2, 20.0, 25.0)],
            ..Default::default()
        };
        let turns = [
            speakers::Turn { start: 0.0, end: 5.5, speaker: 1 },
            speakers::Turn { start: 5.8, end: 12.5, speaker: 2 },
            speakers::Turn { start: 19.5, end: 25.5, speaker: 1 },
        ];
        split_them(&mut t, &turns);
        assert_eq!(t.segments.iter().map(|s| s.speaker.unwrap()).collect::<Vec<_>>(), [2, 3, 1, 2]);
        assert_eq!(t.segments[2].track.as_deref(), Some("me"));
        assert_eq!(t.speaker_name(1), "Baraa");
        assert_eq!(t.speaker_name(2), "Speaker 2", "Them was only a word for all of them");
        assert_eq!(t.speaker_name(3), "Speaker 3");
    }

    #[test]
    fn one_voice_on_the_pc_stays_them() {
        let mut t = Transcript {
            speakers: meeting_speakers(Sources::Both),
            segments: vec![line("them", 2, 0.0, 5.0), line("me", 1, 6.0, 8.0)],
            ..Default::default()
        };
        split_them(&mut t, &[speakers::Turn { start: 0.0, end: 5.0, speaker: 1 }]);
        assert_eq!(t.speaker_name(2), crate::tr!("Them"));
        assert_eq!(t.segments[0].speaker, Some(2));
    }

    #[test]
    fn the_pc_alone_is_speaker_one() {
        assert_eq!(meeting_speakers(Sources::Pc).iter().map(|s| (s.id, s.name.clone())).collect::<Vec<_>>(), [(1, crate::tr!("Them"))]);
        assert_eq!(speaker_id(Sources::Both, Track::Them), 2);
        assert_eq!(speaker_id(Sources::Mic, Track::Me), 1);
    }
}

#[cfg(test)]
mod folder_tests {
    use super::*;

    #[test]
    fn a_folder_inside_onedrive_is_known() {
        let roots = [PathBuf::from(r"C:\Users\x\OneDrive")];
        assert!(synced(Path::new(r"C:\Users\x\OneDrive\Documents\Dawwin"), &roots));
        assert!(synced(Path::new(r"c:\users\x\onedrive\Dawwin"), &roots));
        assert!(synced(Path::new(r"D:\OneDrive - Contoso\Dawwin"), &[]));
        assert!(!synced(Path::new(r"C:\Users\x\Dawwin"), &roots));
        assert!(!synced(Path::new(r"C:\Users\x\OneDriveBackup\Dawwin"), &roots));
        assert!(!synced(Path::new(r"C:\Users\x\Documents\Dawwin"), &roots));
    }
}
