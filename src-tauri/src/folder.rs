//! Transcribing a folder: the recordings inside it, and the files written next to each one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use crate::transcript::{self, Transcript};

/// Every recording Dawwin takes, by extension: the one list the file dialog, drops on the window
/// and folder runs all use (the interface asks for it).
pub const MEDIA_EXT: &[&str] = &[
    "m4a", "mp3", "wav", "ogg", "opus", "oga", "flac", "aac", "wma", "amr", "aiff", "aif", "caf", "w64", "ac3",
    "webm", "mp4", "m4v", "mov", "mkv", "3gp", "avi", "wmv", "flv", "ts", "mts", "m2ts", "mpg", "mpeg",
];
/// Of those, what only ffmpeg reads: the built-in decoder knows none of these.
pub const NEEDS_FFMPEG: &[&str] = &[
    "opus", "wma", "amr", "aiff", "aif", "caf", "w64", "ac3", "avi", "wmv", "flv", "ts", "mts", "m2ts", "mpg", "mpeg",
];
pub const FORMATS: &[&str] = &["txt", "md", "docx", "srt", "vtt", "csv", "tsv", "json"];

/// A recording only ffmpeg reads.
pub fn needs_ffmpeg(path: &Path) -> bool {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    NEEDS_FFMPEG.contains(&ext.as_str())
}

fn is_media(path: &Path) -> bool {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    MEDIA_EXT.contains(&ext.as_str())
}

/// Windows' own folders and hidden ones hold nothing to transcribe.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('$') || name.eq_ignore_ascii_case("System Volume Information")
}

/// Every recording under `dir`, sorted by path. Links are not followed, so a loop cannot trap it.
#[cfg(test)]
pub fn recordings_in(dir: &Path) -> Vec<PathBuf> {
    scan(dir, &AtomicBool::new(false), &mut |_, _| {})
}

/// The same, stopped by `cancel` (with what it found so far), telling `progress` how many
/// recordings it found and how many files it looked at, every few hundred files.
pub fn scan(dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(usize, usize)) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut looked = 0usize;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if cancel.load(Ordering::Relaxed) {
                return found;
            }
            looked += 1;
            if looked % 256 == 0 {
                progress(found.len(), looked);
            }
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
    progress(found.len(), looked);
    found.sort();
    found
}

/// Whether `dir` itself holds a recording, without looking into its subfolders.
pub fn has_recordings_at_top(dir: &Path) -> bool {
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| e.file_type().is_ok_and(|k| k.is_file()) && is_media(&e.path()))
}

/// Recordings Dawwin knows (it has their transcripts), by folder: the names, in lower case.
pub type Known = HashMap<String, Vec<String>>;

pub fn known(recordings: impl Iterator<Item = String>) -> Known {
    let mut out: Known = HashMap::new();
    for r in recordings {
        let p = PathBuf::from(&r);
        let (Some(dir), Some(name)) = (p.parent(), p.file_name()) else { continue };
        out.entry(same_file_key(&dir.display().to_string())).or_default().push(name.to_string_lossy().to_lowercase());
    }
    out
}

/// Whether `file` is one Dawwin wrote next to a recording it knows: its sound ("talk.m4a" next to
/// "talk.mp4", "talk (without silences).m4a"), or its video with subtitles ("talk (Arabic).mp4",
/// "talk (English, Arabic, burned in).mp4"). A name such as "talk (2).mp3" is the user's own.
pub fn is_own_output(file: &Path, known: &Known) -> bool {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else { return false };
    let Some(names) = known.get(&same_file_key(&dir.display().to_string())) else { return false };
    let name = name.to_string_lossy().to_lowercase();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name.as_str(), ""));
    names.iter().filter(|r| **r != name).any(|r| {
        let rstem = r.rsplit_once('.').map_or(r.as_str(), |x| x.0);
        if stem == rstem {
            return matches!(ext, "m4a" | "wav");
        }
        stem.strip_prefix(rstem)
            .and_then(|rest| rest.strip_prefix(" (")?.strip_suffix(')'))
            .is_some_and(|inner| !inner.is_empty() && !inner.chars().all(|c| c.is_ascii_digit()))
    })
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
pub fn write_outputs(t: &Transcript, formats: &[String]) -> Result<Vec<PathBuf>> {
    let recording = Path::new(&t.source_path);
    let mut written = Vec::new();
    for ext in formats {
        let bytes = transcript::export_bytes(t, ext, true)?;
        let dest = free_path(recording, ext);
        std::fs::write(&dest, bytes)?;
        written.push(dest);
    }
    Ok(written)
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
            timed: false,
            outputs: vec![],
            segments: vec![Segment { start: 0.0, end: 5.0, text: "نص الدرس".into(), flag: None, edited: false, voice: vec![], words: vec![], pauses: vec![], heard: vec![] }],
            translations: vec![],
        }
    }

    #[test]
    fn finds_recordings_in_subfolders_only_where_it_should() {
        let dir = temp("scan");
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::create_dir_all(dir.join("$RECYCLE.BIN")).unwrap();
        for f in ["one.MP3", "a/two.m4a", "a/b/three.mp4", "a/notes.txt", ".hidden/no.mp3", "$RECYCLE.BIN/no.wav", "a/film.AVI", "a/b/camera.m2ts"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let names: Vec<String> = recordings_in(&dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["camera.m2ts", "three.mp4", "film.AVI", "two.m4a", "one.MP3"]);
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
    fn the_files_dawwin_wrote_are_not_taken_for_recordings() {
        let k = known(["D:/Talks/talk.mp4".to_string(), "D:/Talks/درس.mp3".to_string()].into_iter());
        let own = |f: &str| is_own_output(Path::new(f), &k);
        assert!(own("D:/Talks/talk.m4a"), "the sound saved from the video");
        assert!(own(r"d:\talks\Talk (without silences).m4a"));
        assert!(own("D:/Talks/talk (Arabic).mp4"));
        assert!(own("D:/Talks/talk (English, Arabic, burned in).mp4"));
        assert!(own("D:/Talks/درس (بدون الصمت).m4a"));
        assert!(!own("D:/Talks/talk.mp4"), "the recording itself");
        assert!(!own("D:/Talks/talk (2).mp4"), "a name the user gave");
        assert!(!own("D:/Talks/talking.m4a"));
        assert!(!own("D:/Other/talk.m4a"), "another folder");
        assert!(!own("D:/Talks/lesson.m4a"));
    }

    #[test]
    fn a_long_scan_can_be_stopped_and_says_how_far_it_got() {
        let dir = temp("stop");
        for k in 0..600 {
            std::fs::write(dir.join(format!("{k}.mp3")), b"x").unwrap();
        }
        let mut heard = Vec::new();
        assert_eq!(scan(&dir, &AtomicBool::new(false), &mut |f, l| heard.push((f, l))).len(), 600);
        assert!(heard.len() >= 3 && heard.last() == Some(&(600, 600)), "{heard:?}");
        let stop = AtomicBool::new(false);
        let got = scan(&dir, &stop, &mut |_, l| if l >= 256 { stop.store(true, Ordering::Relaxed) });
        assert!(got.len() < 600, "{}", got.len());
        assert!(has_recordings_at_top(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_video_is_a_recording_and_what_needs_ffmpeg_is_known() {
        for ext in crate::video::VIDEO_EXT.iter().chain(NEEDS_FFMPEG) {
            assert!(MEDIA_EXT.contains(ext), "{ext} is not taken");
        }
        assert!(needs_ffmpeg(Path::new("film.MPG")) && needs_ffmpeg(Path::new("voice.opus")));
        assert!(!needs_ffmpeg(Path::new("talk.m4a")) && !needs_ffmpeg(Path::new("clip.mkv")), "the built-in decoder reads these");
    }

    #[test]
    fn the_same_file_written_differently_is_the_same_file() {
        assert_eq!(same_file_key("D:/Lessons/One.MP3"), same_file_key("d:\\lessons\\one.mp3"));
    }
}
