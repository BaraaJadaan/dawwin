//! Translating transcripts: a translation model through llama.cpp's server, in a process of its own.
//!
//! `llama-server.exe` ships in `engine\translate`, with its own copy of the GPU libraries, apart
//! from the speech engine's. As with the speech engine, a crash in there (a GPU driver, say) ends
//! that process and not the app, and quitting it gives all its memory back. It listens on
//! 127.0.0.1 only, on a free port, and answers only requests that carry the key it was started with.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::engine::EngineFailed;
use crate::models::Prompt;
use crate::setup::{no_window, Paths};

/// Where it runs: on one GPU, or on the CPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
    /// The GPU's name, when it runs on one
    pub gpu: Option<String>,
}

impl Placement {
    /// What a translation says it ran on: "GPU · NVIDIA …" or "CPU", as transcripts do.
    pub fn label(&self) -> String {
        match &self.gpu {
            Some(g) => format!("GPU · {g}"),
            None => "CPU".into(),
        }
    }
}

/// The devices `llama-server --list-devices` names: "  Vulkan0: NVIDIA GeForce RTX 4060 Laptop GPU (7956 MiB, 7179 MiB free)".
fn parse_devices(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter_map(|line| {
            let (id, rest) = line.trim().split_once(": ")?;
            if id.is_empty() || id.contains(' ') || !rest.ends_with(" MiB free)") {
                return None;
            }
            let name = rest.rsplit_once(" (")?.0.trim();
            Some((id.to_string(), name.to_string()))
        })
        .collect()
}

/// Which device to run on: the GPU the speech engine found best (it knows a dedicated one from one
/// built into the processor, which llama.cpp's list does not say), else the first one. Left to
/// itself, llama.cpp would spread the model over all of them, the slow ones too.
fn choose_device<'a>(devices: &'a [(String, String)], preferred: Option<&str>) -> Option<&'a (String, String)> {
    let preferred = preferred.map(str::trim).filter(|p| !p.is_empty());
    preferred
        .and_then(|p| devices.iter().find(|(_, name)| name == p || name.contains(p) || p.contains(name.as_str())))
        .or_else(|| devices.first())
}

/// The GPUs llama.cpp can use here, by its own names for them.
fn list_devices(exe: &Path) -> Vec<(String, String)> {
    let mut cmd = Command::new(exe);
    cmd.arg("--list-devices").stdin(Stdio::null());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    no_window(&mut cmd);
    match cmd.output() {
        Ok(out) => parse_devices(&(String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))),
        Err(_) => Vec::new(),
    }
}

/// How far a request has come: tokens of the prompt read (of how many), and tokens written.
#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub prompt_done: u64,
    pub prompt_total: u64,
    pub written: u64,
}

/// What a request for JSON gave back: the text, and why it ended ("stop", or "length" when it ran
/// out of room).
#[derive(Clone, Debug, Default)]
pub struct JsonReply {
    pub content: String,
    pub finish: String,
    pub prompt_tokens: u64,
    pub tokens: u64,
}

pub struct TranslatorProcess {
    child: Child,
    base: String,
    key: String,
    agent: ureq::Agent,
    /// The model file it holds
    pub model: String,
    /// How many tokens it has room for
    pub context: u32,
    /// It kept the GPU out
    pub cpu_only: bool,
    /// The model is loaded and the server answers
    pub ready: bool,
    pub placement: Placement,
    tail: Arc<Mutex<VecDeque<String>>>,
}

/// The translator ends when the app ends, however it ends: after a crash, or from the Task
/// Manager, no server stays behind holding the GPU's memory. (The speech engine ends by itself
/// when its stdin closes; llama-server has nothing like that.) The processes in a job made to
/// "kill on job close" go when its last handle does, and the system closes it with the app.
#[cfg(windows)]
fn end_with_the_app(child: &Child) {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        per_process_user_time: i64,
        per_job_user_time: i64,
        limit_flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io_counters: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(job: *mut c_void, class: i32, info: *const c_void, length: u32) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    }
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;

    // one job for the app's whole life; its handle is never closed by hand
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| {
        // SAFETY: plain calls with valid pointers; `limits` outlives the call that reads it.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() {
                return 0;
            }
            let mut limits = ExtendedLimits::default();
            limits.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of::<ExtendedLimits>() as u32;
            if SetInformationJobObject(job, JOB_OBJECT_EXTENDED_LIMIT_INFORMATION, &limits as *const _ as *const c_void, size) == 0 {
                return 0;
            }
            job as usize
        }
    });
    if job != 0 {
        // SAFETY: the child's handle is valid while `child` lives, and the job's for the whole run.
        unsafe {
            AssignProcessToJobObject(job as *mut c_void, child.as_raw_handle() as *mut c_void);
        }
    }
}

#[cfg(not(windows))]
fn end_with_the_app(_child: &Child) {}

/// A port nobody listens on now.
fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// A key no other program can guess: the standard library seeds its hashers from the system's
/// random source.
fn random_key() -> String {
    let mut key = String::new();
    for _ in 0..4 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(std::process::id() as u64);
        key.push_str(&format!("{:016x}", h.finish()));
    }
    key
}

impl TranslatorProcess {
    /// Start the server with a model, on the GPU named `gpu` (the speech engine's choice) when it
    /// is there; `cpu_only` keeps the GPU out of it. It loads in the background:
    /// [`TranslatorProcess::wait_ready`] waits for it.
    pub fn spawn(exe: &Path, paths: &Paths, model: &str, cpu_only: bool, gpu: Option<&str>) -> Result<Self> {
        // one line at a time, and a line is short: 30 seconds of speech
        Self::spawn_with(exe, paths, model, cpu_only, gpu, 4096)
    }

    /// The same, with room for `context` tokens: the minutes read ten minutes of a transcript at once.
    pub fn spawn_with(exe: &Path, paths: &Paths, model: &str, cpu_only: bool, gpu: Option<&str>, context: u32) -> Result<Self> {
        let device = if cpu_only { None } else { choose_device(&list_devices(exe), gpu).cloned() };
        let port = free_port()?;
        let key = random_key();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let mut cmd = Command::new(exe);
        cmd.args(["--model", model, "--host", "127.0.0.1", "--port", &port.to_string(), "--api-key", &key]);
        cmd.args(["--ctx-size", &context.to_string(), "--parallel", "1", "--threads", &threads.to_string(), "--no-ui"]);
        // one device; how much of the model fits on it, llama.cpp works out by itself
        cmd.args(["--device", device.as_ref().map_or("none", |(id, _)| id.as_str())]);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // ggml's loader also looks for its libraries in the working folder: only its own
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        no_window(&mut cmd);
        let mut child = cmd.spawn().with_context(|| crate::tr!("cannot start {}", exe.display()))?;
        end_with_the_app(&child);

        let tail = Arc::new(Mutex::new(VecDeque::with_capacity(400)));
        let log = Arc::new(Mutex::new(std::fs::File::create(paths.logs_dir().join("translate.log")).ok()));
        for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)].into_iter().flatten() {
            let (tail, log) = (tail.clone(), log.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(|l| l.ok()) {
                    if let Some(f) = log.lock().unwrap().as_mut() {
                        let _ = writeln!(f, "{line}");
                    }
                    let mut t = tail.lock().unwrap();
                    if t.len() == 400 {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            });
        }
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            // a line of a transcript takes seconds; on a slow CPU with the larger model, a minute or two
            .timeout_read(Duration::from_secs(600))
            .build();
        Ok(TranslatorProcess {
            child,
            base: format!("http://127.0.0.1:{port}"),
            key,
            agent,
            model: model.to_string(),
            context,
            cpu_only,
            ready: false,
            placement: Placement { gpu: device.map(|(_, name)| name) },
            tail,
        })
    }

    fn last_lines(&self, n: usize) -> String {
        let t = self.tail.lock().unwrap();
        let skip = t.len().saturating_sub(n);
        t.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Wait until the model is loaded and the server answers.
    pub fn wait_ready(&mut self, stop: &AtomicBool) -> Result<()> {
        if self.ready {
            return Ok(());
        }
        let started = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(EngineFailed(crate::tr!("the translator exited ({}). Last output:\n{}", status, self.last_lines(12))).into());
            }
            if stop.load(Ordering::SeqCst) {
                return Err(anyhow!(crate::tr!("Cancelled.")));
            }
            match self.agent.get(&format!("{}/health", self.base)).timeout(Duration::from_secs(5)).call() {
                Ok(r) if r.status() == 200 => break,
                _ => {}
            }
            if started.elapsed() > Duration::from_secs(15 * 60) {
                return Err(EngineFailed(crate::tr!("the translator did not start. Last output:\n{}", self.last_lines(12))).into());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        self.ready = true;
        Ok(())
    }

    /// One line of a transcript in another language.
    pub fn translate(&mut self, prompt: Prompt, text: &str, from: &str, to: &str) -> Result<String> {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return Ok(String::new());
        }
        let (body_prompt, stop) = prompt.build(&text, from, to);
        // room for a translation twice as long as the line, in tokens of a few characters each
        let limit = (text.chars().count() * 2 / 3 + 64).min(2048);
        let body = json!({
            "prompt": body_prompt,
            "n_predict": limit,
            "temperature": 0.0,
            "top_k": 1,
            "stop": stop,
            "cache_prompt": false,
        });
        let reply = self
            .agent
            .post(&format!("{}/completion", self.base))
            .set("Authorization", &format!("Bearer {}", self.key))
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());
        let reply = match reply {
            Ok(r) => r,
            // the server answered: an error about this line, not the GPU's
            Err(ureq::Error::Status(code, r)) => {
                let detail = r.into_string().unwrap_or_default();
                return Err(anyhow!("translator error {code}: {detail}"));
            }
            // only a server that died counts against the GPU
            Err(_) if !self.alive() => {
                return Err(EngineFailed(crate::tr!("the translator exited. Last output:\n{}", self.last_lines(12))).into());
            }
            Err(e) => return Err(anyhow!("{e}")),
        };
        let v: Value = serde_json::from_str(&reply.into_string()?)?;
        let out = v.get("content").and_then(Value::as_str).unwrap_or_default().trim().to_string();
        // a model that runs on to the limit repeats itself: keep what came before the loop
        let ran_out = v.get("stop_type").and_then(Value::as_str) == Some("limit");
        Ok(if ran_out { crate::transcript::collapse_loops(&out) } else { out })
    }

    /// Ask the model, in its own chat format, for JSON held to `schema` (its text, sent as it is so
    /// that its fields keep their order), greedily and without thinking, as `tools/bench-minutes`
    /// measured. The answer comes as it is written: `on` hears how far it is. A stop ends the
    /// server, which is the one way to end a request that is still reading its prompt.
    pub fn ask_json(&mut self, messages: &Value, schema: &str, max_tokens: u32, stop: &AtomicBool, on: &mut dyn FnMut(Progress)) -> Result<JsonReply> {
        let mut body = json!({
            "messages": messages,
            "max_tokens": max_tokens,
            "temperature": 0,
            "top_k": 1,
            "cache_prompt": false,
            "stream": true,
            "return_progress": true,
            "chat_template_kwargs": { "enable_thinking": false },
        })
        .to_string();
        body.pop();
        body.push_str(&format!(",\"response_format\":{{\"type\":\"json_schema\",\"json_schema\":{{\"name\":\"minutes\",\"schema\":{schema}}}}}}}"));
        // a slow processor reads a long prompt for minutes before it writes a word
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout_read(Duration::from_secs(45 * 60)).build();
        let request = agent
            .post(&format!("{}/v1/chat/completions", self.base))
            .set("Authorization", &format!("Bearer {}", self.key))
            .set("Content-Type", "application/json");
        let (tx, rx) = std::sync::mpsc::channel::<Result<Value, String>>();
        std::thread::spawn(move || {
            let reply = match request.send_string(&body) {
                Ok(r) => r,
                Err(ureq::Error::Status(code, r)) => {
                    let _ = tx.send(Err(format!("{code}: {}", r.into_string().unwrap_or_default())));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("{e}")));
                    return;
                }
            };
            for line in BufReader::new(reply.into_reader()).lines() {
                let Ok(line) = line else {
                    let _ = tx.send(Err("the connection closed".into()));
                    return;
                };
                let Some(data) = line.strip_prefix("data: ") else { continue };
                if data.trim() == "[DONE]" {
                    break;
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if tx.send(Ok(v)).is_err() {
                        return;
                    }
                }
            }
        });
        let mut reply = JsonReply::default();
        let mut progress = Progress::default();
        loop {
            if stop.load(Ordering::SeqCst) {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return Err(anyhow!(crate::tr!("Cancelled.")));
            }
            let chunk = match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(chunk)) => chunk,
                Ok(Err(detail)) => {
                    if !self.alive() {
                        return Err(EngineFailed(crate::tr!("the translator exited. Last output:
{}", self.last_lines(12))).into());
                    }
                    return Err(anyhow!("translator error {detail}"));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !self.alive() {
                        return Err(EngineFailed(crate::tr!("the translator exited. Last output:
{}", self.last_lines(12))).into());
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if let Some(p) = chunk.get("prompt_progress") {
                progress.prompt_total = p.get("total").and_then(Value::as_u64).unwrap_or(0);
                progress.prompt_done = p.get("processed").and_then(Value::as_u64).unwrap_or(0);
                on(progress);
            }
            let choice = &chunk["choices"][0];
            if let Some(text) = choice["delta"]["content"].as_str() {
                reply.content.push_str(text);
                progress.written += 1;
                on(progress);
            }
            if let Some(f) = choice["finish_reason"].as_str() {
                reply.finish = f.to_string();
            }
            if let Some(t) = chunk.get("timings") {
                reply.prompt_tokens = t.get("prompt_n").and_then(Value::as_u64).unwrap_or(0);
                reply.tokens = t.get("predicted_n").and_then(Value::as_u64).unwrap_or(progress.written);
            }
        }
        Ok(reply)
    }

    pub fn shutdown(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TranslatorProcess {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What llama.cpp b12237 printed on this laptop, with a GPU of its own and one in the processor.
    const LISTING: &str = "Available devices:\n  Vulkan0: NVIDIA GeForce RTX 4060 Laptop GPU (7956 MiB, 7179 MiB free)\n  Vulkan1: AMD Radeon(TM) Graphics (8060 MiB, 7657 MiB free)\n";

    #[test]
    fn the_devices_are_read_from_the_listing() {
        let d = parse_devices(LISTING);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], ("Vulkan0".to_string(), "NVIDIA GeForce RTX 4060 Laptop GPU".to_string()));
        assert_eq!(d[1].1, "AMD Radeon(TM) Graphics");
        assert!(parse_devices("Available devices:\n").is_empty());
        assert!(parse_devices("error: something: went wrong").is_empty());
    }

    #[test]
    fn it_runs_on_the_gpu_the_speech_engine_chose() {
        let d = parse_devices(LISTING);
        // the one in the processor reports more memory: it must not win on that
        assert_eq!(choose_device(&d, Some("NVIDIA GeForce RTX 4060 Laptop GPU")).unwrap().0, "Vulkan0");
        assert_eq!(choose_device(&d, Some("AMD Radeon(TM) Graphics")).unwrap().0, "Vulkan1");
        assert_eq!(choose_device(&d, None).unwrap().0, "Vulkan0", "no choice made: the first");
        assert_eq!(choose_device(&d, Some("Some other GPU")).unwrap().0, "Vulkan0");
        assert!(choose_device(&[], Some("NVIDIA GeForce RTX 4060 Laptop GPU")).is_none(), "none: the CPU");
        assert_eq!(Placement { gpu: Some("X".into()) }.label(), "GPU · X");
        assert_eq!(Placement::default().label(), "CPU");
    }

    #[test]
    fn every_key_is_different() {
        let (a, b) = (random_key(), random_key());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    /// The real thing: `DAWWIN_TEST_TRANSLATOR=<llama-server.exe> DAWWIN_TEST_MODEL_FILE=<.gguf>
    /// cargo test real_translation -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_translation() {
        let exe = std::path::PathBuf::from(std::env::var("DAWWIN_TEST_TRANSLATOR").expect("DAWWIN_TEST_TRANSLATOR"));
        let model = std::env::var("DAWWIN_TEST_MODEL_FILE").expect("DAWWIN_TEST_MODEL_FILE");
        let root = std::env::temp_dir().join(format!("dawwin-tr-{}", std::process::id()));
        std::fs::create_dir_all(root.join("logs")).unwrap();
        let paths = Paths { local: root.clone(), roaming: root };
        let cpu_only = std::env::var_os("DAWWIN_TEST_CPU").is_some();
        let gpu = std::env::var("DAWWIN_TEST_GPU").ok();
        let mut t = TranslatorProcess::spawn(&exe, &paths, &model, cpu_only, gpu.as_deref()).unwrap();
        let started = Instant::now();
        t.wait_ready(&AtomicBool::new(false)).unwrap();
        println!("ready in {:.1} s on {}", started.elapsed().as_secs_f32(), t.placement.label());
        for (text, from, to) in [
            ("طيب خلينا نبدأ الاجتماع، عندنا ثلاث نقاط اليوم بخصوص الشاشة الرئيسية", "ar", "en"),
            ("We need the login page ready by Thursday.", "en", "ar"),
            ("Vi ses i morgen klokken ti.", "da", "en"),
        ] {
            let started = Instant::now();
            let out = t.translate(Prompt::MiLMMT, text, from, to).unwrap();
            println!("{from}->{to} in {:.1} s: {text} => {out}", started.elapsed().as_secs_f32());
            assert!(!out.is_empty());
            assert_ne!(out, text);
        }
        t.shutdown();
    }
}
