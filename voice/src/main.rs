//! `dawwin-voice.exe`: Dawwin's voice, around audio.cpp's C library (`audiocpp.dll`, from its
//! official release), in a process of its own. It reads one JSON request a line on its standard
//! input and answers each on its standard output; it listens on no port, and it ends when its input
//! closes. The library is loaded when the program starts, from the folder given with `--lib` (its
//! own folder by default), so a missing or broken library is an answer, not a crash at start.
//!
//! `{"cmd":"load","family":"supertonic","model":"…gguf","backend":"vulkan"|"cpu","device":0,"threads":8,"options":{…}}`
//! → `{"ok":true,"device":"Vulkan"|"CPU"}` (a GPU that cannot be used falls back on the processor)
//!
//! `{"cmd":"speak","id":1,"text":"…","language":"ar","voice_id":"F1","voice_ref":"….wav","reference_text":"…","options":{…},"out":"….wav"}`
//! → `{"id":1,"ok":true,"seconds":2.4}`
//!
//! A failure answers `"ok":false,"error":"…"`.

use std::ffi::{c_char, c_void, CStr, CString};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

type Status = i32;
const OK: Status = 0;

#[repr(C)]
struct ModelConfig {
    family_hint: *const c_char,
    config_id: *const c_char,
    weight_id: *const c_char,
    model_spec_override: *const c_char,
}

#[repr(C)]
struct BackendConfig {
    backend: *const c_char,
    device: i32,
    threads: i32,
}

/// The entry points of `audiocpp.h` this program uses (ABI 0.x).
struct Api {
    abi_version: unsafe extern "C" fn() -> u32,
    build_version: unsafe extern "C" fn() -> *const c_char,
    last_error: unsafe extern "C" fn() -> *const c_char,
    options_create: unsafe extern "C" fn() -> *mut c_void,
    options_set: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> Status,
    options_free: unsafe extern "C" fn(*mut c_void),
    registry_create: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Status,
    model_load: unsafe extern "C" fn(*mut c_void, *const c_char, *const ModelConfig, *const c_void, *mut *mut c_void) -> Status,
    model_free: unsafe extern "C" fn(*mut c_void),
    session_create: unsafe extern "C" fn(*const c_void, *const c_char, *const c_char, *const BackendConfig, *const c_void, *mut *mut c_void) -> Status,
    session_free: unsafe extern "C" fn(*mut c_void),
    session_run: unsafe extern "C" fn(*mut c_void, *const c_void, *mut *mut c_void) -> Status,
    request_create: unsafe extern "C" fn() -> *mut c_void,
    request_free: unsafe extern "C" fn(*mut c_void),
    request_set_text: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> Status,
    request_set_voice_id: unsafe extern "C" fn(*mut c_void, *const c_char) -> Status,
    request_set_voice_audio: unsafe extern "C" fn(*mut c_void, *const f32, usize, i32, i32) -> Status,
    request_set_option: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> Status,
    result_free: unsafe extern "C" fn(*mut c_void),
    result_audio: unsafe extern "C" fn(*const c_void, *mut *const f32, *mut usize, *mut i32, *mut i32) -> Status,
}

#[cfg(windows)]
mod dll {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn GetLastError() -> u32;
    }
    // its own folder first for what it needs (the ggml libraries beside it), then the system's
    const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x100;
    const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x1000;

    pub struct Library(*mut c_void);

    impl Library {
        pub fn open(path: &Path) -> Result<Library, String> {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: a valid, terminated path; the handle is never freed (it lives as long as the process)
            let h = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) };
            if h.is_null() {
                // 126: a library it needs is missing, such as the Vulkan loader on a PC without a GPU driver
                let code = unsafe { GetLastError() };
                return Err(format!("cannot load {} (Windows error {code})", path.display()));
            }
            Ok(Library(h))
        }

        pub fn get(&self, name: &str) -> Result<*mut c_void, String> {
            let c = format!("{name}\0");
            // SAFETY: a valid module handle and a terminated name
            let p = unsafe { GetProcAddress(self.0, c.as_ptr()) };
            if p.is_null() {
                Err(format!("{name} is not in audiocpp.dll"))
            } else {
                Ok(p)
            }
        }
    }
}

#[cfg(not(windows))]
mod dll {
    use std::ffi::c_void;
    use std::path::Path;
    pub struct Library;
    impl Library {
        pub fn open(path: &Path) -> Result<Library, String> {
            Err(format!("cannot load {}: Windows only", path.display()))
        }
        pub fn get(&self, name: &str) -> Result<*mut c_void, String> {
            Err(format!("{name}: Windows only"))
        }
    }
}

impl Api {
    fn load(lib: &dll::Library) -> Result<Api, String> {
        macro_rules! f {
            ($name:literal) => {
                // SAFETY: the symbol has the signature audiocpp.h declares for it
                unsafe { std::mem::transmute(lib.get($name)?) }
            };
        }
        let api = Api {
            abi_version: f!("audiocpp_abi_version"),
            build_version: f!("audiocpp_build_version"),
            last_error: f!("audiocpp_last_error"),
            options_create: f!("audiocpp_options_create"),
            options_set: f!("audiocpp_options_set"),
            options_free: f!("audiocpp_options_free"),
            registry_create: f!("audiocpp_registry_create"),
            model_load: f!("audiocpp_model_load"),
            model_free: f!("audiocpp_model_free"),
            session_create: f!("audiocpp_session_create"),
            session_free: f!("audiocpp_session_free"),
            session_run: f!("audiocpp_session_run"),
            request_create: f!("audiocpp_request_create"),
            request_free: f!("audiocpp_request_free"),
            request_set_text: f!("audiocpp_request_set_text"),
            request_set_voice_id: f!("audiocpp_request_set_voice_id"),
            request_set_voice_audio: f!("audiocpp_request_set_voice_audio"),
            request_set_option: f!("audiocpp_request_set_option"),
            result_free: f!("audiocpp_result_free"),
            result_audio: f!("audiocpp_result_audio"),
        };
        // SAFETY: plain calls without arguments
        let abi = unsafe { (api.abi_version)() };
        if abi >> 16 != 0 {
            return Err(format!("audiocpp.dll has ABI {}.{}, this program was made for 0.x", abi >> 16, (abi >> 8) & 0xff));
        }
        Ok(api)
    }

    fn error(&self, what: &str) -> String {
        // SAFETY: the library returns a valid string, never NULL
        let detail = unsafe { CStr::from_ptr((self.last_error)()) }.to_string_lossy().into_owned();
        if detail.is_empty() {
            what.to_string()
        } else {
            format!("{what}: {detail}")
        }
    }
}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', " ")).unwrap()
}

struct Options {
    api: *const Api,
    ptr: *mut c_void,
    _keep: Vec<CString>,
}

impl Options {
    fn new(api: &Api, map: Option<&serde_json::Map<String, Value>>) -> Options {
        // SAFETY: plain allocation
        let ptr = unsafe { (api.options_create)() };
        let mut keep = Vec::new();
        for (k, v) in map.into_iter().flatten() {
            let value = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
            let (k, v) = (cstr(k), cstr(&value));
            // SAFETY: valid options handle and strings (copied by the library)
            unsafe { (api.options_set)(ptr, k.as_ptr(), v.as_ptr()) };
            keep.push(k);
            keep.push(v);
        }
        Options { api, ptr, _keep: keep }
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: freed once, by its own api
        unsafe { ((*self.api).options_free)(self.ptr) };
    }
}

struct Voice {
    api: Api,
    registry: *mut c_void,
    model: *mut c_void,
    session: *mut c_void,
}

impl Voice {
    fn unload(&mut self) {
        // SAFETY: each handle is freed once; freeing NULL is a no-op
        unsafe {
            (self.api.session_free)(self.session);
            (self.api.model_free)(self.model);
        }
        self.session = std::ptr::null_mut();
        self.model = std::ptr::null_mut();
    }

    fn load(&mut self, req: &Value) -> Result<String, String> {
        self.unload();
        let family = req["family"].as_str().unwrap_or("");
        let model_path = req["model"].as_str().ok_or("no model")?;
        if self.registry.is_null() {
            // SAFETY: valid out pointer
            if unsafe { (self.api.registry_create)(std::ptr::null(), &mut self.registry) } != OK {
                return Err(self.api.error("audio.cpp did not start"));
            }
        }
        let (fam, path) = (cstr(family), cstr(model_path));
        let config = ModelConfig { family_hint: if family.is_empty() { std::ptr::null() } else { fam.as_ptr() }, config_id: std::ptr::null(), weight_id: std::ptr::null(), model_spec_override: std::ptr::null() };
        // SAFETY: valid registry, strings and out pointer
        if unsafe { (self.api.model_load)(self.registry, path.as_ptr(), &config, std::ptr::null(), &mut self.model) } != OK {
            return Err(self.api.error("the voice model could not be loaded"));
        }
        let threads = req["threads"].as_i64().unwrap_or(4) as i32;
        let device = req["device"].as_i64().unwrap_or(0) as i32;
        let wanted = req["backend"].as_str().unwrap_or("cpu");
        let options = Options::new(&self.api, req["options"].as_object());
        let (task, mode) = (cstr("tts"), cstr("offline"));
        let mut tried = Vec::new();
        for backend in if wanted == "cpu" { vec!["cpu"] } else { vec![wanted, "cpu"] } {
            let b = cstr(backend);
            let config = BackendConfig { backend: b.as_ptr(), device, threads };
            // SAFETY: valid model, strings, config and out pointer
            if unsafe { (self.api.session_create)(self.model, task.as_ptr(), mode.as_ptr(), &config, options.ptr, &mut self.session) } == OK {
                return Ok(if backend == "cpu" { "CPU".into() } else { "Vulkan".into() });
            }
            tried.push(self.api.error(backend));
        }
        Err(tried.join("; "))
    }

    fn speak(&mut self, req: &Value) -> Result<f64, String> {
        if self.session.is_null() {
            return Err("no voice is loaded".into());
        }
        let api = &self.api;
        // SAFETY: plain allocation, freed below
        let request = unsafe { (api.request_create)() };
        let result = (|| -> Result<f64, String> {
            let text = cstr(req["text"].as_str().unwrap_or(""));
            let lang = req["language"].as_str().filter(|l| !l.is_empty()).map(cstr);
            let mut keep = Vec::new();
            // SAFETY: valid request and strings for every call below
            unsafe {
                if (api.request_set_text)(request, text.as_ptr(), lang.as_ref().map_or(std::ptr::null(), |l| l.as_ptr())) != OK {
                    return Err(api.error("the text was refused"));
                }
                if let Some(id) = req["voice_id"].as_str().filter(|v| !v.is_empty()) {
                    let c = cstr(id);
                    if (api.request_set_voice_id)(request, c.as_ptr()) != OK {
                        return Err(api.error("the voice was refused"));
                    }
                    keep.push(c);
                }
                let mut reference = Vec::new();
                if let Some(path) = req["voice_ref"].as_str().filter(|v| !v.is_empty()) {
                    let (rate, samples) = read_wav(Path::new(path))?;
                    reference = samples;
                    if (api.request_set_voice_audio)(request, reference.as_ptr(), reference.len(), rate as i32, 1) != OK {
                        return Err(api.error("the voice recording was refused"));
                    }
                }
                let mut options: Vec<(String, String)> = req["options"].as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect();
                if let Some(t) = req["reference_text"].as_str().filter(|t| !t.is_empty()) {
                    options.push(("reference_text".into(), t.to_string()));
                }
                for (k, v) in options {
                    let (k, v) = (cstr(&k), cstr(&v));
                    if (api.request_set_option)(request, k.as_ptr(), v.as_ptr()) != OK {
                        return Err(api.error("an option was refused"));
                    }
                    keep.push(k);
                    keep.push(v);
                }
                let mut result: *mut c_void = std::ptr::null_mut();
                if (api.session_run)(self.session, request, &mut result) != OK {
                    (api.result_free)(result);
                    return Err(api.error("the voice could not read this"));
                }
                let (mut samples, mut frames, mut rate, mut channels): (*const f32, usize, i32, i32) = (std::ptr::null(), 0, 0, 0);
                let got = (api.result_audio)(result, &mut samples, &mut frames, &mut rate, &mut channels);
                let out = if got == OK && !samples.is_null() && rate > 0 {
                    let ch = channels.max(1) as usize;
                    let all = std::slice::from_raw_parts(samples, frames * ch);
                    let mono: Vec<f32> = all.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
                    let path = req["out"].as_str().ok_or("no out")?;
                    write_wav(Path::new(path), rate as u32, &mono).map_err(|e| e.to_string())?;
                    Ok(frames as f64 / rate as f64)
                } else {
                    Err(api.error("the voice gave no sound"))
                };
                (api.result_free)(result);
                drop(reference);
                out
            }
        })();
        // SAFETY: freed once
        unsafe { (api.request_free)(request) };
        result
    }
}

fn read_wav(path: &Path) -> Result<(u32, Vec<f32>), String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(format!("{} is not a WAV file", path.display()));
    }
    let (mut rate, mut channels, mut bits, mut format) = (0u32, 1usize, 16u16, 1u16);
    let mut at = 12;
    while at + 8 <= b.len() {
        let size = u32::from_le_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
        let body = &b[at + 8..(at + 8 + size).min(b.len())];
        match &b[at..at + 4] {
            b"fmt " if body.len() >= 16 => {
                format = u16::from_le_bytes([body[0], body[1]]);
                channels = u16::from_le_bytes([body[2], body[3]]).max(1) as usize;
                rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
                if format == 0xFFFE && body.len() >= 26 {
                    format = u16::from_le_bytes([body[24], body[25]]);
                }
            }
            b"data" => {
                let frame: Vec<f32> = match (format, bits) {
                    (1, 16) => body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
                    (3, 32) => body.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
                    _ => return Err(format!("{}: a WAV format this does not read", path.display())),
                };
                return Ok((rate, frame.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()));
            }
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    Err(format!("{} has no sound", path.display()))
}

fn write_wav(path: &Path, rate: u32, samples: &[f32]) -> std::io::Result<()> {
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
    std::fs::write(path, b)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let lib_dir = args.iter().position(|a| a == "--lib").and_then(|i| args.get(i + 1)).map(PathBuf::from).or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))).unwrap_or_default();
    let stdout = std::io::stdout();
    let say = |v: Value| {
        let mut out = stdout.lock();
        let _ = writeln!(out, "{v}");
        let _ = out.flush();
    };
    let mut voice: Result<Voice, String> = dll::Library::open(&lib_dir.join("audiocpp.dll")).and_then(|lib| {
        let api = Api::load(&lib)?;
        // the library stays loaded for the whole run
        std::mem::forget(lib);
        Ok(Voice { api, registry: std::ptr::null_mut(), model: std::ptr::null_mut(), session: std::ptr::null_mut() })
    });
    if let Ok(v) = &voice {
        // SAFETY: plain call
        let build = unsafe { CStr::from_ptr((v.api.build_version)()) }.to_string_lossy().into_owned();
        eprintln!("audio.cpp {build}");
    }
    // what the Tools workflow runs to see that it starts: its library must load too
    if args.iter().any(|a| a == "--version") {
        say(match &voice {
            Ok(_) => json!({ "ok": true }),
            Err(e) => json!({ "ok": false, "error": e }),
        });
        std::process::exit(if voice.is_ok() { 0 } else { 1 });
    }
    for line in std::io::stdin().lock().lines().map_while(|l| l.ok()) {
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let id = req["id"].clone();
        let reply = match (&mut voice, req["cmd"].as_str()) {
            (Err(e), _) => json!({ "id": id, "ok": false, "error": e }),
            (Ok(v), Some("load")) => match v.load(&req) {
                Ok(device) => json!({ "ok": true, "device": device }),
                Err(e) => json!({ "ok": false, "error": e }),
            },
            (Ok(v), Some("speak")) => match v.speak(&req) {
                Ok(seconds) => json!({ "id": id, "ok": true, "seconds": seconds }),
                Err(e) => json!({ "id": id, "ok": false, "error": e }),
            },
            _ => json!({ "id": id, "ok": false, "error": "unknown command" }),
        };
        say(reply);
    }
}
