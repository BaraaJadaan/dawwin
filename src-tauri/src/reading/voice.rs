//! The voice: a program of its own (`dawwin-voice.exe`, around audio.cpp's library), so that a
//! crash in a GPU driver costs a sentence, not the app. It reads one JSON request a line on its
//! standard input and answers each on its standard output; it listens on no port, and it ends when
//! its input closes, so it goes with the app however the app ends.
//!
//! `{"cmd":"load","family":…,"model":…,"backend":"vulkan"|"cpu","device":0,"threads":8,"options":{…}}`
//! → `{"ok":true,"device":"…"}`; `{"cmd":"speak","id":1,"text":…,"language":…,"voice":…,"out":"….wav"}`
//! → `{"id":1,"ok":true,"seconds":2.4}`. A failure says `"ok":false,"error":"…"`.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::setup::no_window;

/// What a model is loaded with.
#[derive(Clone, Debug, PartialEq)]
pub struct Load {
    pub family: String,
    pub model: PathBuf,
    pub gpu: bool,
    pub options: Vec<(String, String)>,
}

/// The voice program died or stopped answering: not this sentence's fault.
#[derive(Debug)]
pub struct VoiceDied(pub String);

impl std::fmt::Display for VoiceDied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VoiceDied {}

pub struct VoiceProcess {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    tail: Arc<Mutex<VecDeque<String>>>,
    next_id: u64,
    pub loaded: Option<Load>,
    pub device: String,
    /// It was started with the GPU build of audio.cpp's library
    pub gpu_library: bool,
}

impl VoiceProcess {
    /// Start the program; `lib` is the folder of audio.cpp's library, when it is not beside it.
    pub fn spawn(exe: &Path, lib: Option<&Path>, log: &Path) -> Result<Self> {
        let mut cmd = Command::new(exe);
        if let Some(lib) = lib {
            cmd.arg("--lib").arg(lib);
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        no_window(&mut cmd);
        let mut child = cmd.spawn().with_context(|| crate::tr!("cannot start {}", exe.display()))?;
        // it ends with the app however the app ends, mid-sentence too, giving back the GPU at once
        crate::translate::end_with_the_app(&child);
        let stdin = child.stdin.take().context("no stdin")?;
        let (tx, lines) = std::sync::mpsc::channel();
        let stdout = child.stdout.take().context("no stdout")?;
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let tail = Arc::new(Mutex::new(VecDeque::with_capacity(200)));
        let stderr = child.stderr.take().context("no stderr")?;
        let (t, log) = (tail.clone(), std::fs::File::create(log).ok());
        std::thread::spawn(move || {
            let mut log = log;
            for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                if let Some(f) = log.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                let mut t = t.lock().unwrap();
                if t.len() == 200 {
                    t.pop_front();
                }
                t.push_back(line);
            }
        });
        Ok(VoiceProcess { child, stdin, lines, tail, next_id: 1, loaded: None, device: String::new(), gpu_library: false })
    }

    /// It holds the GPU: its model is loaded there.
    pub fn on_gpu(&self) -> bool {
        self.loaded.is_some() && !self.device.is_empty() && self.device != "CPU"
    }

    fn last_lines(&self, n: usize) -> String {
        let t = self.tail.lock().unwrap();
        t.iter().skip(t.len().saturating_sub(n)).cloned().collect::<Vec<_>>().join("\n")
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn died(&mut self) -> anyhow::Error {
        // the last lines it wrote come a moment after it ends
        std::thread::sleep(Duration::from_millis(150));
        let status = self.child.try_wait().ok().flatten().map_or_else(|| "no answer".to_string(), |s| s.to_string());
        VoiceDied(crate::tr!("The voice stopped ({}). Last output:\n{}", status, self.last_lines(8))).into()
    }

    fn ask(&mut self, request: Value, timeout: Duration) -> Result<Value> {
        let line = request.to_string();
        if writeln!(self.stdin, "{line}").and_then(|_| self.stdin.flush()).is_err() {
            return Err(self.died());
        }
        loop {
            match self.lines.recv_timeout(timeout) {
                Ok(l) => match serde_json::from_str::<Value>(&l) {
                    Ok(v) => return Ok(v),
                    Err(_) => continue, // something it printed that is not an answer
                },
                Err(RecvTimeoutError::Timeout) if self.alive() => {
                    let _ = self.child.kill();
                    return Err(VoiceDied(crate::tr!("The voice did not answer in {} seconds.", timeout.as_secs())).into());
                }
                Err(_) => return Err(self.died()),
            }
        }
    }

    pub fn load(&mut self, load: &Load) -> Result<()> {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let options: serde_json::Map<String, Value> = load.options.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
        let v = self.ask(
            json!({
                "cmd": "load",
                "family": load.family,
                "model": load.model,
                "backend": if load.gpu { "vulkan" } else { "cpu" },
                "device": 0,
                "threads": threads,
                "options": options,
            }),
            Duration::from_secs(10 * 60),
        )?;
        if v["ok"] != true {
            return Err(anyhow!("{}", v["error"].as_str().unwrap_or("the voice could not be loaded")));
        }
        self.device = v["device"].as_str().unwrap_or("").to_string();
        self.loaded = Some(load.clone());
        Ok(())
    }

    /// Speak `text` into the WAV file `out`; how long it is, in seconds.
    pub fn speak(&mut self, text: &str, language: &str, voice: &Value, out: &Path) -> Result<f64> {
        let id = self.next_id;
        self.next_id += 1;
        let mut req = json!({ "cmd": "speak", "id": id, "text": text, "language": language, "out": out });
        if let (Some(r), Some(v)) = (req.as_object_mut(), voice.as_object()) {
            for (k, x) in v {
                r.insert(k.clone(), x.clone());
            }
        }
        // a sentence takes seconds; a slow processor with a large voice, a minute or two
        let v = self.ask(req, Duration::from_secs(300))?;
        if v["ok"] != true {
            return Err(anyhow!("{}", v["error"].as_str().unwrap_or("the voice could not read this sentence")));
        }
        Ok(v["seconds"].as_f64().unwrap_or(0.0))
    }
}

impl Drop for VoiceProcess {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

// ------------------------------------------------------------------ sound files

/// Mono samples at a rate, from a WAV the voice wrote (16-bit or 32-bit float, any channels).
pub fn read_wav(path: &Path) -> Result<(u32, Vec<f32>)> {
    let b = std::fs::read(path).with_context(|| path.display().to_string())?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(anyhow!("{} is not a WAV file", path.display()));
    }
    let (mut rate, mut channels, mut bits, mut format) = (0u32, 1u16, 16u16, 1u16);
    let mut at = 12;
    while at + 8 <= b.len() {
        let id = &b[at..at + 4];
        let size = u32::from_le_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
        let body = &b[at + 8..(at + 8 + size).min(b.len())];
        if id == b"fmt " && body.len() >= 16 {
            format = u16::from_le_bytes([body[0], body[1]]);
            channels = u16::from_le_bytes([body[2], body[3]]).max(1);
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            bits = u16::from_le_bytes([body[14], body[15]]);
            if format == 0xFFFE && body.len() >= 26 {
                format = u16::from_le_bytes([body[24], body[25]]);
            }
        } else if id == b"data" {
            let frame: Vec<f32> = match (format, bits) {
                (1, 16) => body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
                (1, 24) => body.chunks_exact(3).map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32 / 8_388_608.0).collect(),
                (3, 32) => body.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
                _ => return Err(anyhow!("{}: a WAV format Dawwin does not read", path.display())),
            };
            let ch = channels as usize;
            let mono = frame.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
            return Ok((rate, mono));
        }
        at += 8 + size + (size & 1);
    }
    Err(anyhow!("{} has no sound", path.display()))
}

pub fn write_wav(path: &Path, rate: u32, samples: &[f32]) -> Result<()> {
    let data = samples.len() * 2;
    let mut b = Vec::with_capacity(44 + data);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data as u32).to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    let tmp = path.with_extension("wav.tmp");
    std::fs::write(&tmp, b)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let n = (samples.len() as u64 * to as u64 / from as u64) as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * from as f64 / to as f64;
            let k = x as usize;
            let f = (x - k as f64) as f32;
            let a = samples[k.min(samples.len() - 1)];
            let b = samples[(k + 1).min(samples.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

/// Pieces of a sentence as one sound, a short gap between them and `pause` seconds of silence at
/// the end (the breath before the next sentence); the first piece's rate is the sound's.
pub fn join(pieces: &[(u32, Vec<f32>)], gap: f32, pause: f32) -> (u32, Vec<f32>) {
    let rate = pieces.first().map_or(24_000, |p| p.0);
    let mut out = Vec::new();
    for (k, (r, s)) in pieces.iter().enumerate() {
        if k > 0 {
            out.extend(std::iter::repeat(0.0).take((gap * rate as f32) as usize));
        }
        out.extend(resample(s, *r, rate));
    }
    out.extend(std::iter::repeat(0.0).take((pause * rate as f32) as usize));
    (rate, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wavs_are_read_and_joined_with_their_pauses() {
        let dir = std::env::temp_dir().join(format!("dawwin-voice-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        write_wav(&a, 16_000, &vec![0.5; 16_000]).unwrap();
        let (rate, s) = read_wav(&a).unwrap();
        assert_eq!((rate, s.len()), (16_000, 16_000));
        assert!((s[10] - 0.5).abs() < 0.001);
        let (rate, joined) = join(&[(16_000, s.clone()), (32_000, vec![0.25; 32_000])], 0.1, 0.5);
        assert_eq!(rate, 16_000);
        // one second, a tenth of a second, one second at half the rate, half a second
        assert_eq!(joined.len(), 16_000 + 1_600 + 16_000 + 8_000);
        assert!(read_wav(&dir.join("missing.wav")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
