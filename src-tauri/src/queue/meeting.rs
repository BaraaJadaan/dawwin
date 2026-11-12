//! Recording a meeting, on the queue's side. The recorder (`crate::record`) runs on threads of its
//! own from Record to Stop; this job holds the speech engine meanwhile, like a running job, and
//! turns the pieces it cuts into lines a few seconds behind the speakers, "me" and "them" apart,
//! in time order ("them" first on a tie). It never holds the recording up: if the engine falls more
//! than two minutes behind, live text stops and the rest is transcribed from the file after Stop.
//! What was transcribed live is the final transcript. A recording interrupted by a crash is found
//! at the next start by its journal and offered back.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
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

/// Live text stops when the engine is this far behind the recording.
fn behind_limit() -> f64 {
    if cfg!(debug_assertions) {
        if let Some(s) = std::env::var("DAWWIN_TEST_RECORD_BEHIND_S").ok().and_then(|v| v.parse().ok()) {
            return s;
        }
    }
    120.0
}

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

/// A recording: being made, or found after a crash and being transcribed.
struct Session {
    id: String,
    title: String,
    wav: PathBuf,
    journal_path: PathBuf,
    journal: Mutex<Journal>,
    recorder: Mutex<Option<Recorder>>,
    pieces: Mutex<Option<Receiver<Piece>>>,
    /// The last word from the recorder
    status: Arc<Mutex<Status>>,
    /// Live text stopped: the engine fell too far behind
    behind: std::sync::atomic::AtomicBool,
}

static SESSIONS: Mutex<Vec<Arc<Session>>> = Mutex::new(Vec::new());

fn session(id: &str) -> Option<Arc<Session>> {
    SESSIONS.lock().unwrap().iter().find(|s| s.id == id).cloned()
}

/// The recording being made now, if there is one.
fn recording_now() -> Option<Arc<Session>> {
    SESSIONS.lock().unwrap().iter().find(|s| s.recorder.lock().unwrap().is_some()).cloned()
}

pub fn is_recording() -> bool {
    recording_now().is_some()
}

/// Stop the recording `id` (the job's stop button): true when it was recording.
pub(super) fn stop_if_recording(id: &str) -> bool {
    if let Some(s) = session(id) {
        if let Some(r) = s.recorder.lock().unwrap().as_ref() {
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
    /// Live text stopped; the rest comes after Stop
    pub behind: bool,
    pub sources: Sources,
}

fn view(s: &Session) -> RecordingView {
    RecordingView {
        id: s.id.clone(),
        title: s.title.clone(),
        status: s.status.lock().unwrap().clone(),
        behind: s.behind.load(Ordering::SeqCst),
        sources: s.journal.lock().unwrap().sources,
    }
}

/// Where the sound comes from: Windows, or made-up tracks in debug builds
/// (`DAWWIN_TEST_RECORD="me=<file>|them=<file>"`, files any decoder of Dawwin's reads; `|silent`
/// makes the default microphone send digital silence, as a muted one does).
fn sources(core: &Core) -> Result<Box<dyn source::Factory>> {
    #[cfg(debug_assertions)]
    if let Ok(spec) = std::env::var("DAWWIN_TEST_RECORD") {
        let ff = setup::ffmpeg(&core.paths);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut files = source::Files { me: None, them: None, t0: 0, silent_default: false };
        for part in spec.split('|') {
            match part.split_once('=') {
                Some(("me", p)) => files.me = Some(Arc::new(audio::decode(Path::new(p), ff.as_ref(), &stop)?)),
                Some(("them", p)) => files.them = Some(Arc::new(audio::decode(Path::new(p), ff.as_ref(), &stop)?)),
                _ if part == "silent" => files.silent_default = true,
                _ => {}
            }
        }
        files.t0 = source::now();
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

#[derive(Clone, Debug, Serialize)]
pub struct Interrupted {
    pub journal: String,
    pub title: String,
    pub started_ms: u64,
    pub seconds: f64,
}

impl Core {
    /// Start recording: the job's id. The model for `language` must be there, and the disk room.
    pub fn start_recording(self: &Arc<Self>, language: String, steps: Steps) -> Result<String> {
        if is_recording() {
            return Err(anyhow!(crate::tr!("A recording is already running.")));
        }
        if language == language::AUTO {
            if !setup::model_status(&self.paths, &self.settings.lock().unwrap(), Family::Language).found {
                return Err(anyhow!(crate::tr!("Detecting the language needs its model. Download it in Settings.")));
            }
        } else if setup::model_for(&self.paths, &self.settings.lock().unwrap(), &language).is_none() {
            return Err(anyhow!(crate::tr!("There is no model for this language yet. Choose one in Settings.")));
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
        };
        journal.save(&journal_path)?;
        let status = Arc::new(Mutex::new(Status::default()));
        let (st, app, rec_id) = (status.clone(), self.app.clone(), id.clone());
        let cfg = record::Config {
            sources: rs.sources,
            mic: rs.mic.clone(),
            wav: wav.clone(),
            on_status: Box::new(move |s| {
                *st.lock().unwrap() = s.clone();
                let _ = app.emit("recording", serde_json::json!({ "id": rec_id, "status": s }));
            }),
        };
        let (recorder, pieces) = match Recorder::start(cfg, sources(self)?) {
            Ok(x) => x,
            Err(e) => {
                let _ = std::fs::remove_file(&journal_path);
                return Err(e);
            }
        };
        let s = Arc::new(Session {
            id: id.clone(),
            title: title.clone(),
            wav: wav.clone(),
            journal_path,
            journal: Mutex::new(journal),
            recorder: Mutex::new(Some(recorder)),
            pieces: Mutex::new(Some(pieces)),
            status,
            behind: Default::default(),
        });
        SESSIONS.lock().unwrap().push(s);
        self.cancels.lock().unwrap().insert(id.clone(), Default::default());
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
        self.send(Msg::Record(id.clone()));
        Ok(id)
    }

    pub fn stop_recording(&self) {
        if let Some(s) = recording_now() {
            stop_if_recording(&s.id);
        }
    }

    /// Stop the recording and wait until its file is whole (for quitting).
    pub fn stop_recording_and_wait(&self) {
        if let Some(s) = recording_now() {
            let r = s.recorder.lock().unwrap().take();
            if let Some(r) = r {
                r.stop();
                let _ = r.wait();
            }
            let mut j = s.journal.lock().unwrap();
            j.state = "stopped".into();
            let _ = j.save(&s.journal_path);
        }
    }

    pub fn recording(&self) -> Option<RecordingView> {
        let s = recording_now().or_else(|| SESSIONS.lock().unwrap().first().cloned())?;
        Some(view(&s))
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

    /// Recordings that did not end cleanly (a crash, the power, a forced close).
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
                    title: j.title.clone(),
                    started_ms: j.started_ms,
                    seconds: c.map_or(0.0, |c| c.file_frames as f64 / SR as f64),
                })
            })
            .collect()
    }

    /// An interrupted recording: transcribe it (`keep`), the job's id; or delete it.
    pub fn recover_recording(self: &Arc<Self>, journal_file: &str, keep: bool) -> Result<Option<String>> {
        let path = PathBuf::from(journal_file);
        let dirs = [self.recordings_dir(), default_dir(&self.paths)];
        if !path.parent().is_some_and(|p| dirs.iter().any(|d| d == p)) || !path.to_string_lossy().ends_with(journal::SUFFIX) {
            return Err(anyhow!("not a recording of Dawwin's"));
        }
        let j = Journal::load(&path)?;
        let wav = j.wav_path(&path).ok_or_else(|| anyhow!("not a recording of Dawwin's"))?;
        if !keep {
            let _ = std::fs::remove_file(&wav);
            std::fs::remove_file(&path)?;
            // what was written of its text before the crash
            let _ = history::delete(&self.paths.history_dir(), &j.id);
            let _ = self.app.emit("history-changed", &j.id);
            return Ok(None);
        }
        // only a file laid out the way the recorder writes it is repaired
        record::wav::check(&wav)?;
        record::wav::repair(&wav)?;
        let mut j = j;
        j.state = "stopped".into();
        j.save(&path)?;
        let id = j.id.clone();
        let s = Arc::new(Session {
            id: id.clone(),
            title: j.title.clone(),
            wav: wav.clone(),
            journal_path: path,
            journal: Mutex::new(j.clone()),
            recorder: Mutex::new(None),
            pieces: Mutex::new(None),
            status: Default::default(),
            behind: Default::default(),
        });
        SESSIONS.lock().unwrap().push(s);
        self.cancels.lock().unwrap().insert(id.clone(), Default::default());
        self.update(|s| {
            s.queue.retain(|x| x.id != id);
            s.queue.push(JobView {
                id: id.clone(),
                title: j.title.clone(),
                path: wav.display().to_string(),
                language: j.language.clone(),
                steps: j.steps.clone(),
                kind: "record".into(),
                status: "queued".into(),
                ..Default::default()
            })
        });
        self.send(Msg::Record(id.clone()));
        Ok(Some(id))
    }
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
    let mut j = sess.journal.lock().unwrap();
    j.language = language.to_string();
    let _ = j.save(&sess.journal_path);
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
        speakers: t.speakers.iter().filter(|x| Some(x.id) != me).map(|x| Speaker { id: x.id.saturating_sub(offset), name: x.name.clone() }).collect(),
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

/// One track of a recorded meeting, from its file (the WAV, or the M4A through ffmpeg): what the
/// timing pass and the speaker model hear, rather than both sides mixed.
pub fn channel(core: &Core, source: &Path, track: Track) -> Result<Vec<f32>> {
    if let Ok(c) = record::wav::check(source) {
        let (me, them) = record::wav::Reader::open(source, 0)?.next(c.file_frames as usize)?;
        return Ok(if track == Track::Me { me } else { them });
    }
    let ff = setup::ffmpeg(&core.paths).ok_or_else(|| anyhow!(crate::tr!("Reading this recording needs ffmpeg. Download it in Settings.")))?;
    let mut cmd = std::process::Command::new(&ff);
    let pan = format!("pan=mono|c0=c{}", if track == Track::Me { 0 } else { 1 });
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"]).arg(source).args(["-af", &pan, "-ar", "16000", "-f", "f32le", "pipe:1"]);
    cmd.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    setup::no_window(&mut cmd);
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(anyhow!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
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
        // the recording does not depend on the engine: one still running goes on until Stop, and
        // its journal stays, so that it is offered again
        if let Some(r) = sess.recorder.lock().unwrap().take() {
            let _ = r.wait();
        }
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

/// The places where the recorder stopped giving pieces, added to where the file is to be heard from.
fn take_overflow(o: &Option<record::Overflow>, resume: &mut [Option<usize>; 2]) -> bool {
    let Some(o) = o else { return false };
    let points = *o.lock().unwrap();
    for (r, p) in resume.iter_mut().zip(points) {
        if let Some(p) = p {
            *r = Some(r.map_or(p, |x| x.min(p)));
        }
    }
    points.iter().any(Option::is_some)
}

fn run_session(core: &Arc<Core>, engine: &mut Option<EngineProcess>, sess: &Arc<Session>) -> Result<()> {
    let id = sess.id.as_str();
    let started = Instant::now();
    let j = sess.journal.lock().unwrap().clone();
    // "Detect the language": the first piece with speech tells it, and nothing is heard before
    let mut language = j.language.clone();
    let mut known = language != language::AUTO;
    let mut no_model: Option<String> = None;
    let mut tried: Vec<(usize, usize)> = Vec::new();
    let cancelled = || core.cancels.lock().unwrap().get(id).is_some_and(|c| c.load(Ordering::SeqCst));
    let live_now = sess.recorder.lock().unwrap().is_some();
    // a recording found after a crash and stopped while it waited: it is offered again
    if !live_now && cancelled() {
        core.update_job(id, |x| x.status = "cancelled".into());
        return Ok(());
    }
    core.update_job(id, |x| {
        x.status = if live_now { "recording".into() } else { "loading".into() };
    });
    let overflow = sess.recorder.lock().unwrap().as_ref().map(|r| r.overflow());
    // an engine that does not start (a damaged model, the antivirus, the GPU) never stops the
    // recording: the text then comes from the file after Stop
    let mut ready = if known { prepare(core, engine, &language) } else { Err(anyhow!("the language is not known yet")) };
    if known && ready.is_err() {
        sess.behind.store(true, Ordering::SeqCst);
    }
    let (device, model_label) = ready.as_ref().map(|(_, d, m)| (d.clone(), m.clone())).unwrap_or_default();
    let mut t = Transcript {
        id: id.to_string(),
        title: j.title.clone(),
        source_path: sess.wav.display().to_string(),
        created_ms: j.started_ms,
        duration_s: 0.0,
        language: language.clone(),
        elapsed_s: 0.0,
        device,
        model: model_label,
        partial: true,
        // "Me" and "Them" are its speakers from the first line
        speakers: meeting_speakers(j.sources),
        ..Default::default()
    };
    core.live.lock().unwrap().insert(id.to_string(), t.clone());
    let _ = core.app.emit("transcript-started", id);
    if !live_now {
        core.update_job(id, |x| x.status = "transcribing".into());
    }

    let mut lines = Lines::new(j.sources);
    let mut gates = [echo::Gate::default(), echo::Gate::default()];
    let mut pending: Vec<Piece> = Vec::new();
    // where each track is to be taken up again from the file, when live text stopped
    let mut resume: [Option<usize>; 2] = [None, None];
    let mut last_saved = Instant::now();
    let pieces = sess.pieces.lock().unwrap().take();
    let delay = test_delay();
    let mut shown_s = -1.0;
    if let Some(rx) = pieces {
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(p) => pending.push(p),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(p) = rx.try_recv() {
                pending.push(p);
            }
            let now_s = sess.status.lock().unwrap().seconds;
            if take_overflow(&overflow, &mut resume) {
                sess.behind.store(true, Ordering::SeqCst);
            }
            if !sess.behind.load(Ordering::SeqCst) {
                let oldest = pending.iter().map(|p| p.start).min();
                if oldest.is_some_and(|o| now_s - o as f64 / SR as f64 > behind_limit()) {
                    sess.behind.store(true, Ordering::SeqCst);
                }
            }
            if sess.behind.load(Ordering::SeqCst) {
                // the rest is heard from the file after Stop
                for p in pending.drain(..) {
                    let r = &mut resume[idx(p.track)];
                    *r = Some(r.map_or(p.start, |x| x.min(p.start)));
                }
                if now_s.floor() != shown_s {
                    shown_s = now_s.floor();
                    core.update_job(id, |x| x.duration_s = now_s);
                }
                continue;
            }
            if !known {
                // the earliest piece with speech not tried yet
                let candidate = (0..pending.len())
                    .filter(|&i| !tried.contains(&(idx(pending[i].track), pending[i].start)))
                    .min_by_key(|&i| (pending[i].start, idx(pending[i].track) == 0))
                    .filter(|&i| echo::Gate::default().admit(&pending[i].samples));
                if let Some(i) = candidate {
                    tried.push((idx(pending[i].track), pending[i].start));
                    let samples = pending[i].samples.clone();
                    match heard_language(core, engine, id, &samples) {
                        Ok(Ok(found)) => {
                            language = found;
                            known = true;
                            settle_language(core, sess, &mut t, &language);
                            ready = prepare(core, engine, &language);
                            if ready.is_err() {
                                sess.behind.store(true, Ordering::SeqCst);
                            }
                        }
                        Ok(Err(message)) => {
                            // a language without a model: the recording goes on, the text waits
                            no_model = Some(message);
                            sess.behind.store(true, Ordering::SeqCst);
                        }
                        Err(_) => {} // too little speech in it: the next one
                    }
                }
                if !known {
                    if now_s.floor() != shown_s {
                        shown_s = now_s.floor();
                        core.update_job(id, |x| x.duration_s = now_s);
                    }
                    continue;
                }
            }
            let model_path = ready.as_ref().map(|r| r.0.clone()).unwrap_or_default();
            if let Some(p) = next_piece(&mut pending) {
                if let Some(d) = delay {
                    std::thread::sleep(d);
                }
                let got = hear(core, engine, &model_path, id, &p, &language, &mut gates, j.sources);
                match got {
                    Ok(segs) => {
                        lines.add(p.track, (p.start + p.samples.len()) as f64 / SR as f64, segs);
                        for s in lines.release(false) {
                            show(core, &mut t, id, s);
                        }
                    }
                    Err(_) => {
                        // the engine is gone for now: the recording goes on, the text comes after
                        sess.behind.store(true, Ordering::SeqCst);
                        let r = &mut resume[idx(p.track)];
                        *r = Some(r.map_or(p.start, |x| x.min(p.start)));
                    }
                }
            }
            // the whole state once a second: the levels have an event of their own
            if now_s.floor() != shown_s {
                shown_s = now_s.floor();
                core.update_job(id, |x| {
                    x.duration_s = now_s;
                    x.elapsed_s = started.elapsed().as_secs_f64();
                });
            }
            if last_saved.elapsed() > Duration::from_secs(20) && !t.segments.is_empty() {
                t.duration_s = now_s;
                let _ = history::save(&core.paths.history_dir(), &t);
                last_saved = Instant::now();
            }
        }
        // the recorder is done: its file is whole
        if let Some(r) = sess.recorder.lock().unwrap().take() {
            let _ = r.wait();
        }
        take_overflow(&overflow, &mut resume);
        let mut jj = sess.journal.lock().unwrap();
        jj.state = "stopped".into();
        let _ = jj.save(&sess.journal_path);
    } else {
        // found after a crash: all of it from the file
        for tr in j.sources.tracks() {
            resume[idx(tr)] = Some(0);
        }
    }
    if let Some(message) = no_model {
        return Err(anyhow!(message));
    }
    if !known {
        core.update_job(id, |x| x.status = "detecting".into());
        // the first three minutes, both sides together
        let (me, them) = record::wav::Reader::open(&sess.wav, 0)?.next(SR * 180)?;
        let mix: Vec<f32> = me.iter().zip(&them).map(|(a, b)| a + b).collect();
        language = heard_language(core, engine, id, &mix)?.map_err(|m| anyhow!(m))?;
        settle_language(core, sess, &mut t, &language);
    }
    if ready.is_err() {
        core.update_job(id, |x| x.status = "loading".into());
        ready = prepare(core, engine, &language);
    }
    let (model_path, device, model_label) = ready?;
    t.device = device;
    t.model = model_label;
    let model = model_path;
    // stopped by the user after Stop: what is written is kept, and the recording is offered again
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
    let total = record::wav::check(&sess.wav).map(|c| c.file_frames).unwrap_or(0) as usize;
    let total_s = total as f64 / SR as f64;
    t.duration_s = total_s;
    core.update_job(id, |x| {
        x.status = "transcribing".into();
        x.duration_s = total_s;
    });
    let progress = |core: &Arc<Core>, at: usize| {
        core.update_job(id, |x| {
            x.progress = (at as f64 / total.max(1) as f64).min(1.0);
            x.elapsed_s = started.elapsed().as_secs_f64();
        });
    };
    // what was cut but not heard yet
    while let Some(p) = next_piece(&mut pending) {
        if cancelled() {
            stop_here(&mut t);
            return Ok(());
        }
        let end = p.start + p.samples.len();
        let segs = hear(core, engine, &model, id, &p, &language, &mut gates, j.sources)?;
        lines.add(p.track, end as f64 / SR as f64, segs);
        for s in lines.release(false) {
            show(core, &mut t, id, s);
        }
        progress(core, end);
    }
    // what live text did not reach: from the file, cut where it would have been
    if let Some(from) = resume.iter().flatten().min().copied() {
        let mut reader = record::wav::Reader::open(&sess.wav, from as u64)?;
        let mut cutters: Vec<(Track, Cutter)> = j.sources.tracks().into_iter().filter_map(|tr| resume[idx(tr)].map(|r| (tr, Cutter::new(record::PIECE_S, record::SEARCH_S, r)))).collect();
        let mut at = from;
        let mut queue: Vec<Piece> = Vec::new();
        loop {
            let (me, them) = reader.next(SR * 10)?;
            let n = me.len();
            let last = n == 0;
            for (tr, c) in cutters.iter_mut() {
                let samples = if *tr == Track::Me { &me } else { &them };
                let skip = resume[idx(*tr)].unwrap_or(0).saturating_sub(at).min(n);
                for (start, s) in c.push(&samples[skip..]) {
                    queue.push(Piece { track: *tr, start, samples: s });
                }
            }
            at += n;
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
    }
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

    // kept small: an M4A when there is ffmpeg, the WAV only once that is checked
    core.update_job(id, |x| x.status = "saving_audio".into());
    let kept = keep_small(core, &sess.wav, total_s);
    if kept != sess.wav {
        let _ = std::fs::remove_file(&sess.wav);
    }
    t.source_path = kept.display().to_string();
    // the sound it kept is its own: a folder run never takes it for a recording
    t.outputs = vec![kept.display().to_string()];
    t.outputs_listed = true;
    t.partial = false;
    t.elapsed_s = started.elapsed().as_secs_f64();
    core.live.lock().unwrap().remove(id);
    history::save(&core.paths.history_dir(), &t)?;
    let _ = std::fs::remove_file(&sess.journal_path);
    let _ = core.app.emit("history-changed", id);
    // the steps chosen on the start page: a translation
    if !t.segments.is_empty() && next_error.is_none() {
        if let Some(lang) = j.steps.translate.as_deref().filter(|l| *l != t.language) {
            if let Err(e) = core.translate_then(id, lang, false, false) {
                next_error = Some(format!("{e:#}"));
            }
        }
    }
    core.update_job(id, |x| {
        x.status = if next_error.is_some() { "failed".into() } else { "done".into() };
        x.error = next_error;
        x.progress = 1.0;
        x.eta_s = None;
        x.saved = true;
        x.elapsed_s = started.elapsed().as_secs_f64();
        x.outputs.push(kept.display().to_string());
    });
    Ok(())
}

/// Each piece takes this much longer, to try falling behind (`DAWWIN_TEST_ENGINE_DELAY_MS`, debug builds).
fn test_delay() -> Option<Duration> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("DAWWIN_TEST_ENGINE_DELAY_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis)
}

/// The recording as an M4A next to the WAV, through ffmpeg (AAC at 96 kb/s, both tracks: about
/// 45 MB an hour against 230 for the WAV). The M4A is used only once ffmpeg reads it back as long
/// as the WAV; otherwise the WAV stays.
fn keep_small(core: &Arc<Core>, wav: &Path, seconds: f64) -> PathBuf {
    let Some(ff) = setup::ffmpeg(&core.paths) else { return wav.to_path_buf() };
    let m4a = audio::unique_path(&wav.with_extension("m4a"));
    let mut cmd = std::process::Command::new(&ff);
    cmd.args(["-hide_banner", "-nostdin", "-y", "-i"]).arg(wav).args(["-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart"]).arg(&m4a);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    setup::no_window(&mut cmd);
    let made = cmd.status().is_ok_and(|s| s.success());
    if made && duration_of(&ff, &m4a).is_some_and(|d| (d - seconds).abs() < 1.0) {
        return m4a;
    }
    let _ = std::fs::remove_file(&m4a);
    wav.to_path_buf()
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

#[tauri::command]
pub fn start_recording(app: AppHandle, language: String, steps: Option<Steps>) -> R<String> {
    let core = core_of(&app);
    let language = if models::known_language(&language) || language == language::AUTO { language } else { "ar".into() };
    core.start_recording(language, steps.unwrap_or_default()).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn stop_recording(app: AppHandle) {
    core_of(&app).stop_recording();
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
    let app = window.app_handle().clone();
    std::thread::spawn(move || {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        let quit = app
            .dialog()
            .message(crate::tr!("A recording is running. Stop it and quit? The recording is kept, and its text is written at the next start."))
            .title(crate::tr!("Dawwin"))
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(crate::tr!("Stop and quit"), crate::tr!("Keep recording")))
            .blocking_show();
        if quit {
            core_of(&app).stop_recording_and_wait();
            app.exit(0);
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
    fn pieces_the_recorder_could_not_keep_are_heard_from_the_file() {
        let o: record::Overflow = Arc::new(Mutex::new([None, Some(800)]));
        let mut resume = [Some(400), Some(1200)];
        assert!(take_overflow(&Some(o.clone()), &mut resume));
        assert_eq!(resume, [Some(400), Some(800)], "the earliest place of each track");
        *o.lock().unwrap() = [None, None];
        assert!(!take_overflow(&Some(o), &mut resume));
        assert!(!take_overflow(&None, &mut resume));
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
