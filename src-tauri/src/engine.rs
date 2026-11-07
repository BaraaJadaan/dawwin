//! The speech-to-text engine, behind one interface.
//!
//! Today the only engine is [`PythonWorker`]: Cohere Transcribe Arabic running in
//! Transformers/PyTorch in a child process. A native engine (ONNX via `ort`, or a
//! `candle` port) can replace it by implementing [`Engine`]; nothing else changes.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub device: String,
    pub gpu: Option<String>,
    pub vram_used_gb: Option<f64>,
    /// "bfloat16" | "int8"
    pub dtype: Option<String>,
    /// Why it runs on the CPU although a GPU was wanted
    pub fallback: Option<String>,
}

pub trait Engine: Send {
    /// Start loading the model without waiting (so decoding can overlap with it).
    fn start_loading(&mut self) -> Result<()>;
    /// Load the model if needed and say where it runs.
    fn ensure_loaded(&mut self) -> Result<EngineInfo>;
    /// Transcribe the sample ranges `[start, end)` of `audio` as one batch: one text per range.
    fn transcribe(&mut self, audio: &Audio, ranges: &[(usize, usize)], language: &str) -> Result<Vec<String>>;
    /// Let the audio's cached data go.
    fn forget(&mut self, audio_id: &str);
}

/// Errors that mean "try again with a smaller batch".
#[derive(Debug)]
pub struct OutOfMemory(pub String);
impl std::fmt::Display for OutOfMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "out of GPU memory: {}", self.0)
    }
}
impl std::error::Error for OutOfMemory {}

pub type EventSink = Arc<dyn Fn(&Value) + Send + Sync>;

#[derive(Default)]
pub struct WorkerOptions {
    /// No network: set when the model is on disk
    pub offline: bool,
    /// The folder the model loads from
    pub model_dir: Option<PathBuf>,
    /// A Hugging Face token the user gave the app (only needed to download)
    pub token: Option<String>,
    /// "auto" | "cpu-fast" | "cpu-exact"
    pub run_on: String,
}

pub struct PythonWorker {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<Value>,
    next_id: u64,
    pending_load: Option<u64>,
    loaded: Option<EngineInfo>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    cache_dir: PathBuf,
    raw_files: Vec<(String, PathBuf)>,
    /// When set, a request being waited on is abandoned and the process killed.
    pub abort: Option<Arc<AtomicBool>>,
}

impl PythonWorker {
    /// Start the worker. `offline` stops Hugging Face from touching the network
    /// (set it once the model is in the cache).
    pub fn spawn(paths: &Paths, opts: WorkerOptions, on_event: EventSink) -> Result<Self> {
        paths.install_worker_files()?;
        let mut cmd = Command::new(paths.python());
        cmd.arg("-u")
            .arg(paths.worker_py())
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("TOKENIZERS_PARALLELISM", "false")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            // plain HTTP downloads resume from their .incomplete files after a dropped connection
            .env("HF_HUB_DISABLE_XET", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if opts.offline {
            cmd.env("HF_HUB_OFFLINE", "1").env("TRANSFORMERS_OFFLINE", "1");
        } else {
            cmd.env_remove("HF_HUB_OFFLINE").env_remove("TRANSFORMERS_OFFLINE");
        }
        match &opts.model_dir {
            Some(d) => cmd.env("TAFREEGH_MODEL", d),
            None => cmd.env_remove("TAFREEGH_MODEL"),
        };
        if let Some(t) = &opts.token {
            cmd.env("HF_TOKEN", t);
        }
        cmd.env("TAFREEGH_LANG", crate::i18n::lang());
        match opts.run_on.as_str() {
            "cpu-fast" => cmd.env("TAFREEGH_DEVICE", "cpu").env("TAFREEGH_CPU_MODE", "int8"),
            "cpu-exact" => cmd.env("TAFREEGH_DEVICE", "cpu").env("TAFREEGH_CPU_MODE", "bf16"),
            _ => cmd.env_remove("TAFREEGH_DEVICE").env("TAFREEGH_CPU_MODE", "int8"),
        };
        no_window(&mut cmd);
        let mut child = cmd.spawn().with_context(|| crate::tr!("cannot start {}", paths.python().display()))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (tx, replies) = mpsc::channel::<Value>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if v.get("event").is_some() {
                    on_event(&v);
                } else if tx.send(v).is_err() {
                    break;
                }
            }
        });

        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(64)));
        let tail = stderr_tail.clone();
        let log_path = paths.logs_dir().join("worker.log");
        std::thread::spawn(move || {
            let mut log = std::fs::File::create(log_path).ok();
            for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                if let Some(f) = log.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                // tqdm redraws with \r; keep only what a human would see last
                let shown = line.rsplit('\r').next().unwrap_or("").trim().to_string();
                if shown.is_empty() {
                    continue;
                }
                let mut t = tail.lock().unwrap();
                if t.len() == 60 {
                    t.pop_front();
                }
                t.push_back(shown);
            }
        });

        Ok(PythonWorker {
            child,
            stdin,
            replies,
            next_id: 1,
            pending_load: None,
            loaded: None,
            stderr_tail,
            cache_dir: paths.cache_dir(),
            raw_files: Vec::new(),
            abort: None,
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
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|e| anyhow!(crate::tr!("the engine stopped ({}). Last output:\n{}", e, self.tail())))?;
        Ok(id)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn wait(&mut self, id: u64) -> Result<Value> {
        loop {
            if self.abort.as_ref().is_some_and(|a| a.load(Ordering::SeqCst)) {
                let _ = self.child.kill();
                bail!("stopped");
            }
            match self.replies.recv_timeout(Duration::from_secs(1)) {
                Ok(v) => {
                    if v.get("id").and_then(|i| i.as_u64()) != Some(id) {
                        continue; // a reply to a request nobody waits for any more
                    }
                    if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
                        return Ok(v);
                    }
                    let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error").to_string();
                    if v.get("oom").and_then(|o| o.as_bool()) == Some(true) {
                        return Err(OutOfMemory(err).into());
                    }
                    bail!("{err}");
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Ok(Some(status)) = self.child.try_wait() {
                        bail!(crate::tr!("the engine exited ({}). Last output:\n{}", status, self.tail()));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = self.child.wait();
                    bail!(crate::tr!("the engine exited. Last output:\n{}", self.tail()));
                }
            }
        }
    }

    pub fn call(&mut self, req: Value) -> Result<Value> {
        let id = self.send(req)?;
        self.wait(id)
    }

    pub fn probe(&mut self) -> Result<Value> {
        self.call(json!({"cmd": "probe"}))
    }

    pub fn download(&mut self, dest: &std::path::Path) -> Result<Value> {
        self.call(json!({"cmd": "download", "local_dir": dest}))
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

impl Drop for PythonWorker {
    fn drop(&mut self) {
        for (_, p) in self.raw_files.drain(..) {
            let _ = std::fs::remove_file(p);
        }
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
    }
}

fn info_from(v: &Value) -> EngineInfo {
    EngineInfo {
        device: v.get("device").and_then(|d| d.as_str()).unwrap_or("?").to_string(),
        gpu: v.get("gpu").and_then(|d| d.as_str()).map(String::from),
        vram_used_gb: v.get("vram_used_gb").and_then(|d| d.as_f64()),
        dtype: v.get("dtype").and_then(|d| d.as_str()).map(String::from),
        fallback: v.get("fallback").and_then(|d| d.as_str()).map(String::from),
    }
}

impl Engine for PythonWorker {
    fn start_loading(&mut self) -> Result<()> {
        if self.loaded.is_none() && self.pending_load.is_none() {
            self.pending_load = Some(self.send(json!({"cmd": "load"}))?);
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
        let info = info_from(&v);
        self.loaded = Some(info.clone());
        Ok(info)
    }

    fn transcribe(&mut self, audio: &Audio, ranges: &[(usize, usize)], language: &str) -> Result<Vec<String>> {
        self.ensure_loaded()?;
        let path = self.raw_file(audio)?;
        let v = self.call(json!({
            "cmd": "transcribe",
            "path": path,
            "ranges": ranges.iter().map(|(a, b)| [*a, *b]).collect::<Vec<_>>(),
            "language": language,
        }))?;
        let texts: Vec<String> = v
            .get("texts")
            .and_then(|t| t.as_array())
            .ok_or_else(|| anyhow!("engine reply has no texts"))?
            .iter()
            .map(|t| t.as_str().unwrap_or("").to_string())
            .collect();
        if texts.len() != ranges.len() {
            bail!("engine returned {} texts for {} chunks", texts.len(), ranges.len());
        }
        Ok(texts)
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
}
