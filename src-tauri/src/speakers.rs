//! Who said what. The speaker model (NVIDIA's Streaming Sortformer, in the engine) says who speaks
//! from when to when; here each line of a transcript takes the speaker who talks for most of it,
//! and a line that holds two people's words is split in two where one of its pauses lies at the
//! change. Speakers are per transcript: the same voice in another recording is someone new.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::transcript::{Segment, Speaker, Transcript};

/// Who speaks from when to when, in seconds of the original recording. Speakers are numbered from
/// 1, in the order the model first heard them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: u32,
}

/// The model tells at most this many apart.
pub const MAX_SPEAKERS: usize = 4;
/// A turn shorter than this (a cough, a "yes" over someone) decides nothing.
const MIN_TURN_S: f64 = 0.3;
/// A line is split only when each of two speakers holds this much of it,
const SPLIT_HOLD_S: f64 = 1.5;
/// and one of its pauses lies this close to where the speaker changes.
const SPLIT_NEAR_S: f64 = 0.5;
/// A line in no one's turn takes the speaker of a turn this close to it.
const NEAREST_S: f64 = 1.0;

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// How long each speaker talks inside `[a, b)`, the most first (ties: the lower number).
fn holds(turns: &[Turn], a: f64, b: f64) -> Vec<(u32, f64)> {
    let mut out: Vec<(u32, f64)> = Vec::new();
    for t in turns {
        let o = overlap(t.start, t.end, a, b);
        if o > 0.0 {
            match out.iter_mut().find(|(s, _)| *s == t.speaker) {
                Some(x) => x.1 += o,
                None => out.push((t.speaker, o)),
            }
        }
    }
    out.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
    out
}

/// The speaker of `[a, b)`: who talks for most of it; with nobody, the speaker of the nearest turn
/// when it is close.
fn speaker_of(turns: &[Turn], a: f64, b: f64) -> Option<u32> {
    if let Some((s, _)) = holds(turns, a, b).first() {
        return Some(*s);
    }
    turns
        .iter()
        .map(|t| (t.speaker, (t.start - b).max(a - t.end)))
        .filter(|(_, d)| *d <= NEAREST_S)
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(s, _)| s)
}

/// Where the speaker changes between `x` and `y` inside `[a, b)`: for each change, the stretch it
/// happens in (the gap between the two turns, or where they overlap).
fn changes(turns: &[Turn], a: f64, b: f64, x: u32, y: u32) -> Vec<(f64, f64)> {
    // the turns in the line, one run per speaker until another one speaks
    let mut runs: Vec<(u32, f64, f64)> = Vec::new();
    let mut inside: Vec<&Turn> = turns.iter().filter(|t| overlap(t.start, t.end, a, b) > 0.0 && (t.speaker == x || t.speaker == y)).collect();
    inside.sort_by(|p, q| p.start.total_cmp(&q.start));
    for t in inside {
        match runs.last_mut() {
            Some(r) if r.0 == t.speaker => r.2 = r.2.max(t.end),
            _ => runs.push((t.speaker, t.start, t.end)),
        }
    }
    runs.windows(2).map(|w| (w[0].2.min(w[1].1), w[0].2.max(w[1].1))).filter(|(c0, c1)| *c1 > a && *c0 < b).collect()
}

/// The distance between two stretches of time, nothing when they touch.
fn gap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a0.max(b0) - a1.min(b1)).max(0.0)
}

/// The space in `text` nearest to `share` of its characters, with words on both sides of it.
fn space_near(text: &str, share: f64) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len().max(1) as f64;
    let first = chars.iter().position(|c| !c.is_whitespace())?;
    let last = chars.iter().rposition(|c| !c.is_whitespace())?;
    (first + 1..last)
        .filter(|&i| chars[i].is_whitespace())
        .min_by(|&i, &j| ((i as f64 / len) - share).abs().total_cmp(&((j as f64 / len) - share).abs()))
}

/// Which of the line's pauses to split it at: one within half a second of a change between the
/// two speakers who each hold at least 1.5 s of it, the closest one; and the space in its text
/// that goes with that pause.
fn split_point(s: &Segment, turns: &[Turn]) -> Option<(usize, usize)> {
    let held = holds(turns, s.start, s.end);
    let (x, y) = match held.as_slice() {
        [(x, hx), (y, hy), ..] if *hx >= SPLIT_HOLD_S && *hy >= SPLIT_HOLD_S => (*x, *y),
        _ => return None,
    };
    let mut best: Option<(f64, usize)> = None;
    for (c0, c1) in changes(turns, s.start, s.end, x, y) {
        for (k, p) in s.pauses.iter().enumerate() {
            let d = gap(p[1] as f64, p[2] as f64, c0, c1);
            if d <= SPLIT_NEAR_S && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, k));
            }
        }
    }
    let (_, k) = best?;
    let at = space_near(&s.text, s.pauses[k][0] as f64)?;
    Some((k, at))
}

/// When `share` of the line's speech has gone by: from where the speech is (`voice`), straight
/// lines between its points.
fn voice_at(voice: &[f32], share: f64) -> f32 {
    let n = voice.len() - 1;
    let x = share.clamp(0.0, 1.0) * n as f64;
    let k = (x.floor() as usize).min(n.saturating_sub(1));
    let f = (x - k as f64) as f32;
    voice[k] + (voice[(k + 1).min(n)] - voice[k]) * f
}

/// Line `i` cut in two at its pause `k` and the space at character `at`: the first part ends
/// where the pause starts, the second starts where it ends. What is known about where the speech
/// and the pauses are goes with each part. A translation of the line is cut at the same share of
/// its own text; it stays up to date when it was.
pub fn split_line(t: &mut Transcript, i: usize, k: usize, at: usize) {
    let s = t.segments[i].clone();
    let chars: Vec<char> = s.text.chars().collect();
    let first_text: String = chars[..at].iter().collect::<String>().trim_end().to_string();
    let rest_from = (at + 1..chars.len()).find(|&j| !chars[j].is_whitespace()).unwrap_or(chars.len());
    let second_text: String = chars[rest_from..].iter().collect::<String>().trim_end().to_string();
    let p = s.pauses[k];
    let share = p[0] as f64;
    let (cut0, cut1) = (p[1] as f64, p[2] as f64);
    let mut a = Segment { end: cut0, text: first_text, ..s.clone() };
    let mut b = Segment { start: cut1, text: second_text, ..s.clone() };
    a.pauses = s.pauses.iter().filter(|q| q[0] < p[0]).map(|q| [(q[0] as f64 / share.max(1e-6)) as f32, q[1], q[2]]).collect();
    b.pauses = s.pauses.iter().filter(|q| q[0] > p[0]).map(|q| [((q[0] as f64 - share) / (1.0 - share).max(1e-6)) as f32, q[1], q[2]]).collect();
    if s.voice.len() >= 2 {
        let n = s.voice.len() - 1;
        a.voice = (0..=n).map(|j| voice_at(&s.voice, share * j as f64 / n as f64).min(p[1])).collect();
        b.voice = (0..=n).map(|j| voice_at(&s.voice, share + (1.0 - share) * j as f64 / n as f64).max(p[2])).collect();
    }
    a.words = s.words.iter().filter(|w| (w.0 as usize) < at).copied().collect();
    b.words = s.words.iter().filter(|w| (w.0 as usize) >= rest_from).map(|w| (w.0 - rest_from as u32, w.1)).collect();
    for tr in t.translations.iter_mut() {
        if let Some(line) = tr.lines.get(i).cloned() {
            let current = !line.text.trim().is_empty() && line.from == s.text;
            let (ta, tb) = match space_near(&line.text, share) {
                Some(cut) => {
                    let c: Vec<char> = line.text.chars().collect();
                    (c[..cut].iter().collect::<String>().trim().to_string(), c[cut..].iter().collect::<String>().trim().to_string())
                }
                None => (line.text.trim().to_string(), String::new()),
            };
            let from = |x: &str| if current { x.to_string() } else { String::new() };
            tr.lines[i] = crate::transcript::TranslatedLine { text: ta, from: from(&a.text), edited: line.edited };
            tr.lines.insert(i + 1, crate::transcript::TranslatedLine { text: tb, from: from(&b.text), edited: line.edited });
        }
    }
    t.segments[i] = a;
    t.segments.insert(i + 1, b);
}

/// Give every line of `t` its speaker from the model's `turns`, splitting the lines that hold two
/// people's words where they pause. The speakers are numbered again from 1 in the order they first
/// speak in the transcript. Told apart again, a speaker keeps a name given before only when most
/// of what they say now is what the named one said then (see `names_by_lines`): the model numbers
/// speakers by who is heard first, so a number says nothing about who it was before.
pub fn give_speakers(t: &mut Transcript, turns: &[Turn]) {
    let named: Vec<(f64, f64, String)> = t
        .segments
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .filter_map(|s| {
            let name = t.speakers.iter().find(|x| Some(x.id) == s.speaker).map(|x| x.name.trim().to_string()).filter(|n| !n.is_empty())?;
            Some((s.start, s.end, name))
        })
        .collect();
    t.speakers.clear();
    let turns: Vec<Turn> = turns.iter().copied().filter(|x| x.end - x.start >= MIN_TURN_S).collect();
    let mut i = 0;
    while i < t.segments.len() {
        if t.segments[i].text.trim().is_empty() {
            t.segments[i].speaker = None;
            i += 1;
            continue;
        }
        if let Some((k, at)) = split_point(&t.segments[i], &turns) {
            split_line(t, i, k, at);
            continue; // the first part again: it may hold another change
        }
        let s = &t.segments[i];
        t.segments[i].speaker = speaker_of(&turns, s.start, s.end);
        i += 1;
    }
    number_in_order(t);
    names_by_lines(t, &named);
}

/// Names given before go to the speakers who now say most of the named lines: a name goes to a
/// speaker when over half of that speaker's time now falls in lines that had the name, the closest
/// match first, each name once.
fn names_by_lines(t: &mut Transcript, named: &[(f64, f64, String)]) {
    if named.is_empty() {
        return;
    }
    let mut shared: Vec<(f64, u32, &str)> = Vec::new();
    let mut own: Vec<(u32, f64)> = Vec::new();
    for s in t.segments.iter().filter(|s| !s.text.trim().is_empty()) {
        let Some(id) = s.speaker else { continue };
        match own.iter_mut().find(|(x, _)| *x == id) {
            Some(o) => o.1 += s.end - s.start,
            None => own.push((id, s.end - s.start)),
        }
        for (a, b, name) in named {
            let o = overlap(s.start, s.end, *a, *b);
            if o <= 0.0 {
                continue;
            }
            match shared.iter_mut().find(|(_, x, n)| *x == id && *n == name.as_str()) {
                Some(e) => e.0 += o,
                None => shared.push((o, id, name.as_str())),
            }
        }
    }
    shared.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (mut used_ids, mut used_names): (Vec<u32>, Vec<&str>) = (Vec::new(), Vec::new());
    for (time, id, name) in shared {
        let total = own.iter().find(|(x, _)| *x == id).map_or(0.0, |o| o.1);
        if used_ids.contains(&id) || used_names.contains(&name) || time * 2.0 <= total {
            continue;
        }
        if let Some(sp) = t.speakers.iter_mut().find(|x| x.id == id) {
            sp.name = name.to_string();
        }
        used_ids.push(id);
        used_names.push(name);
    }
}

/// The speakers numbered 1, 2, 3… in the order they first speak, keeping their names.
fn number_in_order(t: &mut Transcript) {
    let mut order: Vec<u32> = Vec::new();
    for s in &t.segments {
        if let Some(x) = s.speaker {
            if !order.contains(&x) {
                order.push(x);
            }
        }
    }
    let names: Vec<Speaker> = std::mem::take(&mut t.speakers);
    t.speakers = order
        .iter()
        .enumerate()
        .map(|(n, old)| Speaker { id: n as u32 + 1, name: names.iter().find(|s| s.id == *old).map(|s| s.name.clone()).unwrap_or_default() })
        .collect();
    for s in t.segments.iter_mut() {
        s.speaker = s.speaker.and_then(|x| order.iter().position(|o| *o == x)).map(|n| n as u32 + 1);
    }
}

/// Give a speaker a name, on every line they say. An empty name is "Speaker 2" again.
pub fn rename(t: &mut Transcript, id: u32, name: &str) {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    match t.speakers.iter_mut().find(|s| s.id == id) {
        Some(s) => s.name = name,
        None => t.speakers.push(Speaker { id, name }),
    }
}

/// One person taken for two: the lines of `from` become `into`'s, and the speakers are numbered
/// again in the order they first speak. `into` keeps its name, or takes `from`'s when it has none.
pub fn merge(t: &mut Transcript, from: u32, into: u32) {
    if from == into {
        return;
    }
    let from_name = t.speakers.iter().find(|s| s.id == from).map(|s| s.name.clone()).unwrap_or_default();
    if let Some(s) = t.speakers.iter_mut().find(|s| s.id == into) {
        if s.name.is_empty() {
            s.name = from_name;
        }
    }
    for s in t.segments.iter_mut() {
        if s.speaker == Some(from) {
            s.speaker = Some(into);
        }
    }
    t.speakers.retain(|s| s.id != from);
    number_in_order(t);
}

/// Give one line to another speaker, or to someone new (`None`: the next free number). The
/// speaker the line gets.
pub fn give_line(t: &mut Transcript, index: usize, speaker: Option<u32>) -> Option<u32> {
    let id = speaker.unwrap_or_else(|| t.speakers.iter().map(|s| s.id).chain(t.segments.iter().filter_map(|s| s.speaker)).max().unwrap_or(0) + 1);
    let line = t.segments.get_mut(index)?;
    line.speaker = Some(id);
    if !t.speakers.iter().any(|s| s.id == id) {
        t.speakers.push(Speaker { id, name: String::new() });
        t.speakers.sort_by_key(|s| s.id);
    }
    Some(id)
}

impl Transcript {
    pub fn has_speakers(&self) -> bool {
        self.segments.iter().any(|s| s.speaker.is_some())
    }

    /// What a speaker is called: the name given, else "Speaker 2".
    pub fn speaker_name(&self, id: u32) -> String {
        match self.speakers.iter().find(|s| s.id == id) {
            Some(s) if !s.name.trim().is_empty() => s.name.trim().to_string(),
            _ => crate::tr!("Speaker {}", id),
        }
    }
}

// ---------------------------------------------------------------- names in subtitles

/// Whether subtitles say who speaks: SRT and burned-in ones put the name in front when the
/// speaker changes (off unless asked for: it takes room on the picture), WebVTT carries it in its
/// voice tag (on: players that know the tag show it their own way).
static NAMES_IN_SRT: AtomicBool = AtomicBool::new(false);
static NAMES_IN_VTT: AtomicBool = AtomicBool::new(true);

pub fn set_subtitle_names(srt: bool, vtt: bool) {
    NAMES_IN_SRT.store(srt, Ordering::Relaxed);
    NAMES_IN_VTT.store(vtt, Ordering::Relaxed);
}

pub fn names_in_srt() -> bool {
    NAMES_IN_SRT.load(Ordering::Relaxed)
}

pub fn names_in_vtt() -> bool {
    NAMES_IN_VTT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{TranslatedLine, Translation};

    fn turn(start: f64, end: f64, speaker: u32) -> Turn {
        Turn { start, end, speaker }
    }

    fn line(start: f64, end: f64, text: &str, pauses: Vec<[f32; 3]>) -> Segment {
        Segment { start, end, text: text.into(), pauses, ..Default::default() }
    }

    fn transcript(segments: Vec<Segment>) -> Transcript {
        Transcript { id: "t".into(), title: "اجتماع".into(), language: "ar".into(), duration_s: 60.0, segments, ..Default::default() }
    }

    fn who(t: &Transcript) -> Vec<Option<u32>> {
        t.segments.iter().map(|s| s.speaker).collect()
    }

    fn texts(t: &Transcript) -> Vec<&str> {
        t.segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn each_line_takes_the_speaker_who_talks_most_in_it() {
        // (lines, turns, the speaker of each line)
        let cases: Vec<(Vec<Segment>, Vec<Turn>, Vec<Option<u32>>)> = vec![
            // one speaker throughout
            (vec![line(0.0, 10.0, "طيب نبدأ", vec![])], vec![turn(0.5, 9.5, 1)], vec![Some(1)]),
            // two speakers, the second one longer, no pause at the change: the longer one
            (vec![line(0.0, 10.0, "طيب نبدأ الاجتماع", vec![])], vec![turn(0.0, 3.0, 1), turn(3.0, 10.0, 2)], vec![Some(1)]),
            // a short turn of another inside is ignored, however placed
            (vec![line(0.0, 4.0, "hello there", vec![])], vec![turn(0.0, 1.8, 1), turn(1.8, 2.0, 2), turn(2.0, 4.0, 1)], vec![Some(1)]),
            // a line in a gap between turns: the nearest one when it is close
            (vec![line(5.2, 5.8, "نعم", vec![])], vec![turn(0.0, 5.0, 1), turn(7.0, 9.0, 2)], vec![Some(1)]),
            // far from every turn: nobody
            (vec![line(20.0, 21.0, "نعم", vec![])], vec![turn(0.0, 5.0, 1)], vec![None]),
            // an empty line has no speaker
            (vec![line(0.0, 2.0, "  ", vec![])], vec![turn(0.0, 2.0, 1)], vec![None]),
        ];
        for (n, (lines, turns, want)) in cases.into_iter().enumerate() {
            let mut t = transcript(lines);
            give_speakers(&mut t, &turns);
            // numbered again in the order they speak, so the second case's 2 is no longer there
            assert_eq!(who(&t), want, "case {n}");
        }
    }

    #[test]
    fn the_speakers_are_numbered_in_the_order_they_first_speak() {
        let mut t = transcript(vec![line(0.0, 5.0, "أول", vec![]), line(5.0, 10.0, "ثاني", vec![]), line(10.0, 15.0, "ثالث", vec![])]);
        give_speakers(&mut t, &[turn(0.0, 5.0, 3), turn(5.0, 10.0, 1), turn(10.0, 15.0, 3)]);
        assert_eq!(who(&t), [Some(1), Some(2), Some(1)]);
        assert_eq!(t.speakers.iter().map(|s| s.id).collect::<Vec<_>>(), [1, 2]);
        assert!(t.speakers.iter().all(|s| s.name.is_empty()));
    }

    #[test]
    fn a_line_with_two_speakers_is_split_where_they_pause() {
        // (turns, pauses of the line, what it becomes)
        let text = "صباح الخير يا جماعة نبدأ؟ نعم تفضل يا سامي";
        let cases: Vec<(Vec<Turn>, Vec<[f32; 3]>, Vec<&str>, Vec<Option<u32>>)> = vec![
            // 4 s each, a pause right at the change: split
            (vec![turn(0.0, 4.0, 1), turn(4.2, 8.0, 2)], vec![[0.55, 3.9, 4.3]], vec!["صباح الخير يا جماعة نبدأ؟", "نعم تفضل يا سامي"], vec![Some(1), Some(2)]),
            // the pause 0.4 s from the change still counts
            (vec![turn(0.0, 4.0, 1), turn(4.0, 8.0, 2)], vec![[0.55, 3.2, 3.6]], vec!["صباح الخير يا جماعة نبدأ؟", "نعم تفضل يا سامي"], vec![Some(1), Some(2)]),
            // the only pause is 1 s away from the change: one line, for the one who talks most
            (vec![turn(0.0, 3.5, 1), turn(3.5, 8.0, 2)], vec![[0.3, 2.0, 2.5]], vec![text], vec![Some(1)]),
            // the second speaker holds only 1.2 s: one line
            (vec![turn(0.0, 6.8, 1), turn(6.8, 8.0, 2)], vec![[0.85, 6.6, 6.9]], vec![text], vec![Some(1)]),
            // no pauses recorded (a transcript from before 1.3): one line
            (vec![turn(0.0, 4.0, 1), turn(4.0, 8.0, 2)], vec![], vec![text], vec![Some(1)]),
        ];
        for (n, (turns, pauses, want_text, want_who)) in cases.into_iter().enumerate() {
            let mut t = transcript(vec![line(0.0, 8.0, text, pauses)]);
            give_speakers(&mut t, &turns);
            assert_eq!(texts(&t), want_text, "case {n}");
            assert_eq!(who(&t), want_who, "case {n}");
        }
    }

    #[test]
    fn the_parts_of_a_split_line_keep_their_times_pauses_and_words() {
        let mut s = line(10.0, 20.0, "one two three, four five six", vec![[0.2, 11.0, 11.3], [0.5, 14.8, 15.4], [0.8, 18.0, 18.3]]);
        s.voice = vec![10.5, 12.0, 13.5, 14.8, 15.4, 16.5, 17.0, 18.0, 18.3, 19.0, 19.5];
        s.words = vec![(0, 10.5), (4, 11.3), (8, 12.0), (15, 15.4), (20, 16.5), (25, 18.3)];
        s.flag = Some("repeat".into());
        let mut t = transcript(vec![s]);
        give_speakers(&mut t, &[turn(10.0, 15.0, 1), turn(15.2, 20.0, 2)]);
        assert_eq!(texts(&t), ["one two three,", "four five six"]);
        let (a, b) = (&t.segments[0], &t.segments[1]);
        assert_eq!((a.start, a.end, b.start, b.end), (10.0, 14.8f32 as f64, 15.4f32 as f64, 20.0));
        assert_eq!(a.pauses.len(), 1);
        assert!((a.pauses[0][0] - 0.4).abs() < 1e-3, "a fifth of the speech is two fifths of the first half: {:?}", a.pauses);
        assert!((b.pauses[0][0] - 0.6).abs() < 1e-3, "{:?}", b.pauses);
        assert_eq!(a.voice.len(), 11);
        assert!(a.voice.iter().all(|v| *v <= 14.8) && b.voice.iter().all(|v| *v >= 15.4), "{:?} {:?}", a.voice, b.voice);
        assert_eq!(a.words, [(0, 10.5), (4, 11.3), (8, 12.0)]);
        assert_eq!(b.words, [(0, 15.4), (5, 16.5), (10, 18.3)], "offsets into its own text");
        assert_eq!(b.flag.as_deref(), Some("repeat"));
    }

    #[test]
    fn a_line_with_three_turns_is_split_twice() {
        let s = line(0.0, 12.0, "سؤال أول هنا جواب طويل هنا سؤال ثاني هنا", vec![[0.33, 3.9, 4.2], [0.66, 7.9, 8.2]]);
        let mut t = transcript(vec![s]);
        give_speakers(&mut t, &[turn(0.0, 4.0, 1), turn(4.1, 8.0, 2), turn(8.1, 12.0, 1)]);
        assert_eq!(texts(&t), ["سؤال أول هنا", "جواب طويل هنا", "سؤال ثاني هنا"]);
        assert_eq!(who(&t), [Some(1), Some(2), Some(1)]);
    }

    #[test]
    fn a_split_takes_the_translation_along() {
        let text = "صباح الخير يا جماعة نبدأ؟ نعم تفضل يا سامي";
        let mut t = transcript(vec![line(0.0, 8.0, text, vec![[0.55, 3.9, 4.3]])]);
        let mut tr = Translation::new("en", "m");
        tr.lines = vec![TranslatedLine { text: "Good morning everyone, shall we start? Yes, go ahead Sami".into(), from: text.into(), edited: false }];
        t.translations.push(tr);
        give_speakers(&mut t, &[turn(0.0, 4.0, 1), turn(4.2, 8.0, 2)]);
        let tr = t.translation("en").unwrap();
        assert_eq!(tr.lines.len(), 2, "one per line still");
        assert!(tr.is_current(0, &t.segments[0]) && tr.is_current(1, &t.segments[1]), "{tr:?}");
        assert_eq!(format!("{} {}", tr.text(0), tr.text(1)), "Good morning everyone, shall we start? Yes, go ahead Sami");
        let en = t.translated("en").unwrap();
        assert_eq!(who(&en), [Some(1), Some(2)], "the translation keeps the same speakers");
    }

    #[test]
    fn told_apart_again_a_name_stays_with_the_lines_it_was_given_to() {
        // before: Sami speaks first and is named, then Huda
        let lines = || vec![line(0.0, 2.0, "مرحبا", vec![]), line(2.0, 10.0, "طيب نبدأ الاجتماع", vec![]), line(10.0, 20.0, "تمام", vec![])];
        let mut t = transcript(lines());
        give_speakers(&mut t, &[turn(0.0, 10.0, 1), turn(10.0, 20.0, 2)]);
        rename(&mut t, 1, "Sami");
        rename(&mut t, 2, "Huda");
        // again: this time a short greeting by someone else is heard first, and is speaker 1
        give_speakers(&mut t, &[turn(0.0, 2.0, 1), turn(2.0, 10.0, 2), turn(10.0, 20.0, 3)]);
        assert_eq!(who(&t), [Some(1), Some(2), Some(3)]);
        assert_eq!(t.speaker_name(2), "Sami", "the one who says Sami's lines now");
        assert_eq!(t.speaker_name(3), "Huda");
        assert!(t.speakers[0].name.is_empty(), "the greeting's voice is nobody named: {:?}", t.speakers);
        // and where the speakers now cut across the named lines, nobody keeps a name
        let mut t = transcript(lines());
        give_speakers(&mut t, &[turn(0.0, 10.0, 1), turn(10.0, 20.0, 2)]);
        rename(&mut t, 1, "Sami");
        give_speakers(&mut t, &[turn(0.0, 20.0, 1)]);
        assert!(t.speakers.iter().all(|x| x.name.is_empty()), "Sami said 10 of the 20 s now heard as one: {:?}", t.speakers);
    }

    #[test]
    fn renaming_merging_and_giving_a_line_to_someone_else() {
        let mut t = transcript((0..4).map(|k| line(k as f64 * 5.0, k as f64 * 5.0 + 5.0, "كلام", vec![])).collect());
        give_speakers(&mut t, &[turn(0.0, 5.0, 1), turn(5.0, 10.0, 2), turn(10.0, 15.0, 3), turn(15.0, 20.0, 2)]);
        assert_eq!(who(&t), [Some(1), Some(2), Some(3), Some(2)]);
        // "Speaker 2", in the interface language
        assert!(t.speaker_name(2).ends_with(" 2") && t.speakers[1].name.is_empty(), "{}", t.speaker_name(2));
        rename(&mut t, 2, "  Sami  ");
        assert_eq!(t.speaker_name(2), "Sami");
        assert_eq!(who(&t), [Some(1), Some(2), Some(3), Some(2)], "renaming changes the name only");
        // the third speaker is Sami too
        merge(&mut t, 3, 2);
        assert_eq!(who(&t), [Some(1), Some(2), Some(2), Some(2)]);
        assert_eq!(t.speakers.len(), 2);
        assert_eq!(t.speaker_name(2), "Sami");
        // the first line was Sami's, and the last one someone new
        assert_eq!(give_line(&mut t, 0, Some(2)), Some(2));
        assert_eq!(give_line(&mut t, 3, None), Some(3));
        assert_eq!(who(&t), [Some(2), Some(2), Some(2), Some(3)]);
        assert!(t.speaker_name(3).ends_with(" 3"));
        // merging into a speaker without a name keeps the name of the one merged
        rename(&mut t, 3, "Huda");
        merge(&mut t, 3, 1);
        assert_eq!(t.speaker_name(1), "Sami", "numbered again in the order they speak: Sami speaks first now");
        assert_eq!(t.speaker_name(2), "Huda");
    }

    #[test]
    fn old_transcripts_load_unchanged_and_new_ones_only_add_what_they_have() {
        let old = r#"{"id":"a","title":"x","source_path":"","created_ms":0,"duration_s":1.0,"language":"ar","segments":[{"start":0.0,"end":1.0,"text":"نعم","edited":false}]}"#;
        let t: Transcript = serde_json::from_str(old).unwrap();
        assert!(!t.has_speakers() && t.speakers.is_empty() && t.detected.is_none());
        let saved = serde_json::to_string(&t).unwrap();
        assert!(!saved.contains("speaker") && !saved.contains("detected"), "{saved}");
        let mut t = t;
        give_speakers(&mut t, &[turn(0.0, 1.0, 1)]);
        let back: Transcript = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back.segments[0].speaker, Some(1));
        assert_eq!(back.speakers, t.speakers);
    }
}
