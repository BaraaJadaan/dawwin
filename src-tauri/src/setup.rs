//! Where things live, the settings, which tools are installed, and where the model is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::{self, Family};
use crate::tools;

#[cfg(windows)]
pub fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
pub fn no_window(_cmd: &mut Command) {}

/// Kill a child and everything it started. The `ffmpeg.exe` Chocolatey or Scoop put on the PATH is
/// a shim that runs the real one as its own child: killing the shim alone leaves that one running,
/// holding the pipes and the half-written file until it has encoded everything.
pub fn kill_tree(child: &mut std::process::Child) {
    if matches!(child.try_wait(), Ok(None)) {
        // its id cannot belong to another process while `child` holds its handle
        #[cfg(windows)]
        {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/T", "/F", "/PID", &child.id().to_string()]);
            cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            no_window(&mut cmd);
            let _ = cmd.status();
        }
    }
    let _ = child.kill();
}

/// Delete a half-written file that a process just killed may still hold for a moment (a killed
/// tree ends one process at a time). Gives up after two seconds.
pub fn remove_when_free(path: &Path) {
    for _ in 0..40 {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => std::thread::sleep(std::time::Duration::from_millis(50)),
            _ => return,
        }
    }
}

/// Run `work`, and kill `children` (with all they started) the moment `cancel` is set, even while
/// `work` is blocked on one of their pipes: a stop must not wait for ffmpeg's next bytes, which on a
/// slow machine can take seconds. A killed child closes its pipes, so `work` then ends too.
pub fn killing_on_stop<T>(children: &[&std::sync::Mutex<std::process::Child>], cancel: &std::sync::atomic::AtomicBool, work: impl FnOnce() -> T) -> T {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Done<'a>(&'a AtomicBool);
    impl Drop for Done<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !finished.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    for c in children {
                        kill_tree(&mut c.lock().unwrap_or_else(|e| e.into_inner()));
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        // set however `work` ends, so the watcher ends with it
        let _done = Done(&finished);
        work()
    })
}

/// While one of these lives, the computer does not go to sleep by itself: a long download or
/// transcription should still be running when the user comes back.
pub struct KeepAwake;

impl KeepAwake {
    pub fn new() -> Self {
        set_awake(true);
        KeepAwake
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        set_awake(false);
    }
}

#[cfg(windows)]
fn set_awake(on: bool) {
    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }
    let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
    // SAFETY: takes flags by value and has no other effect than the power request.
    unsafe {
        SetThreadExecutionState(flags);
    }
}

#[cfg(not(windows))]
fn set_awake(_on: bool) {}

// ---------------------------------------------------------------- paths

#[derive(Clone, Debug)]
pub struct Paths {
    /// %LOCALAPPDATA%\Dawwin: caches, logs, the models by default
    pub local: PathBuf,
    /// %APPDATA%\Dawwin: transcripts (history), settings
    pub roaming: PathBuf,
}

/// The app used to be called Tafreegh. Move what it stored, item by item: the installer may
/// already have made the new folder, and the old one also holds the old install.
fn adopt_old_data(old: &Path, new: &Path, items: &[&str]) {
    for item in items {
        let (from, to) = (old.join(item), new.join(item));
        if from.exists() && !to.exists() {
            let _ = std::fs::create_dir_all(new);
            let _ = std::fs::rename(from, to);
        }
    }
}

impl Paths {
    pub fn new() -> Result<Self> {
        let p = if let Some(home) = std::env::var_os("DAWWIN_HOME").filter(|h| !h.is_empty()) {
            // everything in one folder: for a USB stick, or for trying things out
            let home = PathBuf::from(home);
            Paths { local: home.clone(), roaming: home }
        } else {
            let local = dirs::data_local_dir().context("no LOCALAPPDATA")?.join("Dawwin");
            let roaming = dirs::data_dir().context("no APPDATA")?.join("Dawwin");
            let old_roaming = dirs::data_dir().context("no APPDATA")?.join("Tafreegh");
            adopt_old_data(&old_roaming, &roaming, &["history", "settings.json"]);
            Paths { local, roaming }
        };
        for d in [p.cache_dir(), p.logs_dir(), p.history_dir()] {
            std::fs::create_dir_all(&d).with_context(|| format!("cannot create {}", d.display()))?;
        }
        Ok(p)
    }

    /// Audio handed to the engine is deleted after each job; a crash can leave some behind. Only
    /// the one Dawwin of this data folder may do this (see `instance`): the files may be in use.
    pub fn clear_leftover_audio(&self) {
        for entry in std::fs::read_dir(self.cache_dir()).into_iter().flatten().flatten() {
            if entry.path().extension().is_some_and(|x| x == "f32") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
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
        self.local.join("models")
    }
    fn settings_file(&self) -> PathBuf {
        self.roaming.join("settings.json")
    }

    /// What the Python engine of older versions left behind: unused now, and several gigabytes.
    pub fn old_engine_dirs(&self) -> Vec<PathBuf> {
        // not `engine`: the installer puts the app in this same folder, and the engine next to it
        let mut dirs = vec![
            self.local.join("pyenv"),
            self.local.join("bin"),
            self.local.join("models").join("cohere-transcribe-arabic-07-2026"),
        ];
        if let Some(old) = self.local.parent().map(|p| p.join("Tafreegh")) {
            for name in ["pyenv", "bin", "engine", "cache", "logs", "models"] {
                dirs.push(old.join(name));
            }
        }
        dirs.retain(|d| d.is_dir());
        dirs
    }
}

pub fn folder_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                todo.push(entry.path());
            } else if kind.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/// The speech engine ships in a folder next to the app.
pub fn engine_exe() -> Option<PathBuf> {
    // for trying the app with a stand-in engine (examples/fake_engine.rs); not in release builds
    if cfg!(debug_assertions) {
        if let Some(exe) = std::env::var_os("DAWWIN_TEST_ENGINE").map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(exe);
        }
    }
    let exe = std::env::current_exe().ok()?.parent()?.join("engine").join("dawwin-engine.exe");
    exe.is_file().then_some(exe)
}

/// The translator (llama.cpp's server), downloaded with the first translation model.
pub fn translator_exe(paths: &Paths) -> Option<PathBuf> {
    // for trying the app with a stand-in server; not there in release builds
    if cfg!(debug_assertions) {
        if let Some(exe) = std::env::var_os("DAWWIN_TEST_TRANSLATOR").map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(exe);
        }
    }
    tools::exe(paths, &tools::TRANSLATOR)
}

/// The ffmpeg to use: Dawwin's own, the larger first, else one installed on the computer.
pub fn ffmpeg(paths: &Paths) -> Option<PathBuf> {
    [tools::FFMPEG_VIDEO, tools::FFMPEG_AUDIO].iter().find_map(|t| tools::exe(paths, t)).or_else(installed_ffmpeg)
}

/// An ffmpeg that can burn subtitles into the picture: Dawwin's larger one, or an installed one
/// built with libass and an H.264 encoder.
pub fn burner(paths: &Paths) -> Option<PathBuf> {
    tools::exe(paths, &tools::FFMPEG_VIDEO).or_else(|| installed_ffmpeg().filter(|f| can_burn(f)))
}

fn installed_ffmpeg() -> Option<PathBuf> {
    // for trying the app as it is on a computer without ffmpeg; not there in release builds
    if cfg!(debug_assertions) && std::env::var_os("DAWWIN_TEST_NO_FFMPEG").is_some() {
        return None;
    }
    find_exe("ffmpeg")
}

/// What an ffmpeg lists, by name: its filters or its encoders (`what` is "-filters" or "-encoders").
pub fn ffmpeg_lists(ffmpeg: &Path, what: &str) -> Vec<String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", what]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    no_window(&mut cmd);
    let Ok(out) = cmd.output() else { return Vec::new() };
    // " V....D h264_nvenc   NVIDIA NVENC ...", " ... subtitles  V->V  Render text ..."
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split_whitespace().nth(1).map(String::from)).collect()
}

fn can_burn(ffmpeg: &Path) -> bool {
    static SEEN: std::sync::Mutex<BTreeMap<PathBuf, bool>> = std::sync::Mutex::new(BTreeMap::new());
    if let Some(&known) = SEEN.lock().unwrap().get(ffmpeg) {
        return known;
    }
    let can = ffmpeg_lists(ffmpeg, "-filters").iter().any(|f| f == "subtitles")
        && ffmpeg_lists(ffmpeg, "-encoders").iter().any(|e| crate::video::H264_ENCODERS.contains(&e.as_str()));
    SEEN.lock().unwrap().insert(ffmpeg.to_path_buf(), can);
    can
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// The Cohere model file in use
    #[serde(default)]
    pub model_file: Option<String>,
    /// The model file in use of the other families, by the family's key
    #[serde(default)]
    pub model_files: BTreeMap<String, String>,
    /// Where downloaded models are saved
    #[serde(default)]
    pub download_dir: Option<String>,
    /// "auto" (GPU when there is one) | "cpu"
    #[serde(default)]
    pub run_on: Option<String>,
    /// The interface language ("en", "ar"); unset until the UI picks one
    #[serde(default)]
    pub ui_lang: Option<String>,
    /// A model download that was running and not paused: it continues on the next start
    #[serde(default)]
    pub resume_model: Option<String>,
    /// Ask GitHub for a newer version when the app starts (off unless the user turns it on)
    #[serde(default)]
    pub check_updates: bool,
    /// What transcribing a folder writes next to each recording; unset means text only
    #[serde(default)]
    pub folder_formats: Option<Vec<String>>,
    /// How to write the words Dawwin keeps getting wrong (see `words`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<crate::words::Entry>,
    /// The speaker's name at the start of a subtitle where the speaker changes, in SRT and burned in
    #[serde(default)]
    pub names_in_srt: bool,
    /// The speaker in WebVTT's voice tag; unset means yes
    #[serde(default)]
    pub names_in_vtt: Option<bool>,
    /// How burned-in subtitles look
    #[serde(default)]
    pub look: crate::look::Look,
    /// How large the interface is drawn: the webview's zoom, 1 as it comes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    /// Where the steps save the sound and the videos, and save dialogs open: unset means next
    /// to the recording (see `saving`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<String>,
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
        // replaced in one step, so a crash cannot leave half a settings file
        let tmp = paths.settings_file().with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, paths.settings_file())?;
        Ok(())
    }

    /// The file the user chose for this family of models.
    pub fn chosen_model(&self, family: Family) -> Option<PathBuf> {
        match family {
            Family::Cohere => self.model_file.as_ref(),
            f => self.model_files.get(f.key()),
        }
        .map(PathBuf::from)
    }

    pub fn choose_model(&mut self, family: Family, file: Option<&Path>) {
        let file = file.map(|f| f.display().to_string());
        match (family, file) {
            (Family::Cohere, file) => self.model_file = file,
            (f, Some(file)) => {
                self.model_files.insert(f.key().to_string(), file);
            }
            (f, None) => {
                self.model_files.remove(f.key());
            }
        }
    }

    pub fn run_on(&self) -> &str {
        match self.run_on.as_deref() {
            // the last two are what older versions saved
            Some("cpu") | Some("cpu-fast") | Some("cpu-exact") => "cpu",
            _ => "auto",
        }
    }

    /// Tell the exports whether subtitles name the speaker, and how burned-in ones look.
    pub fn apply_to_subtitles(&self) {
        crate::speakers::set_subtitle_names(self.names_in_srt, self.names_in_vtt.unwrap_or(true));
        crate::look::set(self.look.clone());
    }

    pub fn folder_formats(&self) -> Vec<String> {
        self.folder_formats.clone().unwrap_or_else(|| vec!["txt".to_string()])
    }

    pub fn download_dir(&self, paths: &Paths) -> PathBuf {
        self.download_dir.as_ref().map(PathBuf::from).unwrap_or_else(|| paths.default_model_dir())
    }
}

// ---------------------------------------------------------------- tools

/// Find an executable on PATH, then where winget installs to.
pub fn find_exe(name: &str) -> Option<PathBuf> {
    let file = format!("{name}.exe");
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(l) = dirs::data_local_dir() {
        dirs.push(l.join("Microsoft").join("WinGet").join("Links"));
    }
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

/// For tests: every ffmpeg a feature must work with. Dawwin's own builds first, from
/// DAWWIN_TEST_TOOLS_DIR (a folder laid out like the app's tools folder: `ffmpeg-audio\ffmpeg.exe`,
/// `ffmpeg-video\ffmpeg.exe`), then a full ffmpeg on the PATH. Its own builds lack much of what a
/// full one has, and bugs that only they show have shipped before.
#[cfg(test)]
pub fn test_ffmpegs() -> Vec<(&'static str, PathBuf)> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("DAWWIN_TEST_TOOLS_DIR").map(PathBuf::from) {
        for t in [tools::FFMPEG_AUDIO, tools::FFMPEG_VIDEO] {
            let exe = dir.join(t.folder).join(t.exe);
            if exe.is_file() {
                out.push((t.id, exe));
            }
        }
        assert!(!out.is_empty(), "DAWWIN_TEST_TOOLS_DIR holds no ffmpeg-audio or ffmpeg-video");
    }
    if let Some(ff) = find_exe("ffmpeg") {
        out.push(("installed", ff));
    }
    out
}

/// For tests: a sample file of the kind people bring, made into `dir` with a full ffmpeg on the
/// PATH and `args` (its inputs and options, before the output). None without a full ffmpeg.
#[cfg(test)]
pub fn test_sample(dir: &Path, name: &str, args: &[&str]) -> Option<PathBuf> {
    let maker = find_exe("ffmpeg").filter(|f| ffmpeg_lists(f, "-filters").iter().any(|x| x == "sine"))?;
    let out = dir.join(name);
    let made = Command::new(maker).args(["-nostdin", "-loglevel", "error", "-y"]).args(args).arg(&out).status();
    made.is_ok_and(|s| s.success()).then_some(out)
}

/// For tests: run `f` on a thread of its own, and fail when it takes longer than `limit`: a test
/// of something that once hung must not hang itself.
#[cfg(test)]
pub fn within<T: Send + 'static>(limit: std::time::Duration, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).unwrap_or_else(|_| panic!("{what} did not finish within {limit:?}"))
}

// ---------------------------------------------------------------- model

#[derive(Clone, Debug, Serialize, Default)]
pub struct ModelStatus {
    pub found: bool,
    /// The file the model loads from
    pub path: String,
    pub bytes: u64,
    /// One of ours ("q8", "canary", ...), or empty for a model file of the user's own
    pub variant: String,
}

/// The model file of this family: the one chosen, else one in the download folder or the usual place.
pub fn model_status(paths: &Paths, settings: &Settings, family: Family) -> ModelStatus {
    let chosen = settings.chosen_model(family).filter(|p| models::check_file(p).is_ok_and(|f| f == family));
    let found = chosen
        .or_else(|| models::find_in_folder(&settings.download_dir(paths), Some(family)))
        .or_else(|| models::find_in_folder(&paths.default_model_dir(), Some(family)));
    match found {
        Some(path) => ModelStatus {
            found: true,
            bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            variant: models::variant_of(&path).map(|v| v.id.to_string()).unwrap_or_default(),
            path: path.display().to_string(),
        },
        None => ModelStatus::default(),
    }
}

/// The model file a recording in this language is transcribed with: the best one that is here.
pub fn model_for(paths: &Paths, settings: &Settings, language: &str) -> Option<(Family, ModelStatus)> {
    models::models_for(language).into_iter().find_map(|v| {
        let status = model_status(paths, settings, v.family);
        status.found.then_some((v.family, status))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_data_moves_even_when_the_new_folder_exists() {
        let root = std::env::temp_dir().join(format!("dawwin-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("Tafreegh"), root.join("Dawwin"));
        std::fs::create_dir_all(old.join("history")).unwrap();
        std::fs::create_dir_all(new.join("logs")).unwrap();
        std::fs::write(old.join("history/a.json"), b"x").unwrap();
        std::fs::write(old.join("settings.json"), b"{}").unwrap();
        std::fs::write(new.join("settings.json"), b"{\"new\":1}").unwrap();
        std::fs::write(old.join("tafreegh.exe"), b"old app").unwrap();
        adopt_old_data(&old, &new, &["history", "settings.json", "models"]);
        assert!(new.join("history/a.json").is_file());
        assert!(!old.join("history").exists());
        assert_eq!(std::fs::read_to_string(new.join("settings.json")).unwrap(), "{\"new\":1}", "what is already there is kept");
        assert!(old.join("tafreegh.exe").is_file(), "the old install is not touched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_saved_by_older_versions_still_load() {
        let old = r#"{"model_dir":"D:\\m","download_dir":"D:\\d","run_on":"cpu-fast","ui_lang":"ar","download_wanted":true,"download_total":5,"check_updates":true}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(s.run_on(), "cpu");
        assert_eq!(s.ui_lang.as_deref(), Some("ar"));
        assert!(s.check_updates);
        assert!(s.model_file.is_none(), "the old model cannot be used by this engine");
        assert_eq!(s.folder_formats(), ["txt"]);
    }

    /// A folder of model files named by their architecture, with the settings pointing nowhere.
    fn with_models(name: &str, archs: &[&str]) -> (Paths, Settings) {
        let root = std::env::temp_dir().join(format!("dawwin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let models = root.join("models");
        std::fs::create_dir_all(&models).unwrap();
        for arch in archs {
            let mut b = b"GGUF\x03\0\0\0".to_vec();
            b.extend([0u8; 16]);
            b.extend(20u64.to_le_bytes());
            b.extend(b"general.architecture");
            b.extend(8u32.to_le_bytes());
            b.extend((arch.len() as u64).to_le_bytes());
            b.extend(arch.as_bytes());
            std::fs::write(models.join(format!("my-{arch}.gguf")), b).unwrap();
        }
        (Paths { local: root.clone(), roaming: root }, Settings::default())
    }

    #[test]
    fn a_recording_goes_to_the_best_model_that_is_here() {
        let (paths, s) = with_models("route", &["cohere_asr", "whisper"]);
        let family = |l: &str| model_for(&paths, &s, l).map(|(f, _)| f);
        assert_eq!(family("ar"), Some(Family::Cohere));
        assert_eq!(family("en"), Some(Family::Cohere));
        assert_eq!(family("da"), Some(Family::Whisper), "no Canary here: Whisper can do Danish too");
        assert_eq!(family("tr"), Some(Family::Whisper));

        let (paths, s) = with_models("route2", &["canary"]);
        let family = |l: &str| model_for(&paths, &s, l).map(|(f, _)| f);
        assert_eq!(family("da"), Some(Family::Canary));
        assert_eq!(family("en"), Some(Family::Canary));
        assert_eq!(family("ar"), None, "Arabic needs its own model");
        assert_eq!(family("tr"), None, "Canary has no Turkish");
    }

    #[test]
    fn the_chosen_file_of_each_family_is_kept_apart() {
        let (paths, mut s) = with_models("slots", &["cohere_asr", "canary", "whisper"]);
        let dir = paths.default_model_dir();
        s.choose_model(Family::Whisper, Some(&dir.join("my-whisper.gguf")));
        s.choose_model(Family::Cohere, Some(&dir.join("my-cohere_asr.gguf")));
        assert!(s.model_file.as_deref().unwrap().ends_with("my-cohere_asr.gguf"), "where older versions kept it");
        assert!(model_status(&paths, &s, Family::Whisper).path.ends_with("my-whisper.gguf"));
        assert!(model_status(&paths, &s, Family::Canary).path.ends_with("my-canary.gguf"), "found in the folder");

        // a choice pointing at a file of another family is not believed
        s.choose_model(Family::Canary, Some(&dir.join("my-whisper.gguf")));
        assert!(model_status(&paths, &s, Family::Canary).path.ends_with("my-canary.gguf"));

        s.choose_model(Family::Whisper, None);
        assert!(s.chosen_model(Family::Whisper).is_none());
    }

    #[test]
    fn cleaning_up_the_old_engine_never_touches_the_new_one() {
        let root = std::env::temp_dir().join(format!("dawwin-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let local = root.join("Dawwin");
        for d in ["engine", "models", "pyenv", "cache", "logs"] {
            std::fs::create_dir_all(local.join(d)).unwrap();
        }
        std::fs::create_dir_all(root.join("Tafreegh/pyenv")).unwrap();
        std::fs::create_dir_all(root.join("Tafreegh/engine")).unwrap();
        let paths = Paths { local: local.clone(), roaming: root.join("roaming") };
        let old = paths.old_engine_dirs();
        assert!(old.contains(&local.join("pyenv")));
        assert!(old.contains(&root.join("Tafreegh/pyenv")));
        assert!(old.contains(&root.join("Tafreegh/engine")));
        for keep in ["engine", "models", "cache", "logs"] {
            assert!(!old.contains(&local.join(keep)), "{keep} belongs to this version");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_folder_size_adds_up_subfolders() {
        let root = std::env::temp_dir().join(format!("dawwin-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("one"), vec![0u8; 100]).unwrap();
        std::fs::write(root.join("a/b/two"), vec![0u8; 50]).unwrap();
        assert_eq!(folder_size(&root), 150);
        let _ = std::fs::remove_dir_all(&root);
    }
}
