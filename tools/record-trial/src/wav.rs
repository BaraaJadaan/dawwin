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

/// Both channels of one of our files, or a mono 16-bit file as the left channel.
pub fn read(path: &Path) -> io::Result<(u16, Vec<f32>, Vec<f32>)> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "unsupported WAV");
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad());
    }
    let mut pos = 12;
    let mut channels = 0u16;
    let mut rate = 0u32;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap());
            rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap());
            let bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap());
            if bits != 16 || !(1..=2).contains(&channels) {
                return Err(bad());
            }
        } else if id == b"data" {
            let end = (body + size).min(bytes.len());
            let samples: Vec<f32> = bytes[body..end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect();
            let _ = rate;
            if channels == 1 {
                return Ok((1, samples, Vec::new()));
            }
            let l = samples.iter().step_by(2).copied().collect();
            let r = samples.iter().skip(1).step_by(2).copied().collect();
            return Ok((2, l, r));
        }
        pos = body + size + (size & 1);
    }
    Err(bad())
}
