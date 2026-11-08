//! Transcribing a folder: the recordings inside it, and the files written next to each one.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::transcript::{self, Transcript};

pub const MEDIA_EXT: &[&str] = &["m4a", "mp3", "wav", "ogg", "opus", "oga", "flac", "aac", "wma", "amr", "webm", "mp4", "m4v", "mov", "mkv", "3gp"];
pub const FORMATS: &[&str] = &["txt", "md", "docx", "srt", "vtt", "csv", "tsv", "json"];

fn is_media(path: &Path) -> bool {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    MEDIA_EXT.contains(&ext.as_str())
}

/// Windows' own folders and hidden ones hold nothing to transcribe.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('$') || name.eq_ignore_ascii_case("System Volume Information")
}

/// Every recording under `dir`, sorted by path. Links are not followed, so a loop cannot trap it.
pub fn recordings_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                if !skip_dir(&entry.file_name().to_string_lossy()) {
                    todo.push(path);
                }
            } else if kind.is_file() && is_media(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Windows paths differ only by case and slash direction when they name the same file.
pub fn same_file_key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// `lesson.mp3` + `txt` → `lesson.txt`.
pub fn output_path(recording: &Path, ext: &str) -> PathBuf {
    recording.with_extension(ext)
}

/// A name that is free, so nothing already there is overwritten: `lesson.txt`, `lesson (2).txt`…
fn free_path(recording: &Path, ext: &str) -> PathBuf {
    let first = output_path(recording, ext);
    if !first.exists() {
        return first;
    }
    let stem = recording.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    (2..)
        .map(|n| recording.with_file_name(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Write this transcript next to its recording in each format, under names that are free.
pub fn write_outputs(t: &Transcript, formats: &[String]) -> Result<()> {
    let recording = Path::new(&t.source_path);
    for ext in formats {
        let bytes = transcript::export_bytes(t, ext, true)?;
        std::fs::write(free_path(recording, ext), bytes)?;
    }
    Ok(())
}

/// The formats that have no file next to this recording yet.
pub fn missing_outputs(recording: &Path, formats: &[String]) -> Vec<String> {
    formats.iter().filter(|ext| !output_path(recording, ext).exists()).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Segment;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dawwin-folder-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn transcript_of(recording: &Path) -> Transcript {
        Transcript {
            id: "t1".into(),
            title: "درس".into(),
            source_path: recording.display().to_string(),
            created_ms: 0,
            duration_s: 5.0,
            language: "ar".into(),
            elapsed_s: 1.0,
            device: "cpu".into(),
            model: "m".into(),
            partial: false,
            segments: vec![Segment { start: 0.0, end: 5.0, text: "نص الدرس".into(), flag: None, edited: false, voice: vec![], words: vec![] }],
        }
    }

    #[test]
    fn finds_recordings_in_subfolders_only_where_it_should() {
        let dir = temp("scan");
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::create_dir_all(dir.join("$RECYCLE.BIN")).unwrap();
        for f in ["one.MP3", "a/two.m4a", "a/b/three.mp4", "a/notes.txt", ".hidden/no.mp3", "$RECYCLE.BIN/no.wav"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let names: Vec<String> = recordings_in(&dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["three.mp4", "two.m4a", "one.MP3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_or_missing_folder_has_no_recordings() {
        let dir = temp("empty");
        assert!(recordings_in(&dir).is_empty());
        assert!(recordings_in(&dir.join("nope")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_next_to_the_recording_and_never_overwrites() {
        let dir = temp("write");
        let rec = dir.join("lesson.mp3");
        std::fs::write(dir.join("lesson.txt"), "the user's own notes").unwrap();
        write_outputs(&transcript_of(&rec), &["txt".into(), "json".into()]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("lesson.txt")).unwrap(), "the user's own notes");
        assert!(std::fs::read_to_string(dir.join("lesson (2).txt")).unwrap().contains("نص الدرس"));
        assert!(std::fs::read_to_string(dir.join("lesson.json")).unwrap().contains("نص الدرس"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn knows_which_files_are_still_missing() {
        let dir = temp("missing");
        let rec = dir.join("lesson.mp3");
        std::fs::write(dir.join("lesson.txt"), "x").unwrap();
        assert_eq!(missing_outputs(&rec, &["txt".into(), "srt".into()]), ["srt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_file_written_differently_is_the_same_file() {
        assert_eq!(same_file_key("D:/Lessons/One.MP3"), same_file_key("d:\\lessons\\one.mp3"));
    }
}
