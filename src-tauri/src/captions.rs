//! Subtitles timed and broken the way professional ones are, after the rules Netflix and the BBC
//! give their subtitlers: at most two lines (42 characters each in most languages, fewer in
//! Chinese, Japanese and Korean, counted as each language's guide counts them); on screen for at
//! least 5/6 of a second and at most 7 seconds, and slow enough to read; on when the speaker starts
//! and off when they stop, with a gap of two frames between one and the next; and broken where a
//! sentence or a clause ends or where the speaker pauses, not in the middle of a phrase.
//!
//! The speech model times whole lines only (up to 30 seconds). Inside a line the words are placed
//! by the timing pass (`timing`), and where it has not run, by where the speaker pauses
//! (`Segment::pauses`) and where the speech is (`Segment::voice`), by how far into the text each
//! word is. A translation is placed by the share of its text, cut where its own sentences end.

use crate::script::{self, Count};
use crate::transcript::{Segment, Transcript, Translation};

/// Measured with the timing bench (`tools/timing`): see `Rules::hold_s`.
pub(crate) const HOLD_S: f64 = 0.8;

pub struct Rules {
    /// Characters on a line (as `count` counts them), and lines on the screen
    pub max_line: f64,
    pub max_lines: usize,
    pub count: Count,
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
    /// Where the words of a line are only guessed, a subtitle that ends at a pause stays on until
    /// the speech after it has run this long, so that words the guess put before the pause are
    /// still on the screen when they are said.
    pub hold_s: f64,
}

impl Rules {
    /// The numbers of Netflix's guide for each language, for adult programs. Lines of 42 and 17
    /// characters a second, 20 in English, Arabic and Thai; Chinese 16 a line and 9 a second;
    /// Japanese 13 and 4; Korean 16 and 12; Thai 35, its marks above and below not counted. Khmer,
    /// Lao and Burmese have no guide: they take Thai's lines and the usual reading speed.
    pub fn for_language(language: &str) -> Rules {
        let (max_line, cps) = match language {
            "zh" | "zh-Hant" | "yue" => (16.0, 9.0),
            "ja" => (13.0, 4.0),
            "ko" => (16.0, 12.0),
            "th" => (35.0, 20.0),
            "lo" | "km" | "my" => (35.0, 17.0),
            "en" | "ar" => (42.0, 20.0),
            _ => (42.0, 17.0),
        };
        Rules {
            max_line,
            max_lines: 2,
            count: Count::for_language(language),
            min_s: 5.0 / 6.0,
            max_s: 7.0,
            cps,
            gap_s: 2.0 / 24.0,
            chain_s: 0.5,
            linger_s: 0.5,
            hold_s: HOLD_S,
        }
    }

    fn max_chars(&self) -> f64 {
        self.max_line * self.max_lines as f64
    }

    fn width(&self, text: &str) -> f64 {
        self.count.width(text)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub lines: Vec<String>,
    /// Who says it, when the speakers were told apart
    pub speaker: Option<u32>,
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

/// Chinese and Japanese characters a line may end after: particles that close a phrase.
const PARTICLES_AFTER: &[&str] = &["的", "了", "着", "过", "吗", "呢", "吧", "啊", "地", "得", "は", "が", "を", "に", "で", "と", "も", "へ", "の", "ね", "よ"];
/// And those a line may start with: words that join what follows to what went before.
const JOINS_BEFORE: &[&str] = &["和", "与", "但", "而", "或", "因", "所", "如", "然", "并", "就", "还", "也", "都", "可"];

fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

const CLOSING: &[char] = &['"', '\'', '”', '’', '»', ')', ']', '」', '』', '）', '》', '〉', '】', '〕', '］'];

/// Whether the word ends a sentence, or a clause, in any of the scripts.
fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(CLOSING).ends_with(['.', '!', '?', '؟', '…', '。', '！', '？', '｡', '．', '।', '။'])
}

fn ends_clause(word: &str) -> bool {
    word.trim_end_matches(CLOSING).ends_with([',', '،', ';', '؛', ':', '—', '–', '，', '、', '；', '：', '､', '‥', '၊'])
}

pub(crate) struct Word {
    pub(crate) text: String,
    /// Where it starts and ends in the line's text, as a share of its characters
    pub(crate) from: f64,
    pub(crate) to: f64,
    /// Its first character in the line's text
    pub(crate) at: usize,
    /// It follows the word before after a space (not so between the characters of Chinese, or
    /// the pieces of a word too long for a line)
    pub(crate) spaced: bool,
    /// Written in a script without spaces between words
    pub(crate) unspaced: bool,
}

/// The words of a line as subtitles cut it (see `script::tokens`): a word longer than a line is
/// cut into pieces that fit, so that no subtitle is ever wider than the rules allow.
pub(crate) fn words_in(text: &str, rules: &Rules) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len().max(1) as f64;
    let mut out: Vec<Word> = Vec::new();
    let mut last_end = 0;
    for (a, b) in script::tokens(text) {
        let spaced = !out.is_empty() && chars[last_end..a].iter().any(|c| c.is_whitespace());
        last_end = b;
        let unspaced = chars[a..b].iter().any(|c| script::unspaced(*c));
        // a word too wide for a line: pieces of a line's width, never between a letter and its marks
        let mut start = a;
        let mut first = true;
        while start < b {
            let mut end = start;
            let mut wide = 0.0;
            while end < b {
                let mut next = end + 1;
                while next < b && script::is_mark(chars[next]) {
                    next += 1;
                }
                let w = rules.width(&chars[end..next].iter().collect::<String>());
                if wide + w > rules.max_line && end > start {
                    break;
                }
                wide += w;
                end = next;
            }
            out.push(Word {
                text: chars[start..end].iter().collect(),
                from: start as f64 / len,
                to: end as f64 / len,
                at: start,
                spaced: first && spaced,
                unspaced,
            });
            first = false;
            start = end;
        }
    }
    out
}

/// The words of a line, counted as most languages count them.
#[cfg(test)]
pub(crate) fn words_of(text: &str) -> Vec<Word> {
    words_in(text, &Rules::for_language(""))
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

/// Whether the line's words have times (from the timing pass, or from the model of an older
/// version) that fit its text.
fn has_times(s: &Segment) -> bool {
    let len = s.text.chars().count() as u32;
    !s.words.is_empty() && s.words.iter().all(|w| w.0 <= len) && s.words.windows(2).all(|w| w[0].0 < w[1].0)
}

/// Where each word of a line is said: (start, end) in seconds. Word times are better than any
/// estimate: a word then ends where the next one starts, or where the speaker stops before it.
/// A piece of a word cut for a line's width is placed between its word's start and the next one.
pub(crate) fn word_times(s: &Segment, words: &[Word], from_model: bool) -> Vec<(f64, f64)> {
    let clock = Clock::new(s, words);
    let mut times: Vec<(f64, f64)> = words.iter().map(|w| (clock.at(w.from, true), clock.at(w.to, false))).collect();
    if from_model && has_times(s) {
        let speech_end = clock.at(1.0, false);
        let starts = &s.words;
        for (k, w) in words.iter().enumerate() {
            let at = w.at as u32;
            let i = starts.partition_point(|x| x.0 <= at);
            let Some(i) = i.checked_sub(1) else { continue };
            let (a, t) = (starts[i].0, starts[i].1 as f64);
            times[k].0 = if a == at {
                t
            } else {
                // inside a word: between its start and the next word's, by its characters
                let (b, u) = starts.get(i + 1).map_or((s.text.chars().count() as u32, speech_end), |n| (n.0, n.1 as f64));
                t + (u - t) * (at - a) as f64 / (b - a).max(1) as f64
            };
        }
        for k in 0..times.len() {
            let start = times[k].0;
            let next = times.get(k + 1).map_or(speech_end.max(start), |n| n.0);
            let stop = s.pauses.iter().map(|p| p[1] as f64).find(|&a| a > start && a < next).unwrap_or(next);
            times[k].1 = stop.max(start);
        }
    }
    times
}

/// What it costs to end a subtitle (or a line) after word `k`, before word `k + 1`: nothing at
/// the end of a sentence or at a long pause, a little at a comma or before a clause, more inside a
/// phrase, and most inside a word of Thai or the like, or a word cut for its length.
fn break_cost(words: &[Word], times: &[(f64, f64)], k: usize) -> f64 {
    let (here, next) = (&words[k], &words[k + 1]);
    let pause = times[k + 1].0 - times[k].1;
    let joined_to_next = !next.spaced;
    let mut cost: f64 = if ends_sentence(&here.text) {
        0.0
    } else if ends_clause(&here.text) {
        1.5
    } else if CLAUSE_STARTS.contains(&bare(&next.text).as_str()) {
        3.0
    } else if !joined_to_next && (here.unspaced || next.unspaced) {
        // a space in Thai, Lao, Khmer or Burmese ends a phrase, as a comma does elsewhere
        1.5
    } else if joined_to_next && here.unspaced && next.unspaced && script::wide(here.text.chars().last().unwrap_or(' ')) {
        // between two Chinese or Japanese characters: better after a particle or before a
        // joining word, which no word goes on over
        if PARTICLES_AFTER.contains(&here.text.trim_end_matches(|c: char| !c.is_alphanumeric())) || JOINS_BEFORE.iter().any(|j| next.text.starts_with(j)) {
            3.0
        } else {
            4.0
        }
    } else if joined_to_next {
        // inside a Thai, Lao, Khmer or Burmese word, or a word cut in pieces
        12.0
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

/// The words written as on the line: a space where the text had one.
fn joined(words: &[Word]) -> String {
    let mut out = String::new();
    for (k, w) in words.iter().enumerate() {
        if k > 0 && w.spaced {
            out.push(' ');
        }
        out.push_str(&w.text);
    }
    out
}

/// How wide the words are when joined, as running sums: `span(i, j)` is the width of words
/// `i..j` on one line, worked out once for all the ways a line is cut.
struct Widths {
    sum: Vec<f64>,
    space_before: Vec<f64>,
}

impl Widths {
    fn of(words: &[Word], rules: &Rules) -> Widths {
        let space = rules.width(" ");
        let mut sum = Vec::with_capacity(words.len() + 1);
        let mut space_before = Vec::with_capacity(words.len());
        sum.push(0.0);
        for (k, w) in words.iter().enumerate() {
            let gap = if k > 0 && w.spaced { space } else { 0.0 };
            space_before.push(gap);
            sum.push(sum[k] + gap + rules.width(&w.text));
        }
        Widths { sum, space_before }
    }

    fn span(&self, i: usize, j: usize) -> f64 {
        if j <= i {
            return 0.0;
        }
        self.sum[j] - self.sum[i] - self.space_before[i]
    }
}

/// A subtitle's text in at most `max_lines` lines of at most `max_line` characters, broken at the
/// best place: after punctuation or before a clause, with the lines about as long as each other,
/// the lower one rather the longer. None when it does not fit.
fn lines_of(words: &[Word], times: &[(f64, f64)], rules: &Rules) -> Option<Vec<String>> {
    let wide = Widths::of(words, rules);
    if wide.span(0, words.len()) <= rules.max_line {
        return Some(vec![joined(words)]);
    }
    let mut best: Option<(f64, usize)> = None;
    for k in 0..words.len() - 1 {
        let top = wide.span(0, k + 1);
        let bottom = wide.span(k + 1, words.len());
        if top > rules.max_line || bottom > rules.max_line {
            continue;
        }
        let shape = (top - bottom).abs() / (rules.max_line / 5.25) + if top > bottom { 0.5 } else { 0.0 };
        // without spaces, where the line breaks matters more than how even the two lines are
        let weight = if words[k].unspaced || words[k + 1].unspaced { 1.0 } else { 0.6 };
        let cost = break_cost(words, times, k) * weight + shape;
        if best.is_none_or(|(c, _)| cost < c) {
            best = Some((cost, k));
        }
    }
    let (_, k) = best?;
    Some(vec![joined(&words[..=k]), joined(&words[k + 1..])])
}

/// Lines of at most `max_line`, as many as it takes: for a translation that goes with one subtitle.
fn wrapped(words: &[Word], rules: &Rules) -> Vec<String> {
    let flat = vec![(0.0, 0.0); words.len()];
    if words.is_empty() {
        return Vec::new();
    }
    if let Some(lines) = lines_of(words, &flat, rules) {
        return lines;
    }
    let wide = Widths::of(words, rules);
    let mut lines = Vec::new();
    let mut from = 0;
    for k in 1..=words.len() {
        if k == words.len() || wide.span(from, k + 1) > rules.max_line {
            lines.push(joined(&words[from..k]));
            from = k;
        }
    }
    lines
}

/// The subtitles of one line of a transcript, with the share of its text each one covers.
fn line_cues(s: &Segment, text: &str, from_model: bool, rules: &Rules) -> Vec<(Cue, f64, f64)> {
    let words = words_in(text, rules);
    if words.is_empty() {
        return Vec::new();
    }
    let times = word_times(s, &words, from_model);
    let wide = Widths::of(&words, rules);
    let m = words.len();
    // best[j]: the cheapest way to cut the first j words into subtitles, and where the last one began
    let mut best: Vec<Option<(f64, usize)>> = vec![None; m + 1];
    best[0] = Some((0.0, 0));
    for j in 1..=m {
        for i in (0..j).rev() {
            let Some((before, _)) = best[i] else { continue };
            let span = &words[i..j];
            let chars = wide.span(i, j);
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
            let speed = chars / shown;
            if speed > rules.cps {
                cost += (speed - rules.cps) * 0.5 * 20.0 / rules.cps;
            }
            if dur < 1.2 && !(i == 0 && j == m) {
                cost += (1.2 - dur.max(0.0)) * 2.0;
            }
            if chars < rules.max_line / 2.1 && !(i == 0 && j == m) {
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
    let mut out: Vec<(Cue, f64, f64)> = cuts
        .into_iter()
        .map(|(i, j)| {
            let lines = lines_of(&words[i..j], &times[i..j], rules).unwrap_or_else(|| vec![joined(&words[i..j])]);
            (Cue { start: times[i].0, end: times[j - 1].1, lines, speaker: s.speaker }, words[i].from, words[j - 1].to)
        })
        .collect();
    let guessed = !(from_model && has_times(s));
    if guessed && rules.hold_s > 0.0 {
        for k in 0..out.len().saturating_sub(1) {
            let next = out[k + 1].0.start;
            if next - out[k].0.end >= 0.25 {
                out[k].0.end = next + rules.hold_s;
                out[k + 1].0.start = out[k].0.end;
                out[k + 1].0.end = out[k + 1].0.end.max(out[k + 1].0.start);
            }
        }
    }
    out
}

/// On and off at the right moments, all together: at least the shortest time on screen, a moment
/// after the speech when there is room, never over the next one, never longer than 7 seconds,
/// and gaps under half a second closed to two frames.
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
        end = end.min(start + rules.max_s);
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
    cues_with(t, &Rules::for_language(&t.language))
}

pub(crate) fn cues_with(t: &Transcript, rules: &Rules) -> Vec<Cue> {
    let all = t.segments.iter().flat_map(|s| line_cues(s, &s.text, true, rules).into_iter().map(|(c, _, _)| c)).collect();
    settle(all, rules)
}

/// A translation's words in the pieces that go under subtitles: its sentences, whole. Only a
/// sentence too long for the lines under one subtitle is cut, where a clause ends if it can be,
/// else where the lines are full. As ranges of `theirs`.
fn translation_pieces(theirs: &[Word], rules: &Rules) -> Vec<(usize, usize)> {
    let wide = Widths::of(theirs, rules);
    let room = rules.max_chars();
    let mut out = Vec::new();
    let mut from = 0;
    for k in 0..theirs.len() {
        if k + 1 == theirs.len() || ends_sentence(&theirs[k].text) {
            // one sentence: theirs[from..=k]
            let mut a = from;
            while wide.span(a, k + 1) > room {
                // as much as fits, back to the last clause end in it if there is one
                let mut fit = a + 1;
                while fit < k + 1 && wide.span(a, fit + 1) <= room {
                    fit += 1;
                }
                let clause = (a + 1..fit).rev().find(|&j| ends_clause(&theirs[j - 1].text) && wide.span(a, j) >= room / 3.0);
                let cut = clause.unwrap_or(fit);
                out.push((a, cut));
                a = cut;
            }
            out.push((a, k + 1));
            from = k + 1;
        }
    }
    out
}

/// Subtitles in both languages: the transcript's, each with the part of the translation that
/// goes with it underneath. The translation is cut where its sentences end, never in the middle
/// of one that fits under a subtitle: each sentence goes under the subtitle where its middle is
/// said (by the share of the text), wrapped to the translation's own line length.
pub fn bilingual_cues(t: &Transcript, tr: &Translation) -> Vec<Cue> {
    let rules = Rules::for_language(&t.language);
    let theirs_rules = Rules::for_language(&tr.language);
    let mut all = Vec::new();
    for (i, s) in t.segments.iter().enumerate() {
        let own = line_cues(s, &s.text, true, &rules);
        if own.is_empty() {
            // a line cleared by hand: no subtitle for its old translation to go under
            continue;
        }
        let theirs = words_in(tr.text(i), &theirs_rules);
        let count = own.len();
        let mut parts: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (a, b) in translation_pieces(&theirs, &theirs_rules) {
            let mid = (theirs[a].from + theirs[b - 1].to) / 2.0;
            let k = own.iter().position(|(_, _, to)| mid <= *to).unwrap_or(count.saturating_sub(1));
            parts[k].extend(a..b);
        }
        for ((mut cue, _, _), words) in own.into_iter().zip(parts) {
            if let (Some(&a), Some(&b)) = (words.first(), words.last()) {
                cue.lines.extend(wrapped(&theirs[a..=b], &theirs_rules));
            }
            all.push(cue);
        }
    }
    settle(all, &rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str, voice: Vec<f32>, pauses: Vec<[f32; 3]>) -> Segment {
        Segment { start, end, text: text.into(), flag: None, edited: false, voice, words: vec![], pauses, heard: vec![], speaker: None, track: None }
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
            timed: false,
            outputs: vec![],
            outputs_listed: false,
            segments,
            translations: vec![],
            minutes: None,
            speakers: Vec::new(),
            detected: None,
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

    /// The same, with the time of each word (from the timing pass): three before the pause, the
    /// rest after it.
    fn greeting_timed() -> Segment {
        let mut s = greeting();
        let toks = crate::script::tokens(&s.text);
        s.words = toks
            .iter()
            .enumerate()
            .map(|(k, &(a, _))| (a as u32, if k < 3 { 1.0 + 0.5 * k as f32 } else { 3.4 + 0.5 * (k - 3) as f32 }))
            .collect();
        s
    }

    #[test]
    fn a_subtitle_ends_where_the_speaker_stops() {
        let c = cues(&transcript("en", vec![greeting_timed()]));
        assert_eq!(c[0].text(), "Good morning everyone.", "{c:?}");
        assert!((c[0].start - 1.0).abs() < 0.01, "on when the speech starts: {}", c[0].start);
        assert!(c[0].end > 2.6 && c[0].end <= 3.0, "off a moment after the speaker stops at 2.6 s: {}", c[0].end);
        assert!((c[1].start - 3.4).abs() < 0.05, "the next on when the speaker starts again: {}", c[1].start);
        assert!(c[1].start - c[0].end >= 0.5 - 1e-9, "no flash of a gap between them: {c:?}");
        assert!(c[1].text().starts_with("Today"));
    }

    #[test]
    fn a_guessed_subtitle_stays_into_the_speech_after_the_pause() {
        let c = cues(&transcript("en", vec![greeting()]));
        assert_eq!(c[0].text(), "Good morning everyone.", "{c:?}");
        assert!((c[0].end - (3.4 + HOLD_S)).abs() < 0.05, "still there for words the guess put early: {c:?}");
        assert!(c[1].start >= c[0].end, "{c:?}");
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
        assert!((c[1].start - (3.4 + HOLD_S)).abs() < 0.1, "a translation's times are a guess: {c:?}");
        let both = bilingual_cues(&t, t.translation("en").unwrap());
        assert_eq!(both[0].lines, ["صباح الخير جميعًا.", "Good morning everyone."], "{both:?}");
    }

    #[test]
    fn a_translation_under_both_is_wrapped_and_cut_where_its_sentences_end() {
        // the original's subtitles break after its first sentence; the translation's first
        // sentence is much longer than its share, and still goes whole with the first subtitle
        let mut t = transcript(
            "ar",
            vec![seg(0.0, 12.0, "تمام. التقارير الشهرية بتطلع ناقصة، والأرقام ما بتطابق الفواتير أبدًا.", vec![0.5, 11.5], vec![[0.1, 1.0, 2.0]])],
        );
        let mut tr = Translation::new("en", "m");
        tr.lines = vec![crate::transcript::TranslatedLine {
            text: "Fine, that settles it then. The monthly reports come out incomplete, and the numbers never match the invoices.".into(),
            from: t.segments[0].text.clone(),
            edited: false,
        }];
        t.translations.push(tr);
        let both = bilingual_cues(&t, t.translation("en").unwrap());
        assert!(both.len() >= 2, "{both:?}");
        let latin = |c: &Cue| c.lines.iter().filter(|l| l.is_ascii()).cloned().collect::<Vec<_>>().join(" ");
        for c in &both {
            let part = latin(c);
            assert!(part.is_empty() || part.ends_with('.'), "the translation under it ends where a sentence does: {both:?}");
        }
        assert_eq!(latin(&both[0]), "Fine, that settles it then.", "the first sentence under the first subtitle: {both:?}");
        for c in &both {
            assert!(c.lines.iter().all(|l| l.chars().count() <= 42), "every line wrapped: {c:?}");
        }
        let all: String = both.iter().map(latin).collect::<Vec<_>>().join(" ");
        assert_eq!(all.split_whitespace().count(), 18, "no word of the translation lost: {all}");
    }

    #[test]
    fn a_line_cleared_by_hand_leaves_its_translation_out_of_both() {
        let mut t = transcript("ar", vec![seg(0.0, 4.0, "صباح الخير.", vec![0.2, 3.8], vec![]), seg(4.0, 8.0, "  ", vec![4.2, 7.8], vec![])]);
        let mut tr = Translation::new("en", "m");
        tr.lines = vec![
            crate::transcript::TranslatedLine { text: "Good morning.".into(), from: "صباح الخير.".into(), edited: false },
            crate::transcript::TranslatedLine { text: "Today we have three points.".into(), from: "اليوم عندنا ثلاث نقاط.".into(), edited: false },
        ];
        t.translations.push(tr);
        let both = bilingual_cues(&t, t.translation("en").unwrap());
        assert_eq!(both.len(), 1, "{both:?}");
        assert_eq!(both[0].lines, ["صباح الخير.", "Good morning."]);
    }

    #[test]
    fn a_translated_sentence_too_long_for_one_subtitle_is_cut_at_its_commas() {
        let rules = Rules::for_language("en");
        let theirs = words_in("We will go through all of them with the accounting team on Monday morning, check every invoice against the report, and come back to you with the details on Tuesday.", &rules);
        let pieces = translation_pieces(&theirs, &rules);
        assert!(pieces.len() >= 2, "{pieces:?}");
        for &(a, b) in &pieces[..pieces.len() - 1] {
            assert!(theirs[b - 1].text.ends_with(','), "cut where a clause ends: {}", theirs[b - 1].text);
            assert!(wrapped(&theirs[a..b], &rules).len() <= 2);
        }
    }

    #[test]
    fn reading_speeds_are_netflixs() {
        assert_eq!(Rules::for_language("ar").cps, 20.0);
        assert_eq!(Rules::for_language("en").cps, 20.0);
        assert_eq!(Rules::for_language("da").cps, 17.0);
        assert_eq!((Rules::for_language("zh").max_line, Rules::for_language("zh").cps), (16.0, 9.0));
        assert_eq!((Rules::for_language("ja").max_line, Rules::for_language("ja").cps), (13.0, 4.0));
        assert_eq!((Rules::for_language("ko").max_line, Rules::for_language("ko").cps), (16.0, 12.0));
        assert_eq!(Rules::for_language("th").max_line, 35.0);
    }

    #[test]
    fn timing_follows_the_rules_between_subtitles() {
        let rules = Rules::for_language("en");
        let cue = |start: f64, end: f64| Cue { start, end, lines: vec!["x".into()], speaker: None };
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

    // ------------------------------------------------------------ every script

    /// A line of 28 seconds in each language: the giant subtitle of 1.3.0 is cut into readable ones.
    fn one_long_line(language: &str, text: &str) -> Vec<Cue> {
        cues(&transcript(language, vec![seg(1.0, 29.0, text, vec![1.0, 29.0], vec![])]))
    }

    fn keeps_the_rules(language: &str, text: &str) -> Vec<Cue> {
        let c = one_long_line(language, text);
        let rules = Rules::for_language(language);
        assert!(c.len() >= 3, "{language}: cut into several subtitles: {c:?}");
        for cue in &c {
            assert!(cue.lines.len() <= 2, "{language}: {cue:?}");
            for l in &cue.lines {
                assert!(rules.width(l) <= rules.max_line + 1e-9, "{language}: line of {} > {}: {l}", rules.width(l), rules.max_line);
            }
            assert!(cue.end - cue.start <= 7.0 + 1e-9, "{language}: on screen too long: {cue:?}");
        }
        let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(squeeze(&c.iter().map(|x| x.text()).collect::<String>()), squeeze(text), "{language}: no character lost or doubled");
        c
    }

    #[test]
    fn chinese_is_cut_into_lines_of_16_at_its_own_punctuation() {
        let text = "今天我们开会讨论新产品的发布计划。市场部希望在下个月底之前完成所有的准备工作，包括宣传视频和网站更新。技术部表示时间有点紧张，但是会尽量配合。大家还有什么问题吗？没有的话我们就先到这里，谢谢大家。";
        let c = keeps_the_rules("zh", text);
        // a subtitle or a line never starts with a full stop or a comma
        for cue in &c {
            for l in &cue.lines {
                assert!(!l.starts_with(['。', '，', '？']), "{l}");
            }
        }
        assert!(c.iter().any(|cue| cue.lines.iter().any(|l| l.ends_with('。') || l.ends_with('，'))), "broken at its punctuation: {c:?}");
    }

    #[test]
    fn japanese_lines_hold_13_full_width_characters() {
        keeps_the_rules("ja", "今日は新しいプロジェクトについて話し合います。まず、スケジュールを確認しましょう。来月の終わりまでに、デザインとテストを終わらせる必要があります。何か質問はありますか？なければ、これで終わりにします。");
    }

    #[test]
    fn korean_counts_latin_letters_and_spaces_as_half() {
        keeps_the_rules("ko", "오늘 회의에서는 새로운 앱의 출시 계획을 논의하겠습니다. 마케팅팀은 다음 달 말까지 모든 준비를 마치기를 원합니다. 개발팀은 일정이 조금 빠듯하지만 최대한 맞추겠다고 했습니다. 질문 있으신가요? 없으면 여기서 마치겠습니다. 감사합니다.");
    }

    #[test]
    fn thai_breaks_between_phrases_not_inside_words() {
        let c = keeps_the_rules("th", "เมื่อวานเราไปตลาดด้วยกัน แล้วก็ซื้อผลไม้มาเยอะมาก วันนี้เราจะประชุมเรื่องแผนงานใหม่ ฝ่ายการตลาดอยากให้เสร็จก่อนสิ้นเดือนหน้า ฝ่ายเทคนิคบอกว่าเวลาค่อนข้างกระชั้นชิด แต่จะพยายามทำให้ทัน มีคำถามไหมครับ ถ้าไม่มีก็จบการประชุมเท่านี้");
        let breaks_at_spaces = c.windows(2).filter(|w| w[0].lines.last().is_some_and(|l| ["กัน", "มาก", "ใหม่", "หน้า", "ชิด", "ทัน", "ครับ"].iter().any(|e| l.ends_with(e)))).count();
        assert!(breaks_at_spaces >= c.len() / 2, "most subtitles end where a phrase ends: {c:?}");
    }

    #[test]
    fn khmer_lao_and_burmese_get_subtitles_that_fit() {
        keeps_the_rules("km", "ថ្ងៃនេះយើងនឹងពិភាក្សាអំពីផែនការថ្មី។ ក្រុមទីផ្សារចង់ឱ្យរួចរាល់មុនចុងខែក្រោយ។ ក្រុមបច្ចេកទេសនិយាយថាពេលវេលាតឹងតែងបន្តិច ប៉ុន្តែនឹងព្យាយាមឱ្យអស់ពីសមត្ថភាព។ មានសំណួរទេ? បើគ្មានទេ យើងបញ្ចប់ត្រឹមនេះ។");
        keeps_the_rules("lo", "ມື້ນີ້ພວກເຮົາຈະປະຊຸມກ່ຽວກັບແຜນການໃໝ່ ຝ່າຍການຕະຫຼາດຢາກໃຫ້ສຳເລັດກ່ອນທ້າຍເດືອນໜ້າ ຝ່າຍເຕັກນິກບອກວ່າເວລາຄ່ອນຂ້າງກະຊັ້ນຊິດ ແຕ່ຈະພະຍາຍາມໃຫ້ທັນ ມີຄຳຖາມບໍ່ ຖ້າບໍ່ມີກໍ່ຈົບການປະຊຸມພຽງເທົ່ານີ້");
        keeps_the_rules("my", "ဒီနေ့ ကျွန်တော်တို့ အစီအစဉ်အသစ်အကြောင်း ဆွေးနွေးပါမယ်။ စျေးကွက်ရှာဖွေရေးအဖွဲ့က နောက်လကုန်မတိုင်ခင် ပြီးစေချင်ပါတယ်။ နည်းပညာအဖွဲ့က အချိန်နည်းနည်းကျပ်တယ်လို့ ပြောပါတယ်။ မေးခွန်းရှိပါသလား။ မရှိရင် ဒီမှာပဲ ရပ်ပါမယ်။");
    }

    #[test]
    fn a_word_longer_than_a_line_is_cut_too() {
        let url = "https://example.com/a/very/long/address/that/no/line/of/subtitles/can/hold/at/all";
        let c = one_long_line("en", &format!("Please open {url} before the meeting tomorrow morning."));
        for cue in &c {
            assert!(cue.lines.iter().all(|l| l.chars().count() <= 42), "{c:?}");
        }
        let joined: String = c.iter().map(|x| x.text().replace('\n', "")).collect();
        assert!(joined.contains("https://example.com/a/very/long/address/th"), "{joined}");
    }

    /// A made-up talk in a script, from a stock of its sentences, with pauses here and there.
    fn random_transcript(language: &str, stock: &[&str], seed: u64) -> Transcript {
        let mut x = seed;
        let mut next = |n: u64| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (x >> 33) % n
        };
        let mut segments = Vec::new();
        let mut at = 0.0;
        for _ in 0..(3 + next(5)) {
            let parts = 1 + next(6) as usize;
            let text = (0..parts).map(|_| stock[next(stock.len() as u64) as usize]).collect::<Vec<_>>().join(if matches!(language, "zh" | "ja") { "" } else { " " });
            let dur = 4.0 + next(26) as f64;
            let pause = (next(3) == 0).then(|| [0.4, (at + dur * 0.4) as f32, (at + dur * 0.4 + 0.8) as f32]);
            segments.push(seg(at, at + dur, &text, vec![(at + 0.2) as f32, (at + dur - 0.2) as f32], pause.into_iter().collect()));
            at += dur + 0.3;
        }
        transcript(language, segments)
    }

    #[test]
    fn random_talks_in_every_script_keep_every_rule() {
        let stocks: &[(&str, &[&str])] = &[
            ("en", &["Okay, let's get started.", "The numbers in the monthly report do not match the invoices,", "and nobody is sure yet whether the report or the invoices are wrong.", "Sam, could you take that one?", "Thanks, everyone."]),
            ("ar", &["طيب، خلونا نبدأ.", "الأرقام في التقرير الشهري ما بتطابق الـ invoices،", "ولسا ما حدا متأكد إذا الغلط في التقرير ولا في الفواتير.", "سامي، تقدر تاخذ هالمهمة؟", "شكرًا للجميع."]),
            ("zh", &["好的，我们开始吧。", "月度报告里的数字和发票对不上，", "现在还不确定是报告错了还是发票错了。", "小王，你能负责这个吗？", "谢谢大家。"]),
            ("ja", &["では、始めましょう。", "月次報告の数字が請求書と合いません。", "報告書と請求書のどちらが間違っているのか、まだ分かりません。", "田中さん、お願いできますか？", "ありがとうございました。"]),
            ("ko", &["자, 시작하겠습니다.", "월간 보고서의 숫자가 송장과 맞지 않습니다.", "보고서가 틀렸는지 송장이 틀렸는지 아직 모릅니다.", "민수 씨, 맡아 주시겠어요?", "감사합니다."]),
            ("th", &["เอาล่ะ เริ่มกันเลย", "ตัวเลขในรายงานประจำเดือนไม่ตรงกับใบแจ้งหนี้", "ยังไม่มีใครแน่ใจว่ารายงานหรือใบแจ้งหนี้ผิด", "สมชายช่วยรับเรื่องนี้ได้ไหม", "ขอบคุณทุกคน"]),
            ("km", &["អញ្ចឹង យើងចាប់ផ្តើម។", "លេខក្នុងរបាយការណ៍ប្រចាំខែមិនត្រូវនឹងវិក្កយបត្រទេ។", "សុខា តើអ្នកអាចទទួលបន្ទុកការងារនេះបានទេ?", "អរគុណទាំងអស់គ្នា។"]),
            ("my", &["ကဲ စကြရအောင်။", "လစဉ်အစီရင်ခံစာထဲက ကိန်းဂဏန်းတွေက ငွေတောင်းခံလွှာနဲ့ မကိုက်ညီဘူး။", "ကျေးဇူးတင်ပါတယ်။"]),
            ("lo", &["ເອົາລະ ເລີ່ມກັນເລີຍ", "ຕົວເລກໃນລາຍງານປະຈຳເດືອນບໍ່ກົງກັບໃບແຈ້ງໜີ້", "ຂອບໃຈທຸກຄົນ"]),
        ];
        let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        for (language, stock) in stocks {
            let rules = Rules::for_language(language);
            for seed in 0..20u64 {
                let t = random_transcript(language, stock, seed + 1);
                let c = cues(&t);
                let said: String = t.segments.iter().map(|s| squeeze(&s.text)).collect();
                assert_eq!(c.iter().map(|x| squeeze(&x.text())).collect::<String>(), said, "{language} {seed}: no character lost or doubled");
                for (k, cue) in c.iter().enumerate() {
                    assert!(cue.lines.len() <= 2, "{language} {seed}: {cue:?}");
                    for l in &cue.lines {
                        assert!(rules.width(l) <= rules.max_line + 1e-9, "{language} {seed}: {l}");
                    }
                    assert!(cue.end - cue.start <= 7.0 + 1e-9 && cue.end > cue.start, "{language} {seed}: {cue:?}");
                    if let Some(next) = c.get(k + 1) {
                        assert!(next.start - cue.end >= rules.gap_s - 1e-9, "{language} {seed}: overlap {cue:?} {next:?}");
                    }
                }
            }
        }
    }
}

