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
    let mut child = cmd.spawn().context("could not start ffmpeg")?;
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
        bail!("ffmpeg could not read the file: {}", stderr.trim());
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

    let file = std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| anyhow!("unsupported format ({e}); install ffmpeg for this file type"))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no audio track"))?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.ok_or_else(|| anyhow!("unknown sample rate"))? as usize;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("unsupported codec ({e}); install ffmpeg for this file type"))?;

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
        bail!("no audio decoded");
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

/// Digital silence (or near it): nothing for the model to hear.
pub fn is_silent(audio: &[f32]) -> bool {
    audio.iter().fold(0f32, |m, x| m.max(x.abs())) < 1e-3
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
