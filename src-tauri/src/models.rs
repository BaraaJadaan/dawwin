//! The speech models: which ones there are, where they live, and how they get here.
//!
//! A download survives anything: the connection dropping, the app closing or crashing, the power
//! going out. It writes to `<file>.part`, and `<file>.part.ok` records how much of that is known to
//! be on the disk. A restart continues from there with an HTTP range request, and the file only
//! takes its real name after its checksum matches.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};

/// Fall back on the disk at most this much after a crash or a power cut, instead of the whole download.
const SYNC_EVERY: u64 = 8 * 1024 * 1024;
/// And at most this much time: on a slow connection those megabytes take minutes.
const SYNC_AFTER: Duration = Duration::from_secs(10);
const BUFFER: usize = 256 * 1024;
/// How often a download starts over because the server sent another file, before it gives up
const START_OVERS: u64 = 3;

/// What a model is built on. It decides which languages the model is used for. The translator is
/// the one that is no speech model: the app uses one at a time, whichever of them it is. The
/// speaker and language models do not write text either: they tell who speaks, and in which language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Cohere,
    Canary,
    Whisper,
    Translator,
    Speakers,
    Language,
}

impl Family {
    pub const ALL: [Family; 6] = [Family::Cohere, Family::Canary, Family::Whisper, Family::Translator, Family::Speakers, Family::Language];

    /// What the settings and the interface call it.
    pub fn key(self) -> &'static str {
        match self {
            Family::Cohere => "cohere",
            Family::Canary => "canary",
            Family::Whisper => "whisper",
            Family::Translator => "translator",
            Family::Speakers => "speakers",
            Family::Language => "language",
        }
    }

    fn from_architecture(arch: &str) -> Option<Family> {
        match arch {
            "cohere_asr" => Some(Family::Cohere),
            "canary" => Some(Family::Canary),
            "whisper" => Some(Family::Whisper),
            "sortformer" => Some(Family::Speakers),
            "ecapa_tdnn" => Some(Family::Language),
            _ => None,
        }
    }

    /// The name on a transcript made with a model file of the user's own.
    pub fn label(self) -> &'static str {
        match self {
            Family::Cohere => "Cohere Transcribe Arabic (07-2026)",
            Family::Canary => "NVIDIA Canary",
            Family::Whisper => "OpenAI Whisper",
            Family::Translator => "MiLMMT-46",
            Family::Speakers => "NVIDIA Streaming Sortformer 4spk v2.1",
            Family::Language => "VoxLingua107 ECAPA-TDNN",
        }
    }
}

/// How a translation model wants to be asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    /// Xiaomi's MiLMMT-46, as its model card asks: a plain text to complete, one line.
    MiLMMT,
}

impl Prompt {
    /// The text the model completes with the translation, and where the translation ends.
    pub fn build(self, text: &str, from: &str, to: &str) -> (String, Vec<&'static str>) {
        match self {
            Prompt::MiLMMT => {
                let (s, t) = (milmmt_name(from), milmmt_name(to));
                (format!("Translate this from {s} to {t}:\n{s}: {text}\n{t}:"), vec!["\n"])
            }
        }
    }
}

/// What MiLMMT-46 was trained on, by the names its prompt needs. It also reads other languages,
/// "not guaranteed" to do them well; those it is told the English name of.
pub const MILMMT_LANGUAGES: &[(&str, &str)] = &[
    ("ar", "Arabic"), ("az", "Azerbaijani"), ("bg", "Bulgarian"), ("bn", "Bengali"), ("ca", "Catalan"),
    ("cs", "Czech"), ("da", "Danish"), ("de", "German"), ("el", "Greek"), ("en", "English"),
    ("es", "Spanish"), ("fa", "Persian"), ("fi", "Finnish"), ("fr", "French"), ("he", "Hebrew"),
    ("hi", "Hindi"), ("hr", "Croatian"), ("hu", "Hungarian"), ("id", "Indonesian"), ("it", "Italian"),
    ("ja", "Japanese"), ("kk", "Kazakh"), ("km", "Khmer"), ("ko", "Korean"), ("lo", "Lao"),
    ("ms", "Malay"), ("my", "Burmese"), ("nl", "Dutch"), ("no", "Norwegian"), ("pl", "Polish"),
    ("pt", "Portuguese"), ("ro", "Romanian"), ("ru", "Russian"), ("sk", "Slovak"), ("sl", "Slovenian"),
    ("sv", "Swedish"), ("ta", "Tamil"), ("th", "Thai"), ("tl", "Tagalog"), ("tr", "Turkish"),
    ("ur", "Urdu"), ("uz", "Uzbek"), ("vi", "Vietnamese"), ("yue", "Cantonese"),
    ("zh", "Chinese (Simplified)"), ("zh-Hant", "Chinese (Traditional)"),
];

fn milmmt_name(code: &str) -> &str {
    MILMMT_LANGUAGES.iter().find(|(c, _)| *c == code).map_or_else(|| language_name(code), |(_, n)| *n)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub id: &'static str,
    pub family: Family,
    /// The name on the transcripts it makes
    pub label: &'static str,
    repository: &'static str,
    /// The revision the checksum belongs to: it cannot change under the app.
    revision: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    /// How to ask it, for a translation model
    pub prompt: Option<Prompt>,
}

const COHERE_ARABIC: &str = "handy-computer/cohere-transcribe-arabic-07-2026-gguf";
const COHERE_ARABIC_REVISION: &str = "11a54d6895f94cce78d0ece142b8e552e7822539";

pub const VARIANTS: [Variant; 9] = [
    Variant {
        id: "q8",
        family: Family::Cohere,
        label: "Cohere Transcribe Arabic (07-2026) · Q8",
        repository: COHERE_ARABIC,
        revision: COHERE_ARABIC_REVISION,
        file: "cohere-transcribe-arabic-07-2026-Q8_0.gguf",
        sha256: "910de5c9c57f9fd8a280e1701f9cd96f63768878c1ca9e4ecf23638a5e0fef16",
        size: 2_410_655_136,
        prompt: None,
    },
    Variant {
        id: "q4",
        family: Family::Cohere,
        label: "Cohere Transcribe Arabic (07-2026) · Q4",
        repository: COHERE_ARABIC,
        revision: COHERE_ARABIC_REVISION,
        file: "cohere-transcribe-arabic-07-2026-Q4_K_M.gguf",
        sha256: "4671080cd10ffc6166e1e1b36216f9b51e1455f170d55c59cff7b81332ad4621",
        size: 1_558_162_848,
        prompt: None,
    },
    Variant {
        id: "canary",
        family: Family::Canary,
        label: "NVIDIA Canary 1B v2 · Q4",
        repository: "handy-computer/canary-1b-v2-gguf",
        revision: "692af661b140183b60e44f55ad03c59e1e77d078",
        file: "canary-1b-v2-Q4_K_M.gguf",
        sha256: "49e0a67e219bec95a254c2348460b6350e75a7ac6f93a131e48244b4c7cb53b9",
        size: 735_476_448,
        prompt: None,
    },
    Variant {
        id: "whisper",
        family: Family::Whisper,
        label: "OpenAI Whisper large-v3 turbo · Q4",
        repository: "handy-computer/whisper-large-v3-turbo-gguf",
        revision: "18ada752005f5fddef499256f9a458d0b37a203a",
        file: "whisper-large-v3-turbo-Q4_K_M.gguf",
        sha256: "c6e172d0074f0fa9440b88247854b50643cd4bb58b8515c797bbed16d858c7df",
        size: 536_069_696,
        prompt: None,
    },
    // Translation. Xiaomi's MiLMMT-46 (built on Gemma 3, Gemma terms of use) scored best of ten
    // open translation models in our test on WMT24++ (tools/bench-translate): the 12B best of all
    // over seven directions, and the 4B as good as the 12B from and into Arabic, at a third of its size.
    Variant {
        id: "milmmt-12b",
        family: Family::Translator,
        label: "MiLMMT-46 12B · Q4",
        repository: "mradermacher/MiLMMT-46-12B-v1.0-GGUF",
        revision: "67b6e43ad31a3aad46bafc4f899a9c0d5eec898d",
        file: "MiLMMT-46-12B-v1.0.Q4_K_M.gguf",
        sha256: "8c2d878d8daf8722faf1a42eef02ce046411ede17bf3115fb8b2cce491d5fb03",
        size: 7_300_777_440,
        prompt: Some(Prompt::MiLMMT),
    },
    Variant {
        id: "milmmt-4b",
        family: Family::Translator,
        label: "MiLMMT-46 4B · Q4",
        repository: "mradermacher/MiLMMT-46-4B-v1.0-GGUF",
        revision: "3d05b58ace3d89a89af0574cdc49c84567f33969",
        file: "MiLMMT-46-4B-v1.0.Q4_K_M.gguf",
        sha256: "25424cbb5423afaa22b1b53bfb3d8aa116cfd84b5f8dcfa82363bfb3c4221ddf",
        size: 2_489_893_152,
        prompt: Some(Prompt::MiLMMT),
    },
    // the smallest: a third of the 4B's download, for slow connections and older computers
    Variant {
        id: "milmmt-1b",
        family: Family::Translator,
        label: "MiLMMT-46 1B · Q4",
        repository: "mradermacher/MiLMMT-46-1B-v1.0-GGUF",
        revision: "34df5efbe6592773ec168cc7b307728c08623472",
        file: "MiLMMT-46-1B-v1.0.Q4_K_M.gguf",
        sha256: "74d38ba75108d455326e9deeaf9ab01bb266dfa665eae9c4aa84e84485d4fdf9",
        size: 806_057_408,
        prompt: Some(Prompt::MiLMMT),
    },
    // Who speaks when, up to four people (NVIDIA Open Model License). Only near-reference
    // quantizations exist for it: the smaller ones swap speakers in the middle of a recording.
    Variant {
        id: "speakers",
        family: Family::Speakers,
        label: "NVIDIA Streaming Sortformer 4spk v2.1 · Q8",
        repository: "handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf",
        revision: "60ebab3ef7cae9869be0abcde13360347e96ba03",
        file: "diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf",
        sha256: "a5dacdc650790266c7a362e54e6bf51952015487edaa606c4e11632bc32442a9",
        size: 139_310_336,
        prompt: None,
    },
    // Which language is spoken, of 107 (SpeechBrain's VoxLingua107 ECAPA-TDNN, Apache-2.0)
    Variant {
        id: "language",
        family: Family::Language,
        label: "VoxLingua107 ECAPA-TDNN · Q8",
        repository: "handy-computer/lang-id-voxlingua107-ecapa-gguf",
        revision: "e59f0f3847e77723b7d58447ef9456fa0c7e9436",
        file: "lang-id-voxlingua107-ecapa-Q8_0.gguf",
        sha256: "063e6cb0af3d53085fc7d68356dfde5ecee2ffa7bc79855f7e4ff930081bfe3a",
        size: 26_693_632,
        prompt: None,
    },
];

/// Every spoken language the app offers, with its English name: what Whisper knows.
const LANGUAGES: &[(&str, &str)] = &[
    ("af", "Afrikaans"), ("am", "Amharic"), ("ar", "Arabic"), ("as", "Assamese"), ("az", "Azerbaijani"),
    ("ba", "Bashkir"), ("be", "Belarusian"), ("bg", "Bulgarian"), ("bn", "Bengali"), ("bo", "Tibetan"),
    ("br", "Breton"), ("bs", "Bosnian"), ("ca", "Catalan"), ("cs", "Czech"), ("cy", "Welsh"),
    ("da", "Danish"), ("de", "German"), ("el", "Greek"), ("en", "English"), ("es", "Spanish"),
    ("et", "Estonian"), ("eu", "Basque"), ("fa", "Persian"), ("fi", "Finnish"), ("fo", "Faroese"),
    ("fr", "French"), ("gl", "Galician"), ("gu", "Gujarati"), ("ha", "Hausa"), ("haw", "Hawaiian"),
    ("he", "Hebrew"), ("hi", "Hindi"), ("hr", "Croatian"), ("ht", "Haitian Creole"), ("hu", "Hungarian"),
    ("hy", "Armenian"), ("id", "Indonesian"), ("is", "Icelandic"), ("it", "Italian"), ("ja", "Japanese"),
    ("jw", "Javanese"), ("ka", "Georgian"), ("kk", "Kazakh"), ("km", "Khmer"), ("kn", "Kannada"),
    ("ko", "Korean"), ("la", "Latin"), ("lb", "Luxembourgish"), ("ln", "Lingala"), ("lo", "Lao"),
    ("lt", "Lithuanian"), ("lv", "Latvian"), ("mg", "Malagasy"), ("mi", "Maori"), ("mk", "Macedonian"),
    ("ml", "Malayalam"), ("mn", "Mongolian"), ("mr", "Marathi"), ("ms", "Malay"), ("mt", "Maltese"),
    ("my", "Burmese"), ("ne", "Nepali"), ("nl", "Dutch"), ("nn", "Norwegian Nynorsk"), ("no", "Norwegian"),
    ("oc", "Occitan"), ("pa", "Punjabi"), ("pl", "Polish"), ("ps", "Pashto"), ("pt", "Portuguese"),
    ("ro", "Romanian"), ("ru", "Russian"), ("sa", "Sanskrit"), ("sd", "Sindhi"), ("si", "Sinhala"),
    ("sk", "Slovak"), ("sl", "Slovenian"), ("sn", "Shona"), ("so", "Somali"), ("sq", "Albanian"),
    ("sr", "Serbian"), ("su", "Sundanese"), ("sv", "Swedish"), ("sw", "Swahili"), ("ta", "Tamil"),
    ("te", "Telugu"), ("tg", "Tajik"), ("th", "Thai"), ("tk", "Turkmen"), ("tl", "Filipino"),
    ("tr", "Turkish"), ("tt", "Tatar"), ("uk", "Ukrainian"), ("ur", "Urdu"), ("uz", "Uzbek"),
    ("vi", "Vietnamese"), ("yi", "Yiddish"), ("yo", "Yoruba"), ("yue", "Cantonese"), ("zh", "Chinese"),
];

/// What Canary was trained on. On these it makes fewer mistakes than Whisper, and is several times faster.
const CANARY_LANGUAGES: &[&str] = &[
    "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hr", "hu", "it", "lt", "lv", "mt", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "uk",
];

const RIGHT_TO_LEFT: &[&str] = &["ar", "fa", "he", "ps", "sd", "ur", "yi"];

pub fn languages() -> impl Iterator<Item = &'static str> {
    LANGUAGES.iter().map(|(code, _)| *code)
}

pub fn known_language(code: &str) -> bool {
    languages().any(|l| l == code)
}

pub fn is_right_to_left(code: &str) -> bool {
    RIGHT_TO_LEFT.contains(&code)
}

/// The name of a language in English, or the code of one the app does not know. A language that is
/// only translated into ("zh-Hant") has its name from the translation model's list.
pub fn language_name<'a>(code: &'a str) -> &'a str {
    LANGUAGES.iter().chain(MILMMT_LANGUAGES).find(|(c, _)| *c == code).map_or(code, |(_, name)| *name)
}

/// The models for a spoken language, the best first.
pub fn models_for(language: &str) -> Vec<Variant> {
    let ids: &[&str] = match language {
        // Arabic is what this app is for: only the model trained on its dialects will do
        "ar" => &["q8", "q4"],
        "en" => &["q8", "q4", "canary", "whisper"],
        l if CANARY_LANGUAGES.contains(&l) => &["canary", "whisper"],
        l if known_language(l) => &["whisper"],
        _ => &[],
    };
    ids.iter().filter_map(|id| variant(id)).collect()
}

pub fn variant(id: &str) -> Option<Variant> {
    VARIANTS.into_iter().find(|v| v.id == id)
}

/// The languages a transcript can be translated into: code, and name in English.
pub fn translation_targets() -> impl Iterator<Item = (&'static str, &'static str)> {
    MILMMT_LANGUAGES.iter().copied()
}

pub fn can_translate_into(code: &str) -> bool {
    translation_targets().any(|(c, _)| c == code)
}

/// Which of ours this file is, by name.
pub fn variant_of(path: &Path) -> Option<Variant> {
    let name = path.file_name()?.to_str()?;
    VARIANTS.into_iter().find(|v| v.file.eq_ignore_ascii_case(name))
}

/// Hugging Face, or the mirror named by HF_ENDPOINT (its usual setting) where it is blocked.
pub fn endpoint() -> String {
    std::env::var("HF_ENDPOINT").ok().filter(|e| !e.is_empty()).unwrap_or_else(|| "https://huggingface.co".into()).trim_end_matches('/').to_string()
}

pub fn url(v: Variant) -> String {
    format!("{}/{}/resolve/{}/{}", endpoint(), v.repository, v.revision, v.file)
}

/// Where to download a model by hand.
pub fn files_page(v: Variant) -> String {
    format!("{}/{}/tree/{}", endpoint(), v.repository, v.revision)
}

pub fn part_path(dest: &Path) -> PathBuf {
    with_suffix(dest, ".part")
}

fn marker_path(dest: &Path) -> PathBuf {
    with_suffix(dest, ".part.ok")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// How much of this download is safely on the disk.
pub fn downloaded(dest: &Path) -> u64 {
    let part = std::fs::metadata(part_path(dest)).map(|m| m.len()).unwrap_or(0);
    let marker = std::fs::read_to_string(marker_path(dest)).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    part.min(marker)
}

/// Remove a download that is not finished.
pub fn discard(dest: &Path) {
    let _ = std::fs::remove_file(part_path(dest));
    let _ = std::fs::remove_file(marker_path(dest));
}

/// Take an unfinished download along to another folder. On another drive it stays behind: moving
/// gigabytes is not worth making the user wait for.
pub fn move_unfinished(from: &Path, to: &Path) {
    if from != to && std::fs::rename(part_path(from), part_path(to)).is_ok() {
        let _ = std::fs::rename(marker_path(from), marker_path(to));
    }
}

/// The architecture a GGUF file says it holds: after the key come the value's type (4 bytes), its
/// length (8 bytes) and the text.
fn architecture(head: &[u8]) -> Option<&str> {
    const KEY: &[u8] = b"general.architecture";
    let at = head.windows(KEY.len()).position(|w| w == KEY)? + KEY.len();
    let len = u64::from_le_bytes(head.get(at + 4..at + 12)?.try_into().ok()?) as usize;
    std::str::from_utf8(head.get(at + 12..(at + 12).checked_add(len)?)?).ok()
}

/// A finished model file that the engine can try to load, and what kind it is. Our own files must
/// have their exact size, so a file cut short by a copy is not taken for a model.
pub fn check_file(path: &Path) -> Result<Family> {
    let meta = std::fs::metadata(path).map_err(|_| anyhow!(crate::tr!("That file could not be read.")))?;
    if !meta.is_file() {
        bail!(crate::tr!("That is not a model file."));
    }
    if let Some(v) = variant_of(path) {
        if meta.len() != v.size {
            bail!(crate::tr!("That model file is incomplete."));
        }
    }
    let mut head = vec![0u8; 256 * 1024];
    let mut f = std::fs::File::open(path)?;
    let n = f.read(&mut head)?;
    head.truncate(n);
    if !head.starts_with(b"GGUF") {
        bail!(crate::tr!("That is not a model file."));
    }
    // a translation model is one of ours: its prompt depends on which model it is
    if let Some(v) = variant_of(path).filter(|v| v.family == Family::Translator) {
        return Ok(v.family);
    }
    architecture(&head)
        .and_then(Family::from_architecture)
        .ok_or_else(|| anyhow!(crate::tr!("Dawwin cannot use that model. It works with Cohere Transcribe, Canary and Whisper model files.")))
}

/// Whether the family writes text: the speech models, not the translator nor the ones that tell
/// speakers or languages apart.
pub fn transcribes(f: Family) -> bool {
    matches!(f, Family::Cohere | Family::Canary | Family::Whisper)
}

/// A model file in this folder, of this family if one is named, ours first and the larger first.
pub fn find_in_folder(dir: &Path, family: Option<Family>) -> Option<PathBuf> {
    let wanted = |p: &PathBuf| check_file(p).is_ok_and(|f| family.is_none() || family == Some(f));
    let ours = VARIANTS.iter().map(|v| dir.join(v.file)).find(wanted);
    ours.or_else(|| {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("gguf")))
            .find(wanted)
    })
}

// ---------------------------------------------------------------- download

pub struct Download {
    pub url: String,
    pub dest: PathBuf,
    pub size: u64,
    pub sha256: String,
}

pub enum Event {
    Progress { done: u64 },
    /// The connection is gone; trying again after this many seconds
    Retrying { wait_s: u64, attempt: u64 },
    /// All of it is here; reading it back to check it
    Verifying { done: u64 },
}

#[derive(Debug, PartialEq)]
pub enum Finished {
    Complete,
    Paused,
}

enum Fetch {
    Complete,
    Stopped,
    /// Try again later
    Retry,
    /// The file on the server does not continue what we have
    StartOver,
}

pub fn download(d: &Download, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Finished> {
    if let Some(dir) = d.dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        // a connection that sends nothing for this long is dead, however slow it was before
        .timeout_read(Duration::from_secs(60))
        .tls_connector(std::sync::Arc::new(native_tls::TlsConnector::new()?))
        .build();
    let mut attempt = 0u64;
    let mut start_overs = 0u64;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(Finished::Paused);
        }
        let before = downloaded(&d.dest);
        if before < d.size {
            need_space(d.dest.parent().unwrap_or(Path::new(".")), d.size - before)?;
        }
        match fetch(&agent, d, before, stop, on_event)? {
            Fetch::Complete => break,
            Fetch::Stopped => return Ok(Finished::Paused),
            Fetch::StartOver => {
                // a server that sends another file: from the start again, after a wait, a few times
                discard(&d.dest);
                attempt = 0;
                start_overs += 1;
                if start_overs > START_OVERS {
                    bail!(crate::tr!("The server keeps sending a different file. Try again later, or download it by hand."));
                }
                let wait_s = if cfg!(test) { 0 } else { 10 * start_overs };
                on_event(Event::Retrying { wait_s, attempt: start_overs });
                for _ in 0..wait_s * 5 {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(Finished::Paused);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            Fetch::Retry => {
                // what counts is whether it is getting anywhere, not how often it dropped
                attempt = if downloaded(&d.dest) > before { 1 } else { attempt + 1 };
                let wait_s = [5, 10, 20, 30, 60].get(attempt as usize - 1).copied().unwrap_or(60);
                on_event(Event::Retrying { wait_s, attempt });
                for _ in 0..wait_s * 5 {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(Finished::Paused);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    }
    // for trying the rest of the flow against a stand-in file; not there in release builds
    let trusted = cfg!(debug_assertions) && std::env::var_os("DAWWIN_TEST_TRUST_DOWNLOAD").is_some();
    if !trusted && !verify(d, stop, on_event)? {
        return if stop.load(Ordering::SeqCst) {
            Ok(Finished::Paused)
        } else {
            discard(&d.dest);
            bail!(crate::tr!("The download arrived damaged, so it was removed. Please start it again."))
        };
    }
    std::fs::rename(part_path(&d.dest), &d.dest)?;
    let _ = std::fs::remove_file(marker_path(&d.dest));
    Ok(Finished::Complete)
}

fn fetch(agent: &ureq::Agent, d: &Download, offset: u64, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Fetch> {
    if offset >= d.size {
        return Ok(Fetch::Complete);
    }
    let response = agent
        .get(&d.url)
        .set("User-Agent", concat!("Dawwin/", env!("CARGO_PKG_VERSION")))
        .set("Range", &format!("bytes={offset}-"))
        .call();
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(416, _)) => return Ok(Fetch::StartOver),
        Err(ureq::Error::Status(code, _)) if code == 429 || code >= 500 => return Ok(Fetch::Retry),
        Err(ureq::Error::Status(code, _)) => bail!(crate::tr!("The server refused the download (error {}).", code)),
        Err(ureq::Error::Transport(_)) => return Ok(Fetch::Retry),
    };
    // a server may answer a range request with the whole file: then that is what we write
    let mut at = if response.status() == 206 { offset } else { 0 };
    if response.status() == 206 && content_range_start(&response) != Some(offset) {
        return Ok(Fetch::StartOver);
    }

    let mut file = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(part_path(&d.dest))?;
    file.set_len(at)?; // what was written after the last sync may not have reached the disk
    file.seek(SeekFrom::Start(at))?;
    write_marker(&d.dest, at)?;

    let mut body = response.into_reader();
    let mut buf = vec![0u8; BUFFER];
    let mut unsynced = 0u64;
    let mut synced = Instant::now();
    let mut told = Instant::now();
    on_event(Event::Progress { done: at });
    let end = loop {
        if stop.load(Ordering::SeqCst) {
            break Fetch::Stopped;
        }
        let n = match body.read(&mut buf) {
            Ok(0) => break if at >= d.size { Fetch::Complete } else { Fetch::Retry },
            Ok(n) => n,
            Err(_) => break Fetch::Retry,
        };
        if at + n as u64 > d.size {
            // longer than the file we are after: it is not that file
            break Fetch::StartOver;
        }
        file.write_all(&buf[..n])?;
        at += n as u64;
        unsynced += n as u64;
        if unsynced >= SYNC_EVERY || synced.elapsed() >= SYNC_AFTER {
            file.sync_data()?;
            write_marker(&d.dest, at)?;
            unsynced = 0;
            synced = Instant::now();
        }
        if told.elapsed() >= Duration::from_millis(250) {
            on_event(Event::Progress { done: at });
            told = Instant::now();
        }
        if at == d.size {
            break Fetch::Complete;
        }
    };
    file.sync_data()?;
    write_marker(&d.dest, at)?;
    on_event(Event::Progress { done: at });
    Ok(end)
}

fn content_range_start(r: &ureq::Response) -> Option<u64> {
    r.header("Content-Range")?.trim().strip_prefix("bytes ")?.split('-').next()?.trim().parse().ok()
}

/// Replaced in one step, so that it never reads as less than what is really there.
fn write_marker(dest: &Path, len: u64) -> Result<()> {
    let tmp = with_suffix(dest, ".part.ok.tmp");
    std::fs::write(&tmp, len.to_string())?;
    std::fs::rename(&tmp, marker_path(dest))?;
    Ok(())
}

fn verify(d: &Download, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<bool> {
    let mut f = std::fs::File::open(part_path(&d.dest))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut done = 0u64;
    let mut told = Instant::now();
    on_event(Event::Verifying { done: 0 });
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        done += n as u64;
        if told.elapsed() >= Duration::from_millis(250) {
            on_event(Event::Verifying { done });
            told = Instant::now();
        }
    }
    Ok(format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&d.sha256))
}

#[cfg(windows)]
fn free_space(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, free: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` ends in a NUL and outlives the call; the other two outputs may be null.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
fn free_space(_dir: &Path) -> Option<u64> {
    None
}

fn need_space(dir: &Path, bytes: u64) -> Result<()> {
    let margin = 200 * 1024 * 1024;
    if let Some(free) = free_space(dir) {
        if free < bytes + margin {
            bail!(crate::tr!(
                "Not enough free space: the model needs {} GB more, and {} GB is free. Choose another folder or free some space.",
                format!("{:.1}", bytes as f64 / 1e9),
                format!("{:.1}", free as f64 / 1e9)
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Copy, PartialEq)]
    enum Serve {
        Normal,
        /// Closes the first connection after this many bytes
        DropAfter(usize),
        /// Answers every range request with the whole file
        IgnoresRanges,
        NotFound,
        /// The first answer is a server error
        BusyOnce,
        /// Always sends a file longer than the one asked for: another file
        Longer,
    }

    struct Server {
        url: String,
        ranges: Arc<Mutex<Vec<Option<u64>>>>,
    }

    /// A small web server that speaks just enough HTTP, and can misbehave on request.
    fn serve(body: Vec<u8>, how: Serve) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.gguf", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = ranges.clone();
        std::thread::spawn(move || {
            let mut request_no = 0;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                request_no += 1;
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_lowercase().strip_prefix("range: bytes=") {
                        range = v.trim().trim_end_matches('-').parse::<u64>().ok();
                    }
                }
                seen.lock().unwrap().push(range);
                if how == Serve::NotFound {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                if how == Serve::BusyOnce && request_no == 1 {
                    let _ = stream.write_all(b"HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let from = match (range, how) {
                    (_, Serve::IgnoresRanges) | (None, _) => None,
                    (Some(r), _) => Some(r as usize),
                };
                let longer = [&body[..], b"!"].concat();
                let body = if how == Serve::Longer { &longer } else { &body };
                let from = if how == Serve::Longer { None } else { from };
                let part = &body[from.unwrap_or(0).min(body.len())..];
                let head = match from {
                    Some(r) => format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n", part.len(), r, body.len() - 1, body.len()),
                    None => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len()),
                };
                let _ = stream.write_all(head.as_bytes());
                match how {
                    Serve::DropAfter(n) if request_no == 1 => {
                        let _ = stream.write_all(&part[..n.min(part.len())]);
                    }
                    _ => {
                        let _ = stream.write_all(part);
                    }
                }
            }
        });
        Server { url, ranges }
    }

    fn payload() -> Vec<u8> {
        (0..700_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dawwin-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(server: &Server, dir: &Path, body: &[u8]) -> Download {
        Download { url: server.url.clone(), dest: dir.join("model.gguf"), size: body.len() as u64, sha256: sha(body) }
    }

    #[test]
    fn downloads_a_whole_file_and_checks_it() {
        let (body, dir) = (payload(), folder("whole"));
        let s = serve(body.clone(), Serve::Normal);
        let d = job(&s, &dir, &body);
        let end = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(end, Finished::Complete);
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
        assert!(!part_path(&d.dest).exists() && !marker_path(&d.dest).exists());
    }

    #[test]
    fn pausing_keeps_the_bytes_and_resuming_asks_only_for_the_rest() {
        let (body, dir) = (payload(), folder("pause"));
        let s = serve(body.clone(), Serve::Normal);
        let d = job(&s, &dir, &body);
        let stop = AtomicBool::new(false);
        let end = download(&d, &stop, &mut |e| {
            if let Event::Progress { .. } = e {
                stop.store(true, Ordering::SeqCst);
            }
        });
        // the very first progress event comes before any bytes: pausing there keeps nothing
        assert_eq!(end.unwrap(), Finished::Paused);
        assert!(!d.dest.exists(), "a paused download is not a model");

        // pretend it stopped half way
        std::fs::write(part_path(&d.dest), &body[..300_000]).unwrap();
        write_marker(&d.dest, 300_000).unwrap();
        assert_eq!(downloaded(&d.dest), 300_000);
        let end = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(end, Finished::Complete);
        assert_eq!(s.ranges.lock().unwrap().last().unwrap(), &Some(300_000), "asked for the rest only");
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_crash_falls_back_to_what_was_known_to_be_on_the_disk() {
        let (body, dir) = (payload(), folder("crash"));
        let s = serve(body.clone(), Serve::Normal);
        let d = job(&s, &dir, &body);
        // 400 000 bytes in the file, but only 100 000 were synced; the rest may be garbage
        let mut part = body[..100_000].to_vec();
        part.extend(std::iter::repeat_n(0xAAu8, 300_000));
        std::fs::write(part_path(&d.dest), part).unwrap();
        write_marker(&d.dest, 100_000).unwrap();
        assert_eq!(downloaded(&d.dest), 100_000);
        assert_eq!(download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap(), Finished::Complete);
        assert_eq!(s.ranges.lock().unwrap()[0], Some(100_000));
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_server_that_keeps_sending_another_file_is_given_up_with_a_message() {
        let (body, dir) = (payload(), folder("longer"));
        let s = serve(body.clone(), Serve::Longer);
        let d = job(&s, &dir, &body);
        let mut waits = 0;
        let end = crate::setup::within(Duration::from_secs(30), "giving up", move || {
            download(&d, &AtomicBool::new(false), &mut |e| {
                if let Event::Retrying { .. } = e {
                    waits += 1;
                }
            })
            .map(|_| waits)
        });
        let err = end.expect_err("it never gets the file it wants");
        assert!(err.to_string().contains("different file"), "{err:#}");
        assert_eq!(s.ranges.lock().unwrap().len() as u64, START_OVERS + 1, "a few tries, not for ever");
    }

    #[test]
    fn a_dropped_connection_carries_on_from_where_it_stopped() {
        let (body, dir) = (payload(), folder("drop"));
        let s = serve(body.clone(), Serve::DropAfter(250_000));
        let d = job(&s, &dir, &body);
        let mut retried = false;
        let end = download(&d, &AtomicBool::new(false), &mut |e| {
            if let Event::Retrying { .. } = e {
                retried = true;
            }
        })
        .unwrap();
        assert_eq!(end, Finished::Complete);
        assert!(retried);
        let ranges = s.ranges.lock().unwrap();
        assert_eq!(ranges[0], Some(0));
        assert_eq!(ranges[1], Some(250_000));
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_server_that_ignores_ranges_still_gives_the_right_file() {
        let (body, dir) = (payload(), folder("ignore"));
        let s = serve(body.clone(), Serve::IgnoresRanges);
        let d = job(&s, &dir, &body);
        std::fs::write(part_path(&d.dest), &body[..300_000]).unwrap();
        write_marker(&d.dest, 300_000).unwrap();
        assert_eq!(download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap(), Finished::Complete);
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_busy_server_is_tried_again() {
        let (body, dir) = (payload(), folder("busy"));
        let s = serve(body.clone(), Serve::BusyOnce);
        let d = job(&s, &dir, &body);
        assert_eq!(download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap(), Finished::Complete);
    }

    #[test]
    fn a_missing_file_is_an_error_and_not_a_retry() {
        let (body, dir) = (payload(), folder("missing"));
        let s = serve(body.clone(), Serve::NotFound);
        let d = job(&s, &dir, &body);
        let err = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(err.contains("404"), "{err}");
    }

    #[test]
    fn a_damaged_download_is_thrown_away() {
        let (body, dir) = (payload(), folder("damaged"));
        let s = serve(body.clone(), Serve::Normal);
        let mut d = job(&s, &dir, &body);
        d.sha256 = sha(b"something else");
        assert!(download(&d, &AtomicBool::new(false), &mut |_| {}).is_err());
        assert!(!d.dest.exists());
        assert!(!part_path(&d.dest).exists(), "so the next try starts clean");
    }

    #[test]
    fn pausing_during_the_retry_wait_stops_at_once() {
        let (body, dir) = (payload(), folder("wait"));
        let s = serve(body.clone(), Serve::BusyOnce);
        let d = job(&s, &dir, &body);
        let stop = AtomicBool::new(false);
        let started = Instant::now();
        let end = download(&d, &stop, &mut |e| {
            if let Event::Retrying { .. } = e {
                stop.store(true, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert_eq!(end, Finished::Paused);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// The start of a real GGUF file: header, then the architecture as its first value.
    fn gguf(arch: &str) -> Vec<u8> {
        let mut b = b"GGUF\x03\0\0\0".to_vec();
        b.extend(0u64.to_le_bytes()); // tensors
        b.extend(1u64.to_le_bytes()); // values
        b.extend(20u64.to_le_bytes());
        b.extend(b"general.architecture");
        b.extend(8u32.to_le_bytes()); // a string
        b.extend((arch.len() as u64).to_le_bytes());
        b.extend(arch.as_bytes());
        b.extend(vec![0u8; 1000]);
        b
    }

    #[test]
    fn only_real_model_files_pass_and_they_say_what_they_are() {
        let dir = folder("check");
        let good = dir.join("mine.gguf");
        std::fs::write(&good, gguf("cohere_asr")).unwrap();
        assert_eq!(check_file(&good).unwrap(), Family::Cohere);
        assert_eq!(find_in_folder(&dir, None), Some(good.clone()));
        assert_eq!(find_in_folder(&dir, Some(Family::Cohere)), Some(good));
        assert_eq!(find_in_folder(&dir, Some(Family::Whisper)), None);

        let whisper = dir.join("whisper-small.gguf");
        std::fs::write(&whisper, gguf("whisper")).unwrap();
        assert_eq!(check_file(&whisper).unwrap(), Family::Whisper);
        assert_eq!(find_in_folder(&dir, Some(Family::Whisper)), Some(whisper));

        let canary = dir.join("canary-flash.gguf");
        std::fs::write(&canary, gguf("canary")).unwrap();
        assert_eq!(check_file(&canary).unwrap(), Family::Canary);

        let other = dir.join("llama.gguf");
        std::fs::write(&other, gguf("llama")).unwrap();
        assert!(check_file(&other).is_err(), "not a speech model we know");

        let text = dir.join("notes.gguf");
        std::fs::write(&text, b"not a model").unwrap();
        assert!(check_file(&text).is_err());

        let cut = dir.join("cut.gguf");
        std::fs::write(&cut, &gguf("whisper")[..30]).unwrap();
        assert!(check_file(&cut).is_err(), "the header ends before the architecture");

        // our own file name, cut short by a copy
        let short = dir.join(VARIANTS[1].file);
        std::fs::write(&short, gguf("cohere_asr")).unwrap();
        assert!(check_file(&short).is_err());
    }

    #[test]
    fn each_language_goes_to_the_model_that_does_it_best() {
        let ids = |l: &str| models_for(l).iter().map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(ids("ar"), ["q8", "q4"], "Arabic stays with the model made for it");
        assert_eq!(ids("en"), ["q8", "q4", "canary", "whisper"], "English works with every model");
        for european in ["da", "fr", "es", "de", "it", "ru"] {
            assert_eq!(ids(european), ["canary", "whisper"], "{european}");
        }
        for other in ["tr", "fa", "ja", "zh", "hi"] {
            assert_eq!(ids(other), ["whisper"], "{other}");
        }
        assert!(ids("xx").is_empty());
    }

    #[test]
    fn every_language_has_a_model_and_a_name() {
        assert!(languages().count() >= 99);
        for code in languages() {
            assert!(!models_for(code).is_empty(), "{code}");
            assert_ne!(language_name(code), code, "{code}");
        }
        for code in CANARY_LANGUAGES {
            assert!(known_language(code), "{code}");
        }
        assert_eq!(language_name("tr"), "Turkish");
        assert_eq!(language_name("xx"), "xx");
    }

    #[test]
    fn the_translation_prompt_is_the_one_on_the_model_card() {
        let (p, stop) = Prompt::MiLMMT.build("طيب نبدأ", "ar", "en");
        assert_eq!(p, "Translate this from Arabic to English:\nArabic: طيب نبدأ\nEnglish:");
        assert_eq!(stop, ["\n"]);
        let (p, _) = Prompt::MiLMMT.build("x", "zh", "tl");
        assert_eq!(p, "Translate this from Chinese (Simplified) to Tagalog:\nChinese (Simplified): x\nTagalog:");
        let (p, _) = Prompt::MiLMMT.build("x", "sw", "en");
        assert!(p.starts_with("Translate this from Swahili to English"), "a language it was not trained on goes by its English name");
    }

    #[test]
    fn translation_targets_have_names_and_directions() {
        assert_eq!(translation_targets().count(), 46);
        for (code, _) in translation_targets() {
            assert!(known_language(code) || code == "zh-Hant", "{code}");
            assert_ne!(language_name(code), code, "{code} has a name for file names and headings");
        }
        assert_eq!(language_name("zh-Hant"), "Chinese (Traditional)");
        assert!(can_translate_into("ar") && can_translate_into("da") && can_translate_into("tr"));
        assert!(!can_translate_into("sw"));
        for v in VARIANTS {
            assert_eq!(v.prompt.is_some(), v.family == Family::Translator, "{}", v.id);
        }
    }

    #[test]
    fn only_our_own_translation_models_are_taken_for_translators() {
        let dir = folder("translator");
        // some other Gemma 3 model: not a speech model, and not a translator Dawwin knows how to ask
        let other = dir.join("gemma-3-4b.gguf");
        std::fs::write(&other, gguf("gemma3")).unwrap();
        assert!(check_file(&other).is_err());
        assert_eq!(find_in_folder(&dir, Some(Family::Translator)), None);
        // ours, by name, but cut short
        let short = dir.join(variant("milmmt-4b").unwrap().file);
        std::fs::write(&short, gguf("gemma3")).unwrap();
        assert!(check_file(&short).is_err());
    }

    #[test]
    fn right_to_left_languages_are_known() {
        for l in ["ar", "fa", "he", "ur"] {
            assert!(is_right_to_left(l), "{l}");
        }
        for l in ["en", "da", "tr", "fr"] {
            assert!(!is_right_to_left(l), "{l}");
        }
    }

    /// The real thing, over the real network: `cargo test real_download -- --ignored --nocapture`.
    /// It pauses part way, then resumes, and the checksum has to match at the end.
    #[test]
    #[ignore]
    fn real_download() {
        let v = variant(&std::env::var("DAWWIN_TEST_MODEL").unwrap_or_else(|_| "q4".into())).unwrap();
        let dir = PathBuf::from(std::env::var("DAWWIN_TEST_DIR").expect("DAWWIN_TEST_DIR"));
        let d = Download { url: url(v), dest: dir.join(v.file), size: v.size, sha256: v.sha256.into() };
        let stop = AtomicBool::new(false);
        let first = download(&d, &stop, &mut |e| {
            if let Event::Progress { done } = e {
                if done > 20_000_000 {
                    stop.store(true, Ordering::SeqCst);
                }
            }
        })
        .unwrap();
        assert_eq!(first, Finished::Paused);
        let kept = downloaded(&d.dest);
        println!("paused with {kept} bytes kept");
        assert!(kept > 20_000_000);
        let started = Instant::now();
        let second = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(second, Finished::Complete);
        println!("resumed and finished in {:.0} s", started.elapsed().as_secs_f32());
        assert_eq!(std::fs::metadata(&d.dest).unwrap().len(), v.size);
    }
}
