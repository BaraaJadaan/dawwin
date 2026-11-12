//! A small file beside a recording's WAV: what it is, and whether it ended cleanly. One that is
//! still there at the next start is a recording that was interrupted (a crash, the power, a forced
//! close): its WAV is repaired and offered back.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::Sources;

pub const SUFFIX: &str = ".dawwin-recording.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    /// The transcript's id
    pub id: String,
    pub title: String,
    pub started_ms: u64,
    /// The WAV's file name, in the same folder
    pub wav: String,
    pub rate: u32,
    pub channels: u16,
    /// What each channel holds
    pub left: String,
    pub right: String,
    pub sources: Sources,
    pub language: String,
    #[serde(default)]
    pub steps: crate::queue::Steps,
    #[serde(default)]
    pub mic: Option<String>,
    /// "recording", then "stopped" once the WAV is whole
    pub state: String,
}

impl Journal {
    pub fn path_for(wav: &Path) -> PathBuf {
        let stem = wav.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        wav.with_file_name(format!("{stem}{SUFFIX}"))
    }

    /// The WAV beside the journal. Only a bare file name ending in .wav is taken: a journal is a
    /// file anyone could have put in the folder, and "Delete it" deletes what it names.
    pub fn wav_path(&self, journal: &Path) -> Option<PathBuf> {
        let name = Path::new(&self.wav);
        let bare = name.components().count() == 1 && name.file_name().is_some_and(|n| n == name.as_os_str());
        let wav = self.wav.to_ascii_lowercase().ends_with(".wav") && !self.wav.contains([':', '/', '\\']);
        (bare && wav).then(|| journal.with_file_name(&self.wav))
    }

    /// Written in one step, so a crash cannot leave half a journal.
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Journal> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}

/// The recordings in `dir` that did not end cleanly, with their journals.
pub fn interrupted(dir: &Path) -> Vec<(PathBuf, Journal)> {
    let mut out: Vec<(PathBuf, Journal)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(SUFFIX)))
        .filter_map(|p| Some((p.clone(), Journal::load(&p).ok()?)))
        .filter(|(p, j)| j.wav_path(p).is_some_and(|w| w.is_file()))
        .collect();
    out.sort_by_key(|(_, j)| j.started_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(wav: &str) -> Journal {
        Journal {
            version: 1,
            id: "1".into(),
            title: "t".into(),
            started_ms: 0,
            wav: wav.into(),
            rate: 16000,
            channels: 2,
            left: "me".into(),
            right: "them".into(),
            sources: Sources::Both,
            language: "ar".into(),
            steps: Default::default(),
            mic: None,
            state: "recording".into(),
        }
    }

    #[test]
    fn a_journal_names_only_a_wav_beside_it() {
        let j = Path::new(r"C:\Users\x\Dawwin\Recording 1.dawwin-recording.json");
        assert_eq!(named("Recording 1.wav").wav_path(j), Some(PathBuf::from(r"C:\Users\x\Dawwin\Recording 1.wav")));
        assert_eq!(named("تسجيل 2026-11-12 10-00-00.WAV").wav_path(j).map(|p| p.parent().unwrap().to_path_buf()), Some(PathBuf::from(r"C:\Users\x\Dawwin")));
        for bad in [r"C:\Windows\win.ini", r"..\..\settings.wav", "../x.wav", r"sub\x.wav", "sub/x.wav", "C:x.wav", r"\\server\share\x.wav", "notes.txt", "", ".."] {
            assert_eq!(named(bad).wav_path(j), None, "{bad}");
        }
    }
}
