//! From the devices' own quality to the speech engine's: Windows hands every stream over at
//! 48 kHz, which is kept for listening, and the engine hears it at 16 kHz mono. A low-pass filter
//! (a windowed sinc, flat to 7 kHz), then every third sample; the filter's state carries over from
//! packet to packet, so a stream is filtered as one.

use std::f64::consts::PI;

/// The rate the recording is kept at, for listening.
pub const HIFI: usize = 48_000;
const FACTOR: usize = HIFI / super::SR;
const TAPS: usize = 63;
/// How late the filter makes the sound, in 48 kHz samples.
const DELAY: i64 = (TAPS as i64 - 1) / 2;

pub struct Decimator {
    taps: Vec<f32>,
    /// The last `TAPS - 1` samples of the stream
    hist: Vec<f32>,
    /// Where in the next packet the next output sample is taken
    phase: usize,
    /// Where the next packet is expected, on the clock (100 ns)
    next_ts: Option<u64>,
}

impl Default for Decimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Decimator {
    pub fn new() -> Self {
        let fc = 7_200.0 / HIFI as f64;
        let m = (TAPS - 1) as f64;
        let taps: Vec<f64> = (0..TAPS)
            .map(|n| {
                let x = n as f64 - m / 2.0;
                let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * PI * fc * x).sin() / (PI * x) };
                let w = 0.42 - 0.5 * (2.0 * PI * n as f64 / m).cos() + 0.08 * (4.0 * PI * n as f64 / m).cos();
                sinc * w
            })
            .collect();
        let sum: f64 = taps.iter().sum();
        Decimator { taps: taps.iter().map(|t| (t / sum) as f32).collect(), hist: vec![0.0; TAPS - 1], phase: 0, next_ts: None }
    }

    /// A packet of 48 kHz mono captured at `ts` (100 ns units): the 16 kHz samples it gives, and
    /// the time of the first of them. A packet that does not follow the last one (a gap, a stream
    /// opened again) starts the filter afresh.
    pub fn push(&mut self, ts: u64, input: &[f32]) -> (u64, Vec<f32>) {
        let follows = self.next_ts.is_some_and(|n| (ts as i64 - n as i64).abs() < 200_000);
        if !follows {
            self.hist.iter_mut().for_each(|x| *x = 0.0);
            self.phase = 0;
        }
        self.next_ts = Some(ts + (input.len() as u64 * 10_000_000 / HIFI as u64));
        let mut buf = std::mem::take(&mut self.hist);
        buf.extend_from_slice(input);
        let first = self.phase as i64;
        let mut out = Vec::with_capacity(input.len() / FACTOR + 1);
        let mut i = self.phase;
        while i < input.len() {
            out.push(buf[i..i + TAPS].iter().zip(&self.taps).map(|(a, b)| a * b).sum());
            i += FACTOR;
        }
        self.phase = i - input.len();
        self.hist = buf[buf.len() - (TAPS - 1)..].to_vec();
        let at = ts as i64 + (first - DELAY) * 10_000_000 / HIFI as i64;
        (at.max(0) as u64, out)
    }
}

/// Interleaved frames of `channels` channels, as one mono stream.
pub fn mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * hz * i as f64 / HIFI as f64).sin() as f32 * 0.5).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn speech_passes_and_what_16_khz_cannot_hold_does_not() {
        let mut d = Decimator::new();
        let (_, low) = d.push(1_000, &tone(1_000.0, HIFI));
        assert_eq!(low.len(), HIFI / 3);
        assert!((rms(&low[200..]) - 0.3536).abs() < 0.01, "1 kHz kept: {}", rms(&low[200..]));
        let mut d = Decimator::new();
        let (_, high) = d.push(1_000, &tone(12_000.0, HIFI));
        assert!(rms(&high[200..]) < 0.005, "12 kHz would fold back into speech: {}", rms(&high[200..]));
    }

    #[test]
    fn packets_in_a_row_are_one_stream_and_keep_their_time() {
        let x = tone(440.0, 4_800);
        let mut whole = Decimator::new();
        let (_, a) = whole.push(10_000_000, &x);
        let mut parts = Decimator::new();
        let mut b = Vec::new();
        let mut ts = 10_000_000u64;
        let mut first = None;
        for chunk in x.chunks(481) {
            let (t, out) = parts.push(ts, chunk);
            first.get_or_insert(t);
            b.extend(out);
            ts += chunk.len() as u64 * 10_000_000 / HIFI as u64;
        }
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(p, q)| (p - q).abs() < 1e-5), "split anywhere, the same samples");
        // the filter's delay is taken off the time: under a millisecond either way
        assert!((first.unwrap() as i64 - 10_000_000).abs() < 10_000);
    }

    #[test]
    fn stereo_becomes_one_track() {
        assert_eq!(mono(&[0.2, 0.4, -1.0, 1.0], 2), vec![0.3, 0.0]);
        assert_eq!(mono(&[0.5, 0.25], 1), vec![0.5, 0.25]);
    }
}
