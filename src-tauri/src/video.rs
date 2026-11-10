//! Subtitles into the video itself: ffmpeg copies the video's own streams as they are, with no
//! re-encoding, and adds a subtitle track for each language. So it takes seconds, the picture and
//! sound stay exactly as they were, and players offer the subtitles to turn on and off.
//!
//! An MP4 or MOV gives an MP4, with its subtitles as `mov_text` (what MP4 players read); anything
//! else gives an MKV, which takes every stream the video has, its own subtitles and fonts included.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::setup::no_window;

/// Videos, by extension. The rest of what the app reads is sound only.
const VIDEO_EXT: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "3gp", "ts", "mts", "m2ts", "mpg", "mpeg"];

pub fn is_video(path: &Path) -> bool {
    path.extension().and_then(|x| x.to_str()).is_some_and(|x| VIDEO_EXT.contains(&x.to_lowercase().as_str()))
}

/// What the video with subtitles is saved as: "mp4" or "mkv".
pub fn output_ext(video: &Path) -> &'static str {
    match video.extension().and_then(|x| x.to_str()).map(|x| x.to_lowercase()).as_deref() {
        Some("mp4" | "m4v" | "mov") => "mp4",
        _ => "mkv",
    }
}

/// The three-letter language code containers want (ISO 639-2/B), from the app's two-letter one.
pub fn iso639_2(code: &str) -> &'static str {
    const CODES: &[(&str, &str)] = &[
        ("af", "afr"), ("am", "amh"), ("ar", "ara"), ("as", "asm"), ("az", "aze"), ("ba", "bak"), ("be", "bel"), ("bg", "bul"),
        ("bn", "ben"), ("bo", "tib"), ("br", "bre"), ("bs", "bos"), ("ca", "cat"), ("cs", "cze"), ("cy", "wel"), ("da", "dan"),
        ("de", "ger"), ("el", "gre"), ("en", "eng"), ("es", "spa"), ("et", "est"), ("eu", "baq"), ("fa", "per"), ("fi", "fin"),
        ("fo", "fao"), ("fr", "fre"), ("gl", "glg"), ("gu", "guj"), ("ha", "hau"), ("haw", "haw"), ("he", "heb"), ("hi", "hin"),
        ("hr", "hrv"), ("ht", "hat"), ("hu", "hun"), ("hy", "arm"), ("id", "ind"), ("is", "ice"), ("it", "ita"), ("ja", "jpn"),
        ("jw", "jav"), ("ka", "geo"), ("kk", "kaz"), ("km", "khm"), ("kn", "kan"), ("ko", "kor"), ("la", "lat"), ("lb", "ltz"),
        ("ln", "lin"), ("lo", "lao"), ("lt", "lit"), ("lv", "lav"), ("mg", "mlg"), ("mi", "mao"), ("mk", "mac"), ("ml", "mal"),
        ("mn", "mon"), ("mr", "mar"), ("ms", "may"), ("mt", "mlt"), ("my", "bur"), ("ne", "nep"), ("nl", "dut"), ("nn", "nno"),
        ("no", "nor"), ("oc", "oci"), ("pa", "pan"), ("pl", "pol"), ("ps", "pus"), ("pt", "por"), ("ro", "rum"), ("ru", "rus"),
        ("sa", "san"), ("sd", "snd"), ("si", "sin"), ("sk", "slo"), ("sl", "slv"), ("sn", "sna"), ("so", "som"), ("sq", "alb"),
        ("sr", "srp"), ("su", "sun"), ("sv", "swe"), ("sw", "swa"), ("ta", "tam"), ("te", "tel"), ("tg", "tgk"), ("th", "tha"),
        ("tk", "tuk"), ("tl", "fil"), ("tr", "tur"), ("tt", "tat"), ("uk", "ukr"), ("ur", "urd"), ("uz", "uzb"), ("vi", "vie"),
        ("yi", "yid"), ("yo", "yor"), ("yue", "chi"), ("zh", "chi"), ("zh-Hant", "chi"),
    ];
    CODES.iter().find(|(c, _)| *c == code).map_or("und", |(_, c)| *c)
}

/// One subtitle track: the SubRip text, its language, and the name players show for it.
pub struct Track {
    pub srt: String,
    pub language: String,
    pub title: String,
}

/// How many subtitle streams the video has already, from what `ffmpeg -i` says about it.
fn subtitle_streams(ffmpeg: &Path, video: &Path) -> usize {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-i"]).arg(video).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    no_window(&mut cmd);
    let Ok(out) = cmd.output() else { return 0 };
    String::from_utf8_lossy(&out.stderr).lines().filter(|l| l.trim_start().starts_with("Stream #0:") && l.contains(": Subtitle:")).count()
}

/// The arguments for ffmpeg: the video, then one SubRip file per track, into `dest`.
fn arguments(video: &Path, files: &[PathBuf], tracks: &[Track], kept: usize, dest: &Path) -> Vec<std::ffi::OsString> {
    let mp4 = dest.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4"));
    let mut a: Vec<std::ffi::OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"].iter().map(Into::into).collect();
    a.push(video.into());
    for f in files {
        a.extend(["-f".into(), "srt".into(), "-i".into(), f.into()]);
    }
    // MP4 takes pictures, sound and text subtitles; MKV takes everything there is
    let own: &[&str] = if mp4 { &["-map", "0:v?", "-map", "0:a?", "-map", "0:s?"] } else { &["-map", "0"] };
    a.extend(own.iter().map(Into::into));
    for k in 0..files.len() {
        a.extend(["-map".into(), format!("{}:0", k + 1).into()]);
    }
    a.extend(["-c".into(), "copy".into(), "-c:s".into(), if mp4 { "mov_text" } else { "srt" }.into()]);
    for (k, t) in tracks.iter().enumerate() {
        let n = kept + k;
        a.push(format!("-metadata:s:s:{n}").into());
        a.push(format!("language={}", iso639_2(&t.language)).into());
        a.push(format!("-metadata:s:s:{n}").into());
        a.push(format!("title={}", t.title).into());
        // the first one added is on unless the viewer turns it off
        a.push(format!("-disposition:s:{n}").into());
        a.push(if k == 0 { "default" } else { "0" }.into());
    }
    a.push(dest.into());
    a
}

/// Write `video` with these subtitle tracks added to `dest` (".mp4" or ".mkv", see [`output_ext`]).
pub fn add_subtitles(ffmpeg: &Path, video: &Path, tracks: &[Track], dest: &Path, scratch: &Path) -> Result<()> {
    if same_file(video, dest) {
        bail!(crate::tr!("Choose another name: the video itself cannot be written over."));
    }
    std::fs::create_dir_all(scratch)?;
    let stamp = std::process::id();
    let files: Vec<PathBuf> = (0..tracks.len()).map(|k| scratch.join(format!("subtitles-{stamp}-{k}.srt"))).collect();
    for (f, t) in files.iter().zip(tracks) {
        std::fs::write(f, &t.srt)?;
    }
    // the video's own subtitles come first, and the new ones are counted after them
    let kept = subtitle_streams(ffmpeg, video);
    // written next to where it goes, and named only when it is whole
    let part = dest.with_extension(format!("part.{}", output_ext(dest)));
    let mut cmd = Command::new(ffmpeg);
    cmd.args(arguments(video, &files, tracks, kept, &part)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    no_window(&mut cmd);
    let out = cmd.output().with_context(|| crate::tr!("could not start ffmpeg"));
    for f in &files {
        let _ = std::fs::remove_file(f);
    }
    let out = out?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&part);
        bail!(crate::tr!("ffmpeg could not add the subtitles: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn videos_are_told_from_recordings() {
        assert!(is_video(Path::new("meeting.MP4")) && is_video(Path::new("film.mkv")));
        assert!(!is_video(Path::new("voice.m4a")) && !is_video(Path::new("note.opus")));
        assert_eq!(output_ext(Path::new("a.mov")), "mp4");
        assert_eq!(output_ext(Path::new("a.webm")), "mkv");
        assert_eq!(output_ext(Path::new("a.avi")), "mkv");
    }

    #[test]
    fn every_language_has_its_container_code() {
        for code in crate::models::languages().chain(crate::models::translation_targets().map(|(c, _)| c)) {
            assert_ne!(iso639_2(code), "und", "{code}");
        }
        assert_eq!(iso639_2("ar"), "ara");
        assert_eq!(iso639_2("de"), "ger");
        assert_eq!(iso639_2("xx"), "und");
    }

    #[test]
    fn the_streams_are_copied_and_a_track_added_per_language() {
        let tracks = vec![
            Track { srt: String::new(), language: "ar".into(), title: "Arabic".into() },
            Track { srt: String::new(), language: "en".into(), title: "English".into() },
        ];
        let files = vec![PathBuf::from("a.srt"), PathBuf::from("b.srt")];
        let args: Vec<String> = arguments(Path::new("in.mkv"), &files, &tracks, 1, Path::new("out.mkv")).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let line = args.join(" ");
        assert!(line.contains("-map 0 -map 1:0 -map 2:0 -c copy -c:s srt"), "{line}");
        assert!(line.contains("-metadata:s:s:1 language=ara") && line.contains("-metadata:s:s:2 language=eng"), "after the one it has: {line}");
        assert!(line.contains("-disposition:s:1 default") && line.contains("-disposition:s:2 0"), "{line}");
        let mp4: String = arguments(Path::new("in.mp4"), &files[..1], &tracks[..1], 0, Path::new("out.mp4")).iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        assert!(mp4.contains("-map 0:v? -map 0:a? -map 0:s? -map 1:0 -c copy -c:s mov_text"), "{mp4}");
    }

    /// With ffmpeg installed: a short video made on the spot gets two subtitle tracks.
    #[test]
    fn a_real_video_gets_its_subtitles() {
        let Some(ffmpeg) = crate::setup::find_exe("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("dawwin-video-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for ext in ["mp4", "mkv"] {
            let video = dir.join(format!("in.{ext}"));
            let made = Command::new(&ffmpeg)
                .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10:duration=3", "-f", "lavfi", "-i", "sine=duration=3"])
                .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
                .arg(&video)
                .status();
            if !made.is_ok_and(|s| s.success()) {
                return; // an ffmpeg without these encoders
            }
            let srt = "1\n00:00:00,500 --> 00:00:02,000\nمرحبا\n\n".to_string();
            let tracks = vec![
                Track { srt: srt.clone(), language: "ar".into(), title: "Arabic".into() },
                Track { srt: srt.replace("مرحبا", "Hello"), language: "en".into(), title: "English".into() },
            ];
            let dest = dir.join(format!("out.{ext}"));
            add_subtitles(&ffmpeg, &video, &tracks, &dest, &dir).unwrap();
            assert_eq!(subtitle_streams(&ffmpeg, &dest), 2, "{ext}");
            assert!(!dir.join(format!("out.part.{ext}")).exists());
            assert!(add_subtitles(&ffmpeg, &video, &tracks, &video, &dir).is_err(), "never over the video itself");
        }
    }
}
