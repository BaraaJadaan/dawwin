//! Saving a reading as a sound file: M4A, or M4B with a chapter each for a book, through Dawwin's
//! own ffmpeg. That ffmpeg reads no ffmetadata, so the chapters go in as a small Matroska file of
//! chapters only, which it reads, and its MP4 writer puts them into the file the way players
//! (Apple's too) look for them.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub title: String,
    /// Where it starts in the sound, in milliseconds
    pub start_ms: u64,
}

// ---- a Matroska file of chapters only ----

fn ebml_id(id: u32) -> Vec<u8> {
    let b = id.to_be_bytes();
    let skip = b.iter().position(|&x| x != 0).unwrap_or(3);
    b[skip..].to_vec()
}

/// An EBML size: 8 bytes, always (any reader takes the long form).
fn ebml_size(n: u64) -> Vec<u8> {
    let mut v = (n | 0x0100_0000_0000_0000).to_be_bytes().to_vec();
    v[0] = 0x01;
    v
}

fn element(id: u32, body: &[u8]) -> Vec<u8> {
    let mut v = ebml_id(id);
    v.extend(ebml_size(body.len() as u64));
    v.extend_from_slice(body);
    v
}

fn uint(id: u32, n: u64) -> Vec<u8> {
    element(id, &n.to_be_bytes())
}

/// The chapters as Matroska: times in nanoseconds, each chapter ending where the next starts.
pub fn chapters_mkv(chapters: &[Chapter], total_ms: u64) -> Vec<u8> {
    let header = element(0x1A45DFA3, &[uint(0x4286, 1), uint(0x42F7, 1), uint(0x42F2, 4), uint(0x42F3, 8), element(0x4282, b"matroska"), uint(0x4287, 4), uint(0x4285, 2)].concat());
    let info = element(0x1549A966, &[uint(0x2AD7B1, 1_000_000), element(0x4D80, b"Dawwin"), element(0x5741, b"Dawwin")].concat());
    let mut atoms = Vec::new();
    for (i, c) in chapters.iter().enumerate() {
        let end = chapters.get(i + 1).map_or(total_ms, |n| n.start_ms).max(c.start_ms);
        let display = element(0x80, &[element(0x85, c.title.as_bytes()), element(0x437C, b"und")].concat());
        atoms.extend(element(0xB6, &[uint(0x73C4, i as u64 + 1), uint(0x91, c.start_ms * 1_000_000), uint(0x92, end * 1_000_000), display].concat()));
    }
    let chapters = element(0x1043A770, &element(0x45B9, &[uint(0x45BC, 1), atoms].concat()));
    // ffmpeg opens a Matroska file only with a track and a cluster: an empty subtitle track,
    // never mapped (only the chapters are taken)
    let tracks = element(0x1654AE6B, &element(0xAE, &[uint(0xD7, 1), uint(0x73C5, 1), uint(0x83, 0x11), element(0x86, b"S_TEXT/UTF8")].concat()));
    let cluster = element(0x1F43B675, &uint(0xE7, 0));
    let segment = element(0x18538067, &[info, tracks, chapters, cluster].concat());
    [header, segment].concat()
}

/// Make `dest` (.m4a, or .m4b with chapters) from a WAV of the reading. Stopped by `cancel`.
pub fn save(ffmpeg: &Path, wav: &Path, title: &str, chapters: &[Chapter], total_ms: u64, dest: &Path, cancel: &AtomicBool) -> Result<()> {
    let part = dest.with_extension("part.m4a");
    let marks = dest.with_extension("chapters.mkv");
    let mut cmd = Command::new(ffmpeg);
    // a book's WAV can pass 4 GB, past what its header can say: read it to its end
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-ignore_length", "1", "-i"]).arg(wav);
    if !chapters.is_empty() {
        std::fs::write(&marks, chapters_mkv(chapters, total_ms))?;
        cmd.arg("-i").arg(&marks).args(["-map", "0:a", "-map_chapters", "1"]);
    }
    // the WAV is Dawwin's own and carries no tags (dropping them would drop the chapters' titles)
    cmd.arg("-metadata").arg(format!("title={title}"));
    // speech: AAC at 64 kb/s is plenty, and small enough for a book
    cmd.args(["-c:a", "aac", "-b:a", "64k", "-movflags", "+faststart", "-f", "ipod"]).arg(&part);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
    let mut child = cmd.spawn()?;
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(anyhow::anyhow!("cancelled"));
        }
        match child.try_wait()? {
            Some(status) if status.success() => break Ok(()),
            Some(_) => {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use std::io::Read;
                    let _ = e.read_to_string(&mut err);
                }
                break Err(anyhow::anyhow!("{}", err.trim()));
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    };
    let _ = std::fs::remove_file(&marks);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        bail!(crate::tr!("ffmpeg could not save the sound: {}", e));
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// A mono 16-bit WAV written as it comes, its sizes put in at the end (the largest a header can
/// say when it is bigger: ffmpeg reads it to its end anyway).
pub struct WavWriter {
    file: std::io::BufWriter<std::fs::File>,
    pub rate: u32,
    pub samples: u64,
}

impl WavWriter {
    pub fn create(path: &Path, rate: u32) -> Result<WavWriter> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&[1, 0, 1, 0]);
        h.extend_from_slice(&rate.to_le_bytes());
        h.extend_from_slice(&(rate * 2).to_le_bytes());
        h.extend_from_slice(&[2, 0, 16, 0]);
        h.extend_from_slice(b"data\0\0\0\0");
        file.write_all(&h)?;
        Ok(WavWriter { file, rate, samples: 0 })
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<()> {
        use std::io::Write;
        let mut b = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        self.file.write_all(&b)?;
        self.samples += samples.len() as u64;
        Ok(())
    }

    pub fn ms(&self) -> u64 {
        self.samples * 1000 / self.rate.max(1) as u64
    }

    pub fn finish(self) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        let data = (self.samples * 2).min(u32::MAX as u64 - 36) as u32;
        let mut f = self.file.into_inner().map_err(|e| anyhow::anyhow!("{e}"))?;
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&(data + 36).to_le_bytes())?;
        f.seek(SeekFrom::Start(40))?;
        f.write_all(&data.to_le_bytes())?;
        Ok(())
    }
}

/// Save what Read aloud has open as sound, sentence by sentence through the voices: the job's
/// work. A book (two headings or more) gets a chapter at each heading. `progress` goes from 0 to 1.
pub fn save_reading(r: &crate::reading::Reader, doc: &crate::reading::Doc, ffmpeg: &Path, work: &Path, dest: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Vec<std::path::PathBuf>> {
    use crate::reading::formats::Kind;
    // a copy of its own: listening to the same text meanwhile does not take its sentences
    let copy = r.for_saving(doc);
    let doc = &*copy;
    let wav = work.join(format!("reading-{}.wav", crate::queue::now_ms()));
    let result = (|| -> Result<Vec<std::path::PathBuf>> {
        let book = doc.sentences.iter().filter(|s| s.kind == Kind::Heading).count() >= 2;
        let mut chapters: Vec<Chapter> = Vec::new();
        let mut out: Option<WavWriter> = None;
        let n = doc.sentences.len();
        for (i, s) in doc.sentences.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let ready = r.sentence(&doc.id, i).map_err(|e| anyhow::anyhow!(e))?;
            let (rate, samples) = crate::reading::voice::read_wav(Path::new(&ready.path))?;
            if out.is_none() {
                out = Some(WavWriter::create(&wav, rate)?);
            }
            let w = out.as_mut().unwrap();
            let samples = if rate == w.rate { samples } else { crate::reading::voice::resample(&samples, rate, w.rate) };
            if book && s.kind == Kind::Heading {
                chapters.push(Chapter { title: s.text.clone(), start_ms: w.ms() });
            }
            w.write(&samples)?;
            // the voice is most of the time: the encoding after it is the last part
            progress(0.95 * (i + 1) as f64 / n as f64);
        }
        let w = out.ok_or_else(|| anyhow::anyhow!(crate::tr!("There is nothing to read in it.")))?;
        let total = w.ms();
        w.finish()?;
        // a book's first chapter starts at its beginning
        if chapters.first().is_some_and(|c| c.start_ms > 0) {
            chapters.insert(0, Chapter { title: doc.title.clone(), start_ms: 0 });
        }
        save(ffmpeg, &wav, &doc.title, &chapters, total, dest, cancel)?;
        progress(1.0);
        Ok(vec![dest.to_path_buf()])
    })();
    let _ = std::fs::remove_file(&wav);
    // its sentences' sound goes with it
    r.forget(&doc.id);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MP4 boxes: (type, body) at the top of `data`.
    fn boxes(data: &[u8]) -> Vec<(String, &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 8 <= data.len() {
            let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
            let kind = String::from_utf8_lossy(&data[i + 4..i + 8]).to_string();
            if size < 8 || i + size > data.len() {
                break;
            }
            out.push((kind, &data[i + 8..i + size]));
            i += size;
        }
        out
    }

    fn find<'a>(data: &'a [u8], path: &[&str]) -> Vec<&'a [u8]> {
        let mut level = vec![data];
        for p in path {
            level = level.into_iter().flat_map(|d| boxes(d).into_iter().filter(|(k, _)| k == p).map(|(_, b)| b)).collect();
        }
        level
    }

    /// The chapters a player finds in an MP4: the text track the chapter reference points at.
    fn chapter_titles(mp4: &[u8]) -> Vec<String> {
        let mut titles = Vec::new();
        for trak in find(mp4, &["moov", "trak"]) {
            // a text track: its samples are the chapter titles, each a 16-bit length and UTF-8
            let is_text = find(trak, &["mdia", "hdlr"]).iter().any(|h| h.len() > 12 && &h[8..12] == b"text");
            if !is_text {
                continue;
            }
            let stbl = find(trak, &["mdia", "minf", "stbl"]);
            let stbl = stbl[0];
            let sizes: Vec<u32> = find(stbl, &["stsz"]).first().map(|b| {
                let n = u32::from_be_bytes(b[8..12].try_into().unwrap()) as usize;
                (0..n).map(|i| u32::from_be_bytes(b[12 + 4 * i..16 + 4 * i].try_into().unwrap())).collect()
            }).unwrap_or_default();
            let offsets: Vec<u64> = find(stbl, &["stco"]).first().map(|b| {
                let n = u32::from_be_bytes(b[4..8].try_into().unwrap()) as usize;
                (0..n).map(|i| u32::from_be_bytes(b[8 + 4 * i..12 + 4 * i].try_into().unwrap()) as u64).collect()
            }).unwrap_or_default();
            // how many samples each chunk holds: (first chunk, samples a chunk)
            let per_chunk: Vec<(u32, u32)> = find(stbl, &["stsc"]).first().map(|b| {
                let n = u32::from_be_bytes(b[4..8].try_into().unwrap()) as usize;
                (0..n).map(|i| (u32::from_be_bytes(b[8 + 12 * i..12 + 12 * i].try_into().unwrap()), u32::from_be_bytes(b[12 + 12 * i..16 + 12 * i].try_into().unwrap()))).collect()
            }).unwrap_or_default();
            let mut k = 0;
            for (ci, &chunk) in offsets.iter().enumerate() {
                let count = per_chunk.iter().rev().find(|(first, _)| *first as usize <= ci + 1).map_or(1, |e| e.1);
                let mut at = chunk as usize;
                for _ in 0..count {
                    let Some(&size) = sizes.get(k) else { break };
                    let sample = &mp4[at..at + size as usize];
                    let len = u16::from_be_bytes([sample[0], sample[1]]) as usize;
                    titles.push(String::from_utf8_lossy(&sample[2..2 + len]).to_string());
                    at += size as usize;
                    k += 1;
                }
            }
        }
        titles
    }

    fn tone_wav(path: &Path, seconds: f32) {
        let rate = 24_000u32;
        let n = (rate as f32 * seconds) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&[1, 0, 1, 0]);
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&[2, 0, 16, 0]);
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn the_chapters_file_is_matroska() {
        let mkv = chapters_mkv(&[Chapter { title: "الفصل الأول".into(), start_ms: 0 }, Chapter { title: "Chapter two".into(), start_ms: 1500 }], 3000);
        assert_eq!(&mkv[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
        let s = String::from_utf8_lossy(&mkv);
        assert!(s.contains("matroska") && s.contains("الفصل الأول") && s.contains("Chapter two"));
    }

    /// With Dawwin's own ffmpeg (the audio build): `DAWWIN_TEST_FFMPEG=<its ffmpeg.exe> cargo test
    /// sound -- --ignored`.
    #[test]
    #[ignore]
    fn a_book_is_saved_with_its_chapters_and_a_reading_without() {
        let ff = std::path::PathBuf::from(std::env::var("DAWWIN_TEST_FFMPEG").expect("DAWWIN_TEST_FFMPEG"));
        let dir = std::env::temp_dir().join(format!("dawwin-sound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("reading.wav");
        tone_wav(&wav, 3.0);
        let stop = AtomicBool::new(false);
        let book = dir.join("كتاب مختلق.m4b");
        let chapters = [Chapter { title: "الفصل الأول".into(), start_ms: 0 }, Chapter { title: "Chapter two, with عربي".into(), start_ms: 1500 }];
        save(&ff, &wav, "كتاب مختلق", &chapters, 3000, &book, &stop).unwrap();
        let mp4 = std::fs::read(&book).unwrap();
        assert_eq!(chapter_titles(&mp4), ["الفصل الأول", "Chapter two, with عربي"]);
        assert!(!dir.join("كتاب مختلق.chapters.mkv").exists() && !dir.join("كتاب مختلق.part.m4a").exists(), "nothing left behind");
        let one = dir.join("selection.m4a");
        save(&ff, &wav, "Dawwin", &[], 3000, &one, &stop).unwrap();
        assert!(chapter_titles(&std::fs::read(&one).unwrap()).is_empty());
        stop.store(true, Ordering::Relaxed);
        assert!(save(&ff, &wav, "x", &[], 3000, &dir.join("stopped.m4a"), &stop).is_err());
        assert!(!dir.join("stopped.m4a").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
