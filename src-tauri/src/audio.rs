//! Audio in: any file to 16 kHz mono f32, and the cut points between chunks.
//!
//! ffmpeg decodes when it is installed (every format, including WhatsApp's ogg/opus);
//! otherwise symphonia decodes the common formats (wav, mp3, m4a/aac, mp4, flac, ogg/vorbis).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context, Result};

use crate::setup::no_window;

pub const SR: usize = 16_000;

/// Decode `path` to 16 kHz mono samples. `cancel` stops it early.
pub fn decode(path: &Path, ffmpeg: Option<&PathBuf>, cancel: &AtomicBool) -> Result<Vec<f32>> {
    if let Some(ff) = ffmpeg {
        match decode_ffmpeg(ff, path, cancel) {
            Ok(s) => return Ok(s),
            Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
            Err(e) => {
                // fall through to symphonia; keep ffmpeg's reason if that fails too
                return decode_symphonia(path).map_err(|e2| anyhow!("{e:#}; built-in decoder: {e2:#}"));
            }
        }
    }
    decode_symphonia(path)
}

fn decode_ffmpeg(ffmpeg: &Path, path: &Path, cancel: &AtomicBool) -> Result<Vec<f32>> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd.spawn().with_context(|| crate::tr!("could not start ffmpeg"))?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let mut bytes = Vec::with_capacity(1 << 24);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            bail!("cancelled");
        }
        let n = out.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    let status = child.wait()?;
    let stderr = err_thread.join().unwrap_or_default();
    if !status.success() || bytes.len() < 4 {
        bail!(crate::tr!("ffmpeg could not read the file: {}", stderr.trim()));
    }
    Ok(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

fn decode_symphonia(path: &Path) -> Result<Vec<f32>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).with_context(|| crate::tr!("cannot open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| anyhow!(crate::tr!("unsupported format ({}); install ffmpeg for this file type", e)))?;
    let mut format = probed.format;
    // in a video the default track is the picture: the first sound track it can decode, else that
    let codecs = symphonia::default::get_codecs();
    let track = format
        .tracks()
        .iter()
        .filter(|t| t.codec_params.sample_rate.is_some())
        .find(|t| codecs.make(&t.codec_params, &DecoderOptions::default()).is_ok())
        .or_else(|| format.default_track())
        .ok_or_else(|| anyhow!(crate::tr!("no audio track")))?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.ok_or_else(|| anyhow!("unknown sample rate"))? as usize;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!(crate::tr!("unsupported codec ({}); install ffmpeg for this file type", e)))?;

    let mut mono: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SErr::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SErr::DecodeError(_)) => continue, // skip a damaged packet
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count().max(1);
        let need = decoded.capacity() as u64;
        if sbuf.as_ref().map_or(true, |b| b.capacity() < need as usize * ch) {
            sbuf = Some(SampleBuffer::<f32>::new(need, spec));
        }
        let b = sbuf.as_mut().unwrap();
        b.copy_interleaved_ref(decoded);
        for frame in b.samples().chunks_exact(ch) {
            mono.push(frame.iter().sum::<f32>() / ch as f32);
        }
    }
    if mono.is_empty() {
        bail!(crate::tr!("no audio decoded"));
    }
    resample(&mono, rate, SR)
}

fn resample(input: &[f32], from: usize, to: usize) -> Result<Vec<f32>> {
    use rubato::{FftFixedIn, Resampler};
    if from == to {
        return Ok(input.to_vec());
    }
    let chunk = 4096;
    let mut rs = FftFixedIn::<f32>::new(from, to, chunk, 2, 1)?;
    let mut out = Vec::with_capacity(input.len() * to / from + chunk);
    let mut pos = 0;
    while pos + chunk <= input.len() {
        let o = rs.process(&[&input[pos..pos + chunk]], None)?;
        out.extend_from_slice(&o[0]);
        pos += chunk;
    }
    if pos < input.len() {
        let o = rs.process_partial(Some(&[&input[pos..]][..]), None)?;
        out.extend_from_slice(&o[0]);
    }
    // flush the filter's delay
    let o = rs.process_partial::<&[f32]>(None, None)?;
    out.extend_from_slice(&o[0]);
    let delay = rs.output_delay();
    let want = input.len() * to / from;
    let end = (delay + want).min(out.len());
    Ok(out[delay.min(end)..end].to_vec())
}

/// RMS of 20 ms frames, smoothed over 200 ms (same as numpy's `convolve(..., "same")`).
fn smoothed_rms(audio: &[f32]) -> Vec<f32> {
    let frame = SR / 50;
    let n = audio.len() / frame;
    let rms: Vec<f32> = (0..n)
        .map(|i| {
            let s = &audio[i * frame..(i + 1) * frame];
            (s.iter().map(|x| x * x).sum::<f32>() / frame as f32 + 1e-12).sqrt()
        })
        .collect();
    let k = 10usize;
    let back = k / 2; // numpy 'same' with an even kernel: window [i-5, i+4]
    let mut out = vec![0f32; n];
    for (i, o) in out.iter_mut().enumerate() {
        let lo = i.saturating_sub(back);
        let hi = (i + k - back).min(n);
        *o = rms[lo..hi].iter().sum::<f32>() / k as f32;
    }
    out
}

fn quietest(rms: &[f32], lo_f: usize, hi_f: usize) -> usize {
    let hi_f = hi_f.min(rms.len());
    if lo_f >= hi_f {
        return lo_f;
    }
    let mut best = lo_f;
    for f in lo_f..hi_f {
        if rms[f] < rms[best] {
            best = f;
        }
    }
    best
}

/// Chunks of at most `max_s` seconds, each cut at the quietest point of its last `search_s`
/// seconds so a word is not split in two.
pub fn split_points(audio: &[f32], max_s: f32, search_s: f32) -> Vec<(usize, usize)> {
    let frame = SR / 50;
    let rms = smoothed_rms(audio);
    let max_len = (max_s * SR as f32) as usize;
    let search = (search_s * SR as f32) as usize;
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while audio.len() - start > max_len {
        let lo_f = (start + max_len - search) / frame;
        let hi_f = (start + max_len) / frame;
        let cut = quietest(&rms, lo_f, hi_f) * frame + frame / 2;
        let cut = cut.max(start + frame).min(audio.len());
        chunks.push((start, cut));
        start = cut;
    }
    if start < audio.len() {
        chunks.push((start, audio.len()));
    }
    chunks
}

/// The quietest point in the middle of `[a, b)`, to split a chunk the model looped on.
pub fn split_middle(audio: &[f32], a: usize, b: usize) -> usize {
    let frame = SR / 50;
    let seg = &audio[a..b];
    let rms = smoothed_rms(seg);
    let len = b - a;
    let lo_f = (len * 35 / 100) / frame;
    let hi_f = (len * 65 / 100) / frame;
    let cut = quietest(&rms, lo_f, hi_f.max(lo_f + 1)) * frame + frame / 2;
    (a + cut).clamp(a + frame, b.saturating_sub(frame).max(a + frame))
}

/// Where the speech is inside `[a, b)`: the times (in seconds from the start of the recording) at
/// which 0, 1/n, 2/n … n/n of the voiced part has gone by. The model only times whole chunks; this
/// lets the UI place a word in its chunk by how far into the text it is, skipping pauses.
/// Empty when the range holds too little speech to tell.
pub fn voice_profile(audio: &[f32], a: usize, b: usize, n: usize) -> Vec<f32> {
    let frame = SR / 50;
    let rms = smoothed_rms(&audio[a..b]);
    if rms.len() < 10 || n == 0 {
        return Vec::new();
    }
    let db: Vec<f32> = rms.iter().map(|r| 20.0 * r.max(1e-6).log10()).collect();
    let mut sorted = db.clone();
    sorted.sort_by(|x, y| x.total_cmp(y));
    let loud = sorted[sorted.len() * 9 / 10];
    if loud < -60.0 {
        return Vec::new(); // silence
    }
    // frames within 25 dB of the loud ones are speech; the rest are pauses and breath
    let voiced: Vec<usize> = (0..db.len()).filter(|&i| db[i] > loud - 25.0).collect();
    let total = voiced.len();
    if total < 25 {
        return Vec::new(); // under half a second of speech
    }
    let t0 = a as f32 / SR as f32;
    let fs = frame as f32 / SR as f32;
    let at = |frame_idx: usize| ((t0 + frame_idx as f32 * fs) * 100.0).round() / 100.0;
    (0..=n)
        .map(|k| {
            if k == n {
                at(voiced[total - 1] + 1)
            } else {
                at(voiced[(k * total / n).min(total - 1)])
            }
        })
        .collect()
}

/// Where the speaker stops inside `[a, b)`: each pause of at least a quarter second between words,
/// as [how much of the speech in the range came before it (0 to 1), its start, its end], in seconds
/// from the start of the recording. Subtitles end and start at these, as professional ones do.
pub fn pauses(audio: &[f32], a: usize, b: usize) -> Vec<[f32; 3]> {
    const MIN_FRAMES: usize = 12; // 240 ms
    let frame = SR / 50;
    let rms = smoothed_rms(&audio[a..b]);
    if rms.len() < 10 {
        return Vec::new();
    }
    let db: Vec<f32> = rms.iter().map(|r| 20.0 * r.max(1e-6).log10()).collect();
    let mut sorted = db.clone();
    sorted.sort_by(|x, y| x.total_cmp(y));
    let loud = sorted[sorted.len() * 9 / 10];
    if loud < -60.0 {
        return Vec::new();
    }
    // the same line between speech and pause as `voice_profile`
    let voiced: Vec<bool> = db.iter().map(|d| *d > loud - 25.0).collect();
    let total = voiced.iter().filter(|v| **v).count();
    let (Some(first), Some(last)) = (voiced.iter().position(|v| *v), voiced.iter().rposition(|v| *v)) else { return Vec::new() };
    let t0 = a as f32 / SR as f32;
    let at = |f: usize| ((t0 + f as f32 * frame as f32 / SR as f32) * 100.0).round() / 100.0;
    let mut out = Vec::new();
    let (mut before, mut i) = (0usize, first);
    while i <= last {
        if voiced[i] {
            before += 1;
            i += 1;
            continue;
        }
        let run_start = i;
        while i <= last && !voiced[i] {
            i += 1;
        }
        if i - run_start >= MIN_FRAMES {
            let share = ((before as f32 / total as f32) * 1000.0).round() / 1000.0;
            out.push([share, at(run_start), at(i)]);
        }
    }
    out
}

/// The parts of a recording to keep when its silences are taken out: every pause longer than
/// `min_gap_s` is shortened to `keep_s` (half of it at each side, so speech never starts or ends
/// abruptly), and the silence before the first word and after the last goes too. Silence is what
/// stays near the recording's own quietest level (a room's hum, a fan) or 35 dB under its loud
/// parts, whichever is higher; only a whole `min_gap_s` of it counts, so a quiet word is never cut.
/// As sample ranges of `audio`.
pub fn speech_spans(audio: &[f32], min_gap_s: f32, keep_s: f32) -> Vec<(usize, usize)> {
    let frame = SR / 50;
    let rms = smoothed_rms(audio);
    if rms.is_empty() {
        return Vec::new();
    }
    let db: Vec<f32> = rms.iter().map(|r| 20.0 * r.max(1e-6).log10()).collect();
    let mut sorted = db.clone();
    sorted.sort_by(|x, y| x.total_cmp(y));
    let loud = sorted[sorted.len() * 9 / 10];
    if loud < -60.0 {
        return Vec::new(); // nothing but silence
    }
    // the hum of a room is 30 dB or so under the voices: "35 dB under" alone would keep it all
    let floor = sorted[sorted.len() / 20];
    let line = (loud - 35.0).max(floor + 0.4 * (loud - floor));
    let silent: Vec<bool> = db.iter().map(|d| *d < line).collect();
    let min_gap = (min_gap_s * 50.0).round() as usize;
    let half = ((keep_s / 2.0) * SR as f32) as usize;
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    let mut speech_from: Option<usize> = None;
    while i < silent.len() {
        if !silent[i] {
            speech_from.get_or_insert(i);
            i += 1;
            continue;
        }
        let run = i;
        while i < silent.len() && silent[i] {
            i += 1;
        }
        let (Some(from), at_end) = (speech_from, i >= silent.len()) else { continue };
        if i - run >= min_gap || at_end {
            spans.push((from * frame, run * frame));
            speech_from = None;
        }
    }
    if let Some(from) = speech_from {
        spans.push((from * frame, audio.len()));
    }
    // a little of each silence stays, at both sides
    spans
        .into_iter()
        .map(|(a, b)| (a.saturating_sub(half), (b + half).min(audio.len())))
        .fold(Vec::new(), |mut out: Vec<(usize, usize)>, (a, b)| {
            match out.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
            out
        })
}

/// The kept parts, one after the other.
pub fn join_spans(audio: &[f32], spans: &[(usize, usize)]) -> Vec<f32> {
    let mut out = Vec::with_capacity(spans.iter().map(|(a, b)| b - a).sum());
    for &(a, b) in spans {
        out.extend_from_slice(&audio[a..b]);
    }
    out
}

/// From a time in the shortened recording back to the same moment in the original. `end`: a time
/// that ends something, which at a join stays before the silence that was taken out.
pub fn original_time(spans: &[(usize, usize)], seconds: f64, end: bool) -> f64 {
    let mut at = (seconds * SR as f64).round().max(0.0) as usize;
    for &(a, b) in spans {
        let len = b - a;
        if at < len || (end && at == len) {
            return (a + at) as f64 / SR as f64;
        }
        at -= len;
    }
    spans.last().map_or(seconds, |&(_, b)| b as f64 / SR as f64)
}

/// Digital silence (or near it): nothing for the model to hear.
pub fn is_silent(audio: &[f32]) -> bool {
    audio.iter().fold(0f32, |m, x| m.max(x.abs())) < 1e-3
}

/// `path`, or "name (2).ext", "name (3).ext"… when a file of that name is there already.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| path.with_file_name(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap()
}

/// Save the sound of `source` (a recording or a video) to `dest`. With ffmpeg: an M4A; only
/// `spans` of it when given (as 16 kHz sample ranges, from [`speech_spans`]), cut sample-exact at
/// 48 kHz; otherwise its own sound track, copied as it is when it already is AAC. Without ffmpeg:
/// a WAV at 16 kHz from `samples`, the decoded recording. Returns the file written, whose extension
/// may differ from `dest`'s.
pub fn save_sound(source: &Path, dest: &Path, spans: Option<&[(usize, usize)]>, samples: &[f32], ffmpeg: Option<&PathBuf>, cancel: &AtomicBool) -> Result<PathBuf> {
    let Some(ff) = ffmpeg else {
        let dest = unique_path(&dest.with_extension("wav"));
        match spans {
            Some(s) => write_wav(&dest, &join_spans(samples, s))?,
            None => write_wav(&dest, samples)?,
        }
        return Ok(dest);
    };
    let dest = unique_path(&dest.with_extension("m4a"));
    let part = dest.with_extension("part.m4a");
    let run = |args: &[&str], src: &Path| -> Result<std::process::Output> {
        let mut cmd = Command::new(ff);
        cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(src).args(args).arg(&part);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        no_window(&mut cmd);
        Ok(cmd.output()?)
    };
    let result = match spans {
        None => {
            // the sound track as it is, else made AAC
            let copied = run(&["-vn", "-sn", "-dn", "-map", "0:a:0", "-c:a", "copy"], source)?;
            if copied.status.success() {
                Ok(())
            } else {
                let made = run(&["-vn", "-sn", "-dn", "-map", "0:a:0", "-c:a", "aac", "-b:a", "128k"], source)?;
                if made.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&made.stderr).trim().to_string()) }
            }
        }
        Some(spans) => cut_with_ffmpeg(ff, source, &part, spans, cancel),
    };
    if let Err(why) = result {
        let _ = std::fs::remove_file(&part);
        bail!(crate::tr!("ffmpeg could not save the sound: {}", why));
    }
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

/// One ffmpeg reads the sound at 48 kHz, this keeps the spans, and a second one writes them as AAC.
fn cut_with_ffmpeg(ff: &Path, source: &Path, dest: &Path, spans: &[(usize, usize)], cancel: &AtomicBool) -> std::result::Result<(), String> {
    use std::io::Write;
    const RATE: usize = 48_000;
    let scale = RATE / SR;
    let mut read = Command::new(ff);
    read.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"]).arg(source);
    read.args(["-vn", "-sn", "-dn", "-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "f32le", "-"]);
    read.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    no_window(&mut read);
    let mut write = Command::new(ff);
    write.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-f", "f32le", "-ar", "48000", "-ac", "1", "-i", "-"]);
    write.args(["-c:a", "aac", "-b:a", "96k"]).arg(dest);
    write.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    no_window(&mut write);
    let mut reader = read.spawn().map_err(|e| e.to_string())?;
    let mut writer = write.spawn().map_err(|e| e.to_string())?;
    let mut input = reader.stdout.take().unwrap();
    let mut output = writer.stdin.take().unwrap();
    let mut buf = vec![0u8; 1 << 16];
    let mut carry: Vec<u8> = Vec::new();
    let mut at = 0usize; // samples read so far, at 48 kHz
    let mut span = 0usize;
    let failed = loop {
        if cancel.load(Ordering::Relaxed) {
            break Some("cancelled".to_string());
        }
        let n = match input.read(&mut buf) {
            Ok(0) => break None,
            Ok(n) => n,
            Err(e) => break Some(e.to_string()),
        };
        carry.extend_from_slice(&buf[..n]);
        let whole = carry.len() / 4 * 4;
        let mut keep: Vec<u8> = Vec::with_capacity(whole);
        for sample in carry[..whole].chunks_exact(4) {
            while span < spans.len() && at >= spans[span].1 * scale {
                span += 1;
            }
            if span < spans.len() && at >= spans[span].0 * scale {
                keep.extend_from_slice(sample);
            }
            at += 1;
        }
        carry.drain(..whole);
        if output.write_all(&keep).is_err() {
            break Some("the encoder stopped".to_string());
        }
    };
    drop(output);
    let _ = reader.wait();
    let done = writer.wait_with_output().map_err(|e| e.to_string())?;
    match failed {
        Some(why) => {
            let _ = std::fs::remove_file(dest);
            Err(why)
        }
        None if !done.status.success() => Err(String::from_utf8_lossy(&done.stderr).trim().to_string()),
        None => Ok(()),
    }
}

pub fn write_raw_f32(path: &Path, samples: &[f32]) -> Result<()> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// 16-bit PCM WAV, for playing files the webview cannot play itself.
pub fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&((SR * 2) as u32).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, b)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_everything_and_respect_max() {
        // 95 s of tone with a quiet gap every 7 s
        let n = 95 * SR;
        let audio: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                if (t % 7.0) < 0.4 { 0.0 } else { (t * 440.0 * 6.283).sin() * 0.3 }
            })
            .collect();
        let ch = split_points(&audio, 30.0, 8.0);
        assert_eq!(ch.first().unwrap().0, 0);
        assert_eq!(ch.last().unwrap().1, n);
        for w in ch.windows(2) {
            assert_eq!(w[0].1, w[1].0);
        }
        for (a, b) in &ch {
            assert!(b - a <= 30 * SR, "chunk too long: {}", (b - a) as f32 / SR as f32);
            assert!(b > a);
        }
        // every cut except the end lands in a quiet gap
        for (_, b) in &ch[..ch.len() - 1] {
            let t = *b as f32 / SR as f32;
            assert!((t % 7.0) < 0.45, "cut at {t} is not in a gap");
        }
    }

    #[test]
    fn voice_profile_skips_the_silence() {
        // 5 s of silence, then 5 s of speech-like tone
        let audio: Vec<f32> = (0..10 * SR)
            .map(|i| if i < 5 * SR { 0.0 } else { (i as f32 * 0.07).sin() * 0.3 })
            .collect();
        let p = voice_profile(&audio, 0, audio.len(), 10);
        assert_eq!(p.len(), 11);
        assert!((p[0] - 5.0).abs() < 0.2, "speech starts at 5 s, got {}", p[0]);
        assert!((p[5] - 7.5).abs() < 0.3, "half the speech is gone by 7.5 s, got {}", p[5]);
        assert!(p.windows(2).all(|w| w[0] <= w[1]));
        assert!(voice_profile(&vec![0.0; 3 * SR], 0, 3 * SR, 10).is_empty());
    }

    #[test]
    fn pauses_are_found_between_the_words() {
        // 1 s silence, 2 s speech, 0.6 s pause, 2 s speech, 0.1 s dip, 1 s speech, 1 s silence
        let tone = |i: usize| (i as f32 * 0.07).sin() * 0.3;
        let spans = [(0.0, 1.0, false), (1.0, 3.0, true), (3.0, 3.6, false), (3.6, 5.6, true), (5.6, 5.7, false), (5.7, 6.7, true), (6.7, 7.7, false)];
        let audio: Vec<f32> = (0..(7.7 * SR as f32) as usize)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let speech = spans.iter().any(|(a, b, s)| *s && t >= *a && t < *b);
                if speech { tone(i) } else { 0.0 }
            })
            .collect();
        let p = pauses(&audio, 0, audio.len());
        assert_eq!(p.len(), 1, "silence before and after the speech, and a 0.1 s dip, are no pauses: {p:?}");
        let [share, start, end] = p[0];
        assert!((share - 0.4).abs() < 0.05, "2 of 5 s of speech came before it: {share}");
        assert!((start - 3.0).abs() < 0.2 && (end - 3.6).abs() < 0.2, "{start}..{end}");
        assert!(pauses(&vec![0.0; 3 * SR], 0, 3 * SR).is_empty());
    }

    /// Speech-like tone where `spans` says, silence elsewhere.
    fn speech_at(total_s: f32, spans: &[(f32, f32)]) -> Vec<f32> {
        (0..(total_s * SR as f32) as usize)
            .map(|i| {
                let t = i as f32 / SR as f32;
                if spans.iter().any(|(a, b)| t >= *a && t < *b) { (i as f32 * 0.07).sin() * 0.3 } else { 0.0 }
            })
            .collect()
    }

    #[test]
    fn long_silences_go_and_short_pauses_stay() {
        // silence, speech, a 1.5 s pause, speech, a 0.3 s pause, speech, silence
        let audio = speech_at(9.8, &[(1.0, 3.0), (4.5, 6.5), (6.8, 7.8)]);
        let spans = speech_spans(&audio, 0.7, 0.3);
        let s: Vec<(f32, f32)> = spans.iter().map(|(a, b)| (*a as f32 / SR as f32, *b as f32 / SR as f32)).collect();
        assert_eq!(s.len(), 2, "the 0.3 s pause is speech's own rhythm: {s:?}");
        assert!((s[0].0 - 0.85).abs() < 0.2 && (s[0].1 - 3.15).abs() < 0.2, "{s:?}");
        assert!((s[1].0 - 4.35).abs() < 0.2 && (s[1].1 - 7.95).abs() < 0.2, "{s:?}");
        let kept: f32 = s.iter().map(|(a, b)| b - a).sum();
        assert!(kept > 5.0 && kept < 6.5, "from 9.8 s to about 5.9: {kept}");
        assert_eq!(join_spans(&audio, &spans).len(), spans.iter().map(|(a, b)| b - a).sum::<usize>());
        assert!(speech_spans(&vec![0.0; 3 * SR], 0.7, 0.3).is_empty(), "silence only: nothing to keep");
    }

    #[test]
    fn the_hum_of_a_room_counts_as_silence() {
        // the same, with a steady noise 30 dB under the speech all through, as a meeting room has
        let clean = speech_at(9.8, &[(1.0, 3.0), (4.5, 6.5), (6.8, 7.8)]);
        let noisy: Vec<f32> = clean.iter().enumerate().map(|(i, x)| x + ((i as f32 * 1.7).sin() * (i as f32 * 0.013).cos()) * 0.0095).collect();
        let s = speech_spans(&noisy, 0.7, 0.3);
        assert_eq!(s.len(), 2, "{s:?}");
        let kept: f32 = s.iter().map(|(a, b)| (b - a) as f32 / SR as f32).sum();
        assert!(kept > 5.0 && kept < 6.6, "{kept}");
    }

    #[test]
    fn times_in_the_shortened_recording_lead_back_to_the_original() {
        let spans = [(SR, 3 * SR), (5 * SR, 6 * SR)]; // 1–3 s and 5–6 s: 3 s kept
        assert_eq!(original_time(&spans, 0.0, false), 1.0);
        assert_eq!(original_time(&spans, 1.5, false), 2.5);
        assert_eq!(original_time(&spans, 2.0, true), 3.0, "an end at the join stays before the cut");
        assert_eq!(original_time(&spans, 2.0, false), 5.0, "a start at the join is after it");
        assert_eq!(original_time(&spans, 2.5, false), 5.5);
        assert_eq!(original_time(&spans, 9.0, true), 6.0, "past the end: the end");
    }

    /// With ffmpeg installed: the silences really leave the saved file.
    #[test]
    fn the_shortened_sound_is_saved() {
        let Some(ff) = crate::setup::find_exe("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("dawwin-sound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let audio = speech_at(9.8, &[(1.0, 3.0), (4.5, 6.5), (6.8, 7.8)]);
        let source = dir.join("talk.wav");
        write_wav(&source, &audio).unwrap();
        let spans = speech_spans(&audio, 0.7, 0.3);
        let stop = AtomicBool::new(false);
        let short = save_sound(&source, &dir.join("talk (without silences).m4a"), Some(&spans), &audio, Some(&ff), &stop).unwrap();
        assert_eq!(short.extension().unwrap(), "m4a");
        let back = decode(&short, Some(&ff), &stop).unwrap();
        let kept = spans.iter().map(|(a, b)| b - a).sum::<usize>() as f32 / SR as f32;
        let got = back.len() as f32 / SR as f32;
        assert!((got - kept).abs() < 0.1, "saved {got} s, kept {kept} s");
        // the whole sound, again under the same name: a new name
        let whole = save_sound(&source, &dir.join("talk (without silences).m4a"), None, &audio, Some(&ff), &stop).unwrap();
        assert!(whole.file_name().unwrap().to_string_lossy().ends_with("(2).m4a"), "{whole:?}");
        let wav = save_sound(&source, &dir.join("talk.m4a"), Some(&spans), &audio, None, &stop).unwrap();
        assert_eq!(wav.extension().unwrap(), "wav", "without ffmpeg: a WAV");
    }

    /// An MKV video (OBS, films) is read by the built-in decoder, without ffmpeg. ffmpeg only makes the file.
    #[test]
    fn an_mkv_is_read_without_ffmpeg() {
        let Some(ff) = crate::setup::find_exe("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("dawwin-mkv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mkv = dir.join("clip.mkv");
        let made = Command::new(&ff)
            .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10:duration=4", "-f", "lavfi", "-i", "sine=duration=4"])
            .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
            .arg(&mkv)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return;
        }
        let samples = decode(&mkv, None, &AtomicBool::new(false)).unwrap();
        let secs = samples.len() as f32 / SR as f32;
        assert!((secs - 4.0).abs() < 0.2, "{secs}");
    }

    #[test]
    fn short_audio_is_one_chunk() {
        let audio = vec![0.1f32; 5 * SR];
        assert_eq!(split_points(&audio, 30.0, 8.0), vec![(0, 5 * SR)]);
    }

    #[test]
    fn resample_keeps_length() {
        let input: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample(&input, 44_100, 16_000).unwrap();
        assert!((out.len() as i64 - 16_000).abs() <= 2, "{}", out.len());
    }
}
