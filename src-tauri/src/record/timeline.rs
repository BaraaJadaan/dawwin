//! Two tracks on one clock. Every packet is placed by its capture timestamp; holes become
//! silence, and a packet that overlaps what is already placed loses the overlapping part.
//! Small differences (the jitter of the timestamps) are absorbed, so a steady stream is
//! written as it came.

use std::collections::VecDeque;

pub const RATE: u64 = super::SR as u64;

#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub packets: u64,
    pub frames: u64,
    /// Holes filled with silence when a packet came later than the track's end.
    pub gaps: u64,
    pub gap_frames: u64,
    /// Packets that started before the track's end by more than the tolerance.
    pub overlaps: u64,
    pub dropped_frames: u64,
    /// Silence written because nothing had arrived by the time the file was written.
    pub filled_frames: u64,
    /// The largest difference absorbed without a correction, in frames.
    pub max_jitter: i64,
}

pub struct Track {
    /// Timeline index of `buf[0]`.
    start: u64,
    buf: VecDeque<f32>,
    tolerance: i64,
    pub stats: Stats,
}

impl Track {
    pub fn new(tolerance_frames: i64) -> Self {
        Track { start: 0, buf: VecDeque::new(), tolerance: tolerance_frames, stats: Stats::default() }
    }

    /// Where the next sample goes if nothing is corrected.
    pub fn end(&self) -> u64 {
        self.start + self.buf.len() as u64
    }

    /// Place `samples` whose first one was captured at timeline index `at`.
    pub fn place(&mut self, at: i64, samples: &[f32]) {
        self.stats.packets += 1;
        self.stats.frames += samples.len() as u64;
        let diff = at - self.end() as i64;
        if diff > self.tolerance {
            self.stats.gaps += 1;
            self.stats.gap_frames += diff as u64;
            self.buf.extend(std::iter::repeat(0.0).take(diff as usize));
            self.buf.extend(samples);
        } else if diff < -self.tolerance {
            let skip = ((-diff) as usize).min(samples.len());
            self.stats.overlaps += 1;
            self.stats.dropped_frames += skip as u64;
            self.buf.extend(&samples[skip..]);
        } else {
            self.stats.max_jitter = self.stats.max_jitter.max(diff.abs());
            self.buf.extend(samples);
        }
    }

    /// Hand over everything before timeline index `upto`, with silence where nothing came.
    pub fn take(&mut self, upto: u64) -> Vec<f32> {
        if upto <= self.start {
            return Vec::new();
        }
        let n = (upto - self.start) as usize;
        let have = n.min(self.buf.len());
        let mut out: Vec<f32> = self.buf.drain(..have).collect();
        if out.len() < n {
            self.stats.filled_frames += (n - out.len()) as u64;
            out.resize(n, 0.0);
        }
        self.start = upto;
        out
    }
}

/// Timeline index of a timestamp in 100 ns units, `t0` being the start of the recording.
pub fn index_of(ts: u64, t0: u64) -> i64 {
    let d = ts as i128 - t0 as i128;
    ((d * RATE as i128 + 5_000_000) / 10_000_000) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_packets_are_written_as_they_came() {
        let mut t = Track::new(160);
        for k in 0..10 {
            t.place(k * 160 + (k % 3) - 1, &[1.0; 160]);
        }
        let out = t.take(1600);
        assert_eq!(out.len(), 1600);
        assert!(out[1..].iter().all(|&s| s == 1.0));
        assert_eq!(t.stats.gaps + t.stats.overlaps, 0);
    }

    #[test]
    fn a_hole_becomes_silence_and_later_packets_keep_their_time() {
        let mut t = Track::new(160);
        t.place(0, &[1.0; 160]);
        t.place(16_000, &[2.0; 160]);
        let out = t.take(16_160);
        assert_eq!(out[159], 1.0);
        assert_eq!(out[160], 0.0);
        assert_eq!(out[15_999], 0.0);
        assert_eq!(out[16_000], 2.0);
        assert_eq!(t.stats.gaps, 1);
    }

    #[test]
    fn an_overlapping_packet_loses_only_the_overlap() {
        let mut t = Track::new(10);
        t.place(0, &[1.0; 100]);
        t.place(50, &[2.0; 100]);
        let out = t.take(150);
        assert_eq!(out[99], 1.0);
        assert_eq!(out[100], 2.0);
        assert_eq!(t.stats.dropped_frames, 50);
    }

    #[test]
    fn nothing_arrived_means_silence_and_a_late_packet_is_cut() {
        let mut t = Track::new(10);
        let out = t.take(1000);
        assert_eq!(out, vec![0.0; 1000]);
        t.place(900, &[1.0; 200]);
        let out = t.take(1200);
        assert_eq!(out.len(), 200);
        assert_eq!(out[99], 1.0);
        assert_eq!(out[100], 0.0);
        assert_eq!(t.stats.dropped_frames, 100);
    }
}
