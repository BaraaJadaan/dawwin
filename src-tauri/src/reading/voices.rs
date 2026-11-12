//! The voices Dawwin reads with: models, downloaded once, each with ready-made voices.
//!
//! Chosen by the maintainer's ear in the Voice bench (tools/bench-voice, run 43134364301), by the
//! rule for every model family: VoxCPM 2 is the best (it wants a graphics card), Supertonic 3 the
//! best for its size and the smallest that reads Arabic well (it runs on any processor), Phonon the
//! smallest for English.

use std::path::PathBuf;

use super::text::Lang;
use crate::setup::{Paths, Settings};

/// A file of a model on Hugging Face, pinned to a revision, its size and its SHA-256.
#[derive(Clone, Copy, Debug)]
pub struct ModelFile {
    pub repository: &'static str,
    pub revision: &'static str,
    /// Its path in the repository
    pub path: &'static str,
    /// Its path in the model's own folder
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// How a voice is asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// One the model has built in, by its name
    BuiltIn(&'static str),
    /// A voice file among the model's files (`None`: the model's default voice)
    File(Option<&'static str>),
    /// A recording to speak like, with its words: Dawwin's own, made by VoxCPM 2 from a
    /// description (texts.json "refs"), never from a real person; it comes with the voice program
    Reference(&'static str, &'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Voice {
    /// Its id within the model, for the settings
    pub id: &'static str,
    pub lang: Lang,
    pub woman: bool,
    pub number: u8,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceModel {
    pub id: &'static str,
    pub name: &'static str,
    /// audio.cpp's family
    pub family: &'static str,
    pub files: &'static [ModelFile],
    pub license: &'static str,
    pub license_url: &'static str,
    /// The license has use restrictions that must be shown before the download (OpenRAIL-M)
    pub restricted: bool,
    /// It needs a graphics card to read as fast as it speaks: while the speech engine or the
    /// translator holds the GPU, it waits; a processor voice never does
    pub gpu: bool,
    /// The model's own code for Arabic and for English; empty for a model that takes none
    pub codes: [&'static str; 2],
    /// A sample in Arabic, in English, shipped with the app (`ui/voices/<id>-ar.ogg`, `-en.ogg`)
    pub samples: [bool; 2],
    /// Request options for every sentence
    pub options: &'static [(&'static str, &'static str)],
    /// Options of the loaded model (audio.cpp's session options)
    pub session: &'static [(&'static str, &'static str)],
    pub voices: &'static [Voice],
}

impl VoiceModel {
    pub fn code(&self, lang: Lang) -> &'static str {
        self.codes[if lang == Lang::Ar { 0 } else { 1 }]
    }

    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn reads(&self, lang: Lang) -> bool {
        self.voices.iter().any(|v| v.lang == lang)
    }
}

const fn v(id: &'static str, lang: Lang, woman: bool, number: u8, kind: Kind) -> Voice {
    Voice { id, lang, woman, number, kind }
}

const HF_GGUF: &str = "audio-cpp/audio.cpp-gguf";
const HF_GGUF_REV: &str = "73ce4162f6cce370dd533c6782b772135c1b2a8e";

/// What VoxCPM 2's ready-made voices say (tools/bench-voice/texts.json, "refs").
const REF_AR: &str = "مرحباً، أنا صوت صُنع لتجربة القراءة بصوت عالٍ. أقرأ النصوص العربية بهدوء ووضوح، جملة بعد جملة.";
const REF_EN: &str = "Hello, I am a voice made for reading aloud. I read English text calmly and clearly, one sentence after another.";

pub const VOXCPM2: VoiceModel = VoiceModel {
    id: "voxcpm2",
    name: "VoxCPM 2",
    family: "voxcpm2",
    files: &[ModelFile {
        repository: HF_GGUF,
        revision: HF_GGUF_REV,
        path: "VoxCPM2-GGUF/voxcpm2-q8_0.gguf",
        name: "voxcpm2-q8_0.gguf",
        size: 2_955_007_488,
        sha256: "8df1faf274e1bdeedef1fd9ddd6aba6632536f66f2e080991d2352af35ceef72",
    }],
    license: "Apache-2.0",
    license_url: "https://huggingface.co/openbmb/VoxCPM2",
    restricted: false,
    gpu: true,
    codes: ["", ""],
    samples: [true, true],
    // the recording and its words both shape the voice (upstream's "ultimate clone")
    options: &[("ultimate_clone", "true")],
    // the four voices' recordings stay encoded: Arabic and English take turns within a page
    session: &[("voxcpm2.prompt_cache_slots", "4")],
    voices: &[
        v("ar-f", Lang::Ar, true, 1, Kind::Reference("voxcpm2-ar-f.wav", REF_AR)),
        v("ar-m", Lang::Ar, false, 1, Kind::Reference("voxcpm2-ar-m.wav", REF_AR)),
        v("en-f", Lang::En, true, 1, Kind::Reference("voxcpm2-en-f.wav", REF_EN)),
        v("en-m", Lang::En, false, 1, Kind::Reference("voxcpm2-en-m.wav", REF_EN)),
    ],
};

pub const SUPERTONIC: VoiceModel = VoiceModel {
    id: "supertonic3",
    name: "Supertonic 3",
    family: "supertonic",
    files: &[ModelFile {
        repository: HF_GGUF,
        revision: HF_GGUF_REV,
        path: "Supertonic-3-GGUF/supertonic-3-f16.gguf",
        name: "supertonic-3-f16.gguf",
        size: 312_785_732,
        sha256: "7a44c47d1f387e4fb163e2eed9615b58470c77e26ac8b2b1a6be49dfb8670899",
    }],
    license: "BigScience OpenRAIL-M",
    license_url: "https://huggingface.co/Supertone/supertonic-3/blob/main/LICENSE",
    restricted: true,
    gpu: false,
    codes: ["ar", "en"],
    samples: [true, true],
    options: &[],
    session: &[],
    voices: &[
        v("F1", Lang::Ar, true, 1, Kind::BuiltIn("F1")), v("F2", Lang::Ar, true, 2, Kind::BuiltIn("F2")), v("F3", Lang::Ar, true, 3, Kind::BuiltIn("F3")),
        v("F4", Lang::Ar, true, 4, Kind::BuiltIn("F4")), v("F5", Lang::Ar, true, 5, Kind::BuiltIn("F5")),
        v("M1", Lang::Ar, false, 1, Kind::BuiltIn("M1")), v("M2", Lang::Ar, false, 2, Kind::BuiltIn("M2")), v("M3", Lang::Ar, false, 3, Kind::BuiltIn("M3")),
        v("M4", Lang::Ar, false, 4, Kind::BuiltIn("M4")), v("M5", Lang::Ar, false, 5, Kind::BuiltIn("M5")),
        v("F1", Lang::En, true, 1, Kind::BuiltIn("F1")), v("F2", Lang::En, true, 2, Kind::BuiltIn("F2")), v("F3", Lang::En, true, 3, Kind::BuiltIn("F3")),
        v("F4", Lang::En, true, 4, Kind::BuiltIn("F4")), v("F5", Lang::En, true, 5, Kind::BuiltIn("F5")),
        v("M1", Lang::En, false, 1, Kind::BuiltIn("M1")), v("M2", Lang::En, false, 2, Kind::BuiltIn("M2")), v("M3", Lang::En, false, 3, Kind::BuiltIn("M3")),
        v("M4", Lang::En, false, 4, Kind::BuiltIn("M4")), v("M5", Lang::En, false, 5, Kind::BuiltIn("M5")),
    ],
};

const PHONON_REPO: &str = "gradium/phonon-31m-en-audiocpp";
const PHONON_REV: &str = "2431a0781d54814ea1ddb067849a8c6f24f4aaae";
const PHONON_VOICES: &str = "gradium/phonon-voices";
const PHONON_VOICES_REV: &str = "0ee17205f254b5abc43b266c8a0f215565c352f7";

const fn phonon_voice(id: &'static str, path: &'static str, name: &'static str, sha256: &'static str) -> ModelFile {
    let _ = id;
    ModelFile { repository: PHONON_VOICES, revision: PHONON_VOICES_REV, path, name, size: 256_136, sha256 }
}

pub const PHONON: VoiceModel = VoiceModel {
    id: "phonon",
    name: "Phonon 31M",
    family: "gradium_phonon",
    files: &[
        ModelFile { repository: PHONON_REPO, revision: PHONON_REV, path: "phonon-31m-en-q8_0.gguf", name: "phonon-31m-en-q8_0.gguf", size: 85_204_768, sha256: "0545ad0c500015477b4fbef004810d8d1ba6aca5ccc7cbcde1dc9c7ba9556b05" },
        ModelFile { repository: PHONON_REPO, revision: PHONON_REV, path: "default-voice.safetensors", name: "default-voice.safetensors", size: 256_136, sha256: "f739e8c4c76ee7f92e7c94a82002748c8cd22e99582ac49f7b9b94cd63786e18" },
        // Zoey, Juniper, Easton, Micah
        phonon_voice("zoey", "voices/NbpkqMVS3CJeq2j8.safetensors", "voices/zoey.safetensors", "97c7f8c253413574406ab80dd23f49c3ebb2517163b00a9bf86b9c4cf43e7e23"),
        phonon_voice("juniper", "voices/NIWIOv5vTi5eRMj8.safetensors", "voices/juniper.safetensors", "444378eb06b7c492b86e8fc7d13e4db1a3c0cf3e68891a960bf6f0a41a13ef35"),
        phonon_voice("easton", "voices/LMDIYE0Oa377Qh6Z.safetensors", "voices/easton.safetensors", "45c582673cb20edb04ddcca2b5a8865ddfb0a82599efadb269021e87d20fbdd4"),
        phonon_voice("micah", "voices/40QrMVgAxFiUnP9f.safetensors", "voices/micah.safetensors", "0de1098d1051686a138d068e83ff09deb5053328bc74ec9541a1fdb90bfc24da"),
    ],
    license: "Apache-2.0",
    license_url: "https://huggingface.co/gradium/phonon-31m-en-audiocpp",
    restricted: false,
    gpu: false,
    codes: ["", ""],
    samples: [false, true],
    options: &[],
    session: &[],
    // English only; the default voice is a woman's (about 220 Hz)
    voices: &[
        v("default", Lang::En, true, 1, Kind::File(None)),
        v("zoey", Lang::En, true, 2, Kind::File(Some("voices/zoey.safetensors"))),
        v("juniper", Lang::En, true, 3, Kind::File(Some("voices/juniper.safetensors"))),
        v("easton", Lang::En, false, 1, Kind::File(Some("voices/easton.safetensors"))),
        v("micah", Lang::En, false, 2, Kind::File(Some("voices/micah.safetensors"))),
    ],
};

/// The stand-in of the tests (`DAWWIN_TEST_VOICE`): a tone per sentence, nothing to download.
pub const STAND_IN: VoiceModel = VoiceModel {
    id: "stand-in",
    name: "Stand-in voice",
    family: "stand-in",
    files: &[],
    license: "",
    license_url: "",
    restricted: false,
    gpu: true,
    codes: ["ar", "en"],
    samples: [false, false],
    options: &[],
    session: &[],
    voices: &[
        v("ar-m", Lang::Ar, false, 1, Kind::BuiltIn("ar-m")),
        v("ar-f", Lang::Ar, true, 1, Kind::BuiltIn("ar-f")),
        v("en-m", Lang::En, false, 1, Kind::BuiltIn("en-m")),
        v("en-f", Lang::En, true, 1, Kind::BuiltIn("en-f")),
    ],
};

/// The stand-in voice program, in debug builds only.
pub fn test_voice() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        std::env::var_os("DAWWIN_TEST_VOICE").filter(|v| !v.is_empty()).map(PathBuf::from)
    } else {
        None
    }
}

/// Best first.
pub fn models() -> Vec<VoiceModel> {
    let mut m = vec![VOXCPM2, SUPERTONIC, PHONON];
    if test_voice().is_some() {
        m.insert(0, STAND_IN);
    }
    m
}

pub fn model(id: &str) -> Option<VoiceModel> {
    models().into_iter().find(|m| m.id == id)
}

/// "supertonic3/F1/ar": a voice of a model, for a language.
pub fn voice_key(m: &VoiceModel, v: &Voice) -> String {
    format!("{}/{}/{}", m.id, v.id, v.lang.code())
}

pub fn find_voice(key: &str) -> Option<(VoiceModel, Voice)> {
    let mut parts = key.splitn(3, '/');
    let (m, id, lang) = (parts.next()?, parts.next()?, parts.next()?);
    let m = model(m)?;
    let v = *m.voices.iter().find(|v| v.id == id && v.lang.code() == lang)?;
    Some((m, v))
}

pub fn url(f: &ModelFile) -> String {
    let endpoint = std::env::var("HF_ENDPOINT").ok().filter(|e| !e.is_empty()).unwrap_or_else(|| "https://huggingface.co".into());
    format!("{}/{}/resolve/{}/{}", endpoint.trim_end_matches('/'), f.repository, f.revision, f.path)
}

/// A model's own folder: beside the speech models, under "voices".
pub fn dir(paths: &Paths, m: &VoiceModel) -> PathBuf {
    Settings::load(paths).download_dir(paths).join("voices").join(m.id)
}

pub fn file(paths: &Paths, m: &VoiceModel, f: &ModelFile) -> PathBuf {
    f.name.split('/').fold(dir(paths, m), |p, part| p.join(part))
}

pub fn installed(paths: &Paths, m: &VoiceModel) -> bool {
    m.id == STAND_IN.id || m.files.iter().all(|f| std::fs::metadata(file(paths, m, f)).is_ok_and(|x| x.len() == f.size))
}

/// How much of a model is on the disk, finished files and unfinished ones.
pub fn on_disk(paths: &Paths, m: &VoiceModel) -> u64 {
    m.files
        .iter()
        .map(|f| {
            let p = file(paths, m, f);
            if std::fs::metadata(&p).is_ok_and(|x| x.len() == f.size) {
                f.size
            } else {
                crate::models::downloaded(&p)
            }
        })
        .sum()
}

/// The voice for a language when none was chosen: the best installed model's, a woman's.
pub fn default_voice(paths: &Paths, lang: Lang) -> Option<String> {
    models().into_iter().filter(|m| installed(paths, m)).find_map(|m| {
        let v = m.voices.iter().find(|v| v.lang == lang && v.woman).or_else(|| m.voices.iter().find(|v| v.lang == lang))?;
        Some(voice_key(&m, v))
    })
}

/// What a sentence's request says about its voice, with the model's own options.
pub fn request(m: &VoiceModel, v: &Voice, model_dir: &std::path::Path, program_dir: &std::path::Path) -> serde_json::Value {
    let mut options: serde_json::Map<String, serde_json::Value> = m.options.iter().map(|(k, x)| (k.to_string(), serde_json::Value::from(*x))).collect();
    let mut req = serde_json::json!({});
    match v.kind {
        Kind::BuiltIn(id) => req["voice_id"] = id.into(),
        Kind::File(Some(name)) => {
            options.insert("voice_embedding_file".into(), name.split('/').fold(model_dir.to_path_buf(), |p, part| p.join(part)).display().to_string().into());
        }
        Kind::File(None) => {}
        Kind::Reference(wav, words) => {
            req["voice_ref"] = program_dir.join(wav).display().to_string().into();
            req["reference_text"] = words.into();
        }
    }
    if !options.is_empty() {
        req["options"] = options.into();
    }
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_voice_is_found_by_its_key_and_each_model_has_men_and_women() {
        for m in [VOXCPM2, SUPERTONIC, PHONON] {
            for v in m.voices {
                let (fm, fv) = find_voice(&voice_key(&m, v)).unwrap_or_else(|| panic!("{}", voice_key(&m, v)));
                assert_eq!((fm.id, fv.id, fv.lang), (m.id, v.id, v.lang));
            }
            for lang in [Lang::Ar, Lang::En].into_iter().filter(|&l| m.reads(l)) {
                assert!(m.voices.iter().any(|v| v.lang == lang && v.woman) && m.voices.iter().any(|v| v.lang == lang && !v.woman), "{} {lang:?}", m.id);
            }
            assert_eq!(m.files.iter().map(|f| f.name).collect::<std::collections::HashSet<_>>().len(), m.files.len(), "{}", m.id);
        }
        // Arabic and English both have a voice that needs no graphics card
        assert!(SUPERTONIC.reads(Lang::Ar) && SUPERTONIC.reads(Lang::En) && !SUPERTONIC.gpu);
        assert!(!PHONON.reads(Lang::Ar));
        assert!(find_voice("nothing/F1/ar").is_none());
        assert!(url(&SUPERTONIC.files[0]).ends_with("/audio-cpp/audio.cpp-gguf/resolve/73ce4162f6cce370dd533c6782b772135c1b2a8e/Supertonic-3-GGUF/supertonic-3-f16.gguf"));
    }

    #[test]
    fn each_kind_of_voice_asks_for_itself() {
        let (model_dir, program) = (std::path::Path::new("C:/m"), std::path::Path::new("C:/p"));
        let r = request(&SUPERTONIC, &SUPERTONIC.voices[0], model_dir, program);
        assert_eq!(r, serde_json::json!({ "voice_id": "F1" }));
        let r = request(&PHONON, &PHONON.voices[3], model_dir, program);
        assert!(r["options"]["voice_embedding_file"].as_str().unwrap().ends_with("easton.safetensors"), "{r}");
        assert_eq!(request(&PHONON, &PHONON.voices[0], model_dir, program), serde_json::json!({}));
        let r = request(&VOXCPM2, &VOXCPM2.voices[0], model_dir, program);
        assert!(r["voice_ref"].as_str().unwrap().ends_with("voxcpm2-ar-f.wav"));
        assert_eq!(r["reference_text"], REF_AR);
        assert_eq!(r["options"]["ultimate_clone"], "true");
    }

    #[test]
    fn the_ready_made_voices_say_what_the_bench_made_them_say() {
        let texts: serde_json::Value = serde_json::from_str(include_str!("../../../tools/bench-voice/texts.json")).unwrap();
        assert_eq!(texts["refs"]["text"]["ar"], REF_AR);
        assert_eq!(texts["refs"]["text"]["en"], REF_EN);
    }
}
