//! Pieces for the speech engine, cut as the sound arrives at exactly the places
//! `audio::split_points` cuts a whole recording: at most `max_s` long, at the quietest moment of
//! each piece's last `search_s` (20 ms frames, smoothed over 200 ms). A cut is made only once the
//! sound that decides it has arrived, so the pieces are the same as the offline ones.

use super::SR;

const FRAME: usize = SR / 50;
const K: usize = 10;
const BACK: usize = K / 2;

pub struct Cutter {
    max_len: usize,
    search: usize,
    /// Timeline index of `buf[0]`, a whole number of frames
    base: usize,
    buf: Vec<f32>,
    /// Where the next piece starts
    start: usize,
}

fn frame_rms(s: &[f32]) -> f32 {
    (s.iter().map(|x| x * x).sum::<f32>() / FRAME as f32 + 1e-12).sqrt()
}

impl Cutter {
    /// A cutter for a track whose sound starts at timeline index `from` (0, or where a track is
    /// taken up again after the recording).
    pub fn new(max_s: f32, search_s: f32, from: usize) -> Self {
        let base = from / FRAME * FRAME;
        Cutter {
            max_len: (max_s * SR as f32) as usize,
            search: (search_s * SR as f32) as usize,
            base,
            buf: vec![0.0; from - base],
            start: from,
        }
    }

    fn end(&self) -> usize {
        self.base + self.buf.len()
    }

    /// Where the piece that starts at `self.start` ends, with `n` frames in the whole track
    /// (`usize::MAX` while more is coming).
    fn cut(&self, n: usize) -> usize {
        let lo_f = (self.start + self.max_len - self.search) / FRAME;
        let hi_f = ((self.start + self.max_len) / FRAME).min(n);
        let rms = |f: usize| frame_rms(&self.buf[f * FRAME - self.base..(f + 1) * FRAME - self.base]);
        let best = if lo_f >= hi_f {
            lo_f
        } else {
            let first = lo_f.saturating_sub(BACK);
            let last = (hi_f + K - BACK).min(n);
            let frames: Vec<f32> = (first..last).map(rms).collect();
            let smoothed = |f: usize| {
                let lo = f.saturating_sub(BACK);
                let hi = (f + K - BACK).min(n);
                frames[lo - first..hi - first].iter().sum::<f32>() / K as f32
            };
            let mut best = lo_f;
            let mut best_v = smoothed(lo_f);
            for f in lo_f..hi_f {
                let v = smoothed(f);
                if v < best_v {
                    best = f;
                    best_v = v;
                }
            }
            best
        };
        (best * FRAME + FRAME / 2).max(self.start + FRAME)
    }

    fn take(&mut self, cut: usize) -> (usize, Vec<f32>) {
        let piece = self.buf[self.start - self.base..cut - self.base].to_vec();
        let at = self.start;
        self.start = cut;
        let keep = cut / FRAME * FRAME;
        self.buf.drain(..keep - self.base);
        self.base = keep;
        (at, piece)
    }

    /// More sound; the pieces it completes, as (timeline index, samples).
    pub fn push(&mut self, samples: &[f32]) -> Vec<(usize, Vec<f32>)> {
        self.buf.extend_from_slice(samples);
        let mut out = Vec::new();
        loop {
            // the last frame the smoothing of the search window reads
            let need = ((self.start + self.max_len) / FRAME + K - BACK) * FRAME;
            if self.end() < need {
                break;
            }
            let cut = self.cut(usize::MAX);
            out.push(self.take(cut));
        }
        out
    }

    /// The track is over: the pieces that are left.
    pub fn finish(mut self) -> Vec<(usize, Vec<f32>)> {
        let len = self.end();
        let n = len / FRAME;
        let mut out = Vec::new();
        while len - self.start > self.max_len {
            let cut = self.cut(n).min(len);
            out.push(self.take(cut));
        }
        if self.start < len {
            out.push(self.take(len));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speech-like made-up sound: bursts of noise with pauses of varying length.
    fn speechy(seconds: usize, seed: u64) -> Vec<f32> {
        let mut x = seed;
        let mut rnd = move || {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (x >> 33) as f32 / (1u64 << 31) as f32
        };
        let mut out = Vec::with_capacity(seconds * SR);
        while out.len() < seconds * SR {
            let talk = (0.3 + rnd() * 3.0) * SR as f32;
            let gain = 0.05 + rnd() * 0.4;
            for _ in 0..talk as usize {
                out.push((rnd() - 0.5) * gain);
            }
            let pause = rnd() * 0.8 * SR as f32;
            for _ in 0..pause as usize {
                out.push((rnd() - 0.5) * 0.002);
            }
        }
        out.truncate(seconds * SR);
        out
    }

    #[test]
    fn the_pieces_are_the_ones_a_whole_recording_is_cut_into() {
        let _alone = super::super::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        for (seconds, seed) in [(7, 1), (25, 2), (26, 3), (100, 4), (191, 5)] {
            let a = speechy(seconds, seed);
            let offline = crate::audio::split_points(&a, 25.0, 8.0);
            let mut c = Cutter::new(25.0, 8.0, 0);
            let mut got = Vec::new();
            let mut i = 0;
            let mut step = 1;
            while i < a.len() {
                // packets of every size, as they come from a device
                step = (step * 7919 + 13) % 4000 + 1;
                let j = (i + step).min(a.len());
                got.extend(c.push(&a[i..j]));
                i = j;
            }
            got.extend(c.finish());
            let ranges: Vec<(usize, usize)> = got.iter().map(|(at, s)| (*at, at + s.len())).collect();
            assert_eq!(ranges, offline, "{seconds} s");
            for (at, s) in &got {
                assert_eq!(&a[*at..at + s.len()], &s[..]);
            }
        }
    }

    #[test]
    fn pieces_come_while_the_sound_does_and_at_most_25_seconds_long() {
        let a = speechy(80, 9);
        let mut c = Cutter::new(25.0, 8.0, 0);
        let first = c.push(&a[..26 * SR]);
        assert_eq!(first.len(), 1, "a piece as soon as its cut is decided");
        assert!(first[0].1.len() <= 25 * SR && first[0].1.len() >= 17 * SR);
    }

    #[test]
    fn a_track_taken_up_later_is_cut_where_the_whole_one_would_be() {
        let a = speechy(120, 11);
        let offline = crate::audio::split_points(&a, 25.0, 8.0);
        let (from, _) = offline[2];
        let mut c = Cutter::new(25.0, 8.0, from);
        let mut got = c.push(&a[from..]);
        got.extend(c.finish());
        let ranges: Vec<(usize, usize)> = got.iter().map(|(at, s)| (*at, at + s.len())).collect();
        assert_eq!(ranges, offline[2..]);
    }
}
