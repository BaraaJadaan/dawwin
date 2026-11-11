//! Subtitles into the video itself: ffmpeg copies the video's own streams as they are, with no
//! re-encoding, and adds a subtitle track for each language. So it takes seconds, the picture and
//! sound stay exactly as they were, and players offer the subtitles to turn on and off.
//!
//! An MP4 or MOV whose streams an MP4 holds gives an MP4, with its subtitles as `mov_text` (what
//! MP4 players read); a MOV that an MP4 cannot hold (ProRes, PCM sound) stays a MOV; anything else
//! gives an MKV, which takes every stream the video has, its own subtitles and fonts included. The
//! video's own streams, its own subtitles too, are copied exactly as they are.
//!
//! Or the subtitles are burned into the picture, for players and sites that show no tracks: the
//! picture is encoded again (on the graphics chip when there is one), into an MP4.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{bail, Context, Result};

use crate::setup::no_window;

/// Videos, by extension. The rest of what the app reads is sound only.
const VIDEO_EXT: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "3gp", "ts", "mts", "m2ts", "mpg", "mpeg"];

pub fn is_video(path: &Path) -> bool {
    path.extension().and_then(|x| x.to_str()).is_some_and(|x| VIDEO_EXT.contains(&x.to_lowercase().as_str()))
}

/// One stream of a video, as `ffmpeg -i` names it: "Video", "Audio", "Subtitle", "Data"…, and its codec.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub kind: String,
    pub codec: String,
}

/// The streams `ffmpeg -i` lists ("Stream #0:1[0x2](eng): Audio: aac (LC) …").
fn parse_streams(text: &str) -> Vec<Stream> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Stream #0:"))
        .filter_map(|l| {
            // "Stream #0:1[0x2](eng)" holds no ": " itself
            let (_, rest) = l.split_once(": ")?;
            let (kind, after) = rest.split_once(": ")?;
            let codec = after.split([' ', ',']).next().unwrap_or("").to_string();
            Some(Stream { kind: kind.trim().to_string(), codec })
        })
        .collect()
}

/// The streams of a video.
pub fn streams(ffmpeg: &Path, video: &Path) -> Vec<Stream> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-i"]).arg(video).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    no_window(&mut cmd);
    cmd.output().map(|o| parse_streams(&String::from_utf8_lossy(&o.stderr))).unwrap_or_default()
}

/// What an MP4 holds when it is copied into one. Data streams (a camera's timecode) stay behind.
fn fits_mp4(s: &Stream) -> bool {
    const VIDEO: &[&str] = &["h264", "hevc", "av1", "vp9", "mpeg4", "mpeg2video", "mpeg1video", "mjpeg", "png"];
    const AUDIO: &[&str] = &["aac", "mp3", "ac3", "eac3", "alac", "flac", "opus"];
    match s.kind.as_str() {
        "Video" => VIDEO.contains(&s.codec.as_str()),
        "Audio" => AUDIO.contains(&s.codec.as_str()),
        "Subtitle" => s.codec == "mov_text",
        _ => true,
    }
}

/// What the video with subtitles is saved as: "mp4", "mov" or "mkv", from the video's own kind and
/// what its streams are. A MOV that an MP4 cannot hold (ProRes, PCM sound) stays a MOV.
pub fn container(video: &Path, streams: &[Stream]) -> &'static str {
    let fits = streams.iter().all(fits_mp4);
    match video.extension().and_then(|x| x.to_str()).map(|x| x.to_lowercase()).as_deref() {
        Some("mp4" | "m4v") if fits => "mp4",
        Some("mov") if fits => "mp4",
        Some("mov") => "mov",
        _ => "mkv",
    }
}

/// The same, asking ffmpeg what the video holds.
pub fn output_ext(ffmpeg: &Path, video: &Path) -> &'static str {
    container(video, &streams(ffmpeg, video))
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

/// The name of a language in English, for a subtitle track.
fn track_title(code: &str) -> String {
    crate::models::translation_targets()
        .find(|(c, _)| *c == code)
        .map_or_else(|| crate::models::language_name(code).to_string(), |(_, n)| n.to_string())
}

/// The subtitle tracks for a transcript: its own, its translation's, or both, the translation first
/// (on), the transcript a click away. An error says what is missing.
pub fn tracks(t: &crate::transcript::Transcript, view: &crate::transcript::View) -> std::result::Result<Vec<Track>, String> {
    use crate::transcript::{to_srt, Transcript, View};
    let track = |t: &Transcript| Track { srt: to_srt(t), language: t.language.clone(), title: track_title(&t.language) };
    let translated = |l: &str| t.translated(l).ok_or_else(|| crate::tr!("There is no translation into that language."));
    Ok(match view {
        View::Original => vec![track(t)],
        View::Translation(l) => vec![track(&translated(l)?)],
        View::Both(l) => vec![track(&translated(l)?), track(t)],
    })
}

/// Where the video with subtitles goes by default: next to the video, "name (English, Arabic).mp4",
/// in the container `ext` (see [`container`]).
pub fn default_dest(video: &Path, tracks: &[Track], ext: &str) -> PathBuf {
    let stem = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let names = tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>().join(", ");
    video.with_file_name(format!("{stem} ({names}).{ext}"))
}

/// A name for a scratch file no other export uses, even one running at the same moment.
fn scratch_name(kind: &str, ext: &str) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{kind}-{}-{}-{n}.{ext}", std::process::id(), crate::queue::now_ms())
}

/// The arguments for ffmpeg: the video, then one SubRip file per track, into `dest`. `kept` is how
/// many subtitle streams the video has already: they are copied as they are, picture-based ones
/// (PGS, VobSub) and styled ones (ASS) too, and only the tracks added are written as text.
fn arguments(video: &Path, files: &[PathBuf], tracks: &[Track], kept: usize, dest: &Path) -> Vec<std::ffi::OsString> {
    let ext = dest.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mp4 = ext == "mp4" || ext == "mov";
    let mut a: Vec<std::ffi::OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"].iter().map(Into::into).collect();
    a.push(video.into());
    for f in files {
        a.extend(["-f".into(), "srt".into(), "-i".into(), f.into()]);
    }
    // MP4 and MOV take pictures, sound and text subtitles; MKV takes everything there is
    let own: &[&str] = if mp4 { &["-map", "0:v?", "-map", "0:a?", "-map", "0:s?"] } else { &["-map", "0"] };
    a.extend(own.iter().map(Into::into));
    for k in 0..files.len() {
        a.extend(["-map".into(), format!("{}:0", k + 1).into()]);
    }
    a.extend(["-c".into(), "copy".into()]);
    for (k, t) in tracks.iter().enumerate() {
        let n = kept + k;
        a.push(format!("-c:s:{n}").into());
        a.push(if mp4 { "mov_text" } else { "srt" }.into());
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

/// Write `video` with these subtitle tracks added to `dest` (".mp4", ".mov" or ".mkv", see [`container`]).
pub fn add_subtitles(ffmpeg: &Path, video: &Path, tracks: &[Track], dest: &Path, scratch: &Path) -> Result<()> {
    if same_file(video, dest) {
        bail!(crate::tr!("Choose another name: the video itself cannot be written over."));
    }
    std::fs::create_dir_all(scratch)?;
    let files: Vec<PathBuf> = (0..tracks.len()).map(|_| scratch.join(scratch_name("subtitles", "srt"))).collect();
    for (f, t) in files.iter().zip(tracks) {
        std::fs::write(f, &t.srt)?;
    }
    // the video's own subtitles come first, and the new ones are counted after them
    let kept = streams(ffmpeg, video).iter().filter(|s| s.kind == "Subtitle").count();
    // written next to where it goes, and named only when it is whole
    let ext = dest.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "mkv".into());
    let part = dest.with_extension(format!("part.{ext}"));
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

// ---------------------------------------------------------------- burned in

/// The H.264 encoders burning tries, in this order: NVIDIA's, AMD's and Intel's chips, Windows' own
/// (Media Foundation, which uses whatever the computer has, or software), and x264 of a full ffmpeg.
pub const H264_ENCODERS: &[&str] = &["h264_nvenc", "h264_amf", "h264_qsv", "h264_mf", "libx264"];

/// How the subtitles look: Segoe UI (Windows' Arial breaks Arabic: ث before لا comes out as a box),
/// a little larger than ffmpeg's default, with an outline and a soft shadow to read on any picture.
/// `Encoding=-1` lets libass find each line's direction itself: without it a right-to-left line is
/// laid out from the left, its full stop lands at the start and the Arabic around an English word swaps.
const STYLE: &str = "FontName=Segoe UI,FontSize=18,Outline=1.6,Shadow=0.8,MarginV=18,Encoding=-1";

/// What `ffmpeg -i` says about a video, as much as burning needs.
#[derive(Debug, Default, PartialEq)]
struct Probe {
    duration_s: f64,
    /// Of the whole file, from its "Duration" line
    total_kbps: Option<u64>,
    /// Of the first video stream
    pixels: u64,
    video_kbps: Option<u64>,
    /// The audio streams' codecs, and what their bitrates add up to where they say
    audio: Vec<String>,
    audio_kbps: u64,
}

/// "…, 128 kb/s, …" in a stream line or the duration line.
fn kbps(items: &str) -> Option<u64> {
    items.split(',').map(str::trim).find_map(|i| {
        let n = i.strip_suffix(" kb/s")?;
        n.strip_prefix("bitrate:").unwrap_or(n).trim().parse().ok()
    })
}

fn parse_probe(text: &str) -> Probe {
    let mut p = Probe::default();
    let mut seen_video = false;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("Duration: ") {
            let stamp = rest.split(',').next().unwrap_or("");
            let parts: Vec<f64> = stamp.split(':').filter_map(|x| x.trim().parse().ok()).collect();
            if let [h, m, s] = parts[..] {
                p.duration_s = h * 3600.0 + m * 60.0 + s;
            }
            p.total_kbps = kbps(rest);
        } else if line.starts_with("Stream #") {
            if let Some((_, rest)) = line.split_once(": Video: ") {
                if seen_video {
                    continue;
                }
                seen_video = true;
                for part in rest.split([',', ' ']) {
                    if let Some((w, h)) = part.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<u64>(), h.parse::<u64>()) {
                            if w > 0 && h > 0 && p.pixels == 0 {
                                p.pixels = w * h;
                            }
                        }
                    }
                }
                p.video_kbps = kbps(rest);
            } else if let Some((_, rest)) = line.split_once(": Audio: ") {
                let codec = rest.split([' ', ',']).next().unwrap_or("").to_string();
                p.audio.push(codec);
                // a sound track that does not say: as much as a usual one
                p.audio_kbps += kbps(rest).unwrap_or(128);
            }
        }
    }
    p
}

fn probe(ffmpeg: &Path, video: &Path) -> Probe {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-i"]).arg(video).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    no_window(&mut cmd);
    cmd.output().map(|o| parse_probe(&String::from_utf8_lossy(&o.stderr))).unwrap_or_default()
}

/// The bitrate to encode at, in kb/s: a quarter above the picture's own, so the picture keeps what it
/// had (measured: about 1.5 times the file, SSIM 0.997 against 0.998), and never more than its size
/// needs. A picture that does not say (MKV, WebM) has the file's bitrate less its sound's. Only a
/// floor against nonsense: a floor of 1,500 kb/s made a web video's copy three times its size.
fn bitrate(p: &Probe) -> u64 {
    let most = match p.pixels {
        0..=500_000 => 2_500,
        500_001..=1_000_000 => 5_000,
        1_000_001..=2_100_000 => 8_000,
        2_100_001..=3_700_000 => 14_000,
        _ => 25_000,
    };
    let own = p.video_kbps.or_else(|| p.total_kbps.map(|t| t.saturating_sub(p.audio_kbps)).filter(|k| *k > 0));
    own.map_or(most, |k| (k * 5 / 4).clamp(300.min(most), most))
}

/// The arguments for ffmpeg: the video, with `subtitles` (a file in the folder ffmpeg runs in) on
/// the picture, encoded with `encoder`, into the MP4 `dest`.
fn burn_arguments(video: &Path, subtitles: &str, encoder: &str, p: &Probe, dest: &Path) -> Vec<std::ffi::OsString> {
    let mut a: Vec<std::ffi::OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"].iter().map(Into::into).collect();
    a.push(video.into());
    let filter = format!("subtitles={subtitles}:force_style='{STYLE}',format=nv12");
    for x in ["-map", "0:v:0", "-map", "0:a?", "-vf", &filter, "-c:v", encoder] {
        a.push(x.into());
    }
    match encoder {
        "h264_nvenc" => a.extend(["-preset".into(), "p5".into()]),
        "libx264" => a.extend(["-preset".into(), "faster".into()]),
        _ => {}
    }
    let k = bitrate(p);
    for x in ["-b:v".to_string(), format!("{k}k"), "-maxrate".into(), format!("{}k", k * 3 / 2), "-bufsize".into(), format!("{}k", k * 2)] {
        a.push(x.into());
    }
    // sound MP4 holds goes in as it is; the rest becomes AAC
    const KEPT: &[&str] = &["aac", "mp3", "ac3", "eac3", "alac", "flac", "opus"];
    if p.audio.iter().all(|c| KEPT.contains(&c.as_str())) {
        a.extend(["-c:a".into(), "copy".into()]);
    } else {
        a.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    }
    for x in ["-movflags", "+faststart", "-progress", "pipe:1", "-nostats"] {
        a.push(x.into());
    }
    a.push(dest.into());
    a
}

/// What "name (English, burned in).mp4" says after the names of the languages.
pub fn burned_dest(video: &Path, tracks: &[Track]) -> PathBuf {
    let stem = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let names = tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>().join(", ");
    video.with_file_name(format!("{stem} ({names}, {}).mp4", crate::tr!("burned in")))
}

/// Write `video` to the MP4 `dest` with the subtitles `srt` drawn on its picture. The picture is
/// encoded again, on the graphics chip when there is one; `on_progress` hears how far it is (0 to 1).
pub fn burn_subtitles(ffmpeg: &Path, video: &Path, srt: &str, dest: &Path, scratch: &Path, cancel: &AtomicBool, on_progress: &mut dyn FnMut(f64)) -> Result<()> {
    if same_file(video, dest) {
        bail!(crate::tr!("Choose another name: the video itself cannot be written over."));
    }
    std::fs::create_dir_all(scratch)?;
    let name = scratch_name("burn", "srt");
    std::fs::write(scratch.join(&name), srt)?;
    let result = burn_with_any(ffmpeg, video, &name, dest, scratch, cancel, on_progress);
    let _ = std::fs::remove_file(scratch.join(&name));
    result
}

fn burn_with_any(ffmpeg: &Path, video: &Path, subtitles: &str, dest: &Path, scratch: &Path, cancel: &AtomicBool, on_progress: &mut dyn FnMut(f64)) -> Result<()> {
    let p = probe(ffmpeg, video);
    let has = crate::setup::ffmpeg_lists(ffmpeg, "-encoders");
    let encoders: Vec<&str> = H264_ENCODERS.iter().copied().filter(|e| has.iter().any(|h| h == e)).collect();
    if encoders.is_empty() {
        bail!(crate::tr!("This ffmpeg cannot burn subtitles in. Download the video ffmpeg in Settings."));
    }
    let part = dest.with_extension("part.mp4");
    let mut why = String::new();
    for encoder in encoders {
        let mut cmd = Command::new(ffmpeg);
        cmd.args(burn_arguments(video, subtitles, encoder, &p, &part)).current_dir(scratch).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        no_window(&mut cmd);
        let mut child = cmd.spawn().with_context(|| crate::tr!("could not start ffmpeg"))?;
        let mut err = child.stderr.take().unwrap();
        let err_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut err, &mut s);
            s
        });
        let mut moved = false;
        let mut cancelled = false;
        for line in std::io::BufRead::lines(std::io::BufReader::new(child.stdout.take().unwrap())).map_while(|l| l.ok()) {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                let _ = child.kill();
                break;
            }
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                if us > 0.0 {
                    moved = true;
                    if p.duration_s > 0.0 {
                        on_progress((us / 1e6 / p.duration_s).clamp(0.0, 1.0));
                    }
                }
            }
        }
        let status = child.wait()?;
        let stderr = err_thread.join().unwrap_or_default();
        if cancelled || cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&part);
            bail!("cancelled");
        }
        if status.success() {
            std::fs::rename(&part, dest)?;
            on_progress(1.0);
            return Ok(());
        }
        let _ = std::fs::remove_file(&part);
        why = stderr.trim().to_string();
        // an encoder that could not even start (no such chip, an old driver): the next one
        if moved {
            break;
        }
    }
    bail!(crate::tr!("ffmpeg could not burn the subtitles in: {}", why))
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
    }

    fn stream(kind: &str, codec: &str) -> Stream {
        Stream { kind: kind.into(), codec: codec.into() }
    }

    #[test]
    fn the_container_is_one_that_holds_the_videos_streams() {
        let phone = [stream("Video", "h264"), stream("Audio", "aac"), stream("Data", "none")];
        assert_eq!(container(Path::new("a.mp4"), &phone), "mp4");
        assert_eq!(container(Path::new("a.MOV"), &phone), "mp4", "an iPhone's MOV becomes an MP4");
        // a camera's or an editor's MOV: ProRes, or PCM sound, an MP4 does not hold
        assert_eq!(container(Path::new("a.mov"), &[stream("Video", "prores"), stream("Audio", "pcm_s16le")]), "mov");
        assert_eq!(container(Path::new("a.mov"), &[stream("Video", "h264"), stream("Audio", "pcm_s24le")]), "mov");
        assert_eq!(container(Path::new("a.mp4"), &[stream("Video", "h264"), stream("Audio", "pcm_s16le")]), "mkv");
        assert_eq!(container(Path::new("a.webm"), &phone), "mkv");
        assert_eq!(container(Path::new("a.avi"), &phone), "mkv");
        let listed = parse_streams(PROBE);
        assert_eq!(listed[0], stream("Video", "h264"));
        assert_eq!(listed[2], stream("Audio", "vorbis"));
        assert_eq!(parse_streams("  Stream #0:4: Subtitle: hdmv_pgs_subtitle (pgssub), 1920x1080"), [stream("Subtitle", "hdmv_pgs_subtitle")]);
    }

    #[test]
    fn burned_in_lines_find_their_own_direction() {
        // without it, libass lays an Arabic line out from the left: the full stop at the start
        assert!(STYLE.split(',').any(|x| x == "Encoding=-1"), "{STYLE}");
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
        assert!(line.contains("-map 0 -map 1:0 -map 2:0 -c copy -c:s:1 srt"), "{line}");
        assert!(line.contains("-c:s:2 srt") && !line.contains("-c:s srt") && !line.contains("-c:s:0"), "the video's own subtitles are copied: {line}");
        assert!(line.contains("-metadata:s:s:1 language=ara") && line.contains("-metadata:s:s:2 language=eng"), "after the one it has: {line}");
        assert!(line.contains("-disposition:s:1 default") && line.contains("-disposition:s:2 0"), "{line}");
        let mp4: String = arguments(Path::new("in.mp4"), &files[..1], &tracks[..1], 0, Path::new("out.mp4")).iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        assert!(mp4.contains("-map 0:v? -map 0:a? -map 0:s? -map 1:0 -c copy -c:s:0 mov_text"), "{mp4}");
        let mov: String = arguments(Path::new("in.mov"), &files[..1], &tracks[..1], 0, Path::new("out.mov")).iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        assert!(mov.contains("-map 0:v? -map 0:a? -map 0:s? -map 1:0 -c copy -c:s:0 mov_text"), "{mov}");
    }

    const PROBE: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'in.mp4':
  Duration: 00:01:02.50, start: 0.000000, bitrate: 4521 kb/s
  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709, progressive), 1920x1080 [SAR 1:1 DAR 16:9], 4387 kb/s, 30 fps, 30 tbr, 15360 tbn (default)
  Stream #0:1[0x2](eng): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 128 kb/s (default)
  Stream #0:2[0x3](ara): Audio: vorbis, 44100 Hz, stereo, fltp
  Stream #0:3: Video: mjpeg (Baseline), yuvj420p(pc, bt470bg/unknown/unknown), 300x300, 90k tbr (attached pic)";

    #[test]
    fn what_burning_needs_is_read_from_ffmpeg() {
        let p = parse_probe(PROBE);
        let audio = vec!["aac".into(), "vorbis".into()];
        assert_eq!(p, Probe { duration_s: 62.5, total_kbps: Some(4521), pixels: 1920 * 1080, video_kbps: Some(4387), audio, audio_kbps: 128 + 128 });
        assert_eq!(bitrate(&p), 5483, "a quarter above its own");
        assert_eq!(bitrate(&Probe { pixels: 1920 * 1080, video_kbps: Some(40_000), ..Default::default() }), 8_000, "no more than 1080p needs");
        assert_eq!(bitrate(&Probe { pixels: 1280 * 720, video_kbps: Some(800), ..Default::default() }), 1_000, "a web video stays about its size: no floor of 1,500");
        assert_eq!(bitrate(&Probe { pixels: 1920 * 1080, video_kbps: Some(100), ..Default::default() }), 300, "only a floor against nonsense");
        // MKV and WebM say no bitrate per stream: the file's, less its sound's
        let mkv = parse_probe("  Duration: 00:10:00.00, start: 0.000000, bitrate: 1128 kb/s\n  Stream #0:0: Video: h264 (High), yuv420p(progressive), 1280x720, 25 fps\n  Stream #0:1: Audio: opus, 48000 Hz, stereo, fltp (default)");
        assert_eq!((mkv.total_kbps, mkv.video_kbps, mkv.audio_kbps), (Some(1128), None, 128));
        assert_eq!(bitrate(&mkv), 1_250, "(1128 - 128) a quarter up, not the cap of 5,000");
        assert_eq!(bitrate(&Probe { pixels: 640 * 360, ..Default::default() }), 2_500, "nothing known: by its size");
    }

    #[test]
    fn burning_draws_the_subtitles_and_keeps_the_sound_mp4_can_hold() {
        let line = |audio: &[&str], enc: &str| {
            let p = Probe { duration_s: 10.0, pixels: 1280 * 720, video_kbps: Some(2000), audio: audio.iter().map(|s| s.to_string()).collect(), ..Default::default() };
            burn_arguments(Path::new("in.mkv"), "burn-1.srt", enc, &p, Path::new("out.mp4")).iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
        };
        let l = line(&["aac"], "h264_nvenc");
        assert!(l.contains("-map 0:v:0 -map 0:a? -vf subtitles=burn-1.srt:force_style='FontName=Segoe UI,"), "{l}");
        assert!(l.contains(",format=nv12 -c:v h264_nvenc -preset p5 -b:v 2500k -maxrate 3750k -bufsize 5000k -c:a copy"), "{l}");
        assert!(l.ends_with("-progress pipe:1 -nostats out.mp4"), "{l}");
        assert!(line(&["opus", "ac3"], "h264_mf").contains("-c:v h264_mf -b:v 2500k") && line(&["opus"], "h264_mf").contains("-c:a copy"));
        assert!(line(&["aac", "vorbis"], "h264_amf").contains("-c:a aac -b:a 192k"), "Vorbis does not go into an MP4");
        assert!(line(&[], "libx264").contains("-preset faster"));
    }

    /// With an ffmpeg that can burn (Dawwin's video one, or a full one): Arabic and English on the picture.
    #[test]
    fn a_real_video_gets_subtitles_burned_in() {
        let ffmpeg = std::env::var_os("DAWWIN_TEST_BURNER").map(PathBuf::from).or_else(|| crate::setup::find_exe("ffmpeg"));
        let Some(ffmpeg) = ffmpeg.filter(|f| crate::setup::ffmpeg_lists(f, "-filters").iter().any(|x| x == "subtitles")) else { return };
        let dir = std::env::temp_dir().join(format!("dawwin-burn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("in.mkv");
        let made = Command::new(&ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=25:duration=4", "-f", "lavfi", "-i", "sine=duration=4"])
            .args(["-c:v", "mpeg4", "-c:a", "libvorbis"])
            .arg(&video)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return; // an ffmpeg without these
        }
        let srt = "1\n00:00:00,500 --> 00:00:03,000\nثلاثة لاعبين\nThree players\n\n";
        let dest = dir.join("out.mp4");
        let mut seen = Vec::new();
        burn_subtitles(&ffmpeg, &video, srt, &dest, &dir, &AtomicBool::new(false), &mut |f| seen.push(f)).unwrap();
        assert!(dest.is_file() && !dir.join("out.part.mp4").exists());
        assert_eq!(seen.last(), Some(&1.0));
        let p = probe(&ffmpeg, &dest);
        assert!((p.duration_s - 4.0).abs() < 0.3 && p.pixels == 320 * 180 && p.audio == ["aac"], "{p:?}");
        assert!(std::fs::read_dir(&dir).unwrap().flatten().all(|f| !f.file_name().to_string_lossy().ends_with(".srt")), "the subtitles file is gone");

        let stop = AtomicBool::new(true);
        let err = burn_subtitles(&ffmpeg, &video, srt, &dir.join("stopped.mp4"), &dir, &stop, &mut |_| {}).unwrap_err();
        assert!(err.to_string().contains("cancelled") && !dir.join("stopped.mp4").exists() && !dir.join("stopped.part.mp4").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A PGS stream of empty display sets, like a film's picture subtitles (no ffmpeg can make one).
    fn pgs() -> Vec<u8> {
        let mut b = Vec::new();
        for k in 0..12u32 {
            let pts = 45_000 + k * 9_000; // from 0.5 s, every 0.1 s
            let mut segment = |kind: u8, payload: &[u8]| {
                b.extend_from_slice(b"PG");
                b.extend_from_slice(&pts.to_be_bytes());
                b.extend_from_slice(&0u32.to_be_bytes());
                b.push(kind);
                b.extend_from_slice(&(payload.len() as u16).to_be_bytes());
                b.extend_from_slice(payload);
            };
            let n = (k as u16).to_be_bytes();
            // a composition of 320x180 with no objects, then the end of the set
            segment(0x16, &[0x01, 0x40, 0x00, 0xb4, 0x10, n[0], n[1], if k == 0 { 0x80 } else { 0 }, 0, 0, 0]);
            segment(0x80, &[]);
        }
        b
    }

    const STYLED_ASS: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 384\nPlayResY: 288\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Sign,Arial,16,&Hffffff,&Hffffff,&H0,&H0,0,0,0,0,100,100,0,0,1,1,0,8,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.50,0:00:02.00,Sign,,0,0,0,,{\\pos(160,20)\\c&H00FFFF&}Sign: Hello\n";

    /// A film with picture subtitles (PGS) and styled ones (ASS): with every ffmpeg Dawwin may use,
    /// the new tracks are added and the film's own are copied exactly as they are. And a ProRes MOV,
    /// which an MP4 cannot hold, stays a MOV.
    #[test]
    fn a_films_own_subtitles_are_kept_and_a_prores_mov_stays_a_mov() {
        let dir = std::env::temp_dir().join(format!("dawwin-own-subs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (sup, ass) = (dir.join("film.sup"), dir.join("film.ass"));
        std::fs::write(&sup, pgs()).unwrap();
        std::fs::write(&ass, STYLED_ASS).unwrap();
        let (sup, ass) = (sup.to_string_lossy().into_owned(), ass.to_string_lossy().into_owned());
        let film = crate::setup::test_sample(&dir, "film.mkv", &[
            "-f", "lavfi", "-i", "testsrc=size=320x180:rate=10:duration=3", "-f", "lavfi", "-i", "sine=duration=3", "-i", &sup, "-i", &ass,
            "-map", "0:v", "-map", "1:a", "-map", "2:s", "-map", "3:s", "-c:v", "mpeg4", "-c:a", "aac", "-c:s", "copy",
        ]);
        let prores = crate::setup::test_sample(&dir, "camera.mov", &[
            "-f", "lavfi", "-i", "testsrc=size=320x180:rate=10:duration=3", "-f", "lavfi", "-i", "sine=duration=3",
            "-c:v", "prores_ks", "-c:a", "pcm_s16le",
        ]);
        let (Some(film), Some(prores)) = (film, prores) else { return };
        let srt = "1\n00:00:00,500 --> 00:00:02,000\nطيب خلونا نبدأ.\n\n".to_string();
        let tracks = vec![Track { srt, language: "ar".into(), title: "Arabic".into() }];
        for (name, ff) in crate::setup::test_ffmpegs() {
            assert_eq!(output_ext(&ff, &film), "mkv");
            let dest = dir.join(format!("film-{name}.mkv"));
            add_subtitles(&ff, &film, &tracks, &dest, &dir).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            let subs: Vec<String> = streams(&ff, &dest).into_iter().filter(|s| s.kind == "Subtitle").map(|s| s.codec).collect();
            assert_eq!(subs, ["hdmv_pgs_subtitle", "ass", "subrip"], "{name}");

            assert_eq!(output_ext(&ff, &prores), "mov", "{name}");
            let dest = dir.join(format!("camera-{name}.mov"));
            add_subtitles(&ff, &prores, &tracks, &dest, &dir).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            let kinds: Vec<String> = streams(&ff, &dest).into_iter().map(|s| s.codec).collect();
            assert_eq!(kinds, ["prores", "pcm_s16le", "mov_text"], "{name}");
        }
        assert!(std::fs::read_dir(&dir).unwrap().flatten().all(|f| !f.file_name().to_string_lossy().starts_with("subtitles-")), "no scratch file left");
        let _ = std::fs::remove_dir_all(&dir);
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
            assert_eq!(streams(&ffmpeg, &dest).iter().filter(|s| s.kind == "Subtitle").count(), 2, "{ext}");
            assert!(!dir.join(format!("out.part.{ext}")).exists());
            assert!(add_subtitles(&ffmpeg, &video, &tracks, &video, &dir).is_err(), "never over the video itself");
        }
    }
}
