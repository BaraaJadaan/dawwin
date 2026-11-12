//! The others' voices heard twice: through the PC ("them") and again from the speakers by the
//! microphone ("me"). Measured in `tools/record-trial`: the model writes such an echo out in full
//! even 20 dB under the loopback, and mixes it into the lines of whoever speaks over it. So the
//! microphone goes through echo cancellation (WebRTC's AEC3, ported to Rust as `sonora`) with "them"
//! as the sound the speakers played, and a "me" line that still says what an overlapping "them"
//! line says is dropped. And pieces with no speech in them never reach the model, which invents a
//! line on near-silence.

use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};

use super::SR;

/// 10 ms at 16 kHz: what the canceller takes at a time.
pub const BLOCK: usize = SR / 100;

pub struct Canceller {
    apm: AudioProcessing,
    out: Vec<f32>,
    rendered: Vec<f32>,
}

impl Canceller {
    pub fn new() -> Self {
        let sc = StreamConfig::new(SR as u32, 1);
        let apm = AudioProcessing::builder()
            .config(Config { echo_canceller: Some(EchoCanceller::default()), ..Default::default() })
            .capture_config(sc)
            .render_config(sc)
            .build();
        Canceller { apm, out: vec![0.0; BLOCK], rendered: vec![0.0; BLOCK] }
    }

    /// `me` without what it heard of `them`, in place: both a whole number of blocks, and the
    /// same moments of the recording.
    pub fn process(&mut self, me: &mut [f32], them: &[f32]) {
        for (m, t) in me.chunks_exact_mut(BLOCK).zip(them.chunks_exact(BLOCK)) {
            if self.apm.process_render_f32(&[t], &mut [&mut self.rendered]).is_err() {
                continue;
            }
            if self.apm.process_capture_f32(&[m], &mut [&mut self.out]).is_ok() {
                m.copy_from_slice(&self.out);
            }
        }
    }
}

impl Default for Canceller {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a piece holds speech for the model: its loud part (the 90th percentile of 20 ms
/// frames) at least 15 dB over the quietest the track has been, and over -60 dBFS. The track's
/// floor is learnt as it goes; digital silence does not count, or a muted moment would make the
/// hum of a room look like speech.
pub struct Gate {
    floor: f32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate { floor: f32::INFINITY }
    }
}

impl Gate {
    pub fn admit(&mut self, samples: &[f32]) -> bool {
        let frame = SR / 50;
        let mut db: Vec<f32> = samples
            .chunks_exact(frame)
            .map(|f| 10.0 * (f.iter().map(|x| x * x).sum::<f32>() / frame as f32 + 1e-12).log10())
            .collect();
        if db.len() < 10 {
            return false;
        }
        db.sort_by(|a, b| a.total_cmp(b));
        let loud = db[db.len() * 9 / 10];
        let heard: Vec<f32> = db.iter().copied().filter(|d| *d > -100.0).collect();
        if let Some(q) = heard.get(heard.len() / 20) {
            self.floor = self.floor.min(*q);
        }
        let floor = if self.floor.is_finite() { self.floor } else { -120.0 };
        loud > -60.0 && loud > floor + 15.0
    }
}

/// Words as they are compared: no diacritics or punctuation, one shape for each Arabic letter
/// that is written in several, lower case.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() && !('\u{064B}'..='\u{065F}').contains(c) && *c != '\u{0670}' && *c != '\u{0640}')
                .map(|c| match c {
                    'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
                    'ى' => 'ي',
                    'ة' => 'ه',
                    c => c,
                })
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// A "me" line that says nearly what the "them" lines at the same time say: 70 % of its words
/// are in them (a line of one word: that word).
pub fn doubled(me: &str, them: &[&str]) -> bool {
    let mine = words(me);
    if mine.is_empty() {
        return false;
    }
    let theirs: std::collections::HashSet<String> = them.iter().flat_map(|t| words(t)).collect();
    let shared = mine.iter().filter(|w| theirs.contains(*w)).count();
    if mine.len() == 1 {
        shared == 1
    } else {
        shared as f32 >= 0.7 * mine.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_heard_from_the_speakers_is_doubled_and_ones_own_words_are_not() {
        let them = ["بما كانوا يكذبون وإذا قيل لهم لا تفسدوا في الأرض قالوا إنما نحن مصلحون"];
        assert!(doubled("بِما كانوا يكذبون، وإذا قيل لهم: لا تُفسدوا في الارض قالوا انما نحن مصلحون", &them));
        assert!(!doubled("في أكنة مما تدعونا إليه وفي آذاننا وقر", &them));
        // own speech over the echo: kept, whatever of the echo is in it
        assert!(!doubled("بما كانوا يكذبون في أكنة مما تدعونا إليه وفي آذاننا وقر", &them));
        assert!(doubled("The meeting starts at TEN.", &["the meeting starts at ten"]));
        assert!(!doubled("", &them));
        assert!(!doubled("نعم", &["لا"]));
    }

    #[test]
    fn silence_and_the_hum_of_a_room_are_not_speech() {
        let mut g = Gate::default();
        assert!(!g.admit(&vec![0.0; SR * 5]), "digital silence");
        let mut x = 7u32;
        let mut noise = |amp: f32| {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * amp
        };
        let hum: Vec<f32> = (0..SR * 5).map(|_| noise(0.003)).collect();
        assert!(!g.admit(&hum), "a quiet room");
        let mut talk = hum.clone();
        for (i, s) in talk.iter_mut().enumerate() {
            if (i / (SR / 2)) % 2 == 0 {
                *s += noise(0.3);
            }
        }
        assert!(g.admit(&talk), "someone speaking in it");
        assert!(!g.admit(&hum), "the room again");
    }

    #[test]
    fn the_canceller_takes_out_what_the_speakers_played() {
        let _alone = super::super::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let mut x = 3u32;
        let mut rnd = move || {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        // ten seconds of "them" (bursts of noise), heard 56 ms later 20 dB down
        let them: Vec<f32> = (0..SR * 10).map(|i| if (i / 4000) % 3 != 2 { rnd() * 0.5 } else { 0.0 }).collect();
        let delay = SR * 56 / 1000;
        let mut me: Vec<f32> = (0..them.len()).map(|i| if i >= delay { them[i - delay] * 0.1 } else { 0.0 }).collect();
        let before: f32 = me[SR * 8..].iter().map(|s| s * s).sum();
        Canceller::new().process(&mut me, &them);
        let after: f32 = me[SR * 8..].iter().map(|s| s * s).sum();
        assert!(10.0 * (after / before).log10() < -20.0, "{} dB", 10.0 * (after / before).log10());
    }
}
