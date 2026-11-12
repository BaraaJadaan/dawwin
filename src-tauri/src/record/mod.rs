//! Recording a meeting: the microphone ("me") and what the PC plays ("them") as two tracks on one
//! clock, from capture to file to transcript. Each packet is placed by its capture timestamp, so
//! the tracks stay together and true to the clock through silences and device changes; the WAV
//! (left me, right them) is readable at every moment, and the sound is cut into pieces for the
//! speech engine as it arrives. The recording never waits for the engine. The measurements behind
//! it are in `tools/record-trial`.

pub mod cutter;
pub mod echo;
pub mod journal;
pub mod source;
pub mod timeline;
pub mod wav;
#[cfg(windows)]
pub mod windows;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use source::{Event, Factory};

pub const SR: usize = 16_000;

/// The messages of recording, in Arabic (see `crate::i18n`).
pub static AR: &[(&str, &str)] = &[
    ("Me", "أنا"),
    ("Them", "هم"),
    ("Recording {}", "تسجيل {}"),
    ("A recording is already running.", "هناك تسجيل جارٍ بالفعل."),
    ("Cannot make the folder for recordings, {}: {}", "تعذّر إنشاء مجلد التسجيلات، {}: {}"),
    ("The disk of {} is nearly full. Free some space to record.", "القرص الذي فيه {} شبه ممتلئ. أفرغ بعض المساحة لتسجّل."),
    ("Before you record", "قبل أن تسجّل"),
    (
        "Recording a call may need the consent of everyone on it. Tell the others before you start. The recording stays on this computer.",
        "قد يتطلب تسجيل المكالمة موافقة جميع المشاركين فيها. أخبرهم قبل أن تبدأ. يبقى التسجيل على هذا الحاسوب.",
    ),
    ("Record", "سجّل"),
    ("Cancel", "إلغاء"),
    (
        "A recording is running. Stop it and quit? The recording is kept, and its text is written at the next start.",
        "هناك تسجيل جارٍ. أتوقفه وتغلق دَوِّن؟ يُحفظ التسجيل، ويُكتب نصّه عند التشغيل التالي.",
    ),
    ("Stop and quit", "أوقفه وأغلق"),
    ("Keep recording", "تابع التسجيل"),
];
/// Pieces for the engine: at most this long, cut in their last `SEARCH_S`.
pub const PIECE_S: f32 = 25.0;
pub const SEARCH_S: f32 = 8.0;
/// What is older than this on the clock is final: a packet later than that is cut.
const MARGIN: usize = SR * 4 / 10;
/// Free space on the recording's drive: a warning under the first, a clean stop under the second.
pub const DISK_WARN: u64 = 2_000_000_000;
pub const DISK_STOP: u64 = 500_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    Me,
    Them,
}

impl Track {
    pub fn key(self) -> &'static str {
        match self {
            Track::Me => "me",
            Track::Them => "them",
        }
    }
    fn index(self) -> usize {
        match self {
            Track::Me => 0,
            Track::Them => 1,
        }
    }
}

/// What is recorded: both (the default), the microphone only (a lecture in a room), or the PC's
/// sound only (a webinar, a video).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sources {
    #[default]
    Both,
    Mic,
    Pc,
}

impl Sources {
    pub fn has(self, t: Track) -> bool {
        !matches!((self, t), (Sources::Mic, Track::Them) | (Sources::Pc, Track::Me))
    }
    pub fn tracks(self) -> Vec<Track> {
        [Track::Them, Track::Me].into_iter().filter(|t| self.has(*t)).collect()
    }
}

/// A piece of one track for the speech engine, from timeline index `start`.
pub struct Piece {
    pub track: Track,
    pub start: usize,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Level {
    /// Over the last tenth of a second, in dBFS
    pub db: f32,
    pub peak_db: f32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub seconds: f64,
    pub me: Option<Level>,
    pub them: Option<Level>,
    /// The microphone in use
    pub mic: Option<String>,
    /// It has sent nothing but digital silence for a few seconds (muted, or a device with no sound)
    pub mic_silent: bool,
    /// How the PC's sound is recorded: "process" (all but Dawwin) or "device" (the output device)
    pub them_mode: Option<String>,
    /// What stops a track: "mic-blocked" (Windows' privacy settings), "no-device", "failed"
    pub me_problem: Option<String>,
    pub them_problem: Option<String>,
    /// What Windows said, for the log
    pub problem_detail: Option<String>,
    /// Under 2 GB free on the recording's drive
    pub disk_low: bool,
    /// Stopped because under 500 MB were left
    pub disk_stopped: bool,
    pub ended: bool,
}

pub struct Config {
    pub sources: Sources,
    /// The microphone chosen; none follows the Windows default
    pub mic: Option<String>,
    pub wav: PathBuf,
    /// Called about ten times a second, and at the end
    pub on_status: Box<dyn Fn(&Status) + Send>,
}

pub struct Recorder {
    stop: Arc<AtomicBool>,
    mic: Sender<Option<String>>,
    thread: Option<std::thread::JoinHandle<Result<u64>>>,
}

/// Free bytes on the drive of `dir`.
#[cfg(windows)]
pub fn free_space(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, free: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut free, mut total, mut all) = (0u64, 0u64, 0u64);
    // SAFETY: a NUL-terminated path and three valid pointers.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut all) };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn free_space(_dir: &Path) -> Option<u64> {
    None
}

/// The disk space this test asks the recorder to see, in debug builds (`DAWWIN_TEST_DISK_FREE`).
fn disk_free(dir: &Path) -> Option<u64> {
    if cfg!(debug_assertions) {
        if let Some(v) = std::env::var("DAWWIN_TEST_DISK_FREE").ok().and_then(|v| v.parse().ok()) {
            return Some(v);
        }
    }
    free_space(dir)
}

impl Recorder {
    /// Start recording into `cfg.wav`; the pieces for the engine come through the receiver.
    pub fn start(cfg: Config, factory: Box<dyn Factory>) -> Result<(Recorder, Receiver<Piece>)> {
        let writer = wav::Writer::create(&cfg.wav)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (mic_tx, mic_rx) = mpsc::channel();
        let (pieces_tx, pieces_rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let s = stop.clone();
        let thread = std::thread::Builder::new().name("recorder".into()).spawn(move || run(cfg, factory, writer, s, mic_rx, status, pieces_tx))?;
        Ok((Recorder { stop, mic: mic_tx, thread: Some(thread) }, pieces_rx))
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Record from another microphone from now on (none: the Windows default).
    pub fn switch_mic(&self, id: Option<String>) {
        let _ = self.mic.send(id);
    }

    /// Wait until the WAV is whole: the frames in it.
    pub fn wait(mut self) -> Result<u64> {
        match self.thread.take().map(|t| t.join()) {
            Some(Ok(r)) => r,
            _ => Err(anyhow::anyhow!("the recorder stopped")),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn level(s: &[f32]) -> Level {
    let rms = (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt();
    let peak = s.iter().fold(0f32, |m, x| m.max(x.abs()));
    let db = |v: f32| (20.0 * v.max(1e-6).log10() * 10.0).round() / 10.0;
    Level { db: db(rms), peak_db: db(peak) }
}

#[allow(clippy::too_many_arguments)]
fn run(
    cfg: Config,
    factory: Box<dyn Factory>,
    mut writer: wav::Writer,
    stop: Arc<AtomicBool>,
    mic_rx: Receiver<Option<String>>,
    status: Arc<Mutex<Status>>,
    pieces: Sender<Piece>,
) -> Result<u64> {
    let _awake = crate::setup::KeepAwake::new();
    let t0 = source::now();
    let (tx, rx) = mpsc::channel::<Event>();
    let mut stops: [Option<Arc<AtomicBool>>; 2] = [None, None];
    let mut threads = Vec::new();
    for track in cfg.sources.tracks() {
        let s = Arc::new(AtomicBool::new(false));
        let device = if track == Track::Me { cfg.mic.clone() } else { None };
        threads.push(factory.spawn(track, device, tx.clone(), s.clone()));
        stops[track.index()] = Some(s);
    }
    let tol = (SR / 100) as i64;
    let mut tracks = [timeline::Track::new(tol), timeline::Track::new(tol)];
    let both = cfg.sources == Sources::Both;
    let mut canceller = both.then(echo::Canceller::new);
    let mut cutters = [cutter::Cutter::new(PIECE_S, SEARCH_S, 0), cutter::Cutter::new(PIECE_S, SEARCH_S, 0)];
    let mut written = 0usize;
    let mut last_flush = Instant::now();
    let mut last_checkpoint = Instant::now();
    let mut last_disk: Option<Instant> = None;
    let mut mic_open = false;
    let mut quiet_since: Option<f64> = None;
    let dir = cfg.wav.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut write = |me: &mut Vec<f32>, them: &mut Vec<f32>, writer: &mut wav::Writer, cutters: &mut [cutter::Cutter; 2], last: bool| -> Result<()> {
        if let Some(c) = canceller.as_mut() {
            c.process(me, them);
        }
        writer.append(me, them)?;
        for t in cfg.sources.tracks() {
            let samples = if t == Track::Me { &*me } else { &*them };
            for (start, s) in cutters[t.index()].push(samples) {
                let _ = pieces.send(Piece { track: t, start, samples: s });
            }
        }
        if last {
            for t in cfg.sources.tracks() {
                let c = std::mem::replace(&mut cutters[t.index()], cutter::Cutter::new(PIECE_S, SEARCH_S, 0));
                for (start, s) in c.finish() {
                    let _ = pieces.send(Piece { track: t, start, samples: s });
                }
            }
        }
        Ok(())
    };

    loop {
        let stopping = stop.load(Ordering::SeqCst);
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Event::Packet { track, ts, data }) => {
                tracks[track.index()].place(timeline::index_of(ts, t0), &data);
            }
            Ok(Event::Opened { track, name, mode }) => {
                let mut st = status.lock().unwrap();
                match track {
                    Track::Me => {
                        st.mic = Some(name);
                        st.me_problem = None;
                        mic_open = true;
                        quiet_since = None;
                    }
                    Track::Them => {
                        st.them_mode = mode;
                        st.them_problem = None;
                    }
                }
            }
            Ok(Event::Problem { track, code, detail }) => {
                let mut st = status.lock().unwrap();
                st.problem_detail = Some(detail);
                match track {
                    Track::Me => {
                        st.me_problem = Some(code);
                        mic_open = false;
                    }
                    Track::Them => st.them_problem = Some(code),
                }
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
        // another microphone: the stream changes, the track carries on
        while let Ok(device) = mic_rx.try_recv() {
            if let Some(old) = stops[0].take() {
                old.store(true, Ordering::SeqCst);
                let s = Arc::new(AtomicBool::new(false));
                threads.push(factory.spawn(Track::Me, device, tx.clone(), s.clone()));
                stops[0] = Some(s);
                mic_open = false;
            }
        }
        if stopping {
            break;
        }
        if last_flush.elapsed() >= Duration::from_millis(100) {
            last_flush = Instant::now();
            let now = timeline::index_of(source::now(), t0).max(0) as usize;
            let horizon = now.saturating_sub(MARGIN) / echo::BLOCK * echo::BLOCK;
            if horizon > written {
                let mut me = tracks[0].take(horizon as u64);
                let mut them = tracks[1].take(horizon as u64);
                let seconds = horizon as f64 / SR as f64;
                {
                    let mut st = status.lock().unwrap();
                    st.seconds = seconds;
                    st.me = cfg.sources.has(Track::Me).then(|| level(&me));
                    st.them = cfg.sources.has(Track::Them).then(|| level(&them));
                    // a microphone that sends only digital silence: muted, or a device with no sound
                    if cfg.sources.has(Track::Me) && mic_open {
                        let quiet = me.iter().all(|x| x.abs() < 1e-4);
                        if !quiet {
                            quiet_since = None;
                        } else if quiet_since.is_none() {
                            quiet_since = Some(seconds);
                        }
                        st.mic_silent = quiet_since.is_some_and(|q| seconds - q >= 3.0);
                    } else {
                        st.mic_silent = false;
                    }
                }
                write(&mut me, &mut them, &mut writer, &mut cutters, false)?;
                written = horizon;
                (cfg.on_status)(&status.lock().unwrap());
            }
        }
        if last_checkpoint.elapsed() >= Duration::from_secs(1) {
            last_checkpoint = Instant::now();
            writer.checkpoint()?;
        }
        if last_disk.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
            last_disk = Some(Instant::now());
            if let Some(free) = disk_free(&dir) {
                let mut st = status.lock().unwrap();
                st.disk_low = free < DISK_WARN;
                if free < DISK_STOP {
                    st.disk_stopped = true;
                    stop.store(true, Ordering::SeqCst);
                }
            }
        }
    }
    // the end: every source closed, then what they sent, then the rest of the file, up to Stop
    let stopped_at = timeline::index_of(source::now(), t0).max(0) as u64;
    for s in stops.iter().flatten() {
        s.store(true, Ordering::SeqCst);
    }
    drop(tx);
    for t in threads {
        let _ = t.join();
    }
    while let Ok(ev) = rx.try_recv() {
        if let Event::Packet { track, ts, data } = ev {
            tracks[track.index()].place(timeline::index_of(ts, t0), &data);
        }
    }
    let end = tracks[0].end().max(tracks[1].end()).max(written as u64).max(stopped_at) as usize;
    let end = end.div_ceil(echo::BLOCK) * echo::BLOCK;
    let mut me = tracks[0].take(end as u64);
    let mut them = tracks[1].take(end as u64);
    write(&mut me, &mut them, &mut writer, &mut cutters, true)?;
    writer.checkpoint()?;
    {
        let mut st = status.lock().unwrap();
        st.seconds = end as f64 / SR as f64;
        st.ended = true;
    }
    (cfg.on_status)(&status.lock().unwrap());
    Ok(writer.frames)
}

/// Tests that play sound at clock speed run alone: next to the heavy ones, the made-up source can
/// fall behind the clock as a starved capture thread would.
#[cfg(test)]
pub static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dawwin-rec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Two made-up tracks through the whole recorder at clock speed: the file holds both, where
    /// they were, across a long silence in which no packet came; the pieces cover each track.
    #[test]
    fn two_tracks_stay_on_one_clock_through_a_silence() {
        let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let d = dir("clock");
        let wav = d.join("r.wav");
        let n = SR * 3;
        // "me" speaks at 0.5 s; "them" is silent (no packets at all) until 2 s, then a tone
        let me: Vec<f32> = (0..n).map(|i| if (SR / 2..SR).contains(&i) { 0.3 } else { 0.0 }).collect();
        let them: Vec<f32> = (0..n).map(|i| if i >= 2 * SR { ((i as f32) * 0.05).sin() * 0.3 } else { 0.0 }).collect();
        let files = source::Files { me: Some(Arc::new(me)), them: Some(Arc::new(them)), t0: source::now(), silent_default: false };
        let cfg = Config { sources: Sources::Mic, mic: None, wav: wav.clone(), on_status: Box::new(|_| {}) };
        // the microphone alone: no echo cancellation, so the samples come back as they went in
        let (rec, pieces) = Recorder::start(cfg, Box::new(files)).unwrap();
        std::thread::sleep(Duration::from_millis(3300));
        rec.stop();
        let frames = rec.wait().unwrap();
        assert!(frames as usize >= n, "{frames}");
        let mut r = wav::Reader::open(&wav, 0).unwrap();
        let (left, right) = r.next(frames as usize).unwrap();
        let first = left.iter().position(|x| *x > 0.2).unwrap();
        assert!((first as i64 - (SR / 2) as i64).abs() < 200, "me starts at {first}");
        assert!(right.iter().all(|x| *x == 0.0), "them was not recorded");
        let got: Vec<Piece> = pieces.try_iter().collect();
        assert!(got.iter().all(|p| p.track == Track::Me));
        assert_eq!(got.iter().map(|p| p.samples.len()).sum::<usize>() as u64, frames);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_pc_track_keeps_its_time_after_nothing_came_for_a_while() {
        let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let d = dir("gap");
        let wav = d.join("r.wav");
        let n = SR * 3;
        let them: Vec<f32> = (0..n).map(|i| if i >= 2 * SR { 0.25 } else { 0.0 }).collect();
        let files = source::Files { me: None, them: Some(Arc::new(them)), t0: source::now(), silent_default: false };
        let cfg = Config { sources: Sources::Pc, mic: None, wav: wav.clone(), on_status: Box::new(|_| {}) };
        let (rec, _pieces) = Recorder::start(cfg, Box::new(files)).unwrap();
        std::thread::sleep(Duration::from_millis(3300));
        rec.stop();
        let frames = rec.wait().unwrap();
        let (_, right) = wav::Reader::open(&wav, 0).unwrap().next(frames as usize).unwrap();
        let first = right.iter().position(|x| *x > 0.2).unwrap();
        assert!((first as i64 - 2 * SR as i64).abs() < 200, "them starts at {first}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
