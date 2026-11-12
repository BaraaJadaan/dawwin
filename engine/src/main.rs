//! The speech engine: speech models (Cohere Transcribe, Canary, Whisper) through transcribe.cpp.
//!
//! One process holds one model. The app decides which model a recording needs, and starts a new
//! process to change it.
//!
//! It runs in a process of its own, so a crash in a GPU driver costs a retry on the CPU instead of
//! the whole app, and quitting it gives all its memory back. The app talks to it over stdin and
//! stdout, one JSON object per line. Anything else goes to stderr, which the app keeps as a log.
//!
//!   Requests: {"id": 1, "cmd": "probe" | "load" | "transcribe" | "quit", ...}
//!   Replies:  {"id": 1, "ok": true, ...} or {"id": 1, "ok": false, "error": "...", "oom": false, "device": false}
//!             `device`: the compute device failed (the app then stops using the GPU), as opposed
//!             to the request (a broken model file, a missing audio file).
//!
//!   load:       {"model": <.gguf file>} -> where it runs
//!   transcribe: {"path": <raw little-endian f32, 16 kHz mono>, "ranges": [[start, end], ...],
//!                "language": "ar" | "da" | ..., "vocabulary": ["Thmanyah", ...]}
//!                -> {"texts": [...], "stopped": [...]}, one per range.
//!                `stopped`: the model looped or ran out of tokens, so the text ends early.
//!                `vocabulary`: words the model should hear right, for a model that takes them
//!                (Whisper); the others go without.
//!   speakers:   {"model": <speaker model>, "path": <raw f32>} -> {"turns": [[start_ms, end_ms, speaker], ...],
//!                "max_speakers": 4}: who speaks when, speakers numbered from 1 in the order they
//!                first speak. Turns of different speakers may overlap.
//!   language:   {"model": <language model>, "path": <raw f32>, "ranges": [[start, end], ...]}
//!                -> {"candidates": [[code, name, p], ...]}: the ranges heard as one clip (at most
//!                its first 30 s count), every language the model knows, the likeliest first.
//!   stop:       {"target": <id>}: stop that request (a `speakers` one) as soon as it can; it then
//!                fails with `"stopped": true`. No reply of its own.
//!
//! The speaker and language models run on the processor, next to the speech model wherever that
//! runs: they are small, and never take the GPU's memory from it.
//!
//! `--cpu` keeps the GPU driver out of the process altogether.

use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};
use transcribe_cpp::{
    Backend, BackendMask, CancelToken, Device, DeviceType, DiarizeExtension, DiarizeOptions, DiarizeSession, DiarizeSessionOptions, Error, Feature,
    LangIdOptions, LangIdSession, LangIdSessionOptions, Model, ModelOptions, RunOptions, Session, SessionOptions, SortformerDiarizeOptions,
    SortformerPreset, TimestampKind,
};

/// The model cannot read less than this; it could not hold speech anyway.
const MIN_SAMPLES: usize = 160;
const GB: u64 = 1 << 30;
/// Pieces of audio read at once. Eight is about 1.7 times faster on a GPU, but the shorter pieces of
/// a batch get padded, and on a recitation the 2.4 GB model then went on past the end of the speech
/// and made up English sentences. One at a time gives the same words every time.
const BATCH: u32 = 1;

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
    /// The model takes a list of words to hear right
    vocabulary: bool,
}

/// A model of another role than transcribing, on the processor: the file it came from, so another
/// file replaces it.
struct Helper<S> {
    path: String,
    session: S,
    max_speakers: i32,
    min_samples: usize,
}

struct Engine {
    cpu_only: bool,
    backends_ready: bool,
    loaded: Option<Loaded>,
    speakers: Option<Helper<DiarizeSession>>,
    language: Option<Helper<LangIdSession>>,
}

/// Which request runs now, and a stop that came for one: a stop can arrive before its request
/// starts, while it runs, or just after it ended.
#[derive(Default)]
struct Stops {
    running: Option<(Value, CancelToken)>,
    wanted: Option<Value>,
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
                    Ok((session, vocabulary)) => {
                        let status = json!({
                            "device": "gpu",
                            "gpu": gpu.description,
                            "vram_total_gb": gigabytes(gpu.memory_total),
                            "batch": BATCH,
                            "load_seconds": started.elapsed().as_secs_f32(),
                        });
                        log::info!("loaded on {status}");
                        self.loaded = Some(Loaded { session, status: status.clone(), vocabulary });
                        return Ok(status);
                    }
                    Err(e) => {
                        log::warn!("the GPU could not load the model: {e}");
                        fallback = Some("gpu-failed".to_string());
                    }
                }
            }
        }
        let (session, vocabulary) = open(model_path, None)?;
        let status = json!({
            "device": "cpu",
            "batch": BATCH,
            "fallback": fallback,
            "load_seconds": started.elapsed().as_secs_f32(),
        });
        log::info!("loaded on {status}");
        self.loaded = Some(Loaded { session, status: status.clone(), vocabulary });
        Ok(status)
    }

    fn transcribe(&mut self, req: &Value) -> Result<Value, Error> {
        if self.loaded.is_none() {
            self.load(req)?;
        }
        let loaded = self.loaded.as_mut().expect("just loaded");
        let session = &mut loaded.session;
        let vocabulary: Vec<String> = if loaded.vocabulary { strings(req.get("vocabulary")) } else { Vec::new() };
        let path = req.get("path").and_then(Value::as_str).unwrap_or_default();
        let language = req.get("language").and_then(Value::as_str).unwrap_or("ar");
        let ranges = ranges_of(req);

        let pcm = read_ranges(path, &ranges).map_err(|e| Error::InvalidArgument(format!("{path}: {e}")))?;
        // too short for the model: it would fail the whole batch, and it holds no speech
        let usable: Vec<usize> = (0..pcm.len()).filter(|&i| pcm[i].len() >= MIN_SAMPLES).collect();
        let mut texts = vec![String::new(); pcm.len()];
        let mut stopped = vec![false; pcm.len()];
        if !usable.is_empty() {
            let options = RunOptions {
                language: Some(language.to_string()),
                timestamps: TimestampKind::None,
                vocabulary,
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

    /// Who speaks when in the whole file: Sortformer at its offline operating point (it looks about
    /// 30 s ahead), which is the one NVIDIA's accuracy figures are for.
    fn speakers(&mut self, req: &Value, stop: &CancelToken) -> Result<Value, Error> {
        self.backends()?;
        let model_path = req.get("model").and_then(Value::as_str).unwrap_or_default();
        if self.speakers.as_ref().is_some_and(|h| h.path != model_path) {
            self.speakers = None;
        }
        if self.speakers.is_none() {
            let model = load_on_cpu(model_path)?;
            let info = model.diarize_info()?;
            let session = model.diarize_session_with(&DiarizeSessionOptions { n_threads: cpu_threads() })?;
            self.speakers = Some(Helper { path: model_path.to_string(), session, max_speakers: info.max_speakers, min_samples: 0 });
        }
        let helper = self.speakers.as_mut().expect("just loaded");
        let path = req.get("path").and_then(Value::as_str).unwrap_or_default();
        let pcm = read_all(path).map_err(|e| Error::InvalidArgument(format!("{path}: {e}")))?;
        if pcm.len() < MIN_SAMPLES {
            return Ok(json!({ "turns": [], "max_speakers": helper.max_speakers }));
        }
        let started = Instant::now();
        helper.session.set_cancel_token(stop);
        let options = DiarizeOptions {
            family: Some(DiarizeExtension::Sortformer(SortformerDiarizeOptions { preset: Some(SortformerPreset::VeryHighLatency) })),
        };
        let result = helper.session.run(&pcm, &options);
        helper.session.clear_cancel_token();
        let mut turns: Vec<[i64; 3]> = result?.iter().filter(|s| s.speaker_id > 0 && s.t1_ms > s.t0_ms).map(|s| [s.t0_ms, s.t1_ms, s.speaker_id as i64]).collect();
        turns.sort();
        log::info!("{} speaker turns in {:.1} s of sound, found in {:.1} s", turns.len(), pcm.len() as f64 / 16000.0, started.elapsed().as_secs_f64());
        Ok(json!({ "turns": turns, "max_speakers": helper.max_speakers }))
    }

    /// Which language the ranges are in, by every language the model knows. The app decides
    /// among the ones it has a model for, which the probabilities of all of them let it do.
    fn language(&mut self, req: &Value) -> Result<Value, Error> {
        self.backends()?;
        let model_path = req.get("model").and_then(Value::as_str).unwrap_or_default();
        if self.language.as_ref().is_some_and(|h| h.path != model_path) {
            self.language = None;
        }
        if self.language.is_none() {
            let model = load_on_cpu(model_path)?;
            let info = model.langid_info()?;
            let session = model.langid_session_with(&LangIdSessionOptions { n_threads: cpu_threads() })?;
            let min_samples = (info.min_audio_ms.max(0) as usize) * info.sample_rate.max(1) as usize / 1000;
            self.language = Some(Helper { path: model_path.to_string(), session, max_speakers: 0, min_samples });
        }
        let helper = self.language.as_mut().expect("just loaded");
        let path = req.get("path").and_then(Value::as_str).unwrap_or_default();
        let pcm: Vec<f32> = read_ranges(path, &ranges_of(req)).map_err(|e| Error::InvalidArgument(format!("{path}: {e}")))?.concat();
        if pcm.len() < helper.min_samples.max(MIN_SAMPLES) {
            return Ok(json!({ "candidates": [], "too_short": true }));
        }
        let result = helper.session.run(&pcm, &LangIdOptions { allowed: None })?;
        let candidates: Vec<Value> = result.candidates.iter().map(|c| json!([c.code, c.name, c.p])).collect();
        log::info!("language: {:?} in {:.1} s of speech", result.code(), pcm.len() as f64 / 16000.0);
        Ok(json!({ "candidates": candidates }))
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

/// The speech model's session, and whether it takes a list of words to hear right.
fn open(path: &str, gpu: Option<Device>) -> Result<(Session, bool), Error> {
    let on_gpu = gpu.is_some();
    let options = ModelOptions {
        backend: if on_gpu { Backend::Auto } else { Backend::Cpu },
        device: gpu,
        ..ModelOptions::default()
    };
    let model = Model::load_with(path, &options)?;
    log::info!("model: {}", model.arch());
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    // on the GPU the threads only feed it; on the CPU more than eight gains nothing
    let threads = if on_gpu { available.saturating_sub(2).max(1) } else { available.min(8) };
    let session = model.session_with(&SessionOptions { n_threads: threads as i32, ..SessionOptions::default() })?;
    Ok((session, model.supports(Feature::Vocabulary)))
}

/// Threads for a model on the processor: more than eight gains nothing.
fn cpu_threads() -> i32 {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(8) as i32
}

/// A model of another role, loaded on the processor.
fn load_on_cpu(path: &str) -> Result<Model, Error> {
    let model = Model::load_with(path, &ModelOptions { backend: Backend::Cpu, ..ModelOptions::default() })?;
    log::info!("model on the processor: {}", model.arch());
    Ok(model)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default()
}

fn ranges_of(req: &Value) -> Vec<(usize, usize)> {
    req.get("ranges")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(|p| Some((p.get(0)?.as_u64()? as usize, p.get(1)?.as_u64()? as usize))).collect())
        .unwrap_or_default()
}

/// The whole raw file.
fn read_all(path: &str) -> std::io::Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    Ok(bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
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
            json!({
                "id": id,
                "ok": false,
                "error": e.to_string(),
                "oom": matches!(e, Error::OutOfMemory(_)),
                "device": matches!(e, Error::Backend(_)),
                "stopped": matches!(e, Error::Aborted { .. }),
            })
        }
    }
}

/// A request that can be stopped while it runs: it gets a token of its own, already pulled when
/// its stop came first.
fn stoppable(stops: &Mutex<Stops>, id: &Value) -> CancelToken {
    let token = CancelToken::new();
    let mut s = stops.lock().unwrap();
    if s.wanted.as_ref() == Some(id) {
        token.cancel();
        s.wanted = None;
    }
    s.running = Some((id.clone(), token.clone()));
    token
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
        speakers: None,
        language: None,
    };
    // Requests are read on a thread of their own, so a stop reaches the one that runs.
    let stops = Arc::new(Mutex::new(Stops::default()));
    let (tx, rx) = mpsc::channel::<Value>();
    let reader_stops = stops.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
            if req.get("cmd").and_then(Value::as_str) == Some("stop") {
                let target = req.get("target").cloned().unwrap_or(Value::Null);
                let mut s = reader_stops.lock().unwrap();
                match &s.running {
                    Some((id, token)) if *id == target => token.cancel(),
                    _ => s.wanted = Some(target),
                }
                continue;
            }
            if tx.send(req).is_err() {
                return;
            }
        }
        // the app closed its end of the pipe: what runs stops, and the engine ends after it,
        // so none stays behind holding the GPU
        if let Some((_, token)) = &reader_stops.lock().unwrap().running {
            token.cancel();
        }
    });

    let mut stdout = std::io::stdout();
    for req in rx {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let answer = match req.get("cmd").and_then(Value::as_str) {
            Some("probe") => reply(&id, engine.probe()),
            Some("load") => reply(&id, engine.load(&req)),
            Some("transcribe") => reply(&id, engine.transcribe(&req)),
            Some("speakers") => {
                let token = stoppable(&stops, &id);
                let answer = reply(&id, engine.speakers(&req, &token));
                stops.lock().unwrap().running = None;
                answer
            }
            Some("language") => reply(&id, engine.language(&req)),
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
