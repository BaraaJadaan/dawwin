//! The speech-to-text engine, behind one interface.
//!
//! [`EngineProcess`] runs `dawwin-engine.exe` (speech models through transcribe.cpp) in a child
//! process and talks JSON lines with it. A crash in there, in a GPU driver say, ends that process
//! and not the app. A process holds one model: another model takes another process.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use crate::setup::{no_window, Paths};

/// Decoded audio: 16 kHz mono. `id` lets an engine cache what it derived from it.
pub struct Audio {
    pub id: String,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct EngineInfo {
    /// "gpu" | "cpu"
    pub device: String,
    pub gpu: Option<String>,
    pub vram_total_gb: Option<f64>,
    /// How many pieces of audio to send at once
    pub batch: usize,
    /// Why it runs on the CPU although a GPU was wanted: "gpu-too-small" | "gpu-failed"
    pub fallback: Option<String>,
}

/// What the engine made of one piece of audio.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recognized {
    pub text: String,
    /// The model looped or ran out of tokens: the text ends before the audio does
    pub stopped: bool,
}

/// One language the language model heard: its code (the model's own), its name, and how likely.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub code: String,
    pub name: String,
    pub p: f32,
}

pub trait Engine: Send {
    /// Start loading the model without waiting (so decoding can overlap with it).
    fn start_loading(&mut self) -> Result<()>;
    /// Load the model if needed and say where it runs.
    fn ensure_loaded(&mut self) -> Result<EngineInfo>;
    /// Transcribe the sample ranges `[start, end)` of `audio` as one batch: one result per range.
    fn transcribe(&mut self, audio: &Audio, ranges: &[(usize, usize)], language: &str) -> Result<Vec<Recognized>>;
    /// Let the audio's cached data go.
    fn forget(&mut self, audio_id: &str);
    /// The words the model should hear right, for a model that takes them (Whisper).
    fn set_vocabulary(&mut self, _words: Vec<String>) {}
    /// Who speaks when in all of `audio`, by the speaker model `model`; `stop` ends it early.
    fn speakers(&mut self, _model: &str, _audio: &Audio, _stop: &AtomicBool) -> Result<Vec<crate::speakers::Turn>> {
        bail!("this engine cannot tell speakers apart")
    }
    /// Which language the sample ranges of `audio` are in, by the language model `model`: every
    /// language it knows, the likeliest first. Empty when there is too little speech to tell.
    fn language(&mut self, _model: &str, _audio: &Audio, _ranges: &[(usize, usize)]) -> Result<Vec<Heard>> {
        bail!("this engine cannot tell languages apart")
    }
}

/// The request was stopped (see `Engine::speakers`).
#[derive(Debug)]
pub struct Stopped;
impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stopped")
    }
}
impl std::error::Error for Stopped {}

/// "Try again with a smaller batch."
#[derive(Debug)]
pub struct OutOfMemory(pub String);
impl std::fmt::Display for OutOfMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "out of GPU memory: {}", self.0)
    }
}
impl std::error::Error for OutOfMemory {}

/// The engine itself failed, as opposed to the recording it was given.
#[derive(Debug)]
pub struct EngineFailed(pub String);
impl std::fmt::Display for EngineFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for EngineFailed {}

/// What a failed reply means. Only out of memory, or an error the engine marks as the device's,
/// is the engine's fault (and costs the GPU); a broken model file or a missing audio file is not.
fn reply_error(v: &Value) -> anyhow::Error {
    let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error").to_string();
    if v.get("stopped").and_then(|o| o.as_bool()) == Some(true) {
        Stopped.into()
    } else if v.get("oom").and_then(|o| o.as_bool()) == Some(true) {
        OutOfMemory(err).into()
    } else if v.get("device").and_then(|o| o.as_bool()) == Some(true) {
        EngineFailed(err).into()
    } else {
        anyhow!(err)
    }
}

/// On audio with no speech the model can write a marker such as "@@@ضوضاء" (noise).
fn without_markers(text: &str) -> String {
    if !text.contains("@@@") {
        return text.trim().to_string();
    }
    text.split_whitespace().filter(|w| !w.contains("@@@")).collect::<Vec<_>>().join(" ")
}

pub struct EngineProcess {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<Value>,
    next_id: u64,
    model: String,
    pending_load: Option<u64>,
    loaded: Option<EngineInfo>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    cache_dir: PathBuf,
    raw_files: Vec<(String, PathBuf)>,
    vocabulary: Vec<String>,
}

impl EngineProcess {
    /// Start the engine. `cpu_only` keeps the GPU driver out of it altogether.
    pub fn spawn(exe: &Path, paths: &Paths, model: &str, cpu_only: bool) -> Result<Self> {
        let mut cmd = Command::new(exe);
        if cpu_only {
            cmd.arg("--cpu");
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        no_window(&mut cmd);
        let mut child = cmd.spawn().with_context(|| crate::tr!("cannot start {}", exe.display()))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (tx, replies) = mpsc::channel::<Value>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if tx.send(v).is_err() {
                    break;
                }
            }
        });

        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(64)));
        let tail = stderr_tail.clone();
        let log_path = paths.logs_dir().join("engine.log");
        std::thread::spawn(move || {
            let mut log = std::fs::File::create(log_path).ok();
            for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                if let Some(f) = log.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                let mut t = tail.lock().unwrap();
                if t.len() == 60 {
                    t.pop_front();
                }
                t.push_back(line);
            }
        });

        Ok(EngineProcess {
            child,
            stdin,
            replies,
            next_id: 1,
            model: model.to_string(),
            pending_load: None,
            loaded: None,
            stderr_tail,
            cache_dir: paths.cache_dir(),
            raw_files: Vec::new(),
            vocabulary: Vec::new(),
        })
    }

    fn tail(&self) -> String {
        let t = self.stderr_tail.lock().unwrap();
        t.iter().rev().take(12).cloned().collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
    }

    fn send(&mut self, mut req: Value) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        req["id"] = json!(id);
        let line = serde_json::to_string(&req)? + "\n";
        self.stdin.write_all(line.as_bytes()).and_then(|_| self.stdin.flush()).map_err(|e| {
            anyhow!(EngineFailed(crate::tr!("the engine stopped ({}). Last output:\n{}", e, self.tail())))
        })?;
        Ok(id)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Point this engine at a model. False when it already holds another one.
    pub fn use_model(&mut self, model: &str) -> bool {
        if self.model == model {
            return true;
        }
        if self.loaded.is_some() || self.pending_load.is_some() {
            return false;
        }
        self.model = model.to_string();
        true
    }

    fn wait(&mut self, id: u64) -> Result<Value> {
        self.wait_or_stop(id, None)
    }

    /// Wait for the reply to `id`; when `stop` is set meanwhile, ask the engine to stop it.
    fn wait_or_stop(&mut self, id: u64, stop: Option<&AtomicBool>) -> Result<Value> {
        let mut asked = false;
        loop {
            if !asked && stop.is_some_and(|s| s.load(Ordering::SeqCst)) {
                asked = true;
                self.send(json!({"cmd": "stop", "target": id}))?;
            }
            let tick = if stop.is_some() { Duration::from_millis(200) } else { Duration::from_secs(1) };
            match self.replies.recv_timeout(tick) {
                Ok(v) => {
                    if v.get("id").and_then(|i| i.as_u64()) != Some(id) {
                        continue; // a reply to a request nobody waits for any more
                    }
                    if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
                        return Ok(v);
                    }
                    return Err(reply_error(&v));
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Ok(Some(status)) = self.child.try_wait() {
                        return Err(EngineFailed(crate::tr!("the engine exited ({}). Last output:\n{}", status, self.tail())).into());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = self.child.wait();
                    return Err(EngineFailed(crate::tr!("the engine exited. Last output:\n{}", self.tail())).into());
                }
            }
        }
    }

    /// What this computer has to run on.
    pub fn probe(&mut self) -> Result<Value> {
        let id = self.send(json!({"cmd": "probe"}))?;
        self.wait(id)
    }

    fn raw_file(&mut self, audio: &Audio) -> Result<PathBuf> {
        if let Some((_, p)) = self.raw_files.iter().find(|(id, _)| id == &audio.id) {
            return Ok(p.clone());
        }
        let p = self.cache_dir.join(format!("{}.f32", audio.id));
        crate::audio::write_raw_f32(&p, &audio.samples)?;
        self.raw_files.push((audio.id.clone(), p.clone()));
        Ok(p)
    }

    pub fn shutdown(mut self) {
        let _ = self.send(json!({"cmd": "quit"}));
        for _ in 0..30 {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        for (_, p) in self.raw_files.drain(..) {
            let _ = std::fs::remove_file(p);
        }
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
    }
}

impl Engine for EngineProcess {
    fn start_loading(&mut self) -> Result<()> {
        if self.loaded.is_none() && self.pending_load.is_none() {
            let model = self.model.clone();
            self.pending_load = Some(self.send(json!({"cmd": "load", "model": model}))?);
        }
        Ok(())
    }

    fn ensure_loaded(&mut self) -> Result<EngineInfo> {
        if let Some(i) = &self.loaded {
            return Ok(i.clone());
        }
        self.start_loading()?;
        let id = self.pending_load.take().unwrap();
        let v = self.wait(id)?;
        let info = EngineInfo {
            device: v.get("device").and_then(|d| d.as_str()).unwrap_or("cpu").to_string(),
            gpu: v.get("gpu").and_then(|d| d.as_str()).map(String::from),
            vram_total_gb: v.get("vram_total_gb").and_then(|d| d.as_f64()),
            batch: v.get("batch").and_then(|d| d.as_u64()).unwrap_or(1).max(1) as usize,
            fallback: v.get("fallback").and_then(|d| d.as_str()).map(String::from),
        };
        self.loaded = Some(info.clone());
        Ok(info)
    }

    fn transcribe(&mut self, audio: &Audio, ranges: &[(usize, usize)], language: &str) -> Result<Vec<Recognized>> {
        self.ensure_loaded()?;
        let path = self.raw_file(audio)?;
        let id = self.send(json!({
            "cmd": "transcribe",
            "path": path,
            "ranges": ranges.iter().map(|(a, b)| [*a, *b]).collect::<Vec<_>>(),
            "language": language,
            "vocabulary": self.vocabulary,
        }))?;
        let v = self.wait(id)?;
        let texts = v.get("texts").and_then(|t| t.as_array()).ok_or_else(|| anyhow!("engine reply has no texts"))?;
        let stopped = v.get("stopped").and_then(|t| t.as_array());
        if texts.len() != ranges.len() {
            bail!("engine returned {} texts for {} chunks", texts.len(), ranges.len());
        }
        Ok(texts
            .iter()
            .enumerate()
            .map(|(i, t)| Recognized {
                text: without_markers(t.as_str().unwrap_or("")),
                stopped: stopped.and_then(|s| s.get(i)).and_then(|s| s.as_bool()).unwrap_or(false),
            })
            .collect())
    }

    fn forget(&mut self, audio_id: &str) {
        self.raw_files.retain(|(id, p)| {
            if id == audio_id {
                let _ = std::fs::remove_file(p);
                false
            } else {
                true
            }
        });
    }

    fn set_vocabulary(&mut self, words: Vec<String>) {
        self.vocabulary = words;
    }

    fn speakers(&mut self, model: &str, audio: &Audio, stop: &AtomicBool) -> Result<Vec<crate::speakers::Turn>> {
        let path = self.raw_file(audio)?;
        let id = self.send(json!({"cmd": "speakers", "model": model, "path": path}))?;
        let v = self.wait_or_stop(id, Some(stop))?;
        Ok(turns_of(&v))
    }

    fn language(&mut self, model: &str, audio: &Audio, ranges: &[(usize, usize)]) -> Result<Vec<Heard>> {
        let path = self.raw_file(audio)?;
        let id = self.send(json!({
            "cmd": "language",
            "model": model,
            "path": path,
            "ranges": ranges.iter().map(|(a, b)| [*a, *b]).collect::<Vec<_>>(),
        }))?;
        let v = self.wait(id)?;
        Ok(heard_of(&v))
    }
}

/// The engine's speaker turns, `[start_ms, end_ms, speaker]`, in seconds.
fn turns_of(v: &Value) -> Vec<crate::speakers::Turn> {
    v.get("turns")
        .and_then(|t| t.as_array())
        .map(|t| {
            t.iter()
                .filter_map(|x| {
                    let (a, b, s) = (x.get(0)?.as_f64()?, x.get(1)?.as_f64()?, x.get(2)?.as_u64()?);
                    Some(crate::speakers::Turn { start: a / 1000.0, end: b / 1000.0, speaker: s as u32 })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The engine's languages, `[code, name, p]`.
fn heard_of(v: &Value) -> Vec<Heard> {
    v.get("candidates")
        .and_then(|t| t.as_array())
        .map(|t| {
            t.iter()
                .filter_map(|x| Some(Heard { code: x.get(0)?.as_str()?.to_string(), name: x.get(1)?.as_str()?.to_string(), p: x.get(2)?.as_f64()? as f32 }))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_devices_own_errors_count_against_the_gpu() {
        let broken = reply_error(&json!({"ok": false, "error": "model load failed: bad GGUF", "oom": false, "device": false}));
        assert!(!broken.is::<EngineFailed>() && !broken.is::<OutOfMemory>(), "a broken model file is not the GPU's fault");
        assert!(reply_error(&json!({"ok": false, "error": "backend error: vk", "device": true})).is::<EngineFailed>());
        assert!(reply_error(&json!({"ok": false, "error": "out of memory", "oom": true})).is::<OutOfMemory>());
        assert!(!reply_error(&json!({"ok": false, "error": "x"})).is::<EngineFailed>(), "an engine that does not say");
    }

    #[test]
    fn speaker_turns_and_languages_are_read_from_the_engine() {
        let turns = turns_of(&json!({"turns": [[320, 2400, 1], [2500, 4000, 2]]}));
        assert_eq!(turns.len(), 2);
        assert_eq!((turns[0].start, turns[0].end, turns[0].speaker), (0.32, 2.4, 1));
        let heard = heard_of(&json!({"candidates": [["ar", "Arabic", 0.9], ["iw", "Hebrew", 0.05]]}));
        assert_eq!(heard[1], Heard { code: "iw".into(), name: "Hebrew".into(), p: 0.05 });
        assert!(heard_of(&json!({"candidates": [], "too_short": true})).is_empty());
        assert!(reply_error(&json!({"ok": false, "error": "aborted", "stopped": true})).is::<Stopped>());
    }

    /// The real engine with the real speaker and language models, on transcribe.cpp's own sample
    /// recordings (the Engine workflow): DAWWIN_TEST_ENGINE (the engine), DAWWIN_TEST_SPEAKERS and
    /// DAWWIN_TEST_LANGUAGE (the models), DAWWIN_TEST_SAMPLES (a folder of its `samples/*.wav`).
    #[test]
    #[ignore]
    fn real_speakers_and_language() {
        let var = |k: &str| PathBuf::from(std::env::var_os(k).unwrap_or_else(|| panic!("{k}")));
        let (exe, speakers, language, samples) = (var("DAWWIN_TEST_ENGINE"), var("DAWWIN_TEST_SPEAKERS"), var("DAWWIN_TEST_LANGUAGE"), var("DAWWIN_TEST_SAMPLES"));
        let home = std::env::temp_dir().join(format!("dawwin-real-who-{}", std::process::id()));
        std::fs::create_dir_all(home.join("cache")).unwrap();
        std::fs::create_dir_all(home.join("logs")).unwrap();
        let paths = Paths { local: home.clone(), roaming: home.clone() };
        let mut e = EngineProcess::spawn(&exe, &paths, "", true).unwrap();
        let read = |name: &str| Audio { id: name.replace('.', "-"), samples: crate::audio::decode(&samples.join(name), None, &AtomicBool::new(false)).unwrap() };

        // who speaks: two in the two-speaker mix, one in Kennedy's speech
        for (name, want) in [("sortformer-2spk-mix.wav", 2), ("multitalker-2spk-mix.wav", 2), ("jfk.wav", 1)] {
            let audio = read(name);
            let started = std::time::Instant::now();
            let turns = e.speakers(&speakers.display().to_string(), &audio, &AtomicBool::new(false)).unwrap();
            let mut who: Vec<u32> = turns.iter().filter(|t| t.end - t.start >= 0.3).map(|t| t.speaker).collect();
            who.sort_unstable();
            who.dedup();
            let secs = audio.samples.len() as f64 / crate::audio::SR as f64;
            println!("{name}: {} turns, speakers {who:?}, {secs:.1} s of sound in {:.2} s", turns.len(), started.elapsed().as_secs_f64());
            assert_eq!(who.len(), want, "{name}: {turns:?}");
            e.forget(&audio.id);
        }

        // a stop ends a long run at once
        let long = read("dots-full.wav");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            s2.store(true, Ordering::SeqCst);
        });
        let started = std::time::Instant::now();
        let stopped = e.speakers(&speakers.display().to_string(), &long, &stop);
        println!("stopped after {:.2} s: {:?}", started.elapsed().as_secs_f64(), stopped.as_ref().map(|t| t.len()));
        assert!(stopped.is_err_and(|x| x.is::<Stopped>()), "a stop ends it as stopped");
        e.forget(&long.id);

        // which language: each of the samples, among every language Dawwin knows
        let every: Vec<&str> = crate::models::languages().collect();
        for (name, want) in [("ar-short.wav", "ar"), ("fleurs-en.wav", "en"), ("fleurs-de.wav", "de"), ("fleurs-fr.wav", "fr"), ("fleurs-es.wav", "es"), ("ru-short.wav", "ru"), ("ja-short.wav", "ja"), ("zh-short.wav", "zh")] {
            let audio = read(name);
            let ranges = crate::language::first_speech(&audio.samples);
            let clip = Audio { id: format!("{}-language", audio.id), samples: crate::language::joined(&audio.samples, &ranges) };
            let heard = e.language(&language.display().to_string(), &clip, &[(0, clip.samples.len())]).unwrap();
            e.forget(&clip.id);
            let d = crate::language::decide(&heard, &every).unwrap();
            println!("{name}: {} ({:.2}), next {:?}", d.language, heard[0].p, heard.get(1).map(|h| (&h.code, h.p)));
            assert_eq!(d.language, want, "{name}");
            // with only the Arabic model on the disk, German is still told for what it is
            if want == "de" {
                assert_eq!(crate::language::decide(&heard, &["ar", "en"]).unwrap(), crate::language::Decision { language: "de".into(), name: d.name.clone(), has_model: false });
            }
        }
        e.shutdown();
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_noise_marker_is_not_text() {
        assert_eq!(without_markers("@@@ضوضاء"), "");
        assert_eq!(without_markers("طيب @@@ضوضاء تمام"), "طيب تمام");
        assert_eq!(without_markers("  طيب  تمام "), "طيب  تمام");
    }
}
