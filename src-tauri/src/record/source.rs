//! Where the sound comes from: a "sound source" seam with two sides, Windows capture
//! (`record::windows`) and, in debug builds, files played at clock speed, so that the app can be
//! driven from outside with made-up tracks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use super::Track;

/// What a source tells the recorder.
pub enum Event {
    /// Sound captured at `ts` (the performance counter, in 100 ns units): 16 kHz mono.
    Packet { track: Track, ts: u64, data: Vec<f32> },
    /// The source is open: the device's name, and for "them" how ("process" or "device").
    Opened { track: Track, name: String, mode: Option<String> },
    /// It cannot record: "mic-blocked" (Windows' privacy settings), "no-mic", "failed".
    Problem { track: Track, code: String, detail: String },
}

pub trait Factory: Send + Sync {
    /// Start recording `track` on a thread of its own until `stop`; `device` is the microphone
    /// chosen (none: follow the Windows default).
    fn spawn(&self, track: Track, device: Option<String>, tx: Sender<Event>, stop: Arc<AtomicBool>) -> JoinHandle<()>;
}

/// The performance counter in 100 ns units: the clock of the capture timestamps.
#[cfg(windows)]
pub fn now() -> u64 {
    #[link(name = "kernel32")]
    extern "system" {
        fn QueryPerformanceCounter(count: *mut i64) -> i32;
        fn QueryPerformanceFrequency(freq: *mut i64) -> i32;
    }
    let (mut c, mut f) = (0i64, 0i64);
    // SAFETY: both write one integer through a valid pointer.
    unsafe {
        QueryPerformanceCounter(&mut c);
        QueryPerformanceFrequency(&mut f);
    }
    (c as i128 * 10_000_000 / f.max(1) as i128) as u64
}

#[cfg(not(windows))]
pub fn now() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64 / 100 + 1
}

/// Two made-up tracks played at clock speed, as if captured. Like loopback of a device, nothing
/// comes while a track is digitally silent, so the recorder has to fill the time itself.
pub struct Files {
    pub me: Option<Arc<Vec<f32>>>,
    pub them: Option<Arc<Vec<f32>>>,
    /// When the tracks started: a source opened again (another microphone) carries on from now.
    pub t0: u64,
    /// The default microphone sends digital silence, as a muted one does; a chosen one plays "me"
    pub silent_default: bool,
}

impl Factory for Files {
    fn spawn(&self, track: Track, device: Option<String>, tx: Sender<Event>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        let samples = match track {
            Track::Me => self.me.clone(),
            Track::Them => self.them.clone(),
        };
        let t0 = self.t0;
        let mute = self.silent_default && track == Track::Me && device.as_deref().is_none_or(|d| d == "test-default");
        std::thread::spawn(move || {
            let Some(samples) = samples else { return };
            let samples = if mute { Arc::new(vec![0.0; samples.len()]) } else { samples };
            let name = device.unwrap_or_else(|| format!("test {}", track.key()));
            let _ = tx.send(Event::Opened { track, name, mode: (track == Track::Them).then(|| "process".into()) });
            const N: usize = 160;
            let at = |k: usize| t0 + (k as u64 * 10_000_000 / super::SR as u64);
            let mut k = ((now().saturating_sub(t0)) as u128 * super::SR as u128 / 10_000_000) as usize / N * N;
            while !stop.load(Ordering::Relaxed) && k < samples.len() {
                let wait = at(k + N).saturating_sub(now());
                if wait > 0 {
                    std::thread::sleep(Duration::from_nanos(wait * 100));
                }
                let end = (k + N).min(samples.len());
                let data = samples[k..end].to_vec();
                if (mute || data.iter().any(|x| *x != 0.0)) && tx.send(Event::Packet { track, ts: at(k), data }).is_err() {
                    break;
                }
                k = end;
            }
        })
    }
}
