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

pub struct TranslatorProcess {
    child: Child,
    base: String,
    key: String,
    agent: ureq::Agent,
    /// The model file it holds
    pub model: String,
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

/// Another program took the port between the moment it was free and the translator's start, and
/// answers on it: the transcript's lines must not go there. Start again on another port.
#[derive(Debug)]
pub struct PortTaken;
impl std::fmt::Display for PortTaken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("another program answers on the translator's port")
    }
}
impl std::error::Error for PortTaken {}

/// Which process listens on `port` of 127.0.0.1, from Windows' `netstat -ano` listing. None when
/// that cannot be told.
fn listener_of(listing: &str, port: u16) -> Option<u32> {
    let local = format!("127.0.0.1:{port}");
    listing.lines().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        (f.len() >= 5 && f[0] == "TCP" && f[1] == local && f[3] == "LISTENING").then(|| f[4].parse().ok()).flatten()
    })
}

fn listener_pid(port: u16) -> Option<u32> {
    let mut cmd = Command::new("netstat.exe");
    cmd.args(["-ano", "-p", "TCP"]).stdin(Stdio::null()).stderr(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    listener_of(&String::from_utf8_lossy(&out.stdout), port)
}

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
        let device = if cpu_only { None } else { choose_device(&list_devices(exe), gpu).cloned() };
        let port = free_port()?;
        let key = random_key();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let mut cmd = Command::new(exe);
        cmd.args(["--model", model, "--host", "127.0.0.1", "--port", &port.to_string(), "--api-key", &key]);
        // one line at a time, and a line is short: 30 seconds of speech
        cmd.args(["--ctx-size", "4096", "--parallel", "1", "--threads", &threads.to_string(), "--no-ui"]);
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
                Ok(r) if r.status() == 200 => {
                    // the one that answers must be ours, not a program that took the port first
                    let ours = self.child.id();
                    let port = self.base.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(0);
                    let exited = matches!(self.child.try_wait(), Ok(Some(_)));
                    if exited || listener_pid(port).is_some_and(|pid| pid != ours) {
                        let _ = self.child.kill();
                        return Err(PortTaken.into());
                    }
                    break;
                }
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

    #[test]
    fn the_program_on_a_port_is_read_from_netstat() {
        let listing = "\r\nActive Connections\r\n\r\n  Proto  Local Address          Foreign Address        State           PID\r\n  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1180\r\n  TCP    127.0.0.1:52811        0.0.0.0:0              LISTENING       4242\r\n  TCP    127.0.0.1:52811        127.0.0.1:60001        ESTABLISHED     4242\r\n  TCP    127.0.0.1:60001        127.0.0.1:52811        ESTABLISHED     900\r\n";
        assert_eq!(listener_of(listing, 52811), Some(4242));
        assert_eq!(listener_of(listing, 60001), None, "only who listens counts");
        assert_eq!(listener_of(listing, 135), None, "on this computer's own address only");
    }

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
