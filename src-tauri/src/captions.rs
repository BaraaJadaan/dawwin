//! Subtitles timed and broken the way professional ones are, after the rules Netflix and the BBC
//! give their subtitlers: at most two lines of 42 characters; on screen for at least 5/6 of a
//! second and at most 7 seconds, and slow enough to read; on when the speaker starts and off when
//! they stop, with a gap of two frames between one and the next; and broken where a sentence or a
//! clause ends or where the speaker pauses, not in the middle of a phrase.
//!
//! The speech model times whole lines only (up to 30 seconds). Inside a line, where the speaker
//! pauses (`Segment::pauses`) and where the speech is (`Segment::voice`) place each word in time,
//! by how far into the text it is. A translation is placed the same way: a pause after 40 % of the
//! speech falls after about 40 % of the translation, at the nearest space between two words.

use crate::transcript::{Segment, Transcript, Translation};

pub struct Rules {
    /// Characters on a line, and lines on the screen
    pub max_line: usize,
    pub max_lines: usize,
    /// Seconds on the screen
    pub min_s: f64,
    pub max_s: f64,
    /// Reading speed, in characters a second
    pub cps: f64,
    /// Between one subtitle and the next: two frames at 24 per second
    pub gap_s: f64,
    /// A gap shorter than this is closed: the next one follows right after
    pub chain_s: f64,
    /// How long one stays up after the speaker stops, when nothing follows at once
    pub linger_s: f64,
}

impl Rules {
    /// Netflix's numbers: 20 characters a second for adults in English and Arabic (its English and
    /// Arabic guides), 17 for the other languages; off half a second after the speech ends.
    pub fn for_language(language: &str) -> Rules {
        Rules {
            max_line: 42,
            max_lines: 2,
            min_s: 5.0 / 6.0,
            max_s: 7.0,
            cps: if matches!(language, "en" | "ar") { 20.0 } else { 17.0 },
            gap_s: 2.0 / 24.0,
            chain_s: 0.5,
            linger_s: 0.5,
        }
    }

    fn max_chars(&self) -> usize {
        self.max_line * self.max_lines
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub lines: Vec<String>,
}

impl Cue {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// Words that start a clause: a good place to begin a new subtitle or line.
const CLAUSE_STARTS: &[&str] = &[
    "and", "but", "or", "so", "because", "which", "who", "that", "when", "where", "while", "if", "then", "although", "though", "since", "until",
    "و", "ثم", "لكن", "لكنه", "بس", "لأن", "لان", "عشان", "علشان", "اللي", "الذي", "التي", "يعني", "أو", "او", "إذا", "اذا", "لما", "حتى", "بعدين", "فـ",
];
/// Words that lead into the next one: a poor place to end a subtitle or line.
const LEADS_IN: &[&str] = &[
    "the", "a", "an", "of", "to", "in", "on", "at", "for", "with", "from", "by", "my", "your", "our", "their", "his", "her", "its", "this", "these", "those", "is", "are", "was", "will", "would", "can", "not",
    "في", "على", "من", "عن", "إلى", "الى", "مع", "هذا", "هذه", "هاد", "هاي", "هذي", "ال", "يا", "كل", "بين", "عند", "قبل", "بعد", "لا", "ما", "مش", "مو",
];

fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Whether the word ends a sentence, or a clause.
fn ends_sentence(word: &str) -> bool {
    let w = word.trim_end_matches(['"', '\'', '”', '’', '»', ')', ']']);
    w.ends_with(['.', '!', '?', '؟', '…'])
}

fn ends_clause(word: &str) -> bool {
    let w = word.trim_end_matches(['"', '\'', '”', '’', '»', ')', ']']);
    w.ends_with([',', '،', ';', '؛', ':', '—', '–'])
}

struct Word {
    text: String,
    /// Where it starts and ends in the line's text, as a share of its characters
    from: f64,
    to: f64,
}

fn words_of(text: &str) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len().max(1) as f64;
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let a = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        out.push(Word { text: chars[a..i].iter().collect(), from: a as f64 / len, to: i as f64 / len });
    }
    out
}

/// When a share of a line's text is said: straight lines between known points, which can jump
/// (over a pause: the word before ends at its start, the word after begins at its end).
struct Clock {
    points: Vec<(f64, f64)>,
}

impl Clock {
    /// From the line's pauses, each moved to the nearest space between two of `words`; without
    /// them, from where its speech is; without that, from its start and end.
    fn new(s: &Segment, words: &[Word]) -> Clock {
        let (speech_from, speech_to) = match (s.voice.first(), s.voice.last()) {
            (Some(a), Some(b)) if s.voice.len() >= 2 && b > a => (*a as f64, *b as f64),
            _ => (s.start, s.end),
        };
        let mut points = vec![(0.0, speech_from)];
        if !s.pauses.is_empty() {
            let spaces: Vec<f64> = words.windows(2).map(|w| (w[0].to + w[1].from) / 2.0).collect();
            let mut last_share = 0.0;
            for p in &s.pauses {
                let (share, from, to) = (p[0] as f64, p[1] as f64, p[2] as f64);
                let Some(&at) = spaces.iter().min_by(|a, b| (*a - share).abs().total_cmp(&(*b - share).abs())) else { break };
                // two pauses at the same space: the first one only
                if at <= last_share || from <= points.last().unwrap().1 || from >= speech_to {
                    continue;
                }
                points.push((at, from));
                points.push((at, to.min(speech_to)));
                last_share = at;
            }
        } else if s.voice.len() >= 3 {
            let n = (s.voice.len() - 1) as f64;
            for (k, t) in s.voice.iter().enumerate().skip(1).take(s.voice.len() - 2) {
                points.push((k as f64 / n, *t as f64));
            }
        }
        points.push((1.0, speech_to.max(points.last().unwrap().1)));
        Clock { points }
    }

    /// The time at `share`; at a jump, `after` takes its far side.
    fn at(&self, share: f64, after: bool) -> f64 {
        let p = &self.points;
        let share = share.clamp(0.0, 1.0);
        for k in 0..p.len() - 1 {
            let ((s0, t0), (s1, t1)) = (p[k], p[k + 1]);
            if (s1 - s0).abs() < 1e-12 {
                if (share - s0).abs() < 1e-12 {
                    return if after { t1 } else { t0 };
                }
                continue;
            }
            if share >= s0 && share <= s1 {
                return t0 + (t1 - t0) * (share - s0) / (s1 - s0);
            }
        }
        p.last().unwrap().1
    }
}

/// Where each word of a line is said: (start, end) in seconds. Word times from the model, when an
/// older transcript has them, are better than any estimate.
fn word_times(s: &Segment, words: &[Word], from_model: bool) -> Vec<(f64, f64)> {
    let clock = Clock::new(s, words);
    let mut times: Vec<(f64, f64)> = words.iter().map(|w| (clock.at(w.from, true), clock.at(w.to, false))).collect();
    if from_model && s.words.len() == words.len() {
        for (k, (_, t)) in s.words.iter().enumerate() {
            times[k].0 = *t as f64;
        }
        for k in 0..times.len() {
            let next = times.get(k + 1).map_or(clock.at(1.0, false), |n| n.0);
            times[k].1 = times[k].1.min(next).max(times[k].0);
        }
    }
    times
}

/// What it costs to end a subtitle (or a line) after word `k`, before word `k + 1`: nothing at
/// the end of a sentence or at a long pause, a little at a comma or before a clause, a lot inside
/// a phrase.
fn break_cost(words: &[Word], times: &[(f64, f64)], k: usize) -> f64 {
    let (here, next) = (&words[k], &words[k + 1]);
    let pause = times[k + 1].0 - times[k].1;
    let mut cost: f64 = if ends_sentence(&here.text) {
        0.0
    } else if ends_clause(&here.text) {
        1.5
    } else if CLAUSE_STARTS.contains(&bare(&next.text).as_str()) {
        3.0
    } else {
        6.0
    };
    if pause >= 0.5 {
        cost = cost.min(0.0);
    } else if pause >= 0.25 {
        cost = cost.min(1.0);
    }
    if LEADS_IN.contains(&bare(&here.text).as_str()) {
        cost += 3.0;
    }
    cost
}

fn joined(words: &[Word]) -> String {
    words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
}

/// A subtitle's text in at most `max_lines` lines of at most `max_line` characters, broken at the
/// best place: after punctuation or before a clause, with the lines about as long as each other,
/// the lower one rather the longer. None when it does not fit.
fn lines_of(words: &[Word], times: &[(f64, f64)], rules: &Rules) -> Option<Vec<String>> {
    let text = joined(words);
    let len = text.chars().count();
    if len <= rules.max_line || words.len() == 1 {
        return (len <= rules.max_line || words.len() == 1).then(|| vec![text]);
    }
    let mut best: Option<(f64, usize)> = None;
    for k in 0..words.len() - 1 {
        let top = joined(&words[..=k]).chars().count();
        let bottom = len - top - 1;
        if top > rules.max_line || bottom > rules.max_line {
            continue;
        }
        let shape = (top as f64 - bottom as f64).abs() / 8.0 + if top > bottom { 0.5 } else { 0.0 };
        let cost = break_cost(words, times, k) * 0.6 + shape;
        if best.is_none_or(|(c, _)| cost < c) {
            best = Some((cost, k));
        }
    }
    let (_, k) = best?;
    Some(vec![joined(&words[..=k]), joined(&words[k + 1..])])
}

/// The subtitles of one line of a transcript, with the share of its text each one covers.
fn line_cues(s: &Segment, text: &str, from_model: bool, rules: &Rules) -> Vec<(Cue, f64, f64)> {
    let words = words_of(text);
    if words.is_empty() {
        return Vec::new();
    }
    let times = word_times(s, &words, from_model);
    let m = words.len();
    // best[j]: the cheapest way to cut the first j words into subtitles, and where the last one began
    let mut best: Vec<Option<(f64, usize)>> = vec![None; m + 1];
    best[0] = Some((0.0, 0));
    for j in 1..=m {
        for i in (0..j).rev() {
            let Some((before, _)) = best[i] else { continue };
            let span = &words[i..j];
            let chars = joined(span).chars().count();
            let dur = times[j - 1].1 - times[i].0;
            let single = j - i == 1;
            if !single && (chars > rules.max_chars() || dur > rules.max_s) {
                break; // longer ones only get longer
            }
            if !single && lines_of(span, &times[i..j], rules).is_none() {
                continue;
            }
            let mut cost = 1.0;
            if j < m {
                cost += break_cost(&words, &times, j - 1);
            }
            let shown = dur.max(rules.min_s);
            let speed = chars as f64 / shown;
            if speed > rules.cps {
                cost += (speed - rules.cps) * 0.5;
            }
            if dur < 1.2 && !(i == 0 && j == m) {
                cost += (1.2 - dur.max(0.0)) * 2.0;
            }
            if chars < 20 && !(i == 0 && j == m) {
                cost += 1.0;
            }
            let total = before + cost;
            if best[j].is_none_or(|(c, _)| total < c) {
                best[j] = Some((total, i));
            }
        }
    }
    let mut cuts = Vec::new();
    let mut j = m;
    while j > 0 {
        let (_, i) = best[j].expect("every prefix can be cut, one word a subtitle at worst");
        cuts.push((i, j));
        j = i;
    }
    cuts.reverse();
    cuts.into_iter()
        .map(|(i, j)| {
            let lines = lines_of(&words[i..j], &times[i..j], rules).unwrap_or_else(|| vec![joined(&words[i..j])]);
            (Cue { start: times[i].0, end: times[j - 1].1, lines }, words[i].from, words[j - 1].to)
        })
        .collect()
}

/// On and off at the right moments, all together: at least the shortest time on screen, a moment
/// after the speech when there is room, never over the next one, and gaps under half a second
/// closed to two frames.
fn settle(mut cues: Vec<Cue>, rules: &Rules) -> Vec<Cue> {
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    for k in 0..cues.len() {
        if k > 0 {
            let earliest = cues[k - 1].end + rules.gap_s;
            if cues[k].start < earliest {
                cues[k].start = earliest;
            }
        }
        let start = cues[k].start;
        let spoken = cues[k].end.max(start);
        let next = cues.get(k + 1).map_or(f64::INFINITY, |c| c.start);
        // a moment longer than the speech, but not so long that a flash of a gap is left
        let mut end = (spoken + rules.linger_s).min(next - rules.chain_s).max(spoken);
        end = end.max(start + rules.min_s);
        if next - end < rules.chain_s {
            end = next - rules.gap_s;
        }
        end = end.min(start + rules.max_s.max(spoken - start));
        // a wrong estimate left no room: it keeps its shortest time, and the next one waits
        if end < start + 0.2 {
            end = start + rules.min_s;
        }
        cues[k].end = end;
    }
    cues.retain(|c| c.end > c.start);
    cues
}

/// The transcript's subtitles.
pub fn cues(t: &Transcript) -> Vec<Cue> {
    let rules = Rules::for_language(&t.language);
    let all = t.segments.iter().flat_map(|s| line_cues(s, &s.text, true, &rules).into_iter().map(|(c, _, _)| c)).collect();
    settle(all, &rules)
}

/// Subtitles in both languages: the transcript's, each with the part of the translation that
/// goes with it underneath.
pub fn bilingual_cues(t: &Transcript, tr: &Translation) -> Vec<Cue> {
    let rules = Rules::for_language(&t.language);
    let mut all = Vec::new();
    for (i, s) in t.segments.iter().enumerate() {
        let own = line_cues(s, &s.text, true, &rules);
        let theirs = words_of(tr.text(i));
        let count = own.len();
        let mut lower = -1.0;
        for (k, (mut cue, _, to)) in own.into_iter().enumerate() {
            // the translation's words up to where this subtitle ends, by share of the text
            let upper = if k + 1 == count { 2.0 } else { to };
            let part: Vec<&str> = theirs
                .iter()
                .filter(|w| {
                    let mid = (w.from + w.to) / 2.0;
                    mid > lower && mid <= upper
                })
                .map(|w| w.text.as_str())
                .collect();
            if !part.is_empty() {
                cue.lines.push(part.join(" "));
            }
            lower = upper;
            all.push(cue);
        }
    }
    settle(all, &rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str, voice: Vec<f32>, pauses: Vec<[f32; 3]>) -> Segment {
        Segment { start, end, text: text.into(), flag: None, edited: false, voice, words: vec![], pauses, ..Default::default() }
    }

    fn transcript(language: &str, segments: Vec<Segment>) -> Transcript {
        Transcript {
            id: "c".into(),
            title: "t".into(),
            source_path: String::new(),
            created_ms: 0,
            duration_s: 60.0,
            language: language.into(),
            elapsed_s: 0.0,
            device: String::new(),
            model: String::new(),
            partial: false,
            segments,
            translations: vec![],
        }
    }

    /// Speech from 1 s to 9 s with a pause from 2.6 to 3.4 s, after a quarter of it.
    fn greeting() -> Segment {
        seg(
            0.0,
            10.0,
            "Good morning everyone. Today we have three points about the main screen and the login page.",
            vec![1.0, 9.0],
            vec![[0.22, 2.6, 3.4]],
        )
    }

    #[test]
    fn a_subtitle_ends_where_the_speaker_stops() {
        let c = cues(&transcript("en", vec![greeting()]));
        assert_eq!(c[0].text(), "Good morning everyone.", "{c:?}");
        assert!((c[0].start - 1.0).abs() < 0.01, "on when the speech starts: {}", c[0].start);
        assert!(c[0].end > 2.6 && c[0].end <= 3.0, "off a moment after the speaker stops at 2.6 s: {}", c[0].end);
        assert!((c[1].start - 3.4).abs() < 0.05, "the next on when the speaker starts again: {}", c[1].start);
        assert!(c[1].start - c[0].end >= 0.5 - 1e-9, "no flash of a gap between them: {c:?}");
        assert!(c[1].text().starts_with("Today"));
    }

    #[test]
    fn a_long_monologue_keeps_every_rule() {
        let text = (0..90).map(|i| ["we", "should", "probably", "check", "the", "numbers", "again", "before", "friday"][i % 9]).collect::<Vec<_>>().join(" ");
        let c = cues(&transcript("en", vec![seg(0.0, 30.0, &text, vec![0.5, 29.5], vec![])]));
        let rules = Rules::for_language("en");
        assert!(c.len() >= 5, "{}", c.len());
        for (k, cue) in c.iter().enumerate() {
            assert!(cue.lines.len() <= 2, "{cue:?}");
            assert!(cue.lines.iter().all(|l| l.chars().count() <= 42), "{cue:?}");
            assert!(cue.end - cue.start <= 7.0 + 1e-9 && cue.end - cue.start >= rules.min_s - 1e-9, "{cue:?}");
            if let Some(next) = c.get(k + 1) {
                assert!(next.start - cue.end >= rules.gap_s - 1e-9, "two frames between them: {cue:?} {next:?}");
            }
            assert!(!cue.lines.iter().any(|l| l.ends_with(" the")), "a line does not end on a word that leads into the next: {cue:?}");
        }
        let words: usize = c.iter().map(|cue| cue.text().split_whitespace().count()).sum();
        assert_eq!(words, 90, "no word lost or doubled");
    }

    #[test]
    fn a_line_breaks_after_the_comma() {
        let s = seg(0.0, 5.0, "When the report is ready, we will send it to the whole team", vec![0.2, 4.8], vec![]);
        let c = cues(&transcript("en", vec![s]));
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].lines, ["When the report is ready,", "we will send it to the whole team"]);
    }

    #[test]
    fn a_translation_breaks_at_the_same_pause() {
        let mut t = transcript("ar", vec![seg(0.0, 10.0, "صباح الخير جميعًا. اليوم عندنا ثلاث نقاط عن الشاشة الرئيسية وصفحة الدخول.", vec![1.0, 9.0], vec![[0.22, 2.6, 3.4]])]);
        let mut tr = Translation::new("en", "m");
        tr.lines = vec![crate::transcript::TranslatedLine { text: greeting().text, from: t.segments[0].text.clone(), edited: false }];
        t.translations.push(tr);
        let en = t.translated("en").unwrap();
        let c = cues(&en);
        assert_eq!(c[0].text(), "Good morning everyone.", "{c:?}");
        assert!((c[1].start - 3.4).abs() < 0.05, "{c:?}");
        let both = bilingual_cues(&t, t.translation("en").unwrap());
        assert_eq!(both[0].lines, ["صباح الخير جميعًا.", "Good morning everyone."], "{both:?}");
    }

    #[test]
    fn reading_speeds_are_netflixs() {
        assert_eq!(Rules::for_language("ar").cps, 20.0);
        assert_eq!(Rules::for_language("en").cps, 20.0);
        assert_eq!(Rules::for_language("da").cps, 17.0);
    }

    #[test]
    fn timing_follows_the_rules_between_subtitles() {
        let rules = Rules::for_language("en");
        let cue = |start: f64, end: f64| Cue { start, end, lines: vec!["x".into()] };
        let c = settle(vec![cue(0.0, 0.3), cue(1.5, 2.0), cue(2.6, 4.0), cue(4.05, 6.0)], &rules);
        assert!((c[0].end - 0.833).abs() < 0.01, "at least 5/6 s on screen: {:?}", c[0]);
        assert!((c[1].end - (2.6 - rules.gap_s)).abs() < 0.01, "a gap under half a second is closed: {:?}", c[1]);
        assert!(c[3].start - c[2].end >= rules.gap_s - 1e-9, "two frames between: {c:?}");
        assert!((c[3].end - 6.5).abs() < 0.01, "the last one stays half a second after the speech: {:?}", c[3]);
    }

    #[test]
    fn older_transcripts_without_pauses_still_get_subtitles() {
        let s = seg(10.0, 40.0, "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen", vec![], vec![]);
        let c = cues(&transcript("en", vec![s]));
        assert!(!c.is_empty());
        assert!(c[0].start >= 10.0 && c.last().unwrap().end <= 40.5);
        let words: usize = c.iter().map(|cue| cue.text().split_whitespace().count()).sum();
        assert_eq!(words, 17);
    }
}