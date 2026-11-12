//! The voices Dawwin can read with: models, downloaded once, each with ready-made voices.
//!
//! Which models are offered waits on the listening test (tools/bench-voice). Supertonic 3 is here
//! for now because it is the smallest that reads both languages and runs on any processor; the
//! list is the maintainer's choice.

use std::path::PathBuf;

use super::text::Lang;
use crate::setup::{Paths, Settings};

#[derive(Clone, Copy, Debug)]
pub struct Voice {
    /// The model's own name for it
    pub voice_id: &'static str,
    pub lang: Lang,
    pub woman: bool,
    pub number: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceModel {
    pub id: &'static str,
    pub name: &'static str,
    /// audio.cpp's family
    pub family: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub license: &'static str,
    pub license_url: &'static str,
    /// The license has use restrictions that must be shown before the download (OpenRAIL-M)
    pub restricted: bool,
    /// Better on a GPU; else it is fast enough on the processor
    pub gpu: bool,
    pub voices: &'static [Voice],
}

const fn v(voice_id: &'static str, lang: Lang, woman: bool, number: u8) -> Voice {
    Voice { voice_id, lang, woman, number }
}

pub const SUPERTONIC: VoiceModel = VoiceModel {
    id: "supertonic3",
    name: "Supertonic 3",
    family: "supertonic",
    repository: "audio-cpp/audio.cpp-gguf",
    revision: "73ce4162f6cce370dd533c6782b772135c1b2a8e",
    file: "Supertonic-3-GGUF/supertonic-3-f16.gguf",
    size: 312_785_732,
    sha256: "7a44c47d1f387e4fb163e2eed9615b58470c77e26ac8b2b1a6be49dfb8670899",
    license: "BigScience OpenRAIL-M",
    license_url: "https://huggingface.co/Supertone/supertonic-3/blob/main/LICENSE",
    restricted: true,
    gpu: false,
    voices: &[
        v("F1", Lang::Ar, true, 1), v("F2", Lang::Ar, true, 2), v("F3", Lang::Ar, true, 3), v("F4", Lang::Ar, true, 4), v("F5", Lang::Ar, true, 5),
        v("M1", Lang::Ar, false, 1), v("M2", Lang::Ar, false, 2), v("M3", Lang::Ar, false, 3), v("M4", Lang::Ar, false, 4), v("M5", Lang::Ar, false, 5),
        v("F1", Lang::En, true, 1), v("F2", Lang::En, true, 2), v("F3", Lang::En, true, 3), v("F4", Lang::En, true, 4), v("F5", Lang::En, true, 5),
        v("M1", Lang::En, false, 1), v("M2", Lang::En, false, 2), v("M3", Lang::En, false, 3), v("M4", Lang::En, false, 4), v("M5", Lang::En, false, 5),
    ],
};

/// The stand-in of the tests (`DAWWIN_TEST_VOICE`): a tone per sentence, nothing to download.
pub const STAND_IN: VoiceModel = VoiceModel {
    id: "stand-in",
    name: "Stand-in voice",
    family: "stand-in",
    repository: "",
    revision: "",
    file: "",
    size: 0,
    sha256: "",
    license: "",
    license_url: "",
    restricted: false,
    gpu: true,
    voices: &[v("ar-m", Lang::Ar, false, 1), v("ar-f", Lang::Ar, true, 1), v("en-m", Lang::En, false, 1), v("en-f", Lang::En, true, 1)],
};

/// The stand-in voice program, in debug builds only.
pub fn test_voice() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        std::env::var_os("DAWWIN_TEST_VOICE").filter(|v| !v.is_empty()).map(PathBuf::from)
    } else {
        None
    }
}

pub fn models() -> Vec<VoiceModel> {
    let mut m = vec![SUPERTONIC];
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
    format!("{}/{}/{}", m.id, v.voice_id, v.lang.code())
}

pub fn find_voice(key: &str) -> Option<(VoiceModel, Voice)> {
    let mut parts = key.splitn(3, '/');
    let (m, id, lang) = (parts.next()?, parts.next()?, parts.next()?);
    let m = model(m)?;
    let v = *m.voices.iter().find(|v| v.voice_id == id && v.lang.code() == lang)?;
    Some((m, v))
}

pub fn url(m: &VoiceModel) -> String {
    let endpoint = std::env::var("HF_ENDPOINT").ok().filter(|e| !e.is_empty()).unwrap_or_else(|| "https://huggingface.co".into());
    format!("{}/{}/resolve/{}/{}", endpoint.trim_end_matches('/'), m.repository, m.revision, m.file)
}

/// Where a voice model is kept: beside the speech models, in a folder of its own.
pub fn file(paths: &Paths, m: &VoiceModel) -> PathBuf {
    let name = m.file.rsplit('/').next().unwrap_or(m.file);
    Settings::load(paths).download_dir(paths).join("voices").join(name)
}

pub fn installed(paths: &Paths, m: &VoiceModel) -> bool {
    m.id == STAND_IN.id || std::fs::metadata(file(paths, m)).is_ok_and(|f| f.len() == m.size)
}

/// The voice for a language when none was chosen: the first one installed, a woman's.
pub fn default_voice(paths: &Paths, lang: Lang) -> Option<String> {
    models().into_iter().filter(|m| installed(paths, m)).find_map(|m| {
        let v = m.voices.iter().find(|v| v.lang == lang && v.woman).or_else(|| m.voices.iter().find(|v| v.lang == lang))?;
        Some(voice_key(&m, v))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_voice_is_found_by_its_key() {
        for m in [SUPERTONIC, STAND_IN] {
            // the stand-in is there only when a test asks for it
            for v in m.voices.iter().filter(|_| m.id != STAND_IN.id) {
                let (fm, fv) = find_voice(&voice_key(&m, v)).unwrap_or_else(|| panic!("{}", voice_key(&m, v)));
                assert_eq!((fm.id, fv.voice_id, fv.lang), (m.id, v.voice_id, v.lang));
            }
            assert!(m.voices.iter().any(|v| v.lang == Lang::Ar && v.woman) && m.voices.iter().any(|v| v.lang == Lang::Ar && !v.woman));
            assert!(m.voices.iter().any(|v| v.lang == Lang::En && v.woman) && m.voices.iter().any(|v| v.lang == Lang::En && !v.woman));
        }
        assert!(find_voice("nothing/F1/ar").is_none());
        assert!(url(&SUPERTONIC).ends_with("/audio-cpp/audio.cpp-gguf/resolve/73ce4162f6cce370dd533c6782b772135c1b2a8e/Supertonic-3-GGUF/supertonic-3-f16.gguf"));
    }
}
