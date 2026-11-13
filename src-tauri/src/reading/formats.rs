//! Files into blocks of plain text to read aloud: what a person would read out, without what
//! they would skip (code, link addresses, image paths, subtitle numbers and times).

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Heading,
    Paragraph,
    Item,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: Kind,
    pub text: String,
    /// Who says it, in a transcript whose speakers were told apart
    pub speaker: Option<u32>,
}

/// A block whose line breaks end sentences (the dashes of subtitles).
fn block_with_lines(kind: Kind, text: &str) -> Option<Block> {
    let lines: Vec<String> = text.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect();
    let text = lines.join("
");
    text.chars().any(|c| c.is_alphanumeric()).then_some(Block { kind, text, speaker: None })
}

fn block(kind: Kind, text: &str) -> Option<Block> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().any(|c| c.is_alphanumeric()).then_some(Block { kind, text, speaker: None })
}

/// What a file holds, and a title for it.
pub struct Opened {
    pub title: String,
    pub blocks: Vec<Block>,
}

pub const EXTENSIONS: &[&str] = &["txt", "text", "md", "markdown", "html", "htm", "docx", "srt", "vtt"];

/// The most text read aloud from one file. A book of a thousand pages is a few megabytes; a file the
/// size of a disk, or a Word file whose text unpacks to gigabytes, would take the app's memory, and
/// the app with it, a transcription or a recording under way included.
pub const MAX_TEXT: u64 = 64 << 20;
/// A Word file also carries its pictures, which are not unpacked: it may be larger.
pub const MAX_DOCX: u64 = 256 << 20;

fn too_large(name: &str) -> anyhow::Error {
    anyhow::anyhow!(crate::tr!("{} is too large to read aloud: Dawwin reads files of up to {} MB of text.", name, MAX_TEXT >> 20))
}

pub fn read_path(path: &Path) -> Result<Opened> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    // the size first: nothing is read of a file too large to read aloud
    let size = std::fs::metadata(path).with_context(|| path.display().to_string())?.len();
    if size > if ext == "docx" { MAX_DOCX } else { MAX_TEXT } {
        return Err(too_large(&name));
    }
    let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let (title, blocks) = match ext.as_str() {
        "md" | "markdown" => (None, from_markdown(&decode(&bytes))),
        "html" | "htm" | "xhtml" => from_html(&decode_html(&bytes)),
        "docx" => (None, from_docx(&bytes).map_err(|e| if e.is::<TooLarge>() { too_large(&name) } else { e })?),
        "srt" | "vtt" => (None, from_subtitles(&decode(&bytes))),
        _ => {
            if bytes.iter().take(4096).filter(|&&b| b == 0).count() > 16 && !has_utf16_bom(&bytes) {
                bail!(crate::tr!("{} is not a text file Dawwin can read.", path.display()));
            }
            (None, from_text(&decode(&bytes)))
        }
    };
    Ok(Opened { title: title.filter(|t| !t.trim().is_empty()).unwrap_or(stem), blocks })
}

// ------------------------------------------------------------------ encodings

fn has_utf16_bom(b: &[u8]) -> bool {
    b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF])
}

#[cfg(windows)]
fn code_page(bytes: &[u8], cp: u32) -> String {
    #[link(name = "kernel32")]
    extern "system" {
        fn MultiByteToWideChar(cp: u32, flags: u32, s: *const u8, n: i32, w: *mut u16, wn: i32) -> i32;
    }
    if bytes.is_empty() {
        return String::new();
    }
    let len = bytes.len().min(i32::MAX as usize) as i32;
    // SAFETY: the buffers are valid for the lengths given; the first call only measures.
    unsafe {
        let n = MultiByteToWideChar(cp, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut wide = vec![0u16; n as usize];
        MultiByteToWideChar(cp, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), n);
        String::from_utf16_lossy(&wide)
    }
}

#[cfg(not(windows))]
fn code_page(bytes: &[u8], _cp: u32) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Arabic in Windows-1256 comes in words: runs of bytes above 127. French or German in
/// Windows-1252 has one such byte here and there, between plain letters.
fn looks_arabic_1256(bytes: &[u8]) -> bool {
    let (mut high, mut in_runs, mut run) = (0usize, 0usize, 0usize);
    for &b in bytes.iter().chain(std::iter::once(&b' ')) {
        if b >= 0x80 {
            high += 1;
            run += 1;
        } else {
            if run >= 3 {
                in_runs += run;
            }
            run = 0;
        }
    }
    high > 0 && in_runs * 2 > high
}

/// Text in whatever encoding it was saved in: UTF-8 or UTF-16 (with or without a mark), else the
/// old Windows code page of Arabic or of western languages.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |b: &[u8], le: bool| -> String {
        let units: Vec<u16> = b.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // UTF-16 without its mark: every other byte zero
    let sample = &bytes[..bytes.len().min(4096)];
    let odd_zeros = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
    if sample.len() >= 8 && odd_zeros * 3 > sample.len() / 2 {
        return utf16(bytes, true);
    }
    code_page(bytes, if looks_arabic_1256(bytes) { 1256 } else { 1252 })
}

fn decode_html(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) || has_utf16_bom(bytes) || std::str::from_utf8(bytes).is_ok() {
        return decode(bytes);
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_ascii_lowercase();
    if head.contains("windows-1256") || head.contains("cp1256") {
        return code_page(bytes, 1256);
    }
    if head.contains("iso-8859-6") {
        return code_page(bytes, 28596);
    }
    if head.contains("windows-1252") || head.contains("iso-8859-1") {
        return code_page(bytes, 1252);
    }
    decode(bytes)
}

// ------------------------------------------------------------------ plain text

fn list_marker(line: &str) -> Option<&str> {
    let t = line.trim_start();
    for m in ["- ", "* ", "+ ", "• ", "· ", "– "] {
        if let Some(rest) = t.strip_prefix(m) {
            return Some(rest);
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=3).contains(&digits) {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(r);
        }
    }
    None
}

/// Pasted text and .txt files. With blank lines between paragraphs, the lines of a paragraph are
/// one text (files are often wrapped at a width); without any, each line is a paragraph. A short
/// line on its own between blank lines, with no full stop, is a heading.
pub fn from_text(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.lines().collect();
    let has_blank = lines.iter().any(|l| l.trim().is_empty());
    let mut out = Vec::new();
    if !has_blank {
        for l in lines {
            let (kind, t) = list_marker(l).map_or((Kind::Paragraph, l), |r| (Kind::Item, r));
            out.extend(block(kind, t));
        }
        return out;
    }
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if para.is_empty() {
            return;
        }
        let one = para.len() == 1;
        let t = para.join(" ");
        let heading = one && t.chars().count() <= 70 && !t.trim_end().ends_with(['.', '!', '?', '؟', ':', '،', ',', ';', '؛']);
        out.extend(block(if heading { Kind::Heading } else { Kind::Paragraph }, &t));
        para.clear();
    };
    for l in lines {
        if l.trim().is_empty() {
            flush(&mut para, &mut out);
        } else if let Some(r) = list_marker(l) {
            flush(&mut para, &mut out);
            out.extend(block(Kind::Item, r));
        } else {
            para.push(l);
        }
    }
    flush(&mut para, &mut out);
    out
}

// ------------------------------------------------------------------ markdown

/// Inline Markdown as the words it shows: links by their text, images, addresses and HTML gone.
fn md_inline(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let find = |from: usize, c: char| (from..chars.len()).find(|&k| chars[k] == c);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() => {
                out.push(chars[i + 1]);
                i += 2;
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                // an image: nothing of it is read
                let close = find(i + 2, ']');
                match close {
                    Some(k) if chars.get(k + 1) == Some(&'(') => i = find(k + 2, ')').map_or(chars.len(), |e| e + 1),
                    Some(k) if chars.get(k + 1) == Some(&'[') => i = find(k + 2, ']').map_or(chars.len(), |e| e + 1),
                    _ => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            '[' => {
                if chars.get(i + 1) == Some(&'^') {
                    i = find(i, ']').map_or(chars.len(), |e| e + 1); // a footnote mark
                    continue;
                }
                match find(i + 1, ']') {
                    Some(k) => {
                        let label: String = chars[i + 1..k].iter().collect();
                        let after = match chars.get(k + 1) {
                            Some('(') => find(k + 2, ')').map(|e| e + 1),
                            Some('[') => find(k + 2, ']').map(|e| e + 1),
                            _ => None,
                        };
                        match after {
                            Some(e) => {
                                out.push_str(&md_inline(&label));
                                i = e;
                            }
                            None => {
                                out.push(c);
                                i += 1;
                            }
                        }
                    }
                    None => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            '<' => match find(i + 1, '>') {
                // an autolink or a tag
                Some(k) if chars[i + 1..k].iter().all(|c| !c.is_whitespace() || chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '/') => i = k + 1,
                _ => {
                    out.push(c);
                    i += 1;
                }
            },
            '`' => i += 1,
            '*' | '~' => i += 1,
            '_' => {
                // emphasis, not the _ inside snake_case
                let inside = i > 0 && chars[i - 1].is_alphanumeric() && chars.get(i + 1).is_some_and(|c| c.is_alphanumeric());
                if inside {
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    strip_urls(&out)
}

fn strip_urls(s: &str) -> String {
    s.split(' ')
        .filter(|w| {
            let w = w.trim_matches(|c: char| !c.is_alphanumeric());
            !(w.starts_with("http://") || w.starts_with("https://") || w.starts_with("www.") || w.starts_with("ftp://") || (w.contains("://")))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pasted text written in Markdown: a heading, a fence, a link, or a list of several items.
pub fn looks_like_markdown(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().map(str::trim_start).collect();
    lines.iter().any(|l| (l.starts_with('#') && l.trim_start_matches('#').starts_with(' ')) || l.starts_with("```"))
        || (text.contains("](") && text.contains('['))
        || lines.iter().filter(|l| l.starts_with("- ") || l.starts_with("* ")).count() >= 2
}

pub fn from_markdown(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = text.lines().collect();
    // front matter
    if lines.first().is_some_and(|l| l.trim() == "---") {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---" || l.trim() == "...") {
            lines.drain(..end + 2);
        }
    }
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut fence: Option<String> = None;
    let mut item: Option<String> = None;
    let flush = |para: &mut Vec<String>, item: &mut Option<String>, out: &mut Vec<Block>| {
        if let Some(t) = item.take() {
            out.extend(block(Kind::Item, &md_inline(&t)));
        }
        if !para.is_empty() {
            out.extend(block(Kind::Paragraph, &md_inline(&para.join(" "))));
            para.clear();
        }
    };
    let mut prev_blank = true;
    for line in lines.iter() {
        let t = line.trim();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) && t.trim_start_matches(f.chars().next().unwrap()).trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            flush(&mut para, &mut item, &mut out);
            fence = Some(t.chars().take_while(|&c| c == t.chars().next().unwrap()).collect());
            continue;
        }
        if t.is_empty() {
            flush(&mut para, &mut item, &mut out);
            prev_blank = true;
            continue;
        }
        // code indented by four spaces, after a blank line and outside a list
        if (line.starts_with("    ") || line.starts_with('\t')) && prev_blank && item.is_none() && out.last().map_or(true, |b| b.kind != Kind::Item) {
            continue;
        }
        prev_blank = false;
        // a reference definition: [id]: https://…
        if t.starts_with('[') && t.contains("]:") {
            continue;
        }
        if t.starts_with('#') {
            let level = t.chars().take_while(|&c| c == '#').count();
            if level <= 6 && t[level..].starts_with([' ', '\t']) || t.len() == level {
                flush(&mut para, &mut item, &mut out);
                let h = t[level..].trim().trim_end_matches('#').trim();
                out.extend(block(Kind::Heading, &md_inline(h)));
                continue;
            }
        }
        // a setext heading underlines the line before
        if !para.is_empty() && (t.chars().all(|c| c == '=') || t.chars().all(|c| c == '-')) && t.len() >= 2 {
            let h = para.join(" ");
            para.clear();
            flush(&mut para, &mut item, &mut out);
            out.extend(block(Kind::Heading, &md_inline(&h)));
            continue;
        }
        // a rule
        let bare: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        if bare.len() >= 3 && (bare.chars().all(|c| c == '-') || bare.chars().all(|c| c == '*') || bare.chars().all(|c| c == '_')) {
            flush(&mut para, &mut item, &mut out);
            continue;
        }
        // a table: a row is read cell by cell, the line under the header not at all
        if t.starts_with('|') || (t.contains(" | ") && t.matches('|').count() >= 2) {
            let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
            if cells.iter().all(|c| c.chars().all(|x| matches!(x, '-' | ':' | ' ')) && !c.is_empty()) {
                continue;
            }
            flush(&mut para, &mut item, &mut out);
            let comma = if crate::reading::text::main_lang(t) == crate::reading::text::Lang::Ar { "، " } else { ", " };
            out.extend(block(Kind::Paragraph, &md_inline(&cells.join(comma))));
            continue;
        }
        let t = t.trim_start_matches('>').trim_start();
        if let Some(rest) = list_marker(line) {
            flush(&mut para, &mut item, &mut out);
            let rest = rest.strip_prefix("[ ] ").or_else(|| rest.strip_prefix("[x] ")).unwrap_or(rest);
            item = Some(rest.to_string());
            continue;
        }
        if let Some(it) = item.as_mut() {
            it.push(' ');
            it.push_str(t);
            continue;
        }
        para.push(t.to_string());
    }
    flush(&mut para, &mut item, &mut out);
    out
}

// ------------------------------------------------------------------ html and xml

fn entity(name: &str) -> Option<String> {
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" => ' ',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "copy" => '©',
        "reg" => '®',
        "deg" => '°',
        "euro" => '€',
        "pound" => '£',
        "times" => '×',
        "zwnj" | "zwj" | "lrm" | "rlm" | "shy" => return Some(String::new()),
        n if n.starts_with("#x") || n.starts_with("#X") => return u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32).map(String::from),
        n if n.starts_with('#') => return n[1..].parse::<u32>().ok().and_then(char::from_u32).map(String::from),
        _ => return None,
    };
    Some(c.to_string())
}

pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match rest[1..].find(';').filter(|&k| k <= 10) {
            Some(k) => match entity(&rest[1..k + 1]) {
                Some(e) => {
                    out.push_str(&e);
                    rest = &rest[k + 2..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            },
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A tag: its lower-case name, whether it closes, and the text of the whole tag.
struct Tag<'a> {
    name: String,
    closing: bool,
    raw: &'a str,
}

/// Walk markup: `on_text` gets the text between tags, `on_tag` each tag.
fn walk<'a>(s: &'a str, mut on_tag: impl FnMut(&Tag<'a>), mut on_text: impl FnMut(&'a str)) {
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        if i > 0 {
            on_text(&rest[..i]);
        }
        rest = &rest[i..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let Some(end) = rest.find('>') else {
            on_text(rest);
            return;
        };
        let raw = &rest[1..end];
        let closing = raw.starts_with('/');
        let name: String = raw.trim_start_matches('/').chars().take_while(|c| c.is_alphanumeric() || *c == ':').collect::<String>().to_ascii_lowercase();
        on_tag(&Tag { name, closing, raw });
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        on_text(rest);
    }
}

pub fn from_html(html: &str) -> (Option<String>, Vec<Block>) {
    const SKIP: &[&str] = &["script", "style", "noscript", "template", "svg", "head", "pre", "nav", "footer", "button", "select", "textarea", "iframe", "math"];
    const BLOCKS: &[&str] = &["p", "div", "br", "li", "tr", "blockquote", "section", "article", "header", "main", "aside", "table", "ul", "ol", "dl", "dd", "dt", "figcaption", "hr", "form", "h1", "h2", "h3", "h4", "h5", "h6", "body", "html"];
    let mut out = Vec::new();
    let mut title: Option<String> = None;
    let mut in_title = false;
    let mut skip: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut kind = Kind::Paragraph;
    let flush = |cur: &mut String, kind: Kind, out: &mut Vec<Block>| {
        out.extend(block(kind, &strip_urls(&unescape(cur))));
        cur.clear();
    };
    let out_ref = std::cell::RefCell::new((&mut out, &mut cur, &mut kind, &mut skip, &mut title, &mut in_title));
    walk(
        html,
        |tag| {
            let mut g = out_ref.borrow_mut();
            let (out, cur, kind, skip, _title, in_title) = &mut *g;
            if tag.name == "title" {
                **in_title = !tag.closing;
                return;
            }
            let self_closing = tag.raw.ends_with('/');
            // a page whose head is never closed
            if tag.name == "body" && !tag.closing {
                skip.retain(|s| s != "head");
            }
            if SKIP.contains(&tag.name.as_str()) {
                if tag.closing {
                    if let Some(p) = skip.iter().rposition(|s| *s == tag.name) {
                        skip.truncate(p);
                    }
                } else if !self_closing {
                    skip.push(tag.name.clone());
                }
                return;
            }
            if !skip.is_empty() {
                return;
            }
            if matches!(tag.name.as_str(), "td" | "th") && !tag.closing {
                if !cur.trim().is_empty() {
                    cur.push_str(", ");
                }
                return;
            }
            if BLOCKS.contains(&tag.name.as_str()) {
                flush(cur, **kind, out);
                **kind = match (tag.name.as_str(), tag.closing) {
                    ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => Kind::Heading,
                    ("li" | "dt", false) => Kind::Item,
                    _ => Kind::Paragraph,
                };
            }
            // images are not read, nor their paths
        },
        |text| {
            let mut g = out_ref.borrow_mut();
            let (_out, cur, _kind, skip, title, in_title) = &mut *g;
            if **in_title {
                **title = Some(title.take().unwrap_or_default() + &unescape(text));
                return;
            }
            if skip.is_empty() {
                cur.push_str(text);
            }
        },
    );
    flush(&mut cur, kind, &mut out);
    (title.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")), out)
}

/// A Word file whose text unpacks to more than [`MAX_TEXT`].
#[derive(Debug)]
pub struct TooLarge;

impl std::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("too large")
    }
}

impl std::error::Error for TooLarge {}

pub fn from_docx(bytes: &[u8]) -> Result<Vec<Block>> {
    from_docx_up_to(bytes, MAX_TEXT)
}

/// The text of a Word file, unpacking no more than `limit` bytes of it: what its header says of the
/// size is not trusted (a small file can unpack to gigabytes).
fn from_docx_up_to(bytes: &[u8], limit: u64) -> Result<Vec<Block>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a Word document")?;
    let mut raw = Vec::new();
    zip.by_name("word/document.xml").context("not a Word document")?.take(limit + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > limit {
        return Err(TooLarge.into());
    }
    let xml = String::from_utf8_lossy(&raw).into_owned();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut kind = Kind::Paragraph;
    let mut in_text = false;
    let mut skip_text = 0usize; // field codes, deleted text
    let state = std::cell::RefCell::new((&mut out, &mut cur, &mut kind, &mut in_text, &mut skip_text));
    walk(
        &xml,
        |tag| {
            let mut g = state.borrow_mut();
            let (out, cur, kind, in_text, skip_text) = &mut *g;
            let self_closing = tag.raw.ends_with('/');
            match tag.name.as_str() {
                "w:p" if !tag.closing => {
                    **kind = Kind::Paragraph;
                    cur.clear();
                }
                "w:p" => {
                    out.extend(block(**kind, &unescape(cur)));
                    cur.clear();
                }
                "w:pstyle" => {
                    let v = tag.raw.split("w:val=\"").nth(1).and_then(|v| v.split('"').next()).unwrap_or("").to_ascii_lowercase();
                    if v.starts_with("heading") || v == "title" || v == "subtitle" || v.starts_with("titre") || v.starts_with("berschrift") {
                        **kind = Kind::Heading;
                    } else if v.starts_with("list") && **kind == Kind::Paragraph {
                        **kind = Kind::Item;
                    }
                }
                "w:outlinelvl" => **kind = Kind::Heading,
                "w:numpr" if **kind == Kind::Paragraph => **kind = Kind::Item,
                "w:t" => **in_text = !tag.closing && !self_closing,
                "w:tab" if !tag.closing => cur.push(' '),
                "w:br" | "w:cr" if !tag.closing => cur.push(' '),
                "w:instrtext" | "w:deltext" | "w:footnotereference" => {
                    if !self_closing {
                        if tag.closing {
                            **skip_text = skip_text.saturating_sub(1);
                        } else {
                            **skip_text += 1;
                        }
                    }
                }
                _ => {}
            }
        },
        |text| {
            let mut g = state.borrow_mut();
            let (_, cur, _, in_text, skip_text) = &mut *g;
            if **in_text && **skip_text == 0 {
                cur.push_str(text);
            }
        },
    );
    Ok(out)
}

// ------------------------------------------------------------------ subtitles

fn subtitle_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let nums: Vec<f64> = parts.iter().map(|p| p.trim().parse::<f64>()).collect::<std::result::Result<_, _>>().ok()?;
    Some(match nums.as_slice() {
        [h, m, sec] => h * 3600.0 + m * 60.0 + sec,
        [m, sec] => m * 60.0 + sec,
        _ => return None,
    })
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth_angle = false;
    let mut depth_curly = false;
    let mut depth_square = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth_angle = true,
            '>' if depth_angle => depth_angle = false,
            '{' => depth_curly = true,
            '}' if depth_curly => depth_curly = false,
            // sound descriptions for the deaf: [music], [laughs]
            '[' => depth_square += 1,
            ']' if depth_square > 0 => depth_square -= 1,
            '♪' | '♫' => {}
            c if !depth_angle && !depth_curly && depth_square == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// SRT and WebVTT: the words of the cues, without numbers, times, styles or tags. Cues run on
/// into one paragraph until a pause of two seconds; a line that starts with a dash (another
/// person speaking) starts a sentence.
pub fn from_subtitles(text: &str) -> Vec<Block> {
    let text = text.trim_start_matches('\u{FEFF}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    let mut para = String::new();
    let mut last_end: Option<f64> = None;
    for cue in text.split("\n\n") {
        let lines: Vec<&str> = cue.lines().collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let mut times = lines[at].split("-->");
        let start = times.next().and_then(subtitle_time);
        let end = times.next().and_then(|t| subtitle_time(t.split_whitespace().next().unwrap_or("")));
        if let (Some(s), Some(prev)) = (start, last_end) {
            if s - prev >= 2.0 {
                out.extend(block_with_lines(Kind::Paragraph, &para));
                para.clear();
            }
        }
        last_end = end.or(last_end);
        for line in &lines[at + 1..] {
            let clean = unescape(&strip_tags(line));
            let clean = clean.trim();
            if clean.is_empty() {
                continue;
            }
            if let Some(rest) = clean.strip_prefix('-').or_else(|| clean.strip_prefix('–')) {
                para.push('\n');
                para.push_str(rest.trim());
            } else {
                para.push(' ');
                para.push_str(clean);
            }
        }
    }
    out.extend(block_with_lines(Kind::Paragraph, &para));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(blocks: &[Block]) -> Vec<(Kind, &str)> {
        blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
    }

    #[test]
    fn text_files_in_utf8_utf16_and_windows_1256() {
        assert_eq!(decode("مرحبا".as_bytes()), "مرحبا");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'h', b'i']), "hi");
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("سلام".encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
        assert_eq!(decode(&utf16), "سلام");
        // "مرحبا بكم" saved by old Arabic Windows
        let cp1256 = [0xE3, 0xD1, 0xCD, 0xC8, 0xC7, b' ', 0xC8, 0xDF, 0xE3];
        assert_eq!(decode(&cp1256), "مرحبا بكم");
        // "café crème" in Windows-1252
        let cp1252 = [b'c', b'a', b'f', 0xE9, b' ', b'c', b'r', 0xE8, b'm', b'e'];
        assert_eq!(decode(&cp1252), "café crème");
    }

    #[test]
    fn plain_text_paragraphs_lines_lists_and_headings() {
        let b = from_text("Chapter One\n\nThe first line of a\nwrapped paragraph.\n\n- one\n- two\n");
        assert_eq!(texts(&b), vec![(Kind::Heading, "Chapter One"), (Kind::Paragraph, "The first line of a wrapped paragraph."), (Kind::Item, "one"), (Kind::Item, "two")]);
        let b = from_text("سطر أول.\r\nسطر ثان.");
        assert_eq!(texts(&b), vec![(Kind::Paragraph, "سطر أول."), (Kind::Paragraph, "سطر ثان.")]);
    }

    #[test]
    fn markdown_without_code_links_or_images() {
        let md = "---\ntitle: x\n---\n# The **title**\n\nRead [the guide](https://example.com/guide) and `npm install`.\n![a picture](img/p.png)\n\n```bash\nrm -rf /\n```\n\n    indented code\n\n- [x] done item\n* second\n\nSee <https://example.com> or https://example.com/x.\n\n| Name | Age |\n| --- | --- |\n| Sara | 30 |\n\nSubtitle\n---\n\nsnake_case and _emphasis_[^1].";
        let b = from_markdown(md);
        assert_eq!(texts(&b), vec![
            (Kind::Heading, "The title"),
            (Kind::Paragraph, "Read the guide and npm install."),
            (Kind::Item, "done item"),
            (Kind::Item, "second"),
            (Kind::Paragraph, "See or"),
            (Kind::Paragraph, "Name, Age"),
            (Kind::Paragraph, "Sara, 30"),
            (Kind::Heading, "Subtitle"),
            (Kind::Paragraph, "snake_case and emphasis."),
        ]);
    }

    #[test]
    fn pasted_markdown_is_told_from_plain_text() {
        assert!(looks_like_markdown("# Title

Text."));
        assert!(looks_like_markdown("See [this](https://x.org)."));
        assert!(looks_like_markdown("- one
- two"));
        assert!(!looks_like_markdown("Just a sentence. And #hashtags, and 5 - 3."));
        assert!(!looks_like_markdown("- only one dash line"));
    }

    #[test]
    fn html_as_a_page_reads() {
        let html = "<html><head><title>My &amp; page</title><style>p{}</style><script>var a='<p>x</p>';</script></head><body><nav><a href=/>Home</a></nav><h1>Hello</h1><p>One <b>bold</b> word, <a href=\"https://x.org\">a link</a>.<br>Next&nbsp;line &#1605;&#x0631;</p><ul><li>first</li><li>second</li></ul><img src=a.png alt=pic><pre>code here</pre><table><tr><td>a</td><td>b</td></tr></table><!-- a note --></body></html>";
        let (title, b) = from_html(html);
        assert_eq!(title.as_deref(), Some("My & page"));
        assert_eq!(texts(&b), vec![
            (Kind::Heading, "Hello"),
            (Kind::Paragraph, "One bold word, a link."),
            (Kind::Paragraph, "Next line مر"),
            (Kind::Item, "first"),
            (Kind::Item, "second"),
            (Kind::Paragraph, "a, b"),
        ]);
    }

    #[test]
    fn word_documents() {
        // what Word writes, cut down: a heading, a list item, a field code and a deletion
        let doc = r#"<?xml version="1.0"?><w:document><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>العنوان</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">Hello </w:t></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:t>world &amp; all</w:t></w:r><w:del><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:tab/><w:t>.</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>an item</w:t></w:r></w:p>
<w:p></w:p></w:body></w:document>"#;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, doc.as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let b = from_docx(buf.get_ref()).unwrap();
        assert_eq!(texts(&b), vec![(Kind::Heading, "العنوان"), (Kind::Paragraph, "Hello world & all ."), (Kind::Item, "an item")]);
        assert!(from_docx(b"not a zip").is_err());
    }

    #[test]
    fn a_file_too_large_is_refused_before_it_is_read() {
        let dir = std::env::temp_dir().join(format!("dawwin-read-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // larger than the limit, without writing it: only its size is looked at
        let big = dir.join("huge.txt");
        std::fs::File::create(&big).unwrap().set_len(MAX_TEXT + 1).unwrap();
        let e = read_path(&big).err().expect("refused");
        assert!(format!("{e:#}").contains("too large"), "{e:#}");
        let docx = dir.join("big.docx");
        std::fs::File::create(&docx).unwrap().set_len(MAX_DOCX + 1).unwrap();
        assert!(format!("{:#}", read_path(&docx).err().unwrap()).contains("too large"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_word_file_that_unpacks_to_much_more_is_refused() {
        // a few kilobytes that unpack to a megabyte
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, &vec![b'a'; 1 << 20]).unwrap();
            z.finish().unwrap();
        }
        assert!(buf.get_ref().len() < 64 << 10);
        let e = from_docx_up_to(buf.get_ref(), 64 << 10).err().expect("refused");
        assert!(e.is::<TooLarge>());
        assert!(from_docx_up_to(buf.get_ref(), 2 << 20).is_ok(), "under the limit it reads");
    }

    #[test]
    fn subtitles_without_numbers_times_or_tags() {
        let srt = "1\n00:00:01,000 --> 00:00:03,000\n<i>Hello there,</i>\nmy friend.\n\n2\n00:00:03,500 --> 00:00:05,000\n- How are you?\n- Fine.\n\n3\n00:00:09,000 --> 00:00:10,000\n[music] {\\an8}Later ♪\n";
        let b = from_subtitles(srt);
        assert_eq!(b.len(), 2, "a pause of two seconds starts a paragraph: {b:?}");
        assert_eq!(b[0].text, "Hello there, my friend.
How are you?
Fine.");
        assert_eq!(b[1].text, "Later");
        let vtt = "WEBVTT\n\nNOTE a comment\n\nSTYLE\n::cue { color: red }\n\nintro\n00:01.000 --> 00:02.000 align:start\n<v Sara>مرحبا <c.big>بكم</c>\n";
        assert_eq!(texts(&from_subtitles(vtt)), vec![(Kind::Paragraph, "مرحبا بكم")]);
    }
}
