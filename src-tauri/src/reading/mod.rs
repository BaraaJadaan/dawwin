//! Read aloud: a file, pasted text, or a transcript and its translations, read sentence by
//! sentence. The sentence asked for is made first and the next ones while it plays; what was made
//! is kept, so going back is instant. Each sentence goes to the voice of its own language, a
//! piece at a time when it mixes Arabic and English.

pub mod formats;
pub mod numbers;
pub mod text;
pub mod voice;
pub mod voices;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::queue::Core;
use crate::setup::Paths;
use formats::{Block, Kind};
use text::{Lang, Run};

/// Sentences made ahead of the one playing.
const AHEAD: usize = 3;
/// The breath after a sentence, after the last one of a paragraph, after a heading (seconds).
const PAUSE_SENTENCE: f32 = 0.25;
const PAUSE_PARAGRAPH: f32 = 0.6;
const PAUSE_HEADING: f32 = 0.9;
/// Between the Arabic and the English pieces of one sentence.
const PAUSE_PIECE: f32 = 0.06;

#[derive(Clone, Debug, Serialize)]
pub struct Sentence {
    pub text: String,
    pub block: usize,
    pub kind: Kind,
    #[serde(skip)]
    pub pause: f32,
}

/// What a sentence is read as: cleaned, in runs of one language, numbers in words. Worked out when
/// the sentence is made, so that opening a long book does not wait for all of it.
pub fn spoken(text: &str, doc_lang: Lang) -> Vec<Run> {
    text::runs(&text::normalize(text), doc_lang).into_iter().map(|r| Run { text: numbers::speakable(&r.text, r.lang), lang: r.lang }).collect()
}

pub struct Doc {
    pub id: String,
    /// What its remembered place is filed under
    pub key: String,
    pub title: String,
    pub lang: Lang,
    pub sentences: Vec<Sentence>,
}

/// Blocks into sentences, each with its pause and its runs of one language.
pub fn sentences_of(blocks: &[Block]) -> (Lang, Vec<Sentence>) {
    let all: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
    let lang = text::main_lang(&all);
    let mut out = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        let parts = if b.kind == Kind::Heading { vec![b.text.clone()] } else { text::sentences(&b.text) };
        let n = parts.len();
        for (k, s) in parts.into_iter().enumerate() {
            if !s.chars().any(|c| c.is_alphanumeric()) {
                continue;
            }
            let pause = match (b.kind, k + 1 == n) {
                (Kind::Heading, _) => PAUSE_HEADING,
                (_, true) => PAUSE_PARAGRAPH,
                _ => PAUSE_SENTENCE,
            };
            out.push(Sentence { text: s, block: bi, kind: b.kind, pause });
        }
    }
    (lang, out)
}

/// A transcript, or its translation: lines run on into a paragraph until a pause of two seconds.
pub fn transcript_blocks(t: &crate::transcript::Transcript) -> Vec<Block> {
    lines_to_blocks(t.segments.iter().map(|s| (s.start, s.end, s.text.as_str())))
}

fn lines_to_blocks<'a>(lines: impl Iterator<Item = (f64, f64, &'a str)>) -> Vec<Block> {
    let mut out = Vec::new();
    let mut para = String::new();
    let mut last_end: Option<f64> = None;
    for (start, end, text) in lines {
        if last_end.is_some_and(|e| start - e >= 2.0) && !para.trim().is_empty() {
            out.push(Block { kind: Kind::Paragraph, text: std::mem::take(&mut para).trim().to_string() });
        }
        para.push(' ');
        para.push_str(text.trim());
        last_end = Some(end);
    }
    if !para.trim().is_empty() {
        out.push(Block { kind: Kind::Paragraph, text: para.trim().to_string() });
    }
    out
}

/// Pasted text is named by its first line, cut at a word when it is long.
fn title_of(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").trim_start_matches('#').trim();
    let first = text::sentences(line).into_iter().next().unwrap_or_default();
    if first.chars().count() <= 60 {
        return first;
    }
    let cut: String = first.chars().take(60).collect();
    format!("{}…", cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a))
}

fn short_hash(s: &str) -> String {
    let d = Sha256::digest(s.as_bytes());
    d.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

// ------------------------------------------------------------------ what is remembered

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Prefs {
    /// The voice of each language ("ar", "en"), by its key
    #[serde(default)]
    voices: BTreeMap<String, String>,
    /// Where reading stopped, by document
    #[serde(default)]
    places: BTreeMap<String, usize>,
}

fn prefs_file(paths: &Paths) -> PathBuf {
    paths.roaming.join("reading.json")
}

fn load_prefs(paths: &Paths) -> Prefs {
    std::fs::read(prefs_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_prefs(paths: &Paths, p: &Prefs) {
    let tmp = prefs_file(paths).with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(p).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, prefs_file(paths));
    }
}

// ------------------------------------------------------------------ the reader

#[derive(Clone, Debug, Serialize)]
pub struct Ready {
    pub path: String,
    pub seconds: f64,
    /// Where it was made: "GPU · …" or "CPU"
    pub device: String,
}

type Made = Result<Ready, String>;

struct Work {
    /// (document, sentence), the one waited for first
    wanted: VecDeque<(String, usize)>,
    made: HashMap<(String, usize), (String, Made)>,
}

pub struct Reader {
    paths: Paths,
    app: AppHandle,
    docs: Mutex<HashMap<String, Arc<Doc>>>,
    prefs: Mutex<Prefs>,
    work: Mutex<Work>,
    wake: Condvar,
    done: Condvar,
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The voice died on the GPU: until the app restarts, it reads on the processor
    no_gpu: AtomicBool,
    /// A voice program holds the GPU
    gpu_held: AtomicBool,
    /// The speech engine or the translator wants the GPU: the voice lets go of it
    let_go: AtomicBool,
    /// Every voice program is to end (a model is being deleted)
    end_all: AtomicBool,
}

fn cache_root(paths: &Paths) -> PathBuf {
    paths.cache_dir().join("reading")
}

impl Reader {
    pub fn start(app: AppHandle, paths: Paths) -> Arc<Reader> {
        // what was made last time is not kept: the voices may have changed
        let _ = std::fs::remove_dir_all(cache_root(&paths));
        let _ = std::fs::create_dir_all(cache_root(&paths));
        let _ = app.asset_protocol_scope().allow_directory(cache_root(&paths), true);
        let reader = Arc::new(Reader {
            prefs: Mutex::new(load_prefs(&paths)),
            paths,
            app,
            docs: Mutex::new(HashMap::new()),
            work: Mutex::new(Work { wanted: VecDeque::new(), made: HashMap::new() }),
            wake: Condvar::new(),
            done: Condvar::new(),
            downloads: Mutex::new(HashMap::new()),
            no_gpu: AtomicBool::new(false),
            gpu_held: AtomicBool::new(false),
            let_go: AtomicBool::new(false),
            end_all: AtomicBool::new(false),
        });
        let r = reader.clone();
        std::thread::Builder::new().name("voice".into()).spawn(move || r.run()).expect("voice thread");
        reader
    }

    /// The voice of a language: the one chosen, if it is still there, else the first one installed.
    fn voice_for(&self, lang: Lang) -> Option<(voices::VoiceModel, voices::Voice)> {
        let chosen = self.prefs.lock().unwrap().voices.get(lang.code()).cloned();
        chosen
            .and_then(|k| voices::find_voice(&k))
            .filter(|(m, _)| voices::installed(&self.paths, m))
            .or_else(|| voices::default_voice(&self.paths, lang).and_then(|k| voices::find_voice(&k)))
    }

    /// What decides how a sentence sounds: a sentence made with other voices is made again.
    fn signature(&self) -> String {
        [Lang::Ar, Lang::En].iter().map(|&l| self.voice_for(l).map_or("-".to_string(), |(m, v)| voices::voice_key(&m, &v))).collect::<Vec<_>>().join("|")
    }

    pub fn open(&self, key: String, title: String, blocks: Vec<Block>) -> Result<Arc<Doc>> {
        let (lang, sentences) = sentences_of(&blocks);
        if sentences.is_empty() {
            return Err(anyhow!(crate::tr!("There is nothing to read in it.")));
        }
        let doc = Arc::new(Doc { id: short_hash(&key), key, title, lang, sentences });
        self.docs.lock().unwrap().insert(doc.id.clone(), doc.clone());
        Ok(doc)
    }

    pub fn place(&self, doc: &Doc) -> usize {
        self.prefs.lock().unwrap().places.get(&doc.key).copied().filter(|&i| i < doc.sentences.len()).unwrap_or(0)
    }

    pub fn set_place(&self, doc_id: &str, index: usize) {
        let Some(doc) = self.docs.lock().unwrap().get(doc_id).cloned() else { return };
        let mut p = self.prefs.lock().unwrap();
        // the end is not a place to come back to
        if index + 1 >= doc.sentences.len() {
            p.places.remove(&doc.key);
        } else {
            p.places.insert(doc.key.clone(), index);
        }
        // the oldest go when there are many
        while p.places.len() > 500 {
            let first = p.places.keys().next().cloned().unwrap();
            p.places.remove(&first);
        }
        save_prefs(&self.paths, &p);
    }

    /// The sound of sentence `index`, made now if it is not ready, with the next ones after it.
    pub fn sentence(&self, doc_id: &str, index: usize) -> Made {
        let sig = self.signature();
        let key = (doc_id.to_string(), index);
        let mut w = self.work.lock().unwrap();
        let count = self.docs.lock().unwrap().get(doc_id).map(|d| d.sentences.len()).ok_or_else(|| crate::tr!("That text is no longer open."))?;
        // what is wanted now: this sentence, then the ones after it; other places can wait
        w.wanted.retain(|(d, i)| d == doc_id && *i > index && *i <= index + AHEAD);
        for i in (index..(index + AHEAD + 1).min(count)).rev() {
            let k = (doc_id.to_string(), i);
            let fresh = w.made.get(&k).is_some_and(|(s, m)| *s == sig && m.is_ok());
            if !fresh && !w.wanted.contains(&k) {
                w.wanted.push_front(k);
            }
        }
        // the one waited for goes first
        if let Some(p) = w.wanted.iter().position(|k| *k == key) {
            let k = w.wanted.remove(p).unwrap();
            w.wanted.push_front(k);
        }
        // a failure is tried again when asked for again
        if w.made.get(&key).is_some_and(|(s, m)| *s != sig || m.is_err()) {
            w.made.remove(&key);
        }
        self.wake.notify_all();
        let deadline = std::time::Instant::now() + Duration::from_secs(6 * 60);
        loop {
            if let Some((s, m)) = w.made.get(&key) {
                if *s == sig {
                    return m.clone();
                }
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(crate::tr!("The voice did not answer in {} seconds.", 360));
            }
            w = self.done.wait_timeout(w, left).unwrap().0;
        }
    }

    /// Stop making sentences ahead (reading was stopped, or another text opened).
    pub fn stop(&self) {
        self.work.lock().unwrap().wanted.clear();
    }

    fn run(self: Arc<Self>) {
        // one program for each model in use: Arabic and English may be read by different ones
        let mut procs: HashMap<String, voice::VoiceProcess> = HashMap::new();
        loop {
            let (doc_id, index, sig) = {
                let mut w = self.work.lock().unwrap();
                loop {
                    self.let_go_if_asked(&mut procs);
                    if let Some(k) = w.wanted.pop_front() {
                        let sig = self.signature();
                        if w.made.get(&k).is_some_and(|(s, m)| *s == sig && m.is_ok()) {
                            continue;
                        }
                        break (k.0, k.1, sig);
                    }
                    w = self.wake.wait_timeout(w, Duration::from_secs(1)).unwrap().0;
                }
            };
            let doc = self.docs.lock().unwrap().get(&doc_id).cloned();
            let made = match doc {
                Some(doc) => {
                    let mut result = self.make(&mut procs, &doc, index);
                    // a voice that died (a GPU driver, say) costs a new process and one more try,
                    // on the processor; only a second death is told. One that let go of the GPU
                    // for the speech engine did not fail.
                    if result.as_ref().is_err_and(|e| e.downcast_ref::<voice::VoiceDied>().is_some()) {
                        let gave_way = self.let_go.load(Ordering::SeqCst);
                        if !gave_way && procs.values_mut().any(|p| !p.alive() && p.on_gpu()) {
                            self.no_gpu.store(true, Ordering::SeqCst);
                        }
                        procs.retain(|_, p| p.alive());
                        self.let_go_if_asked(&mut procs);
                        result = self.make(&mut procs, &doc, index);
                        if result.as_ref().is_err_and(|e| e.downcast_ref::<voice::VoiceDied>().is_some()) {
                            procs.retain(|_, p| p.alive());
                        }
                    }
                    self.gpu_held.store(procs.values().any(|p| p.on_gpu()), Ordering::SeqCst);
                    result.map_err(|e| format!("{e:#}"))
                }
                None => Err(crate::tr!("That text is no longer open.")),
            };
            if let Err(e) = &made {
                let _ = self.app.emit("reading-error", serde_json::json!({ "doc": doc_id, "index": index, "error": e }));
            }
            let mut w = self.work.lock().unwrap();
            w.made.insert((doc_id, index), (sig, made));
            self.done.notify_all();
        }
    }

    /// The queue asked for the GPU, or a model is being deleted: the programs concerned end.
    fn let_go_if_asked(&self, procs: &mut HashMap<String, voice::VoiceProcess>) {
        if self.end_all.load(Ordering::SeqCst) {
            procs.clear();
            self.end_all.store(false, Ordering::SeqCst);
        }
        if self.let_go.load(Ordering::SeqCst) {
            procs.retain(|_, p| !p.on_gpu());
            self.let_go.store(false, Ordering::SeqCst);
        }
        self.gpu_held.store(procs.values().any(|p| p.on_gpu()), Ordering::SeqCst);
    }

    /// The voice program; with the folder of the GPU build of audio.cpp's library, when the GPU is
    /// wanted and that build is there (the program's own folder holds the processor build).
    fn program(&self, gpu: bool) -> Option<(PathBuf, Option<PathBuf>)> {
        if let Some(test) = voices::test_voice() {
            return Some((test, None));
        }
        let exe = crate::tools::exe(&self.paths, &crate::tools::VOICE)?;
        let lib = gpu.then(|| crate::tools::exe(&self.paths, &crate::tools::VOICE_GPU)).flatten().and_then(|dll| dll.parent().map(Path::to_path_buf));
        Some((exe, lib))
    }

    /// Whether the speech engine or the translator is at work (it holds the GPU while it is).
    fn gpu_busy(&self) -> bool {
        if cfg!(debug_assertions) && std::env::var_os("DAWWIN_TEST_GPU_BUSY").is_some() {
            return true;
        }
        const IDLE: [&str; 4] = ["queued", "done", "failed", "cancelled"];
        self.app.try_state::<Arc<Core>>().is_some_and(|c| c.snapshot().queue.iter().any(|j| !IDLE.contains(&j.status.as_str())))
    }

    /// Whether this PC has a GPU the speech engine found.
    fn has_gpu(&self) -> bool {
        (cfg!(debug_assertions) && std::env::var_os("DAWWIN_TEST_GPU_BUSY").is_some())
            || self.app.try_state::<Arc<Core>>().is_some_and(|c| c.snapshot().engine.gpu.is_some())
    }

    fn make(&self, procs: &mut HashMap<String, voice::VoiceProcess>, doc: &Doc, index: usize) -> Result<Ready> {
        let s = doc.sentences.get(index).ok_or_else(|| anyhow!("no sentence {index}"))?;
        let dir = cache_root(&self.paths).join(&doc.id);
        std::fs::create_dir_all(&dir)?;
        let mut pieces = Vec::new();
        let mut device = String::new();
        let program_dir = crate::tools::exe(&self.paths, &crate::tools::VOICE).and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
        for (k, run) in spoken(&s.text, doc.lang).iter().enumerate() {
            let (m, v) = self.voice_for(run.lang).ok_or_else(|| anyhow!(crate::tr!("There is no voice for {} yet. Choose one in Settings.", lang_name(run.lang))))?;
            let gpu = m.gpu && !self.no_gpu.load(Ordering::SeqCst) && crate::setup::Settings::load(&self.paths).run_on() != "cpu";
            // the speech engine and the translator have the GPU while they work: a voice that
            // needs it waits, and says so; one on the processor reads on
            if gpu && self.has_gpu() && self.gpu_busy() {
                if procs.get(m.id).is_some_and(|p| p.on_gpu()) {
                    procs.remove(m.id);
                }
                return Err(anyhow!(crate::tr!("{} waits for the graphics card while Dawwin transcribes or translates. Choose another voice to read now, or try again when it is done.", m.name)));
            }
            let model_dir = voices::dir(&self.paths, &m);
            let load = voice::Load { family: m.family.into(), model: model_dir.clone(), gpu, options: Vec::new() };
            let p = self.ready_process(procs, m.id, &load)?;
            device = if p.on_gpu() { format!("GPU · {}", p.device) } else { "CPU".into() };
            let out = dir.join(format!("{index}-{k}.wav"));
            p.speak(&run.text, m.code(run.lang), &voices::request(&m, &v, &model_dir, &program_dir), &out)?;
            pieces.push(voice::read_wav(&out)?);
            let _ = std::fs::remove_file(&out);
        }
        let (rate, samples) = voice::join(&pieces, PAUSE_PIECE, s.pause);
        // a new name each time: the webview would play what it kept of the old one
        let out = dir.join(format!("{index}-{}.wav", short_hash(&self.signature())));
        voice::write_wav(&out, rate, &samples)?;
        Ok(Ready { path: out.display().to_string(), seconds: samples.len() as f64 / rate as f64, device })
    }

    fn ready_process<'a>(&self, procs: &'a mut HashMap<String, voice::VoiceProcess>, model: &str, load: &voice::Load) -> Result<&'a mut voice::VoiceProcess> {
        // a program started with the processor's library cannot use the GPU, nor the other way round
        let wants_gpu_lib = load.gpu && self.program(true).is_some_and(|(_, lib)| lib.is_some());
        if procs.get_mut(model).is_some_and(|p| !p.alive() || p.gpu_library != wants_gpu_lib) {
            procs.remove(model);
        }
        if !procs.contains_key(model) {
            let (exe, lib) = self.program(wants_gpu_lib).ok_or_else(|| anyhow!(crate::tr!("The voice is not installed. Download a voice in Settings.")))?;
            let mut p = voice::VoiceProcess::spawn(&exe, lib.as_deref(), &self.paths.logs_dir().join("voice.log"))?;
            p.gpu_library = lib.is_some();
            procs.insert(model.to_string(), p);
        }
        let p = procs.get_mut(model).unwrap();
        if p.loaded.as_ref() != Some(load) {
            if let Err(e) = p.load(load) {
                // the GPU build of the library does not load here (no Vulkan driver): the processor's
                if p.gpu_library && format!("{e:#}").contains("audiocpp.dll") {
                    self.no_gpu.store(true, Ordering::SeqCst);
                    procs.remove(model);
                    let cpu = voice::Load { gpu: false, ..load.clone() };
                    return self.ready_process(procs, model, &cpu);
                }
                return Err(e);
            }
        }
        Ok(procs.get_mut(model).unwrap())
    }
}

fn lang_name(l: Lang) -> String {
    match l {
        Lang::Ar => crate::tr!("Arabic"),
        Lang::En => crate::tr!("English"),
    }
}

// ------------------------------------------------------------------ what the interface sees

#[derive(Serialize)]
pub struct DocView {
    pub id: String,
    pub title: String,
    pub lang: Lang,
    pub sentences: Vec<Sentence>,
    pub place: usize,
}

#[derive(Serialize)]
pub struct VoiceView {
    pub key: String,
    pub model: String,
    pub lang: Lang,
    pub woman: bool,
    pub number: u8,
}

#[derive(Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub license: String,
    pub license_url: String,
    pub restricted: bool,
    /// It needs a graphics card to read as fast as it speaks
    pub gpu: bool,
    /// The languages it reads
    pub reads: Vec<Lang>,
    /// A sample in Arabic and in English shipped with the app
    pub samples: [bool; 2],
    pub installed: bool,
    pub downloading: bool,
    pub partial: u64,
}

#[derive(Serialize)]
pub struct VoicesView {
    pub models: Vec<ModelView>,
    pub voices: Vec<VoiceView>,
    /// The voice reading each language now ("ar", "en"), by key
    pub chosen: BTreeMap<String, String>,
    pub program: bool,
}

impl Reader {
    pub fn view(&self, doc: &Doc) -> DocView {
        DocView { id: doc.id.clone(), title: doc.title.clone(), lang: doc.lang, sentences: doc.sentences.clone(), place: self.place(doc) }
    }

    pub fn voices(&self) -> VoicesView {
        let downloading = self.downloads.lock().unwrap();
        let models = voices::models();
        let mut chosen = BTreeMap::new();
        for l in [Lang::Ar, Lang::En] {
            if let Some((m, v)) = self.voice_for(l) {
                chosen.insert(l.code().to_string(), voices::voice_key(&m, &v));
            }
        }
        VoicesView {
            voices: models
                .iter()
                .filter(|m| voices::installed(&self.paths, m))
                .flat_map(|m| m.voices.iter().map(move |v| VoiceView { key: voices::voice_key(m, v), model: m.name.into(), lang: v.lang, woman: v.woman, number: v.number }))
                .collect(),
            models: models
                .iter()
                .filter(|m| m.id != voices::STAND_IN.id)
                .map(|m| ModelView {
                    id: m.id.into(),
                    name: m.name.into(),
                    size: m.size(),
                    license: m.license.into(),
                    license_url: m.license_url.into(),
                    restricted: m.restricted,
                    gpu: m.gpu,
                    reads: [Lang::Ar, Lang::En].into_iter().filter(|&l| m.reads(l)).collect(),
                    samples: m.samples,
                    installed: voices::installed(&self.paths, m),
                    downloading: downloading.contains_key(m.id),
                    partial: voices::on_disk(&self.paths, m),
                })
                .collect(),
            chosen,
            program: self.program(false).is_some(),
        }
    }

    pub fn choose(&self, lang: &str, key: &str) -> Result<()> {
        voices::find_voice(key).ok_or_else(|| anyhow!("no voice {key}"))?;
        let mut p = self.prefs.lock().unwrap();
        p.voices.insert(lang.to_string(), key.to_string());
        save_prefs(&self.paths, &p);
        Ok(())
    }

    /// Download a voice model (and the voice program with the first one). Paused, what came stays.
    pub fn download(self: &Arc<Self>, id: &str) -> Result<()> {
        let m = voices::model(id).ok_or_else(|| anyhow!("no voice model {id}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut d = self.downloads.lock().unwrap();
            if d.contains_key(id) {
                return Ok(());
            }
            d.insert(id.to_string(), stop.clone());
        }
        let r = self.clone();
        std::thread::spawn(move || {
            let emit = |state: &str, done: u64, total: u64, error: Option<String>| {
                let _ = r.app.emit("voice-download", serde_json::json!({ "id": m.id, "state": state, "done": done, "total": total, "error": error }));
            };
            let total = m.size();
            let result = (|| -> Result<crate::models::Finished> {
                use crate::models::{Event, Finished};
                // the voice program with the first voice, and the GPU build of its library with
                // the first voice that wants a graphics card
                let tools = [Some(crate::tools::VOICE), m.gpu.then_some(crate::tools::VOICE_GPU)];
                for t in tools.into_iter().flatten().filter(|_| voices::test_voice().is_none()) {
                    if crate::tools::exe(&r.paths, &t).is_some() {
                        continue;
                    }
                    let mut on = |e: Event| {
                        if let Event::Progress { done } = e {
                            emit("program", done, t.size, None);
                        }
                    };
                    if crate::tools::install(&r.paths, &t, &stop, &mut on)? == Finished::Paused {
                        return Ok(Finished::Paused);
                    }
                }
                let mut before = 0;
                for f in m.files {
                    let dest = voices::file(&r.paths, &m, f);
                    if !std::fs::metadata(&dest).is_ok_and(|x| x.len() == f.size) {
                        let job = crate::models::Download { url: voices::url(f), dest, size: f.size, sha256: f.sha256.into() };
                        let got = crate::models::download(&job, &stop, &mut |e| match e {
                            Event::Progress { done } | Event::Verifying { done } => emit("model", before + done, total, None),
                            Event::Retrying { .. } => emit("retrying", before, total, None),
                        })?;
                        if got == Finished::Paused {
                            return Ok(Finished::Paused);
                        }
                    }
                    before += f.size;
                }
                Ok(Finished::Complete)
            })();
            r.downloads.lock().unwrap().remove(m.id);
            match result {
                Ok(crate::models::Finished::Complete) => emit("done", total, total, None),
                Ok(crate::models::Finished::Paused) => emit("paused", 0, total, None),
                Err(e) => emit("failed", 0, total, Some(format!("{e:#}"))),
            }
        });
        Ok(())
    }

    pub fn pause(&self, id: &str) {
        if let Some(s) = self.downloads.lock().unwrap().get(id) {
            s.store(true, Ordering::SeqCst);
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let m = voices::model(id).ok_or_else(|| anyhow!("no voice model {id}"))?;
        self.pause(id);
        // the programs let go of its files first
        self.end_all.store(true, Ordering::SeqCst);
        self.wake.notify_all();
        let until = std::time::Instant::now() + Duration::from_secs(20);
        while self.end_all.load(Ordering::SeqCst) && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        let dir = voices::dir(&self.paths, &m);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|_| anyhow!(crate::tr!("{} is in use. Close what uses it and try again.", dir.display())))?;
        }
        Ok(())
    }
}

/// The speech engine or the translator is about to use the GPU: a voice holding it lets go first,
/// after the sentence it is making (they take turns, as the two of them do).
pub fn let_go_of_gpu(app: &AppHandle) {
    let Some(r) = app.try_state::<Arc<Reader>>() else { return };
    if !r.gpu_held.load(Ordering::SeqCst) {
        return;
    }
    r.let_go.store(true, Ordering::SeqCst);
    r.wake.notify_all();
    let until = std::time::Instant::now() + Duration::from_secs(20);
    while r.gpu_held.load(Ordering::SeqCst) && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ------------------------------------------------------------------ commands

type R<T> = Result<T, String>;

fn reader(app: &AppHandle) -> Arc<Reader> {
    app.state::<Arc<Reader>>().inner().clone()
}

/// Open something to read: a file (`path`), pasted text (`text`), or a transcript (`transcript`,
/// with `language` for one of its translations).
#[tauri::command]
pub async fn reading_open(app: AppHandle, path: Option<String>, text: Option<String>, transcript: Option<String>, language: Option<String>) -> R<DocView> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = reader(&app);
        r.stop();
        let doc = if let Some(p) = path {
            let read = formats::read_path(Path::new(&p)).map_err(|e| format!("{e:#}"))?;
            r.open(format!("file:{}", p.to_lowercase()), read.title, read.blocks)
        } else if let Some(t) = text {
            let title = title_of(&t);
            // text copied from a chat or a page is often Markdown
            let blocks = if formats::looks_like_markdown(&t) { formats::from_markdown(&t) } else { formats::from_text(&t) };
            r.open(format!("text:{}", short_hash(&t)), title, blocks)
        } else if let Some(id) = transcript {
            let core = app.state::<Arc<Core>>();
            let t = core.transcript(&id).map_err(|e| format!("{e:#}"))?;
            let shown = match language.as_deref().filter(|l| !l.is_empty()) {
                Some(l) => t.translated(l).ok_or_else(|| crate::tr!("That translation is not there."))?,
                None => t.clone(),
            };
            r.open(format!("transcript:{id}:{}", language.unwrap_or_default()), t.title.clone(), transcript_blocks(&shown))
        } else {
            Err(anyhow!("nothing to open"))
        }
        .map_err(|e| format!("{e:#}"))?;
        Ok(r.view(&doc))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reading_sentence(app: AppHandle, doc: String, index: usize) -> R<Ready> {
    tauri::async_runtime::spawn_blocking(move || reader(&app).sentence(&doc, index)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn reading_place(app: AppHandle, doc: String, index: usize) {
    reader(&app).set_place(&doc, index);
}

#[tauri::command]
pub fn reading_stop(app: AppHandle) {
    reader(&app).stop();
}

#[tauri::command]
pub fn reading_voices(app: AppHandle) -> VoicesView {
    reader(&app).voices()
}

#[tauri::command]
pub fn reading_choose_voice(app: AppHandle, lang: String, key: String) -> R<VoicesView> {
    let r = reader(&app);
    r.choose(&lang, &key).map_err(|e| format!("{e:#}"))?;
    Ok(r.voices())
}

#[tauri::command]
pub fn reading_download_voice(app: AppHandle, id: String) -> R<()> {
    reader(&app).download(&id).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn reading_pause_voice(app: AppHandle, id: String) {
    reader(&app).pause(&id);
}

#[tauri::command]
pub fn reading_delete_voice(app: AppHandle, id: String) -> R<VoicesView> {
    let r = reader(&app);
    r.delete(&id).map_err(|e| format!("{e:#}"))?;
    Ok(r.voices())
}

/// The files Read aloud opens, for dropping.
#[tauri::command]
pub fn reading_formats() -> Vec<&'static str> {
    formats::EXTENSIONS.to_vec()
}

#[tauri::command]
pub async fn reading_pick_file(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .set_title(crate::tr!("Choose a file to read aloud"))
        .add_filter(crate::tr!("Text"), formats::EXTENSIONS)
        .add_filter(crate::tr!("All files"), &["*"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_become_sentences_with_pauses_and_voices_by_language() {
        let blocks = vec![
            Block { kind: Kind::Heading, text: "عادة القراءة اليومية في البيت وفي العمل".into() },
            Block { kind: Kind::Paragraph, text: "قال المدير: we will ship it next week, no matter what. ارتفعت الأسعار 15%.".into() },
            Block { kind: Kind::Item, text: "The end".into() },
        ];
        let (lang, s) = sentences_of(&blocks);
        assert_eq!(lang, Lang::Ar);
        assert_eq!(s.len(), 4);
        assert_eq!((s[0].kind, s[0].pause), (Kind::Heading, PAUSE_HEADING));
        assert_eq!(s[1].pause, PAUSE_SENTENCE);
        assert_eq!(s[2].pause, PAUSE_PARAGRAPH, "the last sentence of a paragraph");
        assert_eq!(spoken(&s[1].text, lang).iter().map(|r| r.lang).collect::<Vec<_>>(), vec![Lang::Ar, Lang::En]);
        // numbers are worded in the language around them; what is shown stays as written
        assert_eq!(spoken(&s[2].text, lang)[0].text, "ارتفعت الأسعار خمسة عشر بالمئة.");
        assert_eq!(s[2].text, "ارتفعت الأسعار 15%.");
        assert_eq!(spoken(&s[3].text, lang)[0].lang, Lang::En);
    }

    /// The real voices, through the app's own process and request code, on the processor:
    /// `DAWWIN_TEST_VOICE_EXE=<dawwin-voice.exe beside its library and VoxCPM 2's voices>
    /// DAWWIN_TEST_VOICE_DIR=<a folder of voices\<model id>\… as the app downloads them>
    /// [DAWWIN_TEST_VOICE_OUT=<folder>] cargo test real_voice -- --ignored --nocapture`. Each model
    /// there reads a heading, numbers, and Arabic with English inside (in what it reads), with a
    /// man's and a woman's voice. The Voice bench runs it.
    #[test]
    #[ignore]
    fn real_voice_reads_with_each_chosen_model() {
        let exe = PathBuf::from(std::env::var("DAWWIN_TEST_VOICE_EXE").expect("DAWWIN_TEST_VOICE_EXE"));
        let root = PathBuf::from(std::env::var("DAWWIN_TEST_VOICE_DIR").expect("DAWWIN_TEST_VOICE_DIR"));
        let out = std::env::var_os("DAWWIN_TEST_VOICE_OUT").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join(format!("dawwin-real-voice-{}", std::process::id())));
        std::fs::create_dir_all(&out).unwrap();
        let program_dir = exe.parent().unwrap().to_path_buf();
        let mut tried = 0;
        for m in [voices::VOXCPM2, voices::SUPERTONIC, voices::PHONON] {
            let model_dir = root.join("voices").join(m.id);
            if !m.files.iter().all(|f| f.name.split('/').fold(model_dir.clone(), |p, x| p.join(x)).is_file()) {
                println!("{}: not there", m.name);
                continue;
            }
            tried += 1;
            let mut p = voice::VoiceProcess::spawn(&exe, None, &out.join(format!("{}.log", m.id))).unwrap();
            let started = std::time::Instant::now();
            p.load(&voice::Load { family: m.family.into(), model: model_dir.clone(), gpu: false, options: Vec::new() }).unwrap();
            println!("{}: loaded in {:.1} s on {}", m.name, started.elapsed().as_secs_f32(), p.device);
            let text = if m.reads(Lang::Ar) {
                "# عادة القراءة\n\nفي 12 مارس 2026 قال المدير: we will ship it next week, no matter what. رفعنا النسخة على GitHub أمس، وارتفعت الأسعار 15%.\n\nThe meeting is at 10:30 a.m. in room 7."
            } else {
                "# Starting a small garden\n\nYou do not need a large yard to grow your own vegetables. The meeting is at 10:30 a.m. in room 7, and it costs $66.50."
            };
            let (lang, sentences) = sentences_of(&formats::from_markdown(text));
            for woman in [true, false] {
                let mut whole = Vec::new();
                for (i, s) in sentences.iter().enumerate() {
                    let mut pieces = Vec::new();
                    for (k, run) in spoken(&s.text, lang).iter().enumerate() {
                        // a run in a language the model does not read goes to the other model in the app
                        let Some(v) = m.voices.iter().find(|v| v.lang == run.lang && v.woman == woman) else { continue };
                        let wav = out.join(format!("{}-{i}-{k}.wav", m.id));
                        let t = std::time::Instant::now();
                        let secs = p.speak(&run.text, m.code(run.lang), &voices::request(&m, v, &model_dir, &program_dir), &wav).unwrap();
                        println!("  {} {} {:.2} s for {:.2} s: {}", v.id, run.lang.code(), t.elapsed().as_secs_f32(), secs, run.text);
                        assert!(secs > 0.3, "{}", run.text);
                        pieces.push(voice::read_wav(&wav).unwrap());
                        let _ = std::fs::remove_file(&wav);
                    }
                    if !pieces.is_empty() {
                        whole.push(voice::join(&pieces, PAUSE_PIECE, s.pause));
                    }
                }
                // all of it as one file, as it is heard
                let (rate, samples) = voice::join(&whole, 0.0, 0.0);
                let file = out.join(format!("{}-{}.wav", m.id, if woman { "woman" } else { "man" }));
                voice::write_wav(&file, rate, &samples).unwrap();
                println!("  {:.1} s of speech in {}", samples.len() as f64 / rate as f64, file.display());
            }
        }
        assert!(tried > 0, "no voice model in {}", root.display());
    }

    #[test]
    fn pasted_text_is_named_by_its_first_sentence() {
        assert_eq!(title_of("
  رفعنا النسخة على GitHub أمس. وفي نهاية الاجتماع قال المدير"), "رفعنا النسخة على GitHub أمس.");
        assert_eq!(title_of("# Working from home

Text."), "Working from home");
        let long = title_of(&"word ".repeat(40));
        assert!(long.ends_with('…') && long.chars().count() <= 61, "{long}");
    }

    #[test]
    fn a_transcript_runs_on_until_a_long_pause() {
        let b = lines_to_blocks([(0.0, 2.0, "Hello there,"), (2.2, 4.0, "my friend."), (7.0, 8.0, "Later.")].into_iter());
        assert_eq!(b.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), vec!["Hello there, my friend.", "Later."]);
    }
}
