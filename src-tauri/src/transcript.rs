//! Transcripts: the data, the model's repetition loops, and the export formats.

use std::io::Write;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    /// seconds from the start of the recording
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// "repeat": the model looped here and the repetition was cut; check this line
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    #[serde(default)]
    pub edited: bool,
    /// Where the speech is in the line (see `audio::voice_profile`), to find a word in the audio
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voice: Vec<f32>,
    /// When each word starts: (character offset in `text`, seconds from the start of the recording)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<(u32, f32)>,
    /// Where the speaker stops in the line (see `audio::pauses`), for subtitles that end when they do
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pauses: Vec<[f32; 3]>,
    /// Who says it: one of the transcript's `speakers`, when they were told apart
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<u32>,
    /// What the timing pass heard in each stretch of the line between two pauses: (speech starts,
    /// speech ends, rough text). Only used to place the words in time (see `timing`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heard: Vec<(f32, f32, String)>,
    /// In a recorded meeting, whose track the line is from: "me" (the microphone) or "them" (what
    /// the PC played). Its speaker says the same for the reader ("Me", "Them", or one of them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub id: String,
    pub title: String,
    pub source_path: String,
    pub created_ms: u64,
    pub duration_s: f64,
    pub language: String,
    #[serde(default)]
    pub elapsed_s: f64,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub model: String,
    /// cancelled before the end
    #[serde(default)]
    pub partial: bool,
    /// The words of its lines were placed in time by the timing pass (`timing`), for subtitles
    /// and Follow words. Transcripts made without subtitles, and those of older versions, are not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub timed: bool,
    /// The files Dawwin wrote for it (the sound, the video with subtitles, exports): a folder run
    /// never takes them for recordings to transcribe
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
    /// `outputs` is kept for it (from 1.3.2 on): what is not in it is not Dawwin's
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outputs_listed: bool,
    pub segments: Vec<Segment>,
    /// The transcript in other languages, line by line
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translations: Vec<Translation>,
    /// Its minutes: summary, decisions, action items (see `minutes`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<crate::minutes::Minutes>,
    /// The people who speak in it, when they were told apart (see `speakers`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speakers: Vec<Speaker>,
    /// The language Dawwin heard, when it was asked to find it rather than told
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected: Option<String>,
}

/// Someone who speaks in a transcript. Numbered from 1 in the order they first speak; the name is
/// the one the user gave, empty until then ("Speaker 2").
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Speaker {
    pub id: u32,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Translation {
    pub language: String,
    /// The model that translated it
    pub model: String,
    /// One per segment, in the same order: empty where that line is not translated yet
    pub lines: Vec<TranslatedLine>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TranslatedLine {
    pub text: String,
    /// The line as it was when it was translated: when it reads differently now, it changed since
    pub from: String,
    #[serde(default)]
    pub edited: bool,
}

impl Translation {
    pub fn new(language: &str, model: &str) -> Self {
        Translation { language: language.into(), model: model.into(), lines: Vec::new() }
    }

    /// Whether line `i` holds a translation of what the line says now.
    pub fn is_current(&self, i: usize, segment: &Segment) -> bool {
        self.lines.get(i).is_some_and(|l| !l.text.trim().is_empty() && l.from == segment.text)
    }

    pub fn text(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", |l| l.text.trim())
    }
}

impl Transcript {
    pub fn translation(&self, language: &str) -> Option<&Translation> {
        self.translations.iter().find(|t| t.language == language)
    }

    /// The same transcript, in that language: what the export formats write for "the translation".
    pub fn translated(&self, language: &str) -> Option<Transcript> {
        let tr = self.translation(language)?;
        let mut t = self.clone();
        t.language = tr.language.clone();
        t.model = tr.model.clone();
        t.translations.clear();
        for (i, s) in t.segments.iter_mut().enumerate() {
            // with the words of the line timed, a pause is known to fall after that share of its
            // text: the translation places it after the same share of its own
            let len = s.text.chars().count().max(1) as f32;
            for p in s.pauses.iter_mut() {
                if let Some(w) = s.words.iter().find(|w| w.1 >= p[2] - 0.05) {
                    p[0] = w.0 as f32 / len;
                }
            }
            s.text = tr.text(i).to_string();
            s.words.clear();
            s.heard.clear();
            s.flag = None;
        }
        Some(t)
    }
}

/// What an export holds: the transcript, its translation, or both, line by line.
#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Original,
    Translation(String),
    Both(String),
}

impl View {
    pub fn parse(view: Option<&str>, language: Option<&str>) -> View {
        match (view, language) {
            (Some("translation"), Some(l)) => View::Translation(l.to_string()),
            (Some("both"), Some(l)) => View::Both(l.to_string()),
            _ => View::Original,
        }
    }
}

// ---------------------------------------------------------------- loops

/// A run where the same 1–4 words repeat back to back: (first word, words per unit, repeats).
pub fn find_loop(text: &str) -> Option<(usize, usize, usize)> {
    let w: Vec<&str> = text.split_whitespace().collect();
    for n in 1..=4usize {
        let need = if n == 1 { 6 } else if n == 2 { 5 } else { 4 };
        let mut i = 0;
        while i + n * need <= w.len() {
            let mut reps = 1;
            while i + (reps + 1) * n <= w.len() && w[i + reps * n..i + (reps + 1) * n] == w[i..i + n] {
                reps += 1;
            }
            if reps >= need {
                return Some((i, n, reps));
            }
            i += 1;
        }
    }
    None
}

/// Cut every loop down to two repeats and mark the cut with "…". A loop that runs to
/// the end of the text also loses the half word the model stopped in.
pub fn collapse_loops(text: &str) -> String {
    let mut words: Vec<String> = text.split_whitespace().map(String::from).collect();
    while let Some((i, n, reps)) = find_loop(&words.join(" ")) {
        let run_end = i + reps * n;
        let mut tail: Vec<String> = words[run_end..].to_vec();
        // the model usually stops mid-word in a loop: "ست ست ست س"
        if tail.len() == 1 && words[i..i + n].iter().any(|u| u.starts_with(tail[0].as_str())) {
            tail.clear();
        }
        let mut out: Vec<String> = words[..i + 2 * n].to_vec();
        out.push("…".into());
        out.extend(tail);
        words = out;
    }
    words.join(" ")
}

// ---------------------------------------------------------------- formats

pub fn ts(sec: f64) -> String {
    let s = sec.max(0.0).floor() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn ts_ms(sec: f64, sep: char) -> String {
    let ms = (sec.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, sep, ms % 1000)
}

fn visible(t: &Transcript) -> impl Iterator<Item = &Segment> {
    t.segments.iter().filter(|s| !s.text.trim().is_empty())
}

/// The lines with text, by their index, each with its speaker's name where a turn starts: where
/// the speaker changes, not on every line.
fn turns(t: &Transcript) -> Vec<(usize, &Segment, Option<String>)> {
    let mut last = None;
    t.segments
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.text.trim().is_empty())
        .map(|(i, s)| {
            let name = s.speaker.filter(|x| Some(*x) != last).map(|x| t.speaker_name(x));
            last = s.speaker.or(last);
            (i, s, name)
        })
        .collect()
}

/// The speaker of a line, for a column of their own: empty for a line nobody was given.
fn speaker_cell(t: &Transcript, s: &Segment) -> String {
    s.speaker.map(|x| t.speaker_name(x)).unwrap_or_default()
}

pub fn to_txt(t: &Transcript, stamps: bool) -> String {
    let mut out = String::new();
    for (_, s, name) in turns(t) {
        if stamps {
            out.push_str(&format!("[{} - {}] ", ts(s.start), ts(s.end)));
        }
        if let Some(name) = name {
            out.push_str(&format!("{name}: "));
        }
        out.push_str(s.text.trim());
        out.push('\n');
        if !stamps {
            out.push('\n');
        }
    }
    out
}

pub fn to_md(t: &Transcript, stamps: bool) -> String {
    let mut out = format!("# {}\n\n", t.title);
    out.push_str(&format!(
        "- {}: {}\n- {}: {}\n- {}: {}\n\n",
        crate::tr!("Duration"),
        ts(t.duration_s),
        crate::tr!("Language"),
        match t.language.as_str() {
            "en" => crate::tr!("English"),
            "ar" => crate::tr!("Arabic"),
            other => crate::models::language_name(other).to_string(),
        },
        crate::tr!("Model"),
        t.model
    ));
    for (_, s, name) in turns(t) {
        if stamps {
            out.push_str(&format!("**[{}]** ", ts(s.start)));
        }
        if let Some(name) = name {
            out.push_str(&format!("**{name}:** "));
        }
        out.push_str(s.text.trim());
        out.push_str("\n\n");
    }
    out
}

/// Whether a text reads right to left: its first letter of a strong direction is Arabic or Hebrew.
fn reads_right_to_left(text: &str) -> bool {
    text.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(|c| matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'))
}

/// With the names in subtitles asked for (see `speakers::names_in_srt`), a subtitle where the
/// speaker changes starts with the name: "Sami: …". Before a right-to-left line, a right-to-left
/// mark comes first and after the colon: players (and libass, burning in) take the line's direction
/// from its first letter, and "Sami" would lay the Arabic out from the left, its question mark and
/// full stop on the wrong side.
fn srt(t: &Transcript, cues: Vec<crate::captions::Cue>) -> String {
    let named = crate::speakers::names_in_srt() && t.has_speakers();
    let mut out = String::new();
    let mut last = None;
    for (i, c) in cues.into_iter().enumerate() {
        let mut text = c.text();
        if named {
            if let Some(x) = c.speaker.filter(|x| Some(*x) != last) {
                let rlm = if reads_right_to_left(&text) { "\u{200F}" } else { "" };
                text = format!("{rlm}{}:{rlm} {text}", t.speaker_name(x));
            }
            last = c.speaker.or(last);
        }
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, ts_ms(c.start, ','), ts_ms(c.end, ','), text));
    }
    out
}

/// WebVTT carries the speaker in its voice tag, `<v Sami>`, on each of their subtitles.
fn vtt(t: &Transcript, cues: Vec<crate::captions::Cue>) -> String {
    let named = crate::speakers::names_in_vtt() && t.has_speakers();
    let mut out = String::from("WEBVTT\n\n");
    for c in cues {
        let voice = match c.speaker.filter(|_| named) {
            Some(x) => format!("<v {}>", vtt_text(&t.speaker_name(x))),
            None => String::new(),
        };
        out.push_str(&format!("{} --> {}\n{voice}{}\n\n", ts_ms(c.start, '.'), ts_ms(c.end, '.'), vtt_text(&c.text())));
    }
    out
}

/// Text as WebVTT shows it: `<` would start a tag, `&` a character name, and `-->` a timing line.
fn vtt_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A cell a spreadsheet keeps as text: one that starts with `=`, `+`, `-` or `@` would be taken
/// for a formula, so it gets an apostrophe in front, which spreadsheets take to mean "text".
fn text_cell(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@']) {
        format!("'{text}")
    } else {
        text.to_string()
    }
}

/// A quoted CSV cell.
fn csv_cell(text: &str) -> String {
    format!("\"{}\"", text_cell(text.trim()).replace('"', "\"\""))
}

/// A TSV cell, on one line; one that starts with a quote is quoted, or it would be read as quoted.
fn tsv_cell(text: &str) -> String {
    let flat = text_cell(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    if flat.starts_with('"') {
        format!("\"{}\"", flat.replace('"', "\"\""))
    } else {
        flat
    }
}
/// Subtitles timed and broken as professional ones are: see `captions`.
pub fn to_srt(t: &Transcript) -> String {
    srt(t, crate::captions::cues(t))
}

pub fn to_vtt(t: &Transcript) -> String {
    vtt(t, crate::captions::cues(t))
}

fn no_translation() -> anyhow::Error {
    anyhow::anyhow!(crate::tr!("There is no translation into that language."))
}

/// An export of the transcript, its translation, or the two together.
pub fn export_view(t: &Transcript, ext: &str, stamps: bool, view: &View) -> Result<Vec<u8>> {
    let language = match view {
        View::Original => return export_bytes(t, ext, stamps),
        View::Translation(l) => return export_bytes(&t.translated(l).ok_or_else(no_translation)?, ext, stamps),
        View::Both(l) => l,
    };
    let tr = t.translation(language).ok_or_else(no_translation)?;
    let second: Vec<String> = (0..t.segments.len()).map(|i| tr.text(i).to_string()).collect();
    let lines = || t.segments.iter().enumerate().filter(|(_, s)| !s.text.trim().is_empty());
    let named = t.has_speakers();
    Ok(match ext {
        "txt" => {
            let mut out = String::new();
            for (i, s, name) in turns(t) {
                if stamps {
                    out.push_str(&format!("[{} - {}] ", ts(s.start), ts(s.end)));
                }
                if let Some(name) = name {
                    out.push_str(&format!("{name}: "));
                }
                out.push_str(&format!("{}\n{}\n\n", s.text.trim(), second[i]));
            }
            out.into_bytes()
        }
        "md" => {
            let mut out = format!("# {}\n\n", t.title);
            for (i, s, name) in turns(t) {
                if stamps {
                    out.push_str(&format!("**[{}]** ", ts(s.start)));
                }
                if let Some(name) = name {
                    out.push_str(&format!("**{name}:** "));
                }
                out.push_str(&format!("{}  \n{}\n\n", s.text.trim(), second[i]));
            }
            out.into_bytes()
        }
        "srt" => srt(t, crate::captions::bilingual_cues(t, tr)).into_bytes(),
        "vtt" => vtt(t, crate::captions::bilingual_cues(t, tr)).into_bytes(),
        "docx" => to_docx_with(t, Some((&tr.language, &second)), stamps)?,
        "csv" => {
            let mut out = String::from(if named { "\u{feff}start,end,speaker,text,translation\r\n" } else { "\u{feff}start,end,text,translation\r\n" });
            for (i, s) in lines() {
                let who = if named { format!("{},", csv_cell(&speaker_cell(t, s))) } else { String::new() };
                out.push_str(&format!("{},{},{who}{},{}\r\n", secs(s.start), secs(s.end), csv_cell(&s.text), csv_cell(&second[i])));
            }
            out.into_bytes()
        }
        "tsv" => {
            let mut out = String::from(if named { "start\tend\tspeaker\ttext\ttranslation\n" } else { "start\tend\ttext\ttranslation\n" });
            for (i, s) in lines() {
                let who = if named { format!("{}\t", tsv_cell(&speaker_cell(t, s))) } else { String::new() };
                out.push_str(&format!("{}\t{}\t{who}{}\t{}\n", secs(s.start), secs(s.end), tsv_cell(&s.text), tsv_cell(&second[i])));
            }
            out.into_bytes()
        }
        "json" => {
            let mut doc: serde_json::Value = serde_json::from_str(&to_json(t))?;
            doc["translation_language"] = tr.language.clone().into();
            doc["translation_model"] = tr.model.clone().into();
            let texts: Vec<&String> = lines().map(|(i, _)| &second[i]).collect();
            if let Some(segs) = doc["segments"].as_array_mut() {
                for (seg, text) in segs.iter_mut().zip(texts) {
                    seg["translation"] = text.clone().into();
                }
            }
            (serde_json::to_string_pretty(&doc)? + "\n").into_bytes()
        }
        _ => anyhow::bail!("unknown format .{ext}"),
    })
}

/// A transcript in one of the export formats, by file extension.
pub fn export_bytes(t: &Transcript, ext: &str, stamps: bool) -> Result<Vec<u8>> {
    Ok(match ext {
        "txt" => to_txt(t, stamps).into_bytes(),
        "md" => to_md(t, stamps).into_bytes(),
        "srt" => to_srt(t).into_bytes(),
        "vtt" => to_vtt(t).into_bytes(),
        "docx" => to_docx(t, stamps)?,
        "csv" => to_csv(t).into_bytes(),
        "tsv" => to_tsv(t).into_bytes(),
        "json" => to_json(t).into_bytes(),
        _ => anyhow::bail!("unknown format .{ext}"),
    })
}

/// Seconds with three decimals, for the formats programs read.
fn secs(sec: f64) -> String {
    format!("{:.3}", sec.max(0.0))
}

/// Opens in Excel with the Arabic intact: UTF-8 with a BOM, quoted where needed.
/// With speakers told apart, a column of who says each line comes before its text.
pub fn to_csv(t: &Transcript) -> String {
    let named = t.has_speakers();
    let mut out = String::from(if named { "\u{feff}start,end,speaker,text\r\n" } else { "\u{feff}start,end,text\r\n" });
    for s in visible(t) {
        let who = if named { format!("{},", csv_cell(&speaker_cell(t, s))) } else { String::new() };
        out.push_str(&format!("{},{},{who}{}\r\n", secs(s.start), secs(s.end), csv_cell(&s.text)));
    }
    out
}

pub fn to_tsv(t: &Transcript) -> String {
    let named = t.has_speakers();
    let mut out = String::from(if named { "start\tend\tspeaker\ttext\n" } else { "start\tend\ttext\n" });
    for s in visible(t) {
        let who = if named { format!("{}\t", tsv_cell(&speaker_cell(t, s))) } else { String::new() };
        out.push_str(&format!("{}\t{}\t{who}{}\n", secs(s.start), secs(s.end), tsv_cell(&s.text)));
    }
    out
}

/// The words of a line with their start times: `words` holds character offsets into the text.
fn timed_words(s: &Segment) -> Vec<serde_json::Value> {
    let chars: Vec<char> = s.text.chars().collect();
    let mut out = Vec::with_capacity(s.words.len());
    for (i, &(at, start)) in s.words.iter().enumerate() {
        let from = (at as usize).min(chars.len());
        let to = s.words.get(i + 1).map_or(chars.len(), |w| (w.0 as usize).min(chars.len())).max(from);
        let word: String = chars[from..to].iter().collect();
        let word = word.trim();
        if !word.is_empty() {
            out.push(serde_json::json!({ "start": (start as f64 * 1000.0).round() / 1000.0, "word": word }));
        }
    }
    out
}

pub fn to_json(t: &Transcript) -> String {
    let round = |x: f64| (x * 1000.0).round() / 1000.0;
    let segments: Vec<serde_json::Value> = visible(t)
        .map(|s| {
            let mut v = serde_json::json!({ "start": round(s.start), "end": round(s.end), "text": s.text.trim() });
            if let Some(x) = s.speaker {
                v["speaker"] = t.speaker_name(x).into();
            }
            if let Some(track) = &s.track {
                v["track"] = track.clone().into();
            }
            let words = timed_words(s);
            if !words.is_empty() {
                v["words"] = words.into();
            }
            v
        })
        .collect();
    let mut doc = serde_json::json!({
        "title": t.title,
        "language": t.language,
        "duration": round(t.duration_s),
        "model": t.model,
        "segments": segments,
    });
    if t.has_speakers() {
        let mut ids: Vec<u32> = t.segments.iter().filter_map(|s| s.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        doc["speakers"] = ids.into_iter().map(|x| t.speaker_name(x)).collect::<Vec<_>>().into();
    }
    serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A Word document, with right-to-left paragraphs where the language needs them.
pub fn to_docx(t: &Transcript, stamps: bool) -> Result<Vec<u8>> {
    to_docx_with(t, None, stamps)
}

/// With `second` (a language and one line per segment), each line's translation follows it, in grey.
fn to_docx_with(t: &Transcript, second: Option<(&str, &[String])>, stamps: bool) -> Result<Vec<u8>> {
    let rtl = crate::models::is_right_to_left(&t.language);
    let ppr = if rtl { "<w:pPr><w:bidi/><w:spacing w:after=\"160\" w:line=\"312\" w:lineRule=\"auto\"/></w:pPr>" } else { "<w:pPr><w:spacing w:after=\"160\"/></w:pPr>" };
    let text_rpr = if rtl { "<w:rPr><w:rtl/><w:lang w:bidi=\"ar-SY\"/></w:rPr>" } else { "" };
    // the original keeps its line close to its translation
    let ppr = if second.is_some() { ppr.replace("w:after=\"160\"", "w:after=\"40\"") } else { ppr.to_string() };
    let below = second.map(|(language, lines)| {
        let rtl = crate::models::is_right_to_left(language);
        let ppr = if rtl { "<w:pPr><w:bidi/><w:spacing w:after=\"220\"/></w:pPr>" } else { "<w:pPr><w:spacing w:after=\"220\"/></w:pPr>" };
        let rpr = if rtl { "<w:rPr><w:rtl/><w:color w:val=\"4B5563\"/></w:rPr>" } else { "<w:rPr><w:color w:val=\"4B5563\"/></w:rPr>" };
        (ppr, rpr, lines)
    });
    let mut body = String::new();
    body.push_str(&format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Title\"/>{}</w:pPr><w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        if rtl { "<w:bidi/>" } else { "" },
        text_rpr,
        xml(&t.title)
    ));
    body.push_str(&format!(
        "<w:p><w:pPr>{}</w:pPr><w:r><w:rPr><w:color w:val=\"6B7280\"/><w:sz w:val=\"18\"/><w:szCs w:val=\"18\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        if rtl { "<w:bidi/>" } else { "" },
        xml(&crate::tr!("Duration {} · {}", ts(t.duration_s), &t.model))
    ));
    let bold = if rtl { "<w:rPr><w:b/><w:bCs/><w:rtl/><w:lang w:bidi=\"ar-SY\"/></w:rPr>" } else { "<w:rPr><w:b/><w:bCs/></w:rPr>" };
    for (i, s, name) in turns(t) {
        body.push_str("<w:p>");
        body.push_str(&ppr);
        if stamps {
            body.push_str(&format!(
                "<w:r><w:rPr><w:rtl w:val=\"0\"/><w:color w:val=\"0F766E\"/><w:sz w:val=\"18\"/><w:szCs w:val=\"18\"/></w:rPr><w:t xml:space=\"preserve\">[{}]</w:t></w:r><w:r>{}<w:t xml:space=\"preserve\"> </w:t></w:r>",
                ts(s.start),
                text_rpr
            ));
        }
        if let Some(name) = name {
            body.push_str(&format!("<w:r>{bold}<w:t xml:space=\"preserve\">{}: </w:t></w:r>", xml(&name)));
        }
        body.push_str(&format!("<w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r>", text_rpr, xml(s.text.trim())));
        body.push_str("</w:p>");
        if let Some((ppr, rpr, lines)) = &below {
            let text = lines.get(i).map_or("", |l| l.trim());
            body.push_str(&format!("<w:p>{ppr}<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml(text)));
        }
    }
    docx_package(&body, rtl)
}

/// One paragraph of a Word document, right to left when `rtl`, in a style of [`docx_package`]'s.
pub(crate) fn docx_paragraph(text: &str, rtl: bool, style: Option<&str>) -> String {
    let style = style.map(|s| format!("<w:pStyle w:val=\"{s}\"/>")).unwrap_or_default();
    let (bidi, rpr) = if rtl { ("<w:bidi/>", "<w:rPr><w:rtl/></w:rPr>") } else { ("", "") };
    format!("<w:p><w:pPr>{style}{bidi}</w:pPr><w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml(text))
}

/// A Word document around `body` (its paragraphs), with the styles Title and Heading1.
pub(crate) fn docx_package(body: &str, rtl: bool) -> Result<Vec<u8>> {
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}\
<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/>{}</w:sectPr></w:body></w:document>",
        if rtl { "<w:bidi/>" } else { "" }
    );
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Segoe UI\" w:hAnsi=\"Segoe UI\" w:cs=\"Segoe UI\" w:eastAsia=\"Segoe UI\"/>\
<w:sz w:val=\"24\"/><w:szCs w:val=\"26\"/><w:lang w:val=\"en-US\" w:bidi=\"ar-SY\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"120\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/>\
<w:pPr><w:spacing w:after=\"80\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"36\"/><w:szCs w:val=\"36\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/>\n<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"80\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"28\"/><w:szCs w:val=\"28\"/></w:rPr></w:style>\n</w:styles>";
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
</Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
</Relationships>";

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in [
            ("[Content_Types].xml", content_types.to_string()),
            ("_rels/.rels", rels.to_string()),
            ("word/_rels/document.xml.rels", doc_rels.to_string()),
            ("word/styles.xml", styles.to_string()),
            ("word/document.xml", document),
        ] {
            z.start_file(name, opt)?;
            z.write_all(data.as_bytes())?;
        }
        z.finish()?;
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_collapses_the_known_loop() {
        let t = "ناجح 20 20 ما في ست ست ست ست ست ست ست ست ست ست ست ست ست ست س";
        let (i, n, reps) = find_loop(t).unwrap();
        assert_eq!((n, reps), (1, 14));
        assert_eq!(t.split_whitespace().nth(i), Some("ست"));
        assert_eq!(collapse_loops(t), "ناجح 20 20 ما في ست ست …");
    }

    #[test]
    fn ordinary_speech_is_not_a_loop() {
        assert!(find_loop("لا لا لا لا ما بدي هيك شو اسمه شو اسمه").is_none());
        assert!(find_loop("").is_none());
    }

    #[test]
    fn finds_phrase_loops() {
        let t = "قال ما في مشكلة ما في مشكلة ما في مشكلة ما في مشكلة ما في مشكلة وخلص";
        let (_, n, _) = find_loop(t).unwrap();
        assert_eq!(n, 3);
        assert_eq!(collapse_loops(t), "قال ما في مشكلة ما في مشكلة … وخلص");
    }

    #[test]
    fn timestamps() {
        assert_eq!(ts(0.0), "00:00");
        assert_eq!(ts(83.9), "01:23");
        assert_eq!(ts(3725.0), "1:02:05");
        assert_eq!(ts_ms(83.25, ','), "00:01:23,250");
    }

    fn sample() -> Transcript {
        Transcript {
            id: "t1".into(),
            title: "تعديلات".into(),
            source_path: String::new(),
            created_ms: 0,
            duration_s: 40.0,
            language: "ar".into(),
            elapsed_s: 1.0,
            device: "cuda".into(),
            model: "m".into(),
            partial: false,
            timed: false,
            outputs: vec![],
            outputs_listed: false,
            segments: vec![
                Segment { start: 0.0, end: 22.0, text: "يعني مثلا بيجيك اخر شيء ثلاث زرار".into(), flag: None, edited: false, voice: vec![], words: vec![], pauses: vec![], heard: vec![], speaker: None, track: None },
                Segment { start: 22.0, end: 40.0, text: "  ".into(), flag: None, edited: false, voice: vec![], words: vec![], pauses: vec![], heard: vec![], speaker: None, track: None },
            ],
            translations: vec![],
            minutes: None,
            speakers: Vec::new(),
            detected: None,
        }
    }

    fn translated_sample() -> Transcript {
        let mut t = sample();
        let mut tr = Translation::new("en", "Hy-MT2");
        tr.lines = vec![
            TranslatedLine { text: "I mean, at the end you get three buttons".into(), from: t.segments[0].text.clone(), edited: false },
            TranslatedLine::default(),
        ];
        t.translations.push(tr);
        t
    }

    #[test]
    fn a_translation_knows_which_lines_changed_since() {
        let mut t = translated_sample();
        let tr = t.translation("en").unwrap().clone();
        assert!(tr.is_current(0, &t.segments[0]));
        assert!(!tr.is_current(1, &t.segments[1]), "never translated");
        t.segments[0].text = "يعني مثلا ثلاث زرار".into();
        assert!(!tr.is_current(0, &t.segments[0]), "the line was edited after");
        assert!(t.translation("fr").is_none());
    }

    #[test]
    fn the_translation_exports_like_a_transcript_in_that_language() {
        let t = translated_sample();
        let en = t.translated("en").unwrap();
        assert_eq!(en.language, "en");
        assert_eq!(en.model, "Hy-MT2");
        assert_eq!(to_txt(&en, false), "I mean, at the end you get three buttons\n\n");
        let txt = export_view(&t, "txt", false, &View::Translation("en".into())).unwrap();
        assert_eq!(String::from_utf8(txt).unwrap(), "I mean, at the end you get three buttons\n\n");
        assert!(export_view(&t, "txt", false, &View::Translation("fr".into())).is_err());
    }

    #[test]
    fn both_languages_put_each_translation_under_its_line() {
        let t = translated_sample();
        let both = |ext: &str| String::from_utf8(export_view(&t, ext, false, &View::Both("en".into())).unwrap()).unwrap();
        assert_eq!(both("txt"), "يعني مثلا بيجيك اخر شيء ثلاث زرار\nI mean, at the end you get three buttons\n\n");
        let srt = both("srt");
        assert!(srt.starts_with("1\n00:00:00,000 --> "), "{srt}");
        // every subtitle: a part of the line; the translation, one sentence, whole under the one
        // where the middle of it is said, never cut into pieces
        let cues: Vec<&str> = srt.split("\n\n").filter(|c| !c.is_empty()).collect();
        assert!(cues.len() >= 2, "22 seconds do not fit in one subtitle: {srt}");
        assert!(cues[0].contains("يعني"), "{srt}");
        assert!(cues.last().unwrap().contains("زرار"), "{srt}");
        assert_eq!(cues.iter().filter(|c| c.contains("I mean, at the end you get three buttons")).count(), 1, "{srt}");
        assert_eq!(cues.iter().filter(|c| c.contains("I mean")).count(), 1, "{srt}");
        assert_eq!(both("csv").lines().nth(1).unwrap(), "0.000,22.000,\"يعني مثلا بيجيك اخر شيء ثلاث زرار\",\"I mean, at the end you get three buttons\"");
        let json: serde_json::Value = serde_json::from_str(&both("json")).unwrap();
        assert_eq!(json["segments"][0]["translation"], "I mean, at the end you get three buttons");
        assert_eq!(json["translation_language"], "en");
        let docx = export_view(&t, "docx", true, &View::Both("en".into())).unwrap();
        assert_eq!(&docx[..2], b"PK");
    }

    #[test]
    fn transcripts_saved_before_translations_still_load() {
        let old = r#"{"id":"a","title":"x","source_path":"","created_ms":0,"duration_s":1.0,"language":"ar","segments":[]}"#;
        let t: Transcript = serde_json::from_str(old).unwrap();
        assert!(t.translations.is_empty());
        assert!(!serde_json::to_string(&t).unwrap().contains("translations"), "nothing new in files without one");
    }

    #[test]
    fn exports_skip_empty_lines() {
        let t = sample();
        assert_eq!(to_txt(&t, true), "[00:00 - 00:22] يعني مثلا بيجيك اخر شيء ثلاث زرار\n");
        let srt = to_srt(&t);
        assert!(srt.starts_with("1\n00:00:00,000 --> "));
        assert!(to_vtt(&t).starts_with("WEBVTT"));
        let docx = to_docx(&t, true).unwrap();
        assert_eq!(&docx[..2], b"PK");
    }

    #[test]
    fn csv_quotes_and_opens_in_excel() {
        let mut t = sample();
        t.segments[0].text = "قال \"نعم\", ثم مشى".into();
        let csv = to_csv(&t);
        assert!(csv.starts_with("\u{feff}start,end,text\r\n"));
        assert_eq!(csv.lines().nth(1).unwrap(), "0.000,22.000,\"قال \"\"نعم\"\", ثم مشى\"");
        assert_eq!(csv.lines().count(), 2);
    }

    #[test]
    fn what_players_and_spreadsheets_would_read_as_something_else_comes_out_as_text() {
        let mut t = sample();
        t.segments[0].text = "=SUM(A1:A9) <b>bold</b> --> & more".into();
        let vtt = to_vtt(&t);
        assert!(vtt.contains("&lt;b&gt;bold&lt;/b&gt;") && vtt.contains("--&gt; &amp; more"), "{vtt}");
        assert!(vtt.lines().filter(|l| l.contains("-->")).all(|l| l.contains(" --> ") && l.starts_with("00:")), "only timing lines have an arrow: {vtt}");
        assert_eq!(to_csv(&t).lines().nth(1).unwrap(), "0.000,22.000,\"'=SUM(A1:A9) <b>bold</b> --> & more\"");
        for lead in ["+", "-", "@"] {
            t.segments[0].text = format!("{lead}1 تمام");
            assert!(to_csv(&t).lines().nth(1).unwrap().ends_with(&format!("\"'{lead}1 تمام\"")));
            assert!(to_tsv(&t).ends_with(&format!("\t'{lead}1 تمام\n")));
        }
        t.segments[0].text = "\"نعم\" قالها".into();
        assert_eq!(to_tsv(&t).lines().nth(1).unwrap(), "0.000\t22.000\t\"\"\"نعم\"\" قالها\"");
    }

    #[test]
    fn tsv_keeps_one_row_per_line() {
        let mut t = sample();
        t.segments[0].text = "سطر\tفيه\nفواصل".into();
        assert_eq!(to_tsv(&t), "start\tend\ttext\n0.000\t22.000\tسطر فيه فواصل\n");
    }

    #[test]
    fn json_has_the_lines_and_their_words() {
        let mut t = sample();
        t.segments[0].text = "ثلاث زرار".into();
        t.segments[0].words = vec![(0, 1.5), (5, 2.25)];
        let doc: serde_json::Value = serde_json::from_str(&to_json(&t)).unwrap();
        assert_eq!(doc["segments"].as_array().unwrap().len(), 1);
        assert_eq!(doc["segments"][0]["text"], "ثلاث زرار");
        assert_eq!(doc["segments"][0]["words"][0]["word"], "ثلاث");
        assert_eq!(doc["segments"][0]["words"][1]["word"], "زرار");
        assert_eq!(doc["segments"][0]["words"][1]["start"], 2.25);
    }

    /// Three lines: Sami, Sami again, then someone not named yet.
    fn spoken_by_two() -> Transcript {
        let mut t = sample();
        t.segments = vec![
            Segment { start: 0.0, end: 4.0, text: "طيب، نبدأ بالـ agenda؟".into(), speaker: Some(1), ..Default::default() },
            Segment { start: 4.0, end: 8.0, text: "أول نقطة هي الـ login.".into(), speaker: Some(1), ..Default::default() },
            Segment { start: 8.0, end: 12.0, text: "تمام, \"ممتاز\"".into(), speaker: Some(2), ..Default::default() },
        ];
        t.speakers = vec![Speaker { id: 1, name: "Sami".into() }, Speaker { id: 2, name: String::new() }];
        t
    }

    #[test]
    fn the_speakers_name_starts_each_turn_in_text_markdown_and_word() {
        let t = spoken_by_two();
        let other = t.speaker_name(2);
        assert_eq!(to_txt(&t, false), format!("Sami: طيب، نبدأ بالـ agenda؟\n\nأول نقطة هي الـ login.\n\n{other}: تمام, \"ممتاز\"\n\n"));
        assert!(to_txt(&t, true).starts_with("[00:00 - 00:04] Sami: طيب"), "{}", to_txt(&t, true));
        let md = to_md(&t, false);
        assert!(md.contains("**Sami:** طيب، نبدأ") && md.contains("\n\nأول نقطة") && md.contains(&format!("**{other}:** تمام")), "{md}");
        let docx = to_docx(&t, false).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
        let mut doc = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut doc).unwrap();
        assert_eq!(doc.matches("Sami: </w:t>").count(), 1, "once, where the turn starts");
        assert_eq!(doc.matches("<w:b/>").count(), 2, "both names in bold");
    }

    #[test]
    fn tables_get_a_speaker_column_and_json_a_speaker_field() {
        let t = spoken_by_two();
        let other = t.speaker_name(2);
        let csv = to_csv(&t);
        assert!(csv.starts_with("\u{feff}start,end,speaker,text\r\n"), "{csv}");
        assert_eq!(csv.lines().nth(2).unwrap(), "4.000,8.000,\"Sami\",\"أول نقطة هي الـ login.\"", "every row says who");
        assert_eq!(csv.lines().nth(3).unwrap(), format!("8.000,12.000,\"{other}\",\"تمام, \"\"ممتاز\"\"\""));
        let tsv = to_tsv(&t);
        assert_eq!(tsv.lines().next().unwrap(), "start\tend\tspeaker\ttext");
        assert_eq!(tsv.lines().nth(1).unwrap(), "0.000\t4.000\tSami\tطيب، نبدأ بالـ agenda؟");
        let json: serde_json::Value = serde_json::from_str(&to_json(&t)).unwrap();
        assert_eq!(json["segments"][1]["speaker"], "Sami");
        assert_eq!(json["speakers"][1], other.as_str());
        // without speakers, nothing changes for the programs that read these
        assert!(to_csv(&sample()).starts_with("\u{feff}start,end,text\r\n"));
        assert!(serde_json::from_str::<serde_json::Value>(&to_json(&sample())).unwrap().get("speakers").is_none());
    }

    #[test]
    fn subtitles_name_the_speaker_as_asked() {
        let t = spoken_by_two();
        crate::speakers::set_subtitle_names(false, true);
        let srt = to_srt(&t);
        assert!(!srt.contains("Sami"), "off for SRT unless asked: {srt}");
        let vtt = to_vtt(&t);
        assert!(vtt.contains("\n<v Sami>طيب، نبدأ") && vtt.contains("\n<v Sami>أول نقطة"), "WebVTT's voice tag on each of the speaker's: {vtt}");
        crate::speakers::set_subtitle_names(true, false);
        let srt = to_srt(&t);
        assert_eq!(srt.matches("Sami:").count(), 1, "only where the speaker changes: {srt}");
        // an Arabic line keeps its direction behind a Latin name: marks before it and after the colon
        assert!(srt.contains("\n\u{200F}Sami:\u{200F} طيب، نبدأ"), "{srt}");
        let mut en = spoken_by_two();
        en.segments[0].text = "Okay, shall we start?".into();
        assert!(to_srt(&en).contains("\nSami: Okay, shall we start?"), "no marks before an English line");
        // Chinese and Japanese, cut to their own line lengths: the name still in front, once a turn
        let mut zh = spoken_by_two();
        zh.language = "zh".into();
        zh.segments[0].text = "大家好，我们开始吧。今天有三个问题要讨论，第一个是登录页面的设计。".into();
        zh.segments[1].text = "第二个是首页，客户希望登录后首先看到仪表板。".into();
        zh.segments[2].text = "好的，没问题。".into();
        let srt = to_srt(&zh);
        assert!(srt.contains("\nSami: 大家好"), "{srt}");
        assert_eq!(srt.matches("Sami:").count(), 1, "{srt}");
        assert!(srt.contains(&format!("{}: 好的", zh.speaker_name(2))) && !srt.contains('\u{200F}'), "{srt}");
        assert!(!to_vtt(&t).contains("<v "));
        crate::speakers::set_subtitle_names(false, true);
    }

    #[test]
    fn json_leaves_out_words_it_does_not_have() {
        let doc: serde_json::Value = serde_json::from_str(&to_json(&sample())).unwrap();
        assert!(doc["segments"][0].get("words").is_none());
    }
}
