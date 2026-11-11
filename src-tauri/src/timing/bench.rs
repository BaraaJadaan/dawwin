//! The timing bench: the recordings of `tools/timing/speak.ps1` (Windows voices, so the moment
//! each word starts is known) transcribed as the queue does, then each way of timing the words
//! scored against the truth. `tools/timing/README.md` says how to run it.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use serde_json::Value;

use crate::audio::{self, SR};
use crate::captions::{self, words_of};
use crate::engine::{Audio, Engine, EngineProcess};
use crate::queue::{fix_loops, MAX_CHUNK_S, SEARCH_S};
use crate::setup::Paths;
use crate::transcript::{Segment, Transcript};

use super::{align, fold};

struct Truth {
    language: String,
    /// folded word, when it starts
    words: Vec<(String, f64)>,
}

fn read_truth(path: &Path) -> Truth {
    let raw = std::fs::read_to_string(path).unwrap();
    let v: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}')).unwrap();
    let words = v["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| (fold(w["text"].as_str().unwrap_or("")), w["start"].as_f64().unwrap_or(0.0)))
        .filter(|(w, _)| !w.is_empty())
        .collect();
    Truth { language: v["language"].as_str().unwrap_or("en").to_string(), words }
}

/// A quiet room under the voice (about 55 dB below full scale): the voices stop in digital
/// silence, which no microphone gives.
fn with_noise(clean: &[f32]) -> Vec<f32> {
    let mut x: u32 = 12345;
    clean
        .iter()
        .map(|s| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            s + ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.006
        })
        .collect()
}

/// Where the voice is silent for at least a quarter second: (from, to) in seconds.
fn truth_pauses(clean: &[f32]) -> Vec<(f64, f64)> {
    let frame = SR / 50;
    let silent: Vec<bool> = clean.chunks(frame).map(|c| c.iter().all(|s| s.abs() < 1e-4)).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < silent.len() {
        if !silent[i] {
            i += 1;
            continue;
        }
        let a = i;
        while i < silent.len() && silent[i] {
            i += 1;
        }
        if i - a >= 12 && a > 0 && i < silent.len() {
            out.push((a as f64 * 0.02, i as f64 * 0.02));
        }
    }
    out
}

/// The recording transcribed as the queue does it: pieces of up to 30 s, one at a time.
fn transcribe(w: &mut EngineProcess, audio: &Audio, lang: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    for r in audio::split_points(&audio.samples, MAX_CHUNK_S, SEARCH_S) {
        if audio::is_silent(&audio.samples[r.0..r.1]) {
            continue;
        }
        let got = w.transcribe(audio, &[r], lang).unwrap().remove(0);
        out.extend(fix_loops(w, audio, r, got, lang, 0).unwrap().into_iter().filter(|s| !s.text.trim().is_empty()));
    }
    out
}

fn transcript(language: &str, segments: Vec<Segment>) -> Transcript {
    Transcript {
        id: "bench".into(),
        title: "bench".into(),
        source_path: String::new(),
        created_ms: 0,
        duration_s: segments.last().map_or(0.0, |s| s.end),
        language: language.into(),
        elapsed_s: 0.0,
        device: String::new(),
        model: String::new(),
        partial: false,
        timed: false,
        outputs: vec![],
        segments,
        translations: vec![],
    }
}

/// What Follow words did until 1.3.2 (`estimateTime` in app.js): the share of the characters,
/// over the line's speech profile.
fn follow_words_guess(s: &Segment) -> Vec<f64> {
    words_of(&s.text)
        .iter()
        .map(|w| {
            let frac = w.from.clamp(0.0, 1.0);
            let v = &s.voice;
            if v.len() >= 2 {
                let x = frac * (v.len() - 1) as f64;
                let k = x.floor() as usize;
                if k >= v.len() - 1 {
                    v[v.len() - 1] as f64
                } else {
                    v[k] as f64 + (v[k + 1] - v[k]) as f64 * (x - k as f64)
                }
            } else {
                s.start + frac * (s.end - s.start)
            }
        })
        .collect()
}

fn starts(segments: &[Segment], timed: bool) -> Vec<f64> {
    segments.iter().flat_map(|s| captions::word_times(s, &words_of(&s.text), timed).into_iter().map(|t| t.0)).collect()
}

#[derive(Default)]
struct Score {
    matched: usize,
    words: usize,
    within: usize,
    errors: Vec<f64>,
    early: usize,
    before_pause: usize,
    cues: usize,
    gone_before_last: usize,
    gone_soon_after_last: usize,
    on_early: usize,
    on_late: usize,
}

impl Score {
    fn add(&mut self, o: &Score) {
        self.matched += o.matched;
        self.words += o.words;
        self.within += o.within;
        self.errors.extend(&o.errors);
        self.early += o.early;
        self.before_pause += o.before_pause;
        self.cues += o.cues;
        self.gone_before_last += o.gone_before_last;
        self.gone_soon_after_last += o.gone_soon_after_last;
        self.on_early += o.on_early;
        self.on_late += o.on_late;
    }

    fn row(&self, name: &str, method: &str) -> String {
        let mut e: Vec<f64> = self.errors.iter().map(|x| x.abs()).collect();
        e.sort_by(f64::total_cmp);
        let q = |p: f64| e.get(((e.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(f64::NAN);
        let pc = |n: usize, of: usize| format!("{:.0} %", 100.0 * n as f64 / of.max(1) as f64);
        format!(
            "| {name} | {method} | {}/{} | {} | {:.2} | {:.2} | {} | {} | {} | {} | {} | {} | {} |",
            self.matched,
            self.words,
            pc(self.within, self.matched),
            q(0.5),
            q(0.9),
            pc(self.early, self.matched),
            self.before_pause,
            self.cues,
            self.gone_before_last,
            self.gone_soon_after_last,
            self.on_early,
            self.on_late,
        )
    }
}

/// Every word of the transcript, in order, matched to the truth where it can be.
struct Eval {
    /// the truth's start for each transcript word, where they match
    truth_of: Vec<Option<f64>>,
    pauses: Vec<(f64, f64)>,
}

impl Eval {
    fn new(truth: &Truth, segments: &[Segment], pauses: Vec<(f64, f64)>) -> Eval {
        let ours: Vec<String> = segments.iter().flat_map(|s| words_of(&s.text).into_iter().map(|w| fold(&w.text))).collect();
        let theirs: Vec<String> = truth.words.iter().map(|w| w.0.clone()).collect();
        let m = align(&ours, &theirs);
        Eval { truth_of: m.iter().map(|j| j.map(|j| truth.words[j].1)).collect(), pauses }
    }

    fn score(&self, starts: &[f64], cues: &[captions::Cue]) -> Score {
        let mut s = Score { words: starts.len(), ..Default::default() };
        for (k, &guess) in starts.iter().enumerate() {
            let Some(t) = self.truth_of[k] else { continue };
            s.matched += 1;
            let err = guess - t;
            s.errors.push(err);
            if err.abs() <= 0.5 {
                s.within += 1;
            }
            if err < -0.5 {
                s.early += 1;
            }
            if self.pauses.iter().any(|&(a, b)| b - a >= 0.5 && guess < a && t >= b - 0.05) {
                s.before_pause += 1;
            }
        }
        let mut at = 0;
        for c in cues {
            let n = c.text().split_whitespace().count();
            let (i0, i1) = (at, (at + n).min(self.truth_of.len()));
            at += n;
            let first = (i0..i1).find_map(|i| self.truth_of[i]);
            let last = (i0..i1).rev().find_map(|i| self.truth_of[i]);
            let (Some(first), Some(last)) = (first, last) else { continue };
            s.cues += 1;
            if c.end < last {
                s.gone_before_last += 1;
            }
            if c.end < last + 0.3 {
                s.gone_soon_after_last += 1;
            }
            if c.start < first - 1.0 {
                s.on_early += 1;
            }
            if c.start > first + 0.5 {
                s.on_late += 1;
            }
        }
        s
    }
}

pub fn run() {
    let dir = PathBuf::from(std::env::var("DAWWIN_TIMING_DIR").expect("DAWWIN_TIMING_DIR: the folder speak.ps1 wrote"));
    let model = std::env::var("DAWWIN_TIMING_MODEL").expect("DAWWIN_TIMING_MODEL: a speech model file");
    let exe = std::env::var("DAWWIN_TIMING_ENGINE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("engine-bin").join("dawwin-engine.exe"));
    let only = std::env::var("DAWWIN_TIMING_ONLY").ok();
    let home = std::env::temp_dir().join(format!("dawwin-timing-{}", std::process::id()));
    let paths = Paths { local: home.clone(), roaming: home.clone() };
    std::fs::create_dir_all(paths.cache_dir()).unwrap();
    std::fs::create_dir_all(paths.logs_dir()).unwrap();
    let mut engine = EngineProcess::spawn(&exe, &paths, &model, std::env::var("DAWWIN_TIMING_CPU").is_ok()).unwrap();
    let info = engine.ensure_loaded().unwrap();
    println!("model {} on {} {}", Path::new(&model).file_name().unwrap().to_string_lossy(), info.device, info.gpu.unwrap_or_default());

    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".truth.json").map(String::from))
        .filter(|n| only.as_deref().is_none_or(|o| n.contains(o)))
        .collect();
    names.sort();
    let methods = ["subtitles until 1.3.2", "Follow words until 1.3.2", "timing pass", "guess, held 0.5 s", "guess, held 0.8 s", "guess, held 1.2 s"];
    let mut rows = Vec::new();
    let mut totals: Vec<Score> = methods.iter().map(|_| Score::default()).collect();
    for name in &names {
        let truth = read_truth(&dir.join(format!("{name}.truth.json")));
        let clean = audio::decode(&dir.join(format!("{name}.wav")), None, &AtomicBool::new(false)).unwrap();
        let audio = Audio { id: format!("timing-{name}"), samples: with_noise(&clean) };
        let t0 = Instant::now();
        let guessed = transcribe(&mut engine, &audio, &truth.language);
        let transcribe_s = t0.elapsed().as_secs_f64();
        let mut timed = guessed.clone();
        let t1 = Instant::now();
        super::pass(&mut engine, &audio, &mut timed, &truth.language, &AtomicBool::new(false), |_| {}).unwrap();
        let pass_s = t1.elapsed().as_secs_f64();
        engine.forget(&audio.id);
        let eval = Eval::new(&truth, &guessed, truth_pauses(&clean));
        let held = |hold: f64| {
            let mut rules = captions::Rules::for_language(&truth.language);
            rules.hold_s = hold;
            captions::cues_with(&transcript(&truth.language, guessed.clone()), &rules)
        };
        let cues_guessed = held(0.0);
        let cues_timed = captions::cues(&transcript(&truth.language, timed.clone()));
        let follow: Vec<f64> = guessed.iter().flat_map(follow_words_guess).collect();
        let scores = [
            eval.score(&starts(&guessed, false), &cues_guessed),
            eval.score(&follow, &[]),
            eval.score(&starts(&timed, true), &cues_timed),
            eval.score(&starts(&guessed, false), &held(0.5)),
            eval.score(&starts(&guessed, false), &held(0.8)),
            eval.score(&starts(&guessed, false), &held(1.2)),
        ];
        println!(
            "{name}: {:.0} s, {} lines; transcribed in {transcribe_s:.1} s, timing pass {pass_s:.1} s on {} of them",
            clean.len() as f64 / SR as f64,
            guessed.len(),
            timed.iter().filter(|s| !s.heard.is_empty()).count()
        );
        for (k, s) in scores.iter().enumerate() {
            rows.push(s.row(name, methods[k]));
            totals[k].add(s);
        }
        // the cues around the words the maintainer saw go early, and what was heard there
        for (label, cues) in [("until 1.3.2", &cues_guessed), ("timing pass", &cues_timed)] {
            for c in cues.iter().filter(|c| c.text().contains("subject") || c.text().contains("pegged")) {
                println!("  {label}: {:.2} → {:.2}  {}", c.start, c.end, c.text().replace('\n', " / "));
            }
        }
        let dump: Vec<Value> = timed
            .iter()
            .map(|s| serde_json::json!({ "start": s.start, "end": s.end, "text": s.text, "pauses": s.pauses, "heard": s.heard, "words": s.words }))
            .collect();
        let _ = std::fs::write(dir.join(format!("{name}.timed.json")), serde_json::to_string_pretty(&dump).unwrap());
    }
    engine.shutdown();
    let _ = std::fs::remove_dir_all(&home);
    println!();
    println!("| Recording | Words timed by | Words matched | Within 0.5 s | Median error (s) | 90th percentile (s) | Over 0.5 s early | Before a pause they follow | Subtitles | Gone before their last word starts | Gone within 0.3 s of it | On over 1 s early | On over 0.5 s late |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for r in &rows {
        println!("{r}");
    }
    for (k, t) in totals.iter().enumerate() {
        println!("{}", t.row("**all**", methods[k]));
    }
}
