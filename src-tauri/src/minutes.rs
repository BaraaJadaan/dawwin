//! Minutes of a transcript: a short summary, the decisions, the action items (who, what, by when),
//! the topics with their times and the questions left open. A language model writes them through
//! llama.cpp's server (the translator's), answering in JSON held to a schema by the server, never
//! in free text to be parsed. A long recording goes in parts of about ten minutes, each turned into
//! notes, and the notes into the minutes in one more pass.
//!
//! The prompts and the schema are in `prompts/`, read by the benchmark too
//! (`tools/bench-minutes`), so what was measured is what the app asks.

use std::sync::OnceLock;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::transcript::{ts, Transcript};

const PROMPTS: &str = include_str!("../prompts/minutes.json");
const SCHEMA: &str = include_str!("../prompts/minutes-schema.json");

/// A part is at most this long, and holds at most this much text.
pub const PART_S: f64 = 600.0;
pub const PART_CHARS: usize = 12_000;
/// Notes longer than this are combined a few parts at a time first.
pub const NOTES_CHARS: usize = 14_000;

// ---------------------------------------------------------------- the models

/// What a minutes model needs beyond what `models::VARIANTS` says of every model: room for a part
/// of ten minutes and its answer, the memory it takes with that room (to say whether it fits this
/// computer), and the languages it writes well, as measured by `tools/bench-minutes`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Spec {
    pub id: &'static str,
    pub context: u32,
    /// The model with its context, in bytes: on the GPU to run there, else in memory
    pub needs: u64,
    pub languages: &'static [&'static str],
    /// How its chat format is told to answer at once, without thinking first (the
    /// `chat_template_kwargs` the benchmark sent)
    pub no_thinking: &'static str,
}

const ENABLE_THINKING_OFF: &str = r#"{"enable_thinking": false}"#;
/// Jais-3 thinks unless told to keep it low, and then it does not
const REASONING_LOW: &str = r#"{"reasoning_effort": "low"}"#;

/// The room is a part of ten minutes and its answer. What each needs is its file, its context
/// cache for that room in 8 bits (layers × 2 × KV heads × head size × 16,384 × 1.06 bytes) and about
/// half a gigabyte to work in.
pub const MODELS: &[Spec] = &[
    // 36 layers, 8 KV heads of 128
    Spec { id: "min-jais3-7b", context: 16_384, needs: 5_592_226_016 + 1_283_000_000 + 450_000_000, languages: &["ar", "en"], no_thinking: REASONING_LOW },
    // 26 layers, 8 KV heads of 128
    Spec { id: "min-ministral3-3b", context: 16_384, needs: 2_147_023_008 + 927_000_000 + 400_000_000, languages: &["ar", "en"], no_thinking: ENABLE_THINKING_OFF },
    // 28 layers, 8 KV heads of 64
    Spec { id: "min-jais3-0.9b", context: 16_384, needs: 666_192_384 + 499_000_000 + 300_000_000, languages: &["ar", "en"], no_thinking: REASONING_LOW },
];

pub fn model(id: &str) -> Spec {
    MODELS.iter().copied().find(|s| s.id == id).unwrap_or(Spec { id: "", context: 16_384, needs: 0, languages: &["ar", "en"], no_thinking: ENABLE_THINKING_OFF })
}

// ---------------------------------------------------------------- what is kept

/// The minutes, in the transcript's file.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Minutes {
    /// The language they are written in
    pub language: String,
    /// The model that wrote them
    pub model: String,
    /// When they were written; 0 while the first run is under way
    pub written_ms: u64,
    /// What the transcript said when they were written (see [`fingerprint`]): when it reads
    /// differently now, it was corrected since
    pub from: String,
    /// meeting | lecture | interview | other: only a meeting has decisions and actions
    pub kind: String,
    pub summary: String,
    pub summary_edited: bool,
    pub decisions: Vec<Point>,
    pub actions: Vec<Action>,
    pub topics: Vec<Topic>,
    pub open_questions: Vec<Point>,
    /// A run under way, or stopped: the notes done so far, so that it continues
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<Draft>,
}

/// A decision or an open question, and the lines it came from (indexes into the segments).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Point {
    pub text: String,
    pub lines: Vec<usize>,
    /// When its first line starts, in seconds
    pub time: Option<f64>,
    pub edited: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Action {
    pub who: String,
    pub what: String,
    pub due: String,
    pub lines: Vec<usize>,
    pub time: Option<f64>,
    pub edited: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Topic {
    pub title: String,
    pub line: usize,
    pub time: f64,
    pub edited: bool,
}

/// The parts of a run and the notes on those done.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Draft {
    pub language: String,
    pub model: String,
    pub from: String,
    /// Each part: its first and last line, as indexes into the segments
    pub parts: Vec<[usize; 2]>,
    /// The model's answer for each part done, in order
    pub notes: Vec<Value>,
}

impl Minutes {
    pub fn written(&self) -> bool {
        self.written_ms > 0
    }

    /// Any of it changed by hand: writing again would lose it (the panel asks twice then).
    #[cfg(test)]
    pub fn edited(&self) -> bool {
        self.summary_edited
            || self.decisions.iter().any(|d| d.edited)
            || self.actions.iter().any(|a| a.edited)
            || self.topics.iter().any(|t| t.edited)
            || self.open_questions.iter().any(|q| q.edited)
    }

    /// Take the user's corrections: what they changed is marked as theirs, and an item left empty goes.
    pub fn apply_edits(&mut self, mut new: Minutes) {
        fn mark<T: PartialEq + Clone>(old: &[T], new: &mut [T], same: impl Fn(&T, &T) -> bool, set: impl Fn(&mut T)) {
            for n in new.iter_mut() {
                if !old.iter().any(|o| same(o, n)) {
                    set(n);
                }
            }
        }
        new.decisions.retain(|d| !d.text.trim().is_empty());
        new.open_questions.retain(|q| !q.text.trim().is_empty());
        new.actions.retain(|a| !(a.what.trim().is_empty() && a.who.trim().is_empty()));
        new.topics.retain(|t| !t.title.trim().is_empty());
        mark(&self.decisions, &mut new.decisions, |o, n| o == n, |d| d.edited = true);
        mark(&self.open_questions, &mut new.open_questions, |o, n| o == n, |q| q.edited = true);
        mark(&self.actions, &mut new.actions, |o, n| o == n, |a| a.edited = true);
        mark(&self.topics, &mut new.topics, |o, n| o == n, |t| t.edited = true);
        if new.summary.trim() != self.summary.trim() {
            self.summary = new.summary.trim().to_string();
            self.summary_edited = true;
        }
        self.decisions = new.decisions;
        self.actions = new.actions;
        self.topics = new.topics;
        self.open_questions = new.open_questions;
    }
}

/// What a transcript's lines say, in a few characters: it changes with any line.
pub fn fingerprint(t: &Transcript) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for s in &t.segments {
        h.update(s.text.trim().as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- what the model reads

#[derive(Deserialize)]
struct Prompts {
    system: String,
    transcript: String,
    part: String,
    notes: String,
    task: String,
    task_part: String,
    task_combine: String,
    same_language: String,
    other_language: String,
    fields: String,
    /// A recorded meeting: who is who (not part of the benchmark's prompts, which have no such meeting)
    recorded: String,
    recorded_me: String,
    recorded_them: String,
}

fn prompts() -> &'static Prompts {
    static P: OnceLock<Prompts> = OnceLock::new();
    P.get_or_init(|| serde_json::from_str(PROMPTS).expect("prompts/minutes.json"))
}

/// A line of the transcript as the model reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Its index in the segments; the model sees it plus one
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub text: String,
}

/// The lines with text, with who says each when the transcript knows (`speakers`, one per segment).
pub fn lines(t: &Transcript, speakers: &[Option<String>]) -> Vec<Line> {
    t.segments
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.text.trim().is_empty())
        .map(|(i, s)| Line {
            index: i,
            start: s.start,
            end: s.end,
            speaker: speakers.get(i).cloned().flatten().filter(|n| !n.trim().is_empty()),
            text: s.text.split_whitespace().collect::<Vec<_>>().join(" "),
        })
        .collect()
}

/// Who says each line: the name the transcript gives its speaker ("Sara", or "Me" and "Them" in a
/// recorded meeting, whose two tracks are speakers too), else "Speaker 2"; none when the speakers
/// were not told apart.
pub fn speakers(t: &Transcript) -> Vec<Option<String>> {
    t.segments.iter().map(|s| s.speaker.map(|id| t.speaker_name(id))).collect()
}

/// In a recorded meeting, who is who, for the model: the speakers of the microphone's track recorded
/// it, those of the PC's track are the other side of the call (by the names the transcript gives
/// them). None for any other transcript.
pub fn who_is_who(t: &Transcript) -> Option<String> {
    let names = |track: &str| -> String {
        let mut out: Vec<String> = Vec::new();
        for s in t.segments.iter().filter(|s| s.track.as_deref() == Some(track) && !s.text.trim().is_empty()) {
            if let Some(name) = s.speaker.map(|id| t.speaker_name(id)).filter(|n| !out.contains(n)) {
                out.push(name);
            }
        }
        out.join(", ")
    };
    let (me, them) = (names("me"), names("them"));
    if me.is_empty() && them.is_empty() {
        return None;
    }
    let p = prompts();
    let mut said = vec![p.recorded.clone()];
    if !me.is_empty() {
        said.push(p.recorded_me.replace("{me}", &me));
    }
    if !them.is_empty() {
        said.push(p.recorded_them.replace("{them}", &them));
    }
    Some(said.join(" "))
}

/// The request with `note` first in what the model is asked (who is who in a recorded meeting).
pub fn with_note(mut a: Ask, note: Option<&str>) -> Ask {
    if let Some(note) = note {
        if let Some(text) = a.messages[1]["content"].as_str().map(|c| format!("{note}\n\n{c}")) {
            a.messages[1]["content"] = text.into();
        }
    }
    a
}

/// The lines as the model reads them: "[12] 03:41 Sara: text".
pub fn render(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|l| match &l.speaker {
            Some(who) => format!("[{}] {} {}: {}", l.index + 1, ts(l.start), who, l.text),
            None => format!("[{}] {} {}", l.index + 1, ts(l.start), l.text),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The JSON the answer must be, its line numbers held to those given (as the model sees them).
pub fn schema(lowest: usize, highest: usize) -> String {
    SCHEMA.replace("\"LOWEST\"", &lowest.to_string()).replace("\"HIGHEST\"", &highest.to_string())
}

fn name_of(code: &str) -> &str {
    crate::models::language_name(code)
}

fn language_note(source: &str, target: &str) -> String {
    let p = prompts();
    if source == target {
        p.same_language.clone()
    } else {
        p.other_language.replace("{language}", name_of(target))
    }
}

/// What to send: the messages, and the schema's text (sent as it is, so its fields keep their
/// order, which is the order the model writes them in).
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub messages: Value,
    pub schema: String,
}

fn ask(user: Vec<String>, lowest: usize, highest: usize) -> Ask {
    let p = prompts();
    Ask {
        messages: serde_json::json!([
            { "role": "system", "content": p.system },
            { "role": "user", "content": user.join("\n\n") },
        ]),
        schema: schema(lowest, highest),
    }
}

/// The minutes of a recording short enough for one pass.
pub fn ask_whole(lines: &[Line], source: &str, target: &str) -> Ask {
    let p = prompts();
    let highest = lines.last().map_or(1, |l| l.index + 1);
    ask(
        vec![
            p.transcript.clone(),
            render(lines),
            format!("{} {}", p.task.replace("{language}", name_of(target)), language_note(source, target)),
            p.fields.clone(),
        ],
        1,
        highest,
    )
}

/// Notes on one part (`part` of `parts`, from 1).
pub fn ask_part(part_lines: &[Line], part: usize, parts: usize, source: &str, target: &str) -> Ask {
    let p = prompts();
    let (first, last) = (&part_lines[0], &part_lines[part_lines.len() - 1]);
    let intro = p
        .part
        .replace("{part}", &part.to_string())
        .replace("{parts}", &parts.to_string())
        .replace("{from}", &ts(first.start))
        .replace("{to}", &ts(last.start));
    ask(
        vec![
            format!("{} {}", p.transcript, intro),
            render(part_lines),
            format!("{} {}", p.task_part.replace("{language}", name_of(target)), language_note(source, target)),
            p.fields.clone(),
        ],
        first.index + 1,
        last.index + 1,
    )
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

fn numbers(v: &Value) -> String {
    let n: Vec<String> = v.as_array().into_iter().flatten().filter_map(Value::as_u64).map(|n| n.to_string()).collect();
    format!("[{}]", n.join(", "))
}

/// The notes as the last pass reads them; `spans` are each part's first and last start time.
pub fn render_notes(notes: &[Value], spans: &[(f64, f64)]) -> String {
    let mut out = Vec::new();
    for (n, (note, (from, to))) in notes.iter().zip(spans).enumerate() {
        let kind = Some(str_of(note, "kind")).filter(|k| !k.is_empty()).unwrap_or("other");
        let mut block = vec![format!("Part {} ({} to {}), {}:", n + 1, ts(*from), ts(*to), kind), format!("Summary: {}", str_of(note, "summary"))];
        let list = |key: &str| note.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
        for t in list("topics") {
            block.push(format!("Topic [{}]: {}", t.get("line").and_then(Value::as_u64).unwrap_or(0), str_of(&t, "title")));
        }
        for d in list("decisions") {
            block.push(format!("Decision {}: {}", numbers(&d["lines"]), str_of(&d, "text")));
        }
        for a in list("actions") {
            let or_dash = |s: &str| if s.is_empty() { "-".to_string() } else { s.to_string() };
            block.push(format!(
                "Action {}: who: {}; what: {}; due: {}",
                numbers(&a["lines"]),
                or_dash(str_of(&a, "who")),
                str_of(&a, "what"),
                or_dash(str_of(&a, "due"))
            ));
        }
        for q in list("open_questions") {
            block.push(format!("Open question {}: {}", numbers(&q["lines"]), str_of(&q, "text")));
        }
        out.push(block.join("\n"));
    }
    out.join("\n\n")
}

/// The minutes from the notes on the parts; `highest` is the last line number of the recording.
pub fn ask_combine(notes: &[Value], spans: &[(f64, f64)], highest: usize, source: &str, target: &str) -> Ask {
    let p = prompts();
    ask(
        vec![
            p.notes.clone(),
            render_notes(notes, spans),
            format!("{} {}", p.task_combine.replace("{language}", name_of(target)), language_note(source, target)),
            p.fields.clone(),
        ],
        1,
        highest,
    )
}

/// The parts of a long recording: at most `part_s` seconds and `part_chars` of text each, cut where
/// the speaker changes or pauses longest within the last quarter of a part. Each part is the
/// range of `lines` it holds, end not included.
pub fn cut(lines: &[Line], part_s: f64, part_chars: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut first = 0;
    while first < lines.len() {
        let start = lines[first].start;
        let (mut size, mut last) = (0usize, first);
        while last < lines.len() && (last == first || (lines[last].start - start < part_s && size + chars(&lines[last].text) <= part_chars)) {
            size += chars(&lines[last].text);
            last += 1;
        }
        if last < lines.len() {
            let mut best = (last, -1.0f64);
            for i in first + 1..=last {
                if i != last && lines[i].start - start < 0.75 * part_s {
                    continue;
                }
                let gap = (lines[i].start - lines[i - 1].end).max(0.0);
                let score = gap + if lines[i].speaker != lines[i - 1].speaker { 2.0 } else { 0.0 };
                // of places as good, the latest: parts as long as they may be
                if score >= best.1 {
                    best = (i, score);
                }
            }
            last = best.0;
        }
        spans.push((first, last));
        first = last;
    }
    spans
}

// ---------------------------------------------------------------- what the model answers

/// The model's answer, checked against the transcript: line numbers that are not lines of it are
/// dropped, items without text go, topics come in order, an owner who is neither a speaker nor
/// said anywhere in the transcript is not kept, and only a meeting keeps decisions and actions.
/// `same_language`: the minutes are in the transcript's language, so names can be checked.
pub fn check(answer: &Value, t: &Transcript, lines: &[Line], same_language: bool) -> Minutes {
    let known = |n: u64| -> Option<usize> {
        let i = (n as usize).checked_sub(1)?;
        lines.iter().any(|l| l.index == i).then_some(i)
    };
    let line_list = |v: &Value| -> Vec<usize> {
        let mut out: Vec<usize> = v.as_array().into_iter().flatten().filter_map(Value::as_u64).filter_map(known).collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    let time = |ls: &[usize]| ls.first().and_then(|&i| t.segments.get(i)).map(|s| s.start);
    let list = |key: &str| answer.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let clean = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");

    let kind = match str_of(answer, "kind") {
        k @ ("meeting" | "lecture" | "interview") => k.to_string(),
        _ => "other".to_string(),
    };
    let meeting = kind == "meeting";
    let points = |key: &str| -> Vec<Point> {
        let mut out: Vec<Point> = Vec::new();
        for v in list(key) {
            let text = clean(str_of(&v, "text"));
            let ls = line_list(&v["lines"]);
            if text.is_empty() || out.iter().any(|p| p.text == text) {
                continue;
            }
            out.push(Point { time: time(&ls), text, lines: ls, edited: false });
        }
        out
    };
    let decisions = if meeting { points("decisions") } else { Vec::new() };
    let open_questions = points("open_questions");

    // the names a speaker has, and the words said: an owner must be one of them
    let said = fold(&t.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" "));
    let speaker_names: Vec<String> = lines.iter().filter_map(|l| l.speaker.as_deref()).map(fold).collect();
    let real_person = |who: &str| {
        let w = fold(who);
        w.is_empty()
            || !same_language
            || speaker_names.iter().any(|s| s.contains(&w) || w.contains(s.as_str()))
            || w.split(' ').filter(|p| p.chars().count() > 1).any(|p| said.contains(p))
    };
    let mut actions: Vec<Action> = Vec::new();
    if meeting {
        for v in list("actions") {
            let what = clean(str_of(&v, "what"));
            if what.is_empty() {
                continue;
            }
            let mut who = clean(str_of(&v, "who"));
            if !real_person(&who) {
                who.clear();
            }
            let due = clean(str_of(&v, "due"));
            let due = if is_none_said(&due) { String::new() } else { due };
            let ls = line_list(&v["lines"]);
            if actions.iter().any(|a| a.what == what && a.who == who) {
                continue;
            }
            actions.push(Action { time: time(&ls), who, what, due, lines: ls, edited: false });
        }
    }
    let mut topics: Vec<Topic> = Vec::new();
    for v in list("topics") {
        let title = clean(str_of(&v, "title"));
        let Some(line) = v.get("line").and_then(Value::as_u64).and_then(known) else { continue };
        if title.is_empty() || topics.iter().any(|x| x.line == line) {
            continue;
        }
        topics.push(Topic { title, line, time: t.segments[line].start, edited: false });
    }
    topics.sort_by_key(|x| x.line);
    Minutes {
        kind,
        summary: str_of(answer, "summary").to_string(),
        decisions,
        actions,
        topics,
        open_questions,
        ..Default::default()
    }
}

/// "none", "not given", "لا يوجد": a deadline that is none.
fn is_none_said(s: &str) -> bool {
    const NONE: &[&str] = &["-", "none", "n/a", "na", "not specified", "not given", "unspecified", "no deadline", "tbd", "لا يوجد", "غير محدد", "لم يحدد", "بدون"];
    let f = fold(s);
    f.is_empty() || NONE.iter().any(|n| fold(n) == f)
}

/// Text folded for comparing names: lower case, Arabic letter forms and diacritics made one.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ى' => 'ي',
            'ة' => 'ه',
            'ؤ' => 'و',
            'ئ' => 'ي',
            '\u{0640}' | '\u{064b}'..='\u{065f}' | '\u{0670}' => continue,
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The notes put together without the model, when the last pass fails: every part's items in
/// order, the same ones once, the summaries one after another, and a meeting if any part was one.
pub fn merge_notes(notes: &[Value]) -> Value {
    let mut merged = serde_json::json!({ "kind": "other", "topics": [], "decisions": [], "actions": [], "open_questions": [], "summary": "" });
    let mut summaries = Vec::new();
    let mut kinds: Vec<&str> = Vec::new();
    for note in notes {
        kinds.push(str_of(note, "kind"));
        let s = str_of(note, "summary");
        if !s.is_empty() {
            summaries.push(s.to_string());
        }
        for (key, field) in [("topics", "title"), ("decisions", "text"), ("actions", "what"), ("open_questions", "text")] {
            let items = note.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
            let into = merged[key].as_array_mut().expect("an array");
            for item in items {
                let text = fold(str_of(&item, field));
                if !into.iter().any(|x| fold(str_of(x, field)) == text) {
                    into.push(item);
                }
            }
        }
    }
    let kind = if kinds.contains(&"meeting") {
        "meeting"
    } else {
        // the kind most parts are
        ["lecture", "interview", "other"].into_iter().max_by_key(|k| kinds.iter().filter(|x| *x == k).count()).unwrap_or("other")
    };
    merged["kind"] = kind.into();
    merged["summary"] = summaries.join(" ").into();
    merged
}

/// Whether an answer from the model is one: an object with the fields the schema asks for.
pub fn is_answer(v: &Value) -> bool {
    v.is_object() && v.get("summary").is_some_and(Value::is_string) && v.get("kind").is_some()
}

/// The parts a run goes in, by their lines: one when it fits in one pass.
pub fn plan(lines: &[Line]) -> Vec<(usize, usize)> {
    let size: usize = lines.iter().map(|l| chars(&l.text)).sum();
    let span = lines.last().map_or(0.0, |l| l.end) - lines.first().map_or(0.0, |l| l.start);
    if lines.is_empty() {
        return Vec::new();
    }
    if span <= PART_S * 1.25 && size <= PART_CHARS {
        return vec![(0, lines.len())];
    }
    cut(lines, PART_S, PART_CHARS)
}

/// Letters, not bytes: an Arabic letter takes two, and the limits are the benchmark's (Python's).
fn chars(s: &str) -> usize {
    s.chars().count()
}

/// The notes in groups whose text fits one pass: the last pass reads one group's notes at a time.
/// A note that fits with neither neighbour is a group of its own.
pub fn groups(notes: &[Value], spans: &[(f64, f64)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut first = 0;
    while first < notes.len() {
        let mut last = first + 1;
        while last < notes.len() && chars(&render_notes(&notes[first..=last], &spans[first..=last])) <= NOTES_CHARS {
            last += 1;
        }
        out.push((first, last));
        first = last;
    }
    out
}

/// The notes on the parts of a long recording made into the minutes' answer: all of them in one
/// last pass when they fit, else a few at a time first. `ask` asks the model to combine notes
/// (`None`: its answer could not be read; such notes are merged without it). Every round joins
/// notes, so it ends: a note alone in its group is kept as it is, and notes no two of which fit one
/// pass together, or one note too long for the last pass, are merged without the model.
pub fn combine(mut notes: Vec<Value>, mut spans: Vec<(f64, f64)>, mut ask: impl FnMut(&[Value], &[(f64, f64)]) -> Result<Option<Value>>) -> Result<Value> {
    loop {
        let groups = groups(&notes, &spans);
        if groups.len() <= 1 {
            if chars(&render_notes(&notes, &spans)) > NOTES_CHARS {
                return Ok(merge_notes(&notes));
            }
            return Ok(ask(&notes, &spans)?.unwrap_or_else(|| merge_notes(&notes)));
        }
        if groups.len() == notes.len() {
            return Ok(merge_notes(&notes));
        }
        let (mut fewer, mut fewer_spans) = (Vec::new(), Vec::new());
        for (a, b) in groups {
            fewer.push(if b - a == 1 { notes[a].clone() } else { ask(&notes[a..b], &spans[a..b])?.unwrap_or_else(|| merge_notes(&notes[a..b])) });
            fewer_spans.push((spans[a].0, spans[b - 1].1));
        }
        notes = fewer;
        spans = fewer_spans;
    }
}

pub fn ensure_text(t: &Transcript) -> Result<()> {
    if t.segments.iter().all(|s| s.text.trim().is_empty()) {
        bail!(crate::tr!("This transcript has no text to write minutes from."));
    }
    Ok(())
}

// ---------------------------------------------------------------- exports

struct Words {
    minutes: &'static str,
    summary: &'static str,
    decisions: &'static str,
    actions: &'static str,
    topics: &'static str,
    questions: &'static str,
    due: &'static str,
    none: &'static str,
    transcript: &'static str,
    footer: &'static str,
}

/// The headings, in the language of the minutes (not of the interface).
fn words(language: &str) -> Words {
    match language {
        "ar" => Words {
            minutes: "محضر",
            summary: "الملخّص",
            decisions: "القرارات",
            actions: "المهام",
            topics: "المواضيع",
            questions: "أسئلة بلا جواب",
            due: "الموعد",
            none: "لا شيء",
            transcript: "النص الكامل",
            footer: "كُتب هذا المحضر على هذا الحاسوب بنموذج",
        },
        _ => Words {
            minutes: "Minutes",
            summary: "Summary",
            decisions: "Decisions",
            actions: "Action items",
            topics: "Topics",
            questions: "Open questions",
            due: "due",
            none: "None",
            transcript: "Transcript",
            footer: "Written on this computer by",
        },
    }
}

fn stamp(time: Option<f64>) -> String {
    time.map(|t| format!("[{}] ", ts(t))).unwrap_or_default()
}

fn action_line(a: &Action, w: &Words) -> String {
    let mut s = String::new();
    if !a.who.is_empty() {
        s.push_str(&a.who);
        s.push_str(": ");
    }
    s.push_str(&a.what);
    if !a.due.is_empty() {
        s.push_str(&format!(" ({} {})", w.due, a.due));
    }
    s
}

/// The minutes as plain sections: (heading, lines). Decisions and actions only for a meeting.
fn sections(m: &Minutes) -> Vec<(&'static str, Vec<String>)> {
    let w = words(&m.language);
    let mut out = vec![(w.summary, vec![m.summary.clone()])];
    let or_none = |v: Vec<String>| if v.is_empty() { vec![w.none.to_string()] } else { v };
    if m.kind == "meeting" {
        out.push((w.decisions, or_none(m.decisions.iter().map(|d| format!("{}{}", stamp(d.time), d.text)).collect())));
        out.push((w.actions, or_none(m.actions.iter().map(|a| format!("{}{}", stamp(a.time), action_line(a, &w))).collect())));
    }
    if !m.topics.is_empty() {
        out.push((w.topics, m.topics.iter().map(|t| format!("{} {}", ts(t.time), t.title)).collect()));
    }
    if !m.open_questions.is_empty() {
        out.push((w.questions, m.open_questions.iter().map(|q| format!("{}{}", stamp(q.time), q.text)).collect()));
    }
    out
}

/// The minutes alone, or followed by the transcript, as text, Markdown or Word.
pub fn export(t: &Transcript, m: &Minutes, ext: &str, with_transcript: bool) -> Result<Vec<u8>> {
    let w = words(&m.language);
    let footer = format!("{} {}.", w.footer, m.model);
    Ok(match ext {
        "txt" => {
            let mut out = format!("{} · {}\n\n", w.minutes, t.title);
            for (heading, items) in sections(m) {
                out.push_str(heading);
                out.push('\n');
                let bullet = heading != w.summary;
                for i in items {
                    out.push_str(if bullet { "- " } else { "" });
                    out.push_str(&i);
                    out.push('\n');
                }
                out.push('\n');
            }
            out.push_str(&footer);
            out.push('\n');
            if with_transcript {
                out.push_str(&format!("\n\n{}\n\n", w.transcript));
                out.push_str(&crate::transcript::to_txt(t, true));
            }
            out.into_bytes()
        }
        "md" => {
            let mut out = format!("# {} · {}\n\n", w.minutes, md(&t.title));
            for (heading, items) in sections(m) {
                out.push_str(&format!("## {heading}\n\n"));
                let bullet = heading != w.summary;
                for i in items {
                    out.push_str(&format!("{}{}\n", if bullet { "- " } else { "" }, md(&i)));
                }
                out.push('\n');
            }
            out.push_str(&format!("*{}*\n", md(&footer)));
            if with_transcript {
                out.push_str(&format!("\n## {}\n\n", w.transcript));
                for s in t.segments.iter().filter(|s| !s.text.trim().is_empty()) {
                    out.push_str(&format!("**[{}]** {}\n\n", ts(s.start), md(s.text.trim())));
                }
            }
            out.into_bytes()
        }
        "docx" => {
            let rtl = crate::models::is_right_to_left(&m.language);
            let mut body = crate::transcript::docx_paragraph(&format!("{} · {}", w.minutes, t.title), rtl, Some("Title"));
            for (heading, items) in sections(m) {
                body.push_str(&crate::transcript::docx_paragraph(heading, rtl, Some("Heading1")));
                let bullet = heading != w.summary;
                for i in items {
                    body.push_str(&crate::transcript::docx_paragraph(&format!("{}{}", if bullet { "• " } else { "" }, i), rtl, None));
                }
            }
            body.push_str(&crate::transcript::docx_paragraph(&footer, rtl, None));
            if with_transcript {
                let trtl = crate::models::is_right_to_left(&t.language);
                body.push_str(&crate::transcript::docx_paragraph(w.transcript, rtl, Some("Heading1")));
                for s in t.segments.iter().filter(|s| !s.text.trim().is_empty()) {
                    body.push_str(&crate::transcript::docx_paragraph(&format!("[{}] {}", ts(s.start), s.text.trim()), trtl, None));
                }
            }
            crate::transcript::docx_package(&body, rtl)?
        }
        _ => bail!("unknown format .{ext}"),
    })
}

/// Markdown that shows as written: the characters that would format it are escaped.
fn md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '#' | '<' | '>' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Segment;

    fn transcript(lines: &[(f64, &str)]) -> Transcript {
        let segments = lines
            .iter()
            .enumerate()
            .map(|(i, (start, text))| Segment {
                start: *start,
                end: lines.get(i + 1).map_or(start + 4.0, |n| n.0 - 0.3),
                text: text.to_string(),
                flag: None,
                edited: false,
                voice: vec![],
                words: vec![],
                pauses: vec![],
                heard: vec![],
                speaker: None,
                track: None,
            })
            .collect();
        Transcript {
            id: "t1".into(),
            title: "Weekly".into(),
            source_path: String::new(),
            created_ms: 0,
            duration_s: 60.0,
            language: "en".into(),
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
            speakers: vec![],
            detected: None,
        }
    }

    fn meeting() -> Transcript {
        transcript(&[
            (0.0, "Okay let's start, the launch first."),
            (6.0, ""),
            (9.0, "We move the launch to March 15."),
            (15.0, "Sara will fix the login bug by Thursday."),
            (21.0, "Who pays for the venue? Nobody knows yet."),
        ])
    }

    #[test]
    fn the_model_reads_numbered_lines_with_times_and_speakers() {
        let t = meeting();
        let names = vec![Some("Omar".to_string()), None, Some("Omar".into()), Some("Sara".into()), None];
        let ls = lines(&t, &names);
        assert_eq!(ls.len(), 4, "the empty line is left out");
        assert_eq!(
            render(&ls),
            "[1] 00:00 Omar: Okay let's start, the launch first.\n[3] 00:09 Omar: We move the launch to March 15.\n\
             [4] 00:15 Sara: Sara will fix the login bug by Thursday.\n[5] 00:21 Who pays for the venue? Nobody knows yet."
        );
        let a = ask_whole(&ls, "en", "en");
        let schema: Value = serde_json::from_str(&a.schema).unwrap();
        assert_eq!(schema["properties"]["decisions"]["items"]["properties"]["lines"]["items"]["maximum"], 5);
        assert!(a.schema.find("\"kind\"").unwrap() < a.schema.find("\"summary\"").unwrap(), "the schema keeps its order");
        let user = a.messages[1]["content"].as_str().unwrap();
        assert!(user.contains("Write the minutes of this recording in English. Keep names as the transcript writes them"));
        let a = ask_whole(&ls, "ar", "en");
        assert!(a.messages[1]["content"].as_str().unwrap().contains("Write names in English letters"));
    }

    /// The benchmark (tools/bench-minutes/minutes.py) asks in the same words: it wrote this file.
    #[test]
    fn the_app_asks_as_the_benchmark_measured() {
        let fixture: Value = serde_json::from_str(include_str!("../../tools/bench-minutes/same_as_app.json")).unwrap();
        let t = transcript(
            &fixture["lines"].as_array().unwrap().iter().map(|l| (l[0].as_f64().unwrap(), l[2].as_str().unwrap())).collect::<Vec<_>>(),
        );
        let names: Vec<Option<String>> = fixture["lines"].as_array().unwrap().iter().map(|l| Some(l[1].as_str().unwrap().to_string()).filter(|s| !s.is_empty())).collect();
        let ls = lines(&t, &names);
        let whole = ask_whole(&ls, "ar", "ar");
        assert_eq!(whole.messages, fixture["whole"]["messages"]);
        assert_eq!(serde_json::from_str::<Value>(&whole.schema).unwrap(), fixture["whole"]["schema"]);
        let part = ask_part(&ls[1..3], 2, 3, "ar", "en");
        assert_eq!(part.messages, fixture["part"]["messages"]);
        assert_eq!(serde_json::from_str::<Value>(&part.schema).unwrap(), fixture["part"]["schema"]);
        let notes = fixture["notes"].as_array().unwrap().clone();
        let spans = [(ls[0].start, ls[0].start), (ls[1].start, ls[2].start)];
        let combine = ask_combine(&notes, &spans, ls.last().unwrap().index + 1, "ar", "ar");
        assert_eq!(combine.messages, fixture["combine"]["messages"]);
    }

    /// Each model is told not to think the way the benchmark told it (the workflow's `kwargs`, else
    /// `enable_thinking: false`), and each is one the benchmark measured.
    #[test]
    fn every_model_is_asked_as_it_was_measured() {
        let workflow = include_str!("../../.github/workflows/minutes-bench.yml");
        for spec in MODELS {
            let key = spec.id.strip_prefix("min-").unwrap();
            let line = workflow.lines().find(|l| l.contains(&format!("key: {key},"))).unwrap_or_else(|| panic!("{key} was not measured"));
            let sent = line.split("kwargs: '").nth(1).map_or(r#"{"enable_thinking": false}"#, |k| k.split('\'').next().unwrap());
            assert_eq!(serde_json::from_str::<Value>(spec.no_thinking).unwrap(), serde_json::from_str::<Value>(sent).unwrap(), "{key}");
            let variant = crate::models::variant(spec.id).unwrap();
            let measured = format!("{}/resolve/{}/{}", variant.repository, variant.revision, variant.file);
            assert!(line.contains(&measured), "{key}: the file measured is the one downloaded, at its revision");
            assert!(spec.needs > variant.size, "{key}");
        }
    }

    #[test]
    fn speakers_are_named_as_the_transcript_names_them_and_a_recorded_meeting_says_who_is_who() {
        let mut t = meeting();
        t.speakers = vec![crate::transcript::Speaker { id: 1, name: "سامي".into() }, crate::transcript::Speaker { id: 2, name: String::new() }];
        t.segments[0].speaker = Some(1);
        t.segments[2].speaker = Some(2);
        let named = speakers(&t);
        assert_eq!(named[0].as_deref(), Some("سامي"));
        assert_eq!(named[2].as_deref(), Some("Speaker 2"));
        assert_eq!(named[3], None, "a line nobody was found for");
        assert_eq!(lines(&t, &named)[0].speaker.as_deref(), Some("سامي"));
        assert_eq!(who_is_who(&t), None, "not a recorded meeting");

        // a recorded meeting: the microphone's track is "Me", the PC's is "Them" (or names given)
        t.speakers = vec![crate::transcript::Speaker { id: 1, name: "Me".into() }, crate::transcript::Speaker { id: 2, name: "Lina".into() }];
        for (i, s) in t.segments.iter_mut().enumerate() {
            s.track = Some(if i % 2 == 0 { "me" } else { "them" }.into());
            s.speaker = Some(if i % 2 == 0 { 1 } else { 2 });
        }
        let note = who_is_who(&t).unwrap();
        assert!(note.contains("Me: the person who recorded") && note.contains("Lina: the other side"), "{note}");
        let a = with_note(ask_whole(&lines(&t, &speakers(&t)), "en", "en"), Some(&note));
        let user = a.messages[1]["content"].as_str().unwrap();
        assert!(user.starts_with(&note) && user.contains("Me: ") && user.contains("Lina: "));
        assert_eq!(with_note(ask_whole(&lines(&t, &[]), "en", "en"), None).messages, ask_whole(&lines(&t, &[]), "en", "en").messages);
    }

    #[test]
    fn a_long_recording_is_cut_where_the_speaker_changes_or_pauses() {
        // a line every 10 s for 25 minutes; the speaker changes at 8:20 and 17:50, a long pause at 9:30
        let mut ls: Vec<Line> = (0..150)
            .map(|i| Line { index: i, start: i as f64 * 10.0, end: i as f64 * 10.0 + 9.5, speaker: Some("A".into()), text: "word ".repeat(10) })
            .collect();
        for l in ls.iter_mut().skip(50) {
            l.speaker = Some("B".into());
        }
        for l in ls.iter_mut().skip(107) {
            l.speaker = Some("C".into());
        }
        ls[56].end -= 6.0; // a pause of 6.5 s after line 56
        let spans = cut(&ls, 600.0, 12_000);
        assert_eq!(spans[0], (0, 57), "a pause of 6.5 s at 9:30 wins over the speaker change at 8:20");
        assert_eq!(spans[1].0, 57);
        assert_eq!(spans[1].1, 107, "the next part ends where C starts");
        assert_eq!(spans.last().unwrap().1, 150, "every line is in a part");
        for (a, b) in spans.iter().zip(spans.iter().skip(1)) {
            assert_eq!(a.1, b.0, "parts follow each other without a gap");
        }
        // text that is too much for one part is cut sooner
        let dense: Vec<Line> = (0..20).map(|i| Line { index: i, start: i as f64, end: i as f64 + 0.9, speaker: None, text: "x".repeat(1000) }).collect();
        assert!(cut(&dense, 600.0, 4000).iter().all(|(a, b)| b - a <= 4));
        // with nothing to tell the places apart, a part is as long as it may be
        let even: Vec<Line> = (0..240).map(|i| Line { index: i, start: i as f64 * 10.0, end: i as f64 * 10.0 + 9.0, speaker: None, text: "x".repeat(40) }).collect();
        assert_eq!(cut(&even, 600.0, 12_000).len(), 4);
        // one pass when it fits
        assert_eq!(plan(&ls[..40]), vec![(0, 40)]);
        assert!(plan(&ls).len() >= 3);
        assert!(plan(&[]).is_empty());
    }

    #[test]
    fn the_answer_is_checked_against_the_transcript() {
        let t = meeting();
        let names = vec![Some("Omar".to_string()), None, Some("Omar".into()), Some("Sara".into()), Some("Lina".into())];
        let ls = lines(&t, &names);
        let answer = serde_json::json!({
            "kind": "meeting",
            "topics": [{ "title": "Venue", "line": 5 }, { "title": "Launch", "line": 1 }, { "title": "Gap", "line": 2 }, { "title": "", "line": 3 }],
            "decisions": [
                { "text": "Launch moved to March 15.", "lines": [3, 2, 99] },
                { "text": "  ", "lines": [3] },
                { "text": "Launch moved to March 15.", "lines": [3] },
            ],
            "actions": [
                { "who": "Sara", "what": "Fix the login bug", "due": "Thursday", "lines": [4] },
                { "who": "Khaled", "what": "Book the venue", "due": "none", "lines": [5] },
                { "who": "", "what": "", "due": "", "lines": [5] },
            ],
            "open_questions": [{ "text": "Who pays for the venue?", "lines": [5] }],
            "summary": "The launch moves to March 15.",
        });
        let m = check(&answer, &t, &ls, true);
        assert_eq!(m.kind, "meeting");
        assert_eq!(m.decisions.len(), 1, "empty and repeated items go");
        assert_eq!(m.decisions[0].lines, vec![2], "line 2 is empty and 99 does not exist: only line 3 (index 2) stays");
        assert_eq!(m.decisions[0].time, Some(9.0));
        assert_eq!(m.actions.len(), 2);
        assert_eq!(m.actions[0].who, "Sara");
        assert_eq!(m.actions[1].who, "", "Khaled is no speaker and is never said: not kept");
        assert_eq!(m.actions[1].due, "", "\"none\" is no deadline");
        assert_eq!(m.topics.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["Launch", "Venue"], "in order; a topic on no line goes");
        assert_eq!(m.topics[1].time, 21.0);
        assert_eq!(m.open_questions[0].lines, vec![4]);
        // in another language, names cannot be compared letter by letter: they are kept
        let m = check(&answer, &t, &ls, false);
        assert_eq!(m.actions[1].who, "Khaled");
        // a name said in the transcript is a person even when not a speaker
        let t2 = transcript(&[(0.0, "Khaled, can you book the venue?"), (5.0, "Sure.")]);
        let m = check(&serde_json::json!({ "kind": "meeting", "actions": [{ "who": "خالد", "what": "x", "due": "", "lines": [1] }, { "who": "Khaled", "what": "y", "due": "", "lines": [1] }] }), &t2, &lines(&t2, &[]), true);
        assert_eq!(m.actions[0].who, "", "not said");
        assert_eq!(m.actions[1].who, "Khaled");
    }

    #[test]
    fn only_a_meeting_has_decisions_and_actions() {
        let t = meeting();
        let ls = lines(&t, &[]);
        let answer = serde_json::json!({
            "kind": "lecture", "topics": [], "summary": "A talk.",
            "decisions": [{ "text": "x", "lines": [1] }], "actions": [{ "who": "", "what": "y", "due": "", "lines": [1] }],
            "open_questions": [],
        });
        let m = check(&answer, &t, &ls, true);
        assert!(m.decisions.is_empty() && m.actions.is_empty());
        assert_eq!(m.kind, "lecture");
        assert_eq!(check(&serde_json::json!({ "kind": "chat", "summary": "" }), &t, &ls, true).kind, "other");
        // no lists at all is no crash
        let m = check(&serde_json::json!({}), &t, &ls, true);
        assert!(m.topics.is_empty() && m.summary.is_empty());
    }

    #[test]
    fn notes_merge_without_the_model_when_the_last_pass_fails() {
        let notes = vec![
            serde_json::json!({ "kind": "meeting", "summary": "Part one.", "topics": [{ "title": "Launch", "line": 1 }], "decisions": [{ "text": "Launch on March 15.", "lines": [3] }], "actions": [], "open_questions": [] }),
            serde_json::json!({ "kind": "other", "summary": "Part two.", "topics": [{ "title": "Venue", "line": 40 }], "decisions": [{ "text": "launch on march 15", "lines": [41] }], "actions": [{ "who": "Sara", "what": "Book it", "due": "", "lines": [42] }], "open_questions": [] }),
        ];
        let m = merge_notes(&notes);
        assert_eq!(m["kind"], "meeting");
        assert_eq!(m["summary"], "Part one. Part two.");
        assert_eq!(m["decisions"].as_array().unwrap().len(), 1, "the same decision once");
        assert_eq!(m["topics"].as_array().unwrap().len(), 2);
        assert_eq!(m["actions"][0]["who"], "Sara");
        assert!(is_answer(&m));
        assert_eq!(merge_notes(&[serde_json::json!({ "kind": "lecture", "summary": "" })])["kind"], "lecture");
        // the notes as the last pass reads them
        let text = render_notes(&notes, &[(0.0, 590.0), (600.0, 1190.0)]);
        assert!(text.starts_with("Part 1 (00:00 to 09:50), meeting:\nSummary: Part one.\nTopic [1]: Launch\nDecision [3]: Launch on March 15."));
        assert!(text.contains("Action [42]: who: Sara; what: Book it; due: -"));
        // many parts are combined a group at a time
        let many: Vec<Value> = (0..120).map(|_| notes[1].clone()).collect();
        let spans: Vec<(f64, f64)> = (0..120).map(|i| (i as f64 * 600.0, i as f64 * 600.0 + 590.0)).collect();
        let g = groups(&many, &spans);
        assert!(g.len() > 1 && g[0].0 == 0 && g.last().unwrap().1 == 120);
        assert!(g.iter().all(|(a, b)| chars(&render_notes(&many[*a..*b], &spans[*a..*b])) <= NOTES_CHARS || b - a == 1));
    }

    /// Long Arabic notes on a meeting of hours: combining them ends, whatever the model answers.
    #[test]
    fn combining_notes_always_ends() {
        let note = |letters: usize| serde_json::json!({ "kind": "meeting", "summary": "ب".repeat(letters), "topics": [], "decisions": [], "actions": [], "open_questions": [] });
        let spans = |n: usize| -> Vec<(f64, f64)> { (0..n).map(|i| (i as f64 * 600.0, i as f64 * 600.0 + 590.0)).collect() };
        // letters are counted, not bytes: two notes of 5,000 Arabic letters (20,000 bytes) fit one pass
        assert_eq!(groups(&[note(5_000), note(5_000)], &spans(2)), [(0, 2)]);

        // no two fit one pass together: merged without the model, which is never asked
        let mut asked = 0;
        let m = combine(vec![note(8_000); 18], spans(18), |_, _| {
            asked += 1;
            Ok(None)
        })
        .unwrap();
        assert_eq!(asked, 0);
        assert_eq!(m["summary"].as_str().unwrap().chars().filter(|&c| c == 'ب').count(), 18 * 8_000);

        // a model that gives back as much as it was given: every round still joins notes
        let mut asked = 0;
        let m = combine(vec![note(5_000); 18], spans(18), |notes, _| {
            asked += 1;
            assert!(notes.len() >= 2, "a note alone is not sent again");
            Ok(Some(notes[0].clone()))
        })
        .unwrap();
        assert!(asked <= 18, "{asked} passes");
        assert!(is_answer(&m));

        // one note too long for the last pass: kept without the model
        let m = combine(vec![note(15_000)], spans(1), |_, _| panic!("not asked")).unwrap();
        assert_eq!(m["summary"].as_str().unwrap().chars().count(), 15_000);

        // the usual case: the model combines them
        let m = combine(vec![note(100), note(100)], spans(2), |notes, _| Ok(Some(serde_json::json!({ "kind": "meeting", "summary": format!("{} notes", notes.len()) })))).unwrap();
        assert_eq!(m["summary"], "2 notes");
    }

    /// The minutes change only their own field of the transcript's file: the speakers, the tracks
    /// and anything a later version adds are still there after they are written and corrected.
    #[test]
    fn writing_and_correcting_minutes_keeps_the_rest_of_the_file() {
        let dir = std::env::temp_dir().join(format!("dawwin-minutes-keep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut t = meeting();
        t.id = "keep-1".into();
        crate::history::save(&dir, &t).unwrap();
        let path = dir.join("keep-1.json");
        let mut raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        // 1.4's speakers and 1.5's tracks, and a field some later version adds
        raw["speakers"] = serde_json::json!([{ "id": 1, "name": "Sara" }]);
        raw["segments"][0]["speaker"] = 1.into();
        raw["segments"][0]["track"] = "me".into();
        raw["later"] = serde_json::json!({ "kept": true });
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();

        let answer = serde_json::json!({ "kind": "meeting", "summary": "S.", "decisions": [{ "text": "A", "lines": [1] }], "actions": [], "topics": [], "open_questions": [] });
        let m = Minutes { written_ms: 1, ..check(&answer, &t, &lines(&t, &[]), true) };
        crate::history::modify_minutes(&dir, "keep-1", |x| *x = Some(m.clone())).unwrap();
        let mut corrected = m.clone();
        corrected.summary = "Mine.".into();
        crate::history::modify_minutes(&dir, "keep-1", |x| x.get_or_insert_with(Minutes::default).apply_edits(corrected)).unwrap();

        let raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["speakers"][0]["name"], "Sara");
        assert_eq!(raw["segments"][0]["speaker"], 1);
        assert_eq!(raw["segments"][0]["track"], "me");
        assert_eq!(raw["later"]["kept"], true);
        let kept = crate::history::load(&dir, "keep-1").unwrap().minutes.unwrap();
        assert!(kept.summary == "Mine." && kept.summary_edited && kept.decisions[0].text == "A");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A part holds at most `PART_CHARS` letters, Arabic or not, as the benchmark cut them.
    #[test]
    fn parts_count_letters_not_bytes() {
        let segs: Vec<(f64, String)> = (0..20).map(|i| (i as f64 * 20.0, "ب".repeat(500))).collect();
        let t = transcript(&segs.iter().map(|(s, x)| (*s, x.as_str())).collect::<Vec<_>>());
        let ls = lines(&t, &[]);
        // 10,000 letters in under seven minutes: one pass (it was two by bytes)
        assert_eq!(plan(&ls), [(0, 20)]);
        let parts = cut(&ls, PART_S, 4_000);
        assert!(parts.iter().all(|&(a, b)| ls[a..b].iter().map(|l| chars(&l.text)).sum::<usize>() <= 4_000));
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn edits_are_marked_as_the_users_and_empty_items_go() {
        let t = meeting();
        let mut m = check(
            &serde_json::json!({ "kind": "meeting", "summary": "S.", "decisions": [{ "text": "A", "lines": [1] }, { "text": "B", "lines": [3] }], "actions": [], "topics": [], "open_questions": [] }),
            &t,
            &lines(&t, &[]),
            true,
        );
        assert!(!m.edited());
        let mut new = m.clone();
        new.decisions[0].text = "A, corrected".into();
        new.decisions[1].text = " ".into();
        m.apply_edits(new);
        assert_eq!(m.decisions.len(), 1);
        assert!(m.decisions[0].edited && m.decisions[0].lines == vec![0]);
        assert!(!m.summary_edited);
        assert!(m.edited());
        let mut new = m.clone();
        new.summary = "Mine.".into();
        m.apply_edits(new);
        assert!(m.summary_edited && m.summary == "Mine.");
    }

    #[test]
    fn the_fingerprint_changes_with_any_line() {
        let mut t = meeting();
        let a = fingerprint(&t);
        assert_eq!(a.len(), 16);
        t.segments[3].text.push('!');
        assert_ne!(fingerprint(&t), a);
    }

    #[test]
    fn exports_have_the_minutes_alone_or_with_the_transcript() {
        let mut t = meeting();
        let mut m = check(
            &serde_json::json!({
                "kind": "meeting", "summary": "The launch moves.", "topics": [{ "title": "Launch", "line": 1 }],
                "decisions": [{ "text": "Launch on March 15.", "lines": [3] }],
                "actions": [{ "who": "Sara", "what": "Fix the login bug", "due": "Thursday", "lines": [4] }],
                "open_questions": [],
            }),
            &t,
            &lines(&t, &[]),
            true,
        );
        m.language = "en".into();
        m.model = "Test model".into();
        let txt = String::from_utf8(export(&t, &m, "txt", false).unwrap()).unwrap();
        assert!(txt.starts_with("Minutes · Weekly\n\nSummary\nThe launch moves.\n\nDecisions\n- [00:09] Launch on March 15.\n\nAction items\n- [00:15] Sara: Fix the login bug (due Thursday)\n"));
        assert!(txt.contains("Topics\n- 00:00 Launch\n"));
        assert!(!txt.contains("Open questions"), "no empty heading");
        assert!(txt.trim_end().ends_with("Written on this computer by Test model."));
        let both = String::from_utf8(export(&t, &m, "txt", true).unwrap()).unwrap();
        assert!(both.contains("Transcript\n\n[00:00 - 00:05] Okay let's start"));
        let md_text = String::from_utf8(export(&t, &m, "md", true).unwrap()).unwrap();
        assert!(md_text.contains("## Decisions\n\n- \\[00:09\\] Launch on March 15.\n"));
        assert!(md_text.contains("**[00:09]** We move the launch to March 15."));
        let docx = export(&t, &m, "docx", true).unwrap();
        assert!(docx.starts_with(b"PK"));
        // an Arabic lecture: Arabic headings, no decisions or actions
        t.language = "ar".into();
        m.language = "ar".into();
        m.kind = "lecture".into();
        let txt = String::from_utf8(export(&t, &m, "txt", false).unwrap()).unwrap();
        assert!(txt.starts_with("محضر · Weekly\n\nالملخّص\n"));
        assert!(!txt.contains("القرارات") && !txt.contains("المهام"));
        assert!(export(&t, &m, "srt", false).is_err());
    }

    #[test]
    fn names_fold_for_comparing() {
        assert_eq!(fold("أحمد"), fold("احمد"));
        assert_eq!(fold("سارة"), "ساره");
        assert_eq!(fold("Sara,  Omar!"), "sara omar");
        assert!(is_none_said("غير محدد") && is_none_said("None") && !is_none_said("Thursday"));
    }
}
