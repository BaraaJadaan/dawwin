//! A two-channel 16 kHz 16-bit WAV that stays readable if the program is killed: the sizes in
//! its header are written again every second, and the bytes are flushed to the disk then.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const RATE: u32 = 16_000;
pub const CHANNELS: u16 = 2;
const HEADER: u64 = 44;

pub fn header(data_bytes: u64) -> [u8; 44] {
    let data = data_bytes.min(u32::MAX as u64 - 36) as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    h[24..28].copy_from_slice(&RATE.to_le_bytes());
    h[28..32].copy_from_slice(&(RATE * CHANNELS as u32 * 2).to_le_bytes());
    h[32..34].copy_from_slice(&(CHANNELS * 2).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

pub struct Writer {
    file: File,
    pub frames: u64,
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

impl Writer {
    pub fn create(path: &Path) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.write_all(&header(0))?;
        file.sync_data()?;
        Ok(Writer { file, frames: 0 })
    }

    /// Left and right have the same length.
    pub fn append(&mut self, left: &[f32], right: &[f32]) -> io::Result<()> {
        let mut bytes = Vec::with_capacity(left.len() * 4);
        for (l, r) in left.iter().zip(right) {
            bytes.extend_from_slice(&to_i16(*l).to_le_bytes());
            bytes.extend_from_slice(&to_i16(*r).to_le_bytes());
        }
        self.file.write_all(&bytes)?;
        self.frames += left.len().min(right.len()) as u64;
        Ok(())
    }

    /// The header says how much is there, and it is on the disk.
    pub fn checkpoint(&mut self) -> io::Result<()> {
        let data = self.frames * CHANNELS as u64 * 2;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(data))?;
        self.file.seek(SeekFrom::End(0))?;
        self.file.sync_data()
    }
}

/// What a WAV's header says, against what the file holds.
pub struct Check {
    pub header_frames: u64,
    pub file_frames: u64,
}

pub fn check(path: &Path) -> io::Result<Check> {
    let mut f = File::open(path)?;
    let mut h = [0u8; 44];
    f.read_exact(&mut h)?;
    if &h[0..4] != b"RIFF" || &h[8..12] != b"WAVE" || &h[36..40] != b"data" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not one of our WAV files"));
    }
    let data = u32::from_le_bytes(h[40..44].try_into().unwrap()) as u64;
    let len = f.metadata()?.len();
    Ok(Check { header_frames: data / 4, file_frames: len.saturating_sub(HEADER) / 4 })
}

/// Make the header say what the file holds (after a crash), cutting a half frame off the end.
pub fn repair(path: &Path) -> io::Result<u64> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    let frames = len.saturating_sub(HEADER) / 4;
    f.set_len(HEADER + frames * 4)?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&header(frames * 4))?;
    f.sync_all()?;
    Ok(frames)
}

/// Reads one of our files a stretch at a time: both channels, from a frame on.
pub struct Reader {
    file: File,
    left: u64,
}

impl Reader {
    pub fn open(path: &Path, from_frame: u64) -> io::Result<Self> {
        let c = check(path)?;
        let frames = c.header_frames.max(c.file_frames).min(c.file_frames);
        let mut file = File::open(path)?;
        let from = from_frame.min(frames);
        file.seek(SeekFrom::Start(HEADER + from * 4))?;
        Ok(Reader { file, left: frames - from })
    }

    /// Up to `n` frames, as (me, them); empty at the end.
    pub fn next(&mut self, n: usize) -> io::Result<(Vec<f32>, Vec<f32>)> {
        let n = (n as u64).min(self.left) as usize;
        let mut bytes = vec![0u8; n * 4];
        self.file.read_exact(&mut bytes)?;
        self.left -= n as u64;
        let s = |b: &[u8]| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0;
        Ok((bytes.chunks_exact(4).map(|f| s(&f[0..2])).collect(), bytes.chunks_exact(4).map(|f| s(&f[2..4])).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dawwin-wav-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_file_cut_off_by_a_crash_is_read_up_to_its_last_whole_frame() {
        let d = dir("crash");
        let p = d.join("r.wav");
        let mut w = Writer::create(&p).unwrap();
        w.append(&[0.5; 16_000], &[-0.25; 16_000]).unwrap();
        w.checkpoint().unwrap();
        // a second more, never announced in the header, and half a frame
        w.append(&[0.5; 16_000], &[-0.25; 16_000]).unwrap();
        drop(w);
        let mut f = OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(&[1, 2]).unwrap();
        drop(f);
        let c = check(&p).unwrap();
        assert_eq!((c.header_frames, c.file_frames), (16_000, 32_000));
        assert_eq!(repair(&p).unwrap(), 32_000);
        let c = check(&p).unwrap();
        assert_eq!((c.header_frames, c.file_frames), (32_000, 32_000));
        let mut r = Reader::open(&p, 16_000).unwrap();
        let (me, them) = r.next(100_000).unwrap();
        assert_eq!(me.len(), 16_000);
        assert!((me[0] - 0.5).abs() < 1e-3 && (them[0] + 0.25).abs() < 1e-3);
        assert!(r.next(10).unwrap().0.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
