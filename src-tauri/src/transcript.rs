//! Transcripts: the data, the model's repetition loops, and the export formats.

use std::io::Write;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub segments: Vec<Segment>,
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

pub fn to_txt(t: &Transcript, stamps: bool) -> String {
    let mut out = String::new();
    for s in visible(t) {
        if stamps {
            out.push_str(&format!("[{} - {}] ", ts(s.start), ts(s.end)));
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
        if t.language == "en" { crate::tr!("English") } else { crate::tr!("Arabic") },
        crate::tr!("Model"),
        t.model
    ));
    for s in visible(t) {
        if stamps {
            out.push_str(&format!("**[{}]** ", ts(s.start)));
        }
        out.push_str(s.text.trim());
        out.push_str("\n\n");
    }
    out
}

/// Subtitle cues: each segment (up to 30 s) is split into lines of about 14 words, timed
/// by their share of the segment's characters. Timing inside a segment is an estimate.
pub fn cues(t: &Transcript) -> Vec<(f64, f64, String)> {
    let mut out = Vec::new();
    for s in visible(t) {
        let words: Vec<&str> = s.text.split_whitespace().collect();
        let dur = (s.end - s.start).max(0.1);
        let n = ((words.len() as f64 / 14.0).max(dur / 7.0)).ceil().max(1.0) as usize;
        let n = n.min(words.len().max(1));
        let per = words.len().div_ceil(n);
        let groups: Vec<String> = words.chunks(per.max(1)).map(|c| c.join(" ")).collect();
        let total: usize = groups.iter().map(|g| g.chars().count() + 1).sum();
        let mut at = s.start;
        for g in groups {
            let share = (g.chars().count() + 1) as f64 / total as f64;
            let end = (at + dur * share).min(s.end);
            out.push((at, end, g));
            at = end;
        }
    }
    out
}

pub fn to_srt(t: &Transcript) -> String {
    let mut out = String::new();
    for (i, (a, b, text)) in cues(t).into_iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, ts_ms(a, ','), ts_ms(b, ','), text));
    }
    out
}

pub fn to_vtt(t: &Transcript) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (a, b, text) in cues(t) {
        out.push_str(&format!("{} --> {}\n{}\n\n", ts_ms(a, '.'), ts_ms(b, '.'), text));
    }
    out
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A Word document with right-to-left paragraphs.
pub fn to_docx(t: &Transcript, stamps: bool) -> Result<Vec<u8>> {
    let rtl = t.language != "en";
    let ppr = if rtl { "<w:pPr><w:bidi/><w:spacing w:after=\"160\" w:line=\"312\" w:lineRule=\"auto\"/></w:pPr>" } else { "<w:pPr><w:spacing w:after=\"160\"/></w:pPr>" };
    let text_rpr = if rtl { "<w:rPr><w:rtl/><w:lang w:bidi=\"ar-SY\"/></w:rPr>" } else { "" };
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
    for s in visible(t) {
        body.push_str("<w:p>");
        body.push_str(ppr);
        if stamps {
            body.push_str(&format!(
                "<w:r><w:rPr><w:rtl w:val=\"0\"/><w:color w:val=\"0F766E\"/><w:sz w:val=\"18\"/><w:szCs w:val=\"18\"/></w:rPr><w:t xml:space=\"preserve\">[{}]</w:t></w:r><w:r>{}<w:t xml:space=\"preserve\"> </w:t></w:r>",
                ts(s.start),
                text_rpr
            ));
        }
        body.push_str(&format!("<w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r>", text_rpr, xml(s.text.trim())));
        body.push_str("</w:p>");
    }
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
</w:styles>";
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
            segments: vec![
                Segment { start: 0.0, end: 22.0, text: "يعني مثلا بيجيك اخر شيء ثلاث زرار".into(), flag: None, edited: false, voice: vec![] },
                Segment { start: 22.0, end: 40.0, text: "  ".into(), flag: None, edited: false, voice: vec![] },
            ],
        }
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
}
