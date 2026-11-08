//! The speech engine: Cohere Transcribe Arabic through transcribe.cpp.
//!
//! It runs in a process of its own, so a crash in a GPU driver costs a retry on the CPU instead of
//! the whole app, and quitting it gives all its memory back. The app talks to it over stdin and
//! stdout, one JSON object per line. Anything else goes to stderr, which the app keeps as a log.
//!
//!   Requests: {"id": 1, "cmd": "probe" | "load" | "transcribe" | "quit", ...}
//!   Replies:  {"id": 1, "ok": true, ...} or {"id": 1, "ok": false, "error": "...", "oom": false}
//!
//!   load:       {"model": <.gguf file>} -> where it runs
//!   transcribe: {"path": <raw little-endian f32, 16 kHz mono>, "ranges": [[start, end], ...],
//!                "language": "ar" | "en"} -> {"texts": [...], "stopped": [...]}, one per range.
//!                `stopped`: the model looped or ran out of tokens, so the text ends early.
//!
//! `--cpu` keeps the GPU driver out of the process altogether.

use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};
use transcribe_cpp::{Backend, BackendMask, Device, DeviceType, Error, Model, ModelOptions, RunOptions, Session, SessionOptions, TimestampKind};

/// The model cannot read less than this; it could not hold speech anyway.
const MIN_SAMPLES: usize = 160;
const GB: u64 = 1 << 30;

struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

struct Loaded {
    session: Session,
    status: Value,
}

struct Engine {
    cpu_only: bool,
    backends_ready: bool,
    loaded: Option<Loaded>,
}

impl Engine {
    /// Register the compute backends. Once per process, before anything looks at devices.
    fn backends(&mut self) -> Result<(), Error> {
        if !self.backends_ready {
            if self.cpu_only {
                transcribe_cpp::init_backends_with(None::<&Path>, BackendMask::for_backend(Backend::Cpu))?;
            } else {
                transcribe_cpp::init_backends_default()?;
            }
            self.backends_ready = true;
        }
        Ok(())
    }

    /// What this computer has to run on, without loading anything.
    fn probe(&mut self) -> Result<Value, Error> {
        self.backends()?;
        let gpu = best_gpu();
        Ok(json!({
            "cpu_only": self.cpu_only,
            "gpu": gpu.as_ref().map(|d| d.description.clone()),
            "vram_total_gb": gpu.as_ref().map(|d| gigabytes(d.memory_total)),
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    fn load(&mut self, req: &Value) -> Result<Value, Error> {
        if let Some(l) = &self.loaded {
            return Ok(l.status.clone());
        }
        self.backends()?;
        let model_path = req.get("model").and_then(Value::as_str).unwrap_or_default();
        let model_bytes = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);
        let started = Instant::now();

        let mut fallback = None;
        let gpu = best_gpu();
        if let Some(gpu) = gpu {
            if gpu.memory_total != 0 && gpu.memory_total < model_bytes + GB / 2 {
                fallback = Some("gpu-too-small".to_string());
            } else {
                match open(model_path, Some(gpu.clone())) {
                    Ok(session) => {
                        let status = json!({
                            "device": "gpu",
                            "gpu": gpu.description,
                            "vram_total_gb": gigabytes(gpu.memory_total),
                            "batch": gpu_batch(gpu.memory_total, model_bytes),
                            "load_seconds": started.elapsed().as_secs_f32(),
                        });
                        log::info!("loaded on {status}");
                        self.loaded = Some(Loaded { session, status: status.clone() });
                        return Ok(status);
                    }
                    Err(e) => {
                        log::warn!("the GPU could not load the model: {e}");
                        fallback = Some("gpu-failed".to_string());
                    }
                }
            }
        }
        let session = open(model_path, None)?;
        let status = json!({
            "device": "cpu",
            "batch": 1,
            "fallback": fallback,
            "load_seconds": started.elapsed().as_secs_f32(),
        });
        log::info!("loaded on {status}");
        self.loaded = Some(Loaded { session, status: status.clone() });
        Ok(status)
    }

    fn transcribe(&mut self, req: &Value) -> Result<Value, Error> {
        if self.loaded.is_none() {
            self.load(req)?;
        }
        let session = &mut self.loaded.as_mut().expect("just loaded").session;
        let path = req.get("path").and_then(Value::as_str).unwrap_or_default();
        let language = req.get("language").and_then(Value::as_str).unwrap_or("ar");
        let ranges: Vec<(usize, usize)> = req
            .get("ranges")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(|p| Some((p.get(0)?.as_u64()? as usize, p.get(1)?.as_u64()? as usize))).collect())
            .unwrap_or_default();

        let pcm = read_ranges(path, &ranges).map_err(|e| Error::InvalidArgument(format!("{path}: {e}")))?;
        // too short for the model: it would fail the whole batch, and it holds no speech
        let usable: Vec<usize> = (0..pcm.len()).filter(|&i| pcm[i].len() >= MIN_SAMPLES).collect();
        let mut texts = vec![String::new(); pcm.len()];
        let mut stopped = vec![false; pcm.len()];
        if !usable.is_empty() {
            let options = RunOptions {
                language: Some(language.to_string()),
                timestamps: TimestampKind::None,
                ..RunOptions::default()
            };
            let batch: Vec<&[f32]> = usable.iter().map(|&i| pcm[i].as_slice()).collect();
            let results = session.run_batch(&batch, &options)?;
            for (&i, result) in usable.iter().zip(results) {
                let (text, ended_early) = outcome(result)?;
                texts[i] = text;
                stopped[i] = ended_early;
            }
        }
        Ok(json!({ "texts": texts, "stopped": stopped }))
    }
}

/// The text of one piece of audio. The model stops by itself when it starts repeating or runs out
/// of tokens, and what it wrote until then is good: keep it, and say the rest of the audio is unread.
fn outcome(result: Result<transcribe_cpp::Transcript, Error>) -> Result<(String, bool), Error> {
    match result {
        Ok(t) => Ok((t.text.trim().to_string(), false)),
        Err(Error::OutputRepetition { partial, .. }) | Err(Error::OutputTruncated { partial, .. }) => {
            Ok((partial.map(|p| p.text.trim().to_string()).unwrap_or_default(), true))
        }
        Err(e) => Err(e),
    }
}

/// A dedicated GPU first, then the one with the most memory.
fn best_gpu() -> Option<Device> {
    transcribe_cpp::devices()
        .into_iter()
        .filter(|d| matches!(d.device_type, DeviceType::Gpu | DeviceType::Igpu))
        .max_by_key(|d| (matches!(d.device_type, DeviceType::Gpu), d.memory_total))
}

/// Pieces of audio read at once: more is faster, as long as they fit next to the model.
fn gpu_batch(memory_total: u64, model_bytes: u64) -> u32 {
    match memory_total.saturating_sub(model_bytes) {
        0 => 4, // the driver did not say
        free if free >= 2 * GB => 8,
        free if free >= GB => 4,
        _ => 2,
    }
}

fn open(path: &str, gpu: Option<Device>) -> Result<Session, Error> {
    let on_gpu = gpu.is_some();
    let options = ModelOptions {
        backend: if on_gpu { Backend::Auto } else { Backend::Cpu },
        device: gpu,
        ..ModelOptions::default()
    };
    let model = Model::load_with(path, &options)?;
    if model.arch() != "cohere_asr" {
        return Err(Error::ModelLoad(format!("{path} is not a Cohere Transcribe model ({})", model.arch())));
    }
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    // on the GPU the threads only feed it; on the CPU more than eight gains nothing
    let threads = if on_gpu { available.saturating_sub(2).max(1) } else { available.min(8) };
    model.session_with(&SessionOptions { n_threads: threads as i32, ..SessionOptions::default() })
}

fn read_ranges(path: &str, ranges: &[(usize, usize)]) -> std::io::Result<Vec<Vec<f32>>> {
    let mut file = std::fs::File::open(path)?;
    let mut out = Vec::with_capacity(ranges.len());
    for &(a, b) in ranges {
        file.seek(SeekFrom::Start(a as u64 * 4))?;
        let mut bytes = vec![0u8; b.saturating_sub(a) * 4];
        file.read_exact(&mut bytes)?;
        out.push(bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect());
    }
    Ok(out)
}

fn gigabytes(bytes: u64) -> f64 {
    (bytes as f64 / GB as f64 * 100.0).round() / 100.0
}

fn reply(id: &Value, result: Result<Value, Error>) -> Value {
    match result {
        Ok(mut v) => {
            v["id"] = id.clone();
            v["ok"] = json!(true);
            v
        }
        Err(e) => {
            log::error!("{e}");
            json!({ "id": id, "ok": false, "error": e.to_string(), "oom": matches!(e, Error::OutOfMemory(_)) })
        }
    }
}

fn main() {
    static LOGGER: StderrLog = StderrLog;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    transcribe_cpp::init_logging();

    let mut engine = Engine {
        cpu_only: std::env::args().any(|a| a == "--cpu"),
        backends_ready: false,
        loaded: None,
    };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    // the app closing its end of the pipe ends the loop: no engine stays behind holding the GPU
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let answer = match req.get("cmd").and_then(Value::as_str) {
            Some("probe") => reply(&id, engine.probe()),
            Some("load") => reply(&id, engine.load(&req)),
            Some("transcribe") => reply(&id, engine.transcribe(&req)),
            Some("quit") => {
                let _ = writeln!(stdout, "{}", json!({ "id": id, "ok": true }));
                let _ = stdout.flush();
                break;
            }
            other => json!({ "id": id, "ok": false, "error": format!("unknown command {other:?}") }),
        };
        if writeln!(stdout, "{answer}").and_then(|_| stdout.flush()).is_err() {
            break;
        }
    }
}
