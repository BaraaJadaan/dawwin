//! Where things live, which tools are installed, where the model is, and the one-time
//! setup of the Python env the inference worker runs in.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MODEL_ID: &str = "CohereLabs/cohere-transcribe-arabic-07-2026";
pub const MODEL_URL: &str = "https://huggingface.co/CohereLabs/cohere-transcribe-arabic-07-2026";
pub const TOKENS_URL: &str = "https://huggingface.co/settings/tokens";
pub const WORKER_PY: &str = include_str!("../../worker/worker.py");
pub const REQUIREMENTS: &str = include_str!("../../worker/requirements.txt");
const PYTHON_VERSION: &str = "3.12";
const UV_VERSION: &str = "0.11.25";

#[cfg(windows)]
pub fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
pub fn no_window(_cmd: &mut Command) {}

// ---------------------------------------------------------------- paths

#[derive(Clone, Debug)]
pub struct Paths {
    /// %LOCALAPPDATA%\Tafreegh: the Python env, the worker script, uv, caches, logs, the model by default
    pub local: PathBuf,
    /// %APPDATA%\Tafreegh: transcripts (history), settings
    pub roaming: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let local = dirs::data_local_dir().context("no LOCALAPPDATA")?.join("Tafreegh");
        let roaming = dirs::data_dir().context("no APPDATA")?.join("Tafreegh");
        let p = Paths { local, roaming };
        for d in [p.engine_dir(), p.cache_dir(), p.logs_dir(), p.history_dir()] {
            std::fs::create_dir_all(&d).with_context(|| format!("cannot create {}", d.display()))?;
        }
        Ok(p)
    }
    pub fn env_dir(&self) -> PathBuf {
        self.local.join("pyenv")
    }
    pub fn python(&self) -> PathBuf {
        self.env_dir().join("Scripts").join("python.exe")
    }
    pub fn engine_dir(&self) -> PathBuf {
        self.local.join("engine")
    }
    pub fn worker_py(&self) -> PathBuf {
        self.engine_dir().join("worker.py")
    }
    pub fn requirements(&self) -> PathBuf {
        self.engine_dir().join("requirements.txt")
    }
    pub fn bin_dir(&self) -> PathBuf {
        self.local.join("bin")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.local.join("cache")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.local.join("logs")
    }
    pub fn history_dir(&self) -> PathBuf {
        self.roaming.join("history")
    }
    pub fn default_model_dir(&self) -> PathBuf {
        self.local.join("models").join("cohere-transcribe-arabic-07-2026")
    }
    fn settings_file(&self) -> PathBuf {
        self.roaming.join("settings.json")
    }
    fn token_file(&self) -> PathBuf {
        self.roaming.join("hf-token")
    }
    fn env_marker(&self) -> PathBuf {
        self.env_dir().join("tafreegh-env.json")
    }

    /// Write the embedded worker script next to the env (it changes with app updates).
    pub fn install_worker_files(&self) -> Result<()> {
        write_if_changed(&self.worker_py(), WORKER_PY)?;
        write_if_changed(&self.requirements(), REQUIREMENTS)?;
        Ok(())
    }
}

fn write_if_changed(path: &Path, text: &str) -> Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() != Some(text) {
        std::fs::write(path, text)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// A model folder the user imported or downloaded into
    #[serde(default)]
    pub model_dir: Option<String>,
    /// Where "Download model" saves the files
    #[serde(default)]
    pub download_dir: Option<String>,
    /// "auto" (GPU when there is one) | "cpu-fast" (int8) | "cpu-exact" (bf16, much slower)
    #[serde(default)]
    pub run_on: Option<String>,
}

impl Settings {
    pub fn run_on(&self) -> &str {
        match self.run_on.as_deref() {
            Some("cpu-fast") => "cpu-fast",
            Some("cpu-exact") => "cpu-exact",
            _ => "auto",
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Settings {
        std::fs::read(paths.settings_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.roaming)?;
        std::fs::write(paths.settings_file(), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- Hugging Face token

/// The token the app was given, if any. Hugging Face's own login (`hf auth login`) or
/// HF_TOKEN are used by the worker without the app reading them.
pub fn app_token(paths: &Paths) -> Option<String> {
    let t = std::fs::read_to_string(paths.token_file()).ok()?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

pub fn save_token(paths: &Paths, token: &str) -> Result<()> {
    let t = token.trim();
    if !t.starts_with("hf_") || t.len() < 20 || t.chars().any(char::is_whitespace) {
        bail!("That does not look like a Hugging Face token (they start with “hf_”).");
    }
    std::fs::create_dir_all(&paths.roaming)?;
    std::fs::write(paths.token_file(), t)?;
    Ok(())
}

pub fn forget_token(paths: &Paths) -> Result<()> {
    let f = paths.token_file();
    if f.exists() {
        std::fs::remove_file(f)?;
    }
    Ok(())
}

/// "app" (pasted in Tafreegh), "env" (HF_TOKEN), "hf-login" (hf auth login), or "" (none)
pub fn token_source(paths: &Paths) -> &'static str {
    if app_token(paths).is_some() {
        "app"
    } else if std::env::var_os("HF_TOKEN").is_some_and(|v| !v.is_empty()) {
        "env"
    } else if hf_home().join("token").is_file() {
        "hf-login"
    } else {
        ""
    }
}

// ---------------------------------------------------------------- tools

/// Find an executable on PATH, in the app's own bin folder, then where winget, uv and cargo install to.
pub fn find_exe(name: &str) -> Option<PathBuf> {
    let file = format!("{name}.exe");
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(l) = dirs::data_local_dir() {
        dirs.push(l.join("Tafreegh").join("bin"));
        dirs.push(l.join("Microsoft").join("WinGet").join("Links"));
    }
    if let Some(h) = dirs::home_dir() {
        dirs.push(h.join(".local").join("bin"));
        dirs.push(h.join(".cargo").join("bin"));
    }
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Tools {
    pub uv: Option<String>,
    pub ffmpeg: Option<String>,
    /// NVIDIA driver version, when there is an NVIDIA GPU
    pub nvidia_driver: Option<String>,
}

pub fn tools() -> Tools {
    Tools {
        uv: find_exe("uv").map(|p| p.display().to_string()),
        ffmpeg: find_exe("ffmpeg").map(|p| p.display().to_string()),
        nvidia_driver: nvidia_driver(),
    }
}

fn nvidia_driver() -> Option<String> {
    let exe = find_exe("nvidia-smi").or_else(|| {
        let p = PathBuf::from(r"C:\Windows\System32\nvidia-smi.exe");
        p.is_file().then_some(p)
    })?;
    let mut cmd = Command::new(exe);
    cmd.args(["--query-gpu=driver_version", "--format=csv,noheader"]);
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    (out.status.success() && !v.is_empty()).then_some(v)
}

/// Which PyTorch build to install: CUDA 12.8 needs NVIDIA driver 572 or newer on Windows.
fn torch_backend(driver: Option<&str>) -> &'static str {
    match driver.and_then(|d| d.split('.').next()?.parse::<u32>().ok()) {
        Some(major) if major >= 572 => "cu128",
        Some(_) => "auto", // older NVIDIA driver: let uv pick the CUDA build it supports
        None => "cpu",
    }
}

/// uv makes the Python env. Use an installed one, else fetch the official build into the app's folder.
fn ensure_uv(paths: &Paths, log: &dyn Fn(String)) -> Result<PathBuf> {
    if let Some(uv) = find_exe("uv") {
        return Ok(uv);
    }
    let bin = paths.bin_dir();
    std::fs::create_dir_all(&bin)?;
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/uv-x86_64-pc-windows-msvc.zip");
    let zip_path = bin.join("uv.zip");
    log(format!("Downloading uv {UV_VERSION} (the tool that builds the Python environment)…"));
    curl_download(&url, &zip_path, log)?;
    let sums = bin.join("uv.zip.sha256");
    curl_download(&format!("{url}.sha256"), &sums, log)?;
    let want = std::fs::read_to_string(&sums)?.split_whitespace().next().unwrap_or("").to_lowercase();
    let got = format!("{:x}", Sha256::digest(std::fs::read(&zip_path)?));
    if want.len() != 64 || want != got {
        let _ = std::fs::remove_file(&zip_path);
        bail!("The uv download is damaged (checksum mismatch). Press Set up again.");
    }
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name().rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if name == "uv.exe" {
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes)?;
            std::fs::write(bin.join("uv.exe"), bytes)?;
        }
    }
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_file(&sums);
    let uv = bin.join("uv.exe");
    if !uv.is_file() {
        bail!("uv.exe was not in the downloaded archive");
    }
    Ok(uv)
}

/// Download with Windows' own curl.exe: retries and resumes on a flaky connection.
fn curl_download(url: &str, dest: &Path, log: &dyn Fn(String)) -> Result<()> {
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-L", "--fail", "-sS", "--retry", "20", "--retry-all-errors", "--retry-delay", "3", "--connect-timeout", "30", "-C", "-", "-o"])
        .arg(dest)
        .arg(url);
    run_logged(cmd, log).with_context(|| format!("download failed: {url}"))
}

// ---------------------------------------------------------------- Python env

fn requirements_hash() -> String {
    let mut h = Sha256::new();
    h.update(REQUIREMENTS.as_bytes());
    h.update(PYTHON_VERSION.as_bytes());
    format!("{:x}", h.finalize())
}

/// Is the Python env there and built from the requirements this app ships?
pub fn env_ready(paths: &Paths) -> bool {
    env_marker(paths).is_some_and(|m| m.get("requirements").and_then(|r| r.as_str()) == Some(&requirements_hash()))
}

/// What the env was built with ("cu128", "cpu", …), if it is ready.
pub fn env_backend(paths: &Paths) -> Option<String> {
    env_marker(paths)?.get("torch_backend")?.as_str().map(String::from)
}

fn env_marker(paths: &Paths) -> Option<serde_json::Value> {
    if !paths.python().is_file() {
        return None;
    }
    serde_json::from_slice(&std::fs::read(paths.env_marker()).ok()?).ok()
}

/// Create (or update) the env with uv. `log` gets each line uv prints.
pub fn setup_env(paths: &Paths, log: &dyn Fn(String)) -> Result<()> {
    let uv = ensure_uv(paths, log)?;
    paths.install_worker_files()?;
    let env = paths.env_dir();
    let driver = nvidia_driver();
    let backend = torch_backend(driver.as_deref());
    match &driver {
        Some(d) => log(format!("NVIDIA driver {d}: installing PyTorch for the GPU ({backend}).")),
        None => log("No NVIDIA GPU found: installing the CPU build of PyTorch. Transcription will be slow.".into()),
    }
    if !paths.python().is_file() {
        log(format!("Creating a Python {PYTHON_VERSION} environment in {}", env.display()));
        let mut cmd = Command::new(&uv);
        cmd.arg("venv").arg(&env).args(["--python", PYTHON_VERSION, "--clear"]);
        run_logged(cmd, log).context("uv venv failed")?;
    }
    log(if backend == "cpu" {
        "Installing PyTorch and Transformers (about 1 GB the first time)…".into()
    } else {
        "Installing PyTorch with CUDA and Transformers (about 3 GB the first time)…".into()
    });
    let mut cmd = Command::new(&uv);
    cmd.args(["pip", "install", "--python"])
        .arg(paths.python())
        .arg("-r")
        .arg(paths.requirements())
        .args(["--torch-backend", backend])
        .env("UV_HTTP_TIMEOUT", "600")
        .env("UV_HTTP_RETRIES", "10");
    run_logged(cmd, log).context("installing packages failed (press Set up again to continue)")?;

    log("Checking the environment…".into());
    let mut cmd = Command::new(paths.python());
    cmd.args([
        "-c",
        "import torch, transformers; print('torch', torch.__version__, '| CUDA', torch.cuda.is_available(), '| transformers', transformers.__version__)",
    ]);
    run_logged(cmd, log).context("the new environment does not import torch")?;
    let marker = serde_json::json!({
        "requirements": requirements_hash(),
        "python": PYTHON_VERSION,
        "torch_backend": backend,
        "nvidia_driver": driver,
    });
    std::fs::write(paths.env_marker(), serde_json::to_string_pretty(&marker)?)?;
    log("Engine ready.".into());
    Ok(())
}

fn run_logged(mut cmd: Command, log: &dyn Fn(String)) -> Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("NO_COLOR", "1").env("PYTHONUTF8", "1");
    no_window(&mut cmd);
    let mut child = cmd.spawn()?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx2 = tx.clone();
    let t1 = std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(|l| l.ok()) {
            let _ = tx.send(l);
        }
    });
    let t2 = std::thread::spawn(move || {
        for l in BufReader::new(err).lines().map_while(|l| l.ok()) {
            let _ = tx2.send(l);
        }
    });
    for line in rx {
        let line = line.trim_end().to_string();
        if !line.is_empty() {
            log(line);
        }
    }
    let _ = t1.join();
    let _ = t2.join();
    let status = child.wait()?;
    if !status.success() {
        bail!("exit code {:?}", status.code());
    }
    Ok(())
}

// ---------------------------------------------------------------- model

pub fn hf_home() -> PathBuf {
    if let Some(h) = std::env::var_os("HF_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(h);
    }
    dirs::home_dir().unwrap_or_default().join(".cache").join("huggingface")
}

pub fn hub_cache() -> PathBuf {
    if let Some(h) = std::env::var_os("HF_HUB_CACHE").filter(|v| !v.is_empty()) {
        return PathBuf::from(h);
    }
    hf_home().join("hub")
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct ModelStatus {
    pub found: bool,
    /// The folder the model loads from
    pub dir: String,
    pub bytes: u64,
    /// "folder" (imported or downloaded by the app) | "hf-cache" | ""
    pub source: String,
    /// Where "Download model" saves to
    pub download_dir: String,
    /// "app" | "env" | "hf-login" | ""
    pub token: String,
}

/// Check that `dir` holds the model. A Hugging Face cache folder (`models--…`) resolves to
/// its snapshot. Returns the folder to load from and the size of the weights.
pub fn validate_model_dir(dir: &Path) -> Result<(PathBuf, u64)> {
    let dir = resolve_cache_repo(dir).unwrap_or_else(|| dir.to_path_buf());
    let need = |f: &str| -> Result<std::fs::Metadata> {
        std::fs::metadata(dir.join(f)).map_err(|_| anyhow!("{f} is missing"))
    };
    let weights = need("model.safetensors")?.len();
    let config = std::fs::read_to_string(dir.join("config.json")).map_err(|_| anyhow!("config.json is missing"))?;
    need("tokenizer_config.json")?;
    if need("tokenizer.json").is_err() && need("tokenizer.model").is_err() {
        bail!("tokenizer.json is missing");
    }
    if !config.contains("\"cohere_asr\"") {
        bail!("config.json is not a Cohere Transcribe model");
    }
    if weights < 1_000_000_000 {
        bail!("model.safetensors is incomplete ({:.1} GB)", weights as f64 / 1e9);
    }
    Ok((dir, weights))
}

/// `…/models--CohereLabs--…` (or its `snapshots` folder) → the newest complete snapshot.
fn resolve_cache_repo(dir: &Path) -> Option<PathBuf> {
    let repo = if dir.join("snapshots").is_dir() {
        dir.to_path_buf()
    } else if dir.file_name()? == "snapshots" {
        dir.parent()?.to_path_buf()
    } else {
        return None;
    };
    if let Ok(h) = std::fs::read_to_string(repo.join("refs").join("main")) {
        let s = repo.join("snapshots").join(h.trim());
        if s.is_dir() {
            return Some(s);
        }
    }
    std::fs::read_dir(repo.join("snapshots"))
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.join("model.safetensors").exists())
}

pub fn model_status(paths: &Paths, settings: &Settings) -> ModelStatus {
    let download_dir = settings
        .download_dir
        .clone()
        .unwrap_or_else(|| paths.default_model_dir().display().to_string());
    let token = token_source(paths).to_string();
    let found = |dir: PathBuf, bytes: u64, source: &str| ModelStatus {
        found: true,
        dir: dir.display().to_string(),
        bytes,
        source: source.into(),
        download_dir: download_dir.clone(),
        token: token.clone(),
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = &settings.model_dir {
        candidates.push(PathBuf::from(d));
    }
    candidates.push(PathBuf::from(&download_dir));
    candidates.push(paths.default_model_dir());
    for c in candidates {
        if let Ok((dir, bytes)) = validate_model_dir(&c) {
            return found(dir, bytes, "folder");
        }
    }
    let repo = hub_cache().join(format!("models--{}", MODEL_ID.replace('/', "--")));
    if let Ok((dir, bytes)) = validate_model_dir(&repo) {
        return found(dir, bytes, "hf-cache");
    }
    ModelStatus { found: false, dir: String::new(), bytes: 0, source: String::new(), download_dir, token }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_torch_build_for_the_driver() {
        assert_eq!(torch_backend(Some("596.36")), "cu128");
        assert_eq!(torch_backend(Some("572.16")), "cu128");
        assert_eq!(torch_backend(Some("551.86")), "auto");
        assert_eq!(torch_backend(None), "cpu");
    }

    #[test]
    fn rejects_folders_without_the_model() {
        let d = std::env::temp_dir().join(format!("tafreegh-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("config.json"), "{\"model_type\": \"cohere_asr\"}").unwrap();
        let err = validate_model_dir(&d).unwrap_err().to_string();
        assert!(err.contains("model.safetensors"), "{err}");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
