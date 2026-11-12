//! XHTML and XML, as little as e-books need: the text to read (paragraphs on lines of their own,
//! scripts, styles and notes' markers left out), tags with their attributes, entities.

/// One tag: its name in lower case without a prefix, its attributes, and whether it closes.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub name: String,
    pub closing: bool,
    pub attrs: Vec<(String, String)>,
}

impl Tag {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name) || k.rsplit(':').next().is_some_and(|l| l.eq_ignore_ascii_case(name))).map(|(_, v)| v.as_str())
    }
}

pub enum Item<'a> {
    Tag(Tag),
    Text(&'a str),
}

/// The tags and text of a document, in order. Comments, CDATA markers, doctypes and processing
/// instructions are skipped (CDATA's text is kept).
pub fn items(doc: &str) -> Vec<Item<'_>> {
    let mut out = Vec::new();
    let mut rest = doc;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push(Item::Text(rest));
            break;
        };
        if lt > 0 {
            out.push(Item::Text(&rest[..lt]));
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |i| &r[i + 3..]);
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").unwrap_or(r.len());
            out.push(Item::Text(&r[..end]));
            rest = r.get(end + 3..).unwrap_or("");
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.find('>').map_or("", |i| &rest[i + 1..]);
        } else {
            let end = tag_end(rest);
            if let Some(t) = parse_tag(&rest[1..end]) {
                out.push(Item::Tag(t));
            }
            rest = rest.get(end + 1..).unwrap_or("");
        }
    }
    out
}

/// The `>` that ends a tag, past quoted attribute values.
fn tag_end(s: &str) -> usize {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return i,
            _ => {}
        }
    }
    s.len().saturating_sub(1)
}

fn parse_tag(inner: &str) -> Option<Tag> {
    let inner = inner.trim().trim_end_matches('/').trim();
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(r) => (true, r.trim()),
        None => (false, inner),
    };
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let full = &inner[..name_end];
    if full.is_empty() {
        return None;
    }
    let name = full.rsplit(':').next().unwrap_or(full).to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest.find(|c: char| c == '=' || c.is_whitespace()).unwrap_or(rest.len());
        let key = rest[..key_end].to_string();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(r) = rest.strip_prefix('=') {
            let r = r.trim_start();
            if let Some(q) = r.chars().next().filter(|c| *c == '"' || *c == '\'') {
                let body = &r[1..];
                let end = body.find(q).unwrap_or(body.len());
                value = decode(&body[..end]);
                rest = body.get(end + 1..).unwrap_or("").trim_start();
            } else {
                let end = r.find(char::is_whitespace).unwrap_or(r.len());
                value = decode(&r[..end]);
                rest = r[end..].trim_start();
            }
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    Some(Tag { name, closing, attrs })
}

/// Entities to characters: the five of XML, numbers, and the common ones of HTML.
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|&e| e <= 12);
        let ch = end.and_then(|e| {
            let name = &rest[1..e];
            if let Some(n) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(n, 16).ok().and_then(char::from_u32)
            } else if let Some(n) = name.strip_prefix('#') {
                n.parse().ok().and_then(char::from_u32)
            } else {
                Some(match name {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    "nbsp" => '\u{a0}',
                    "laquo" => '«',
                    "raquo" => '»',
                    "hellip" => '…',
                    "mdash" => '—',
                    "ndash" => '–',
                    "lsquo" => '‘',
                    "rsquo" => '’',
                    "ldquo" => '“',
                    "rdquo" => '”',
                    "shy" => '\u{ad}',
                    "zwnj" => '\u{200c}',
                    "zwj" => '\u{200d}',
                    "rlm" => '\u{200f}',
                    "lrm" => '\u{200e}',
                    _ => return None,
                })
            }
        });
        match (ch, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

const BLOCKS: &[&str] = &[
    "p", "div", "br", "h1", "h2", "h3", "h4", "h5", "h6", "li", "tr", "td", "th", "blockquote", "section", "article", "header", "footer", "aside", "figcaption", "dt", "dd", "pre", "hr", "table", "ul", "ol", "title",
];
const SKIPPED: &[&str] = &["script", "style", "head", "svg", "math", "rt", "rp"];

/// The text of an (X)HTML document to read: a line per paragraph, heading or list item.
/// Footnote markers (`<sup>` holding only a number, or an `epub:type="noteref"` link) are left out.
pub fn text(doc: &str) -> String {
    let mut out = String::new();
    let mut skip = 0usize;
    let mut skip_tag: Vec<String> = Vec::new();
    let all = items(doc);
    let mut i = 0;
    while i < all.len() {
        match &all[i] {
            Item::Tag(t) => {
                let noteref = t.attr("type").is_some_and(|v| v.split_whitespace().any(|w| w == "noteref")) || (t.name == "sup" && matches!(all.get(i + 1), Some(Item::Text(x)) if x.trim().chars().all(|c| c.is_ascii_digit() || matches!(c, '*' | '[' | ']' | '(' | ')') || ('\u{0660}'..='\u{0669}').contains(&c))));
                if !t.closing && (SKIPPED.contains(&t.name.as_str()) || noteref) {
                    skip += 1;
                    skip_tag.push(t.name.clone());
                } else if t.closing && skip_tag.last() == Some(&t.name) {
                    skip_tag.pop();
                    skip -= 1;
                } else if skip == 0 && BLOCKS.contains(&t.name.as_str()) && !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
            }
            Item::Text(x) if skip == 0 => {
                let t = decode(x);
                // white space as a browser shows it: runs become one space
                let mut last_space = out.ends_with(' ') || out.ends_with('\n') || out.is_empty();
                for c in t.chars() {
                    if c.is_whitespace() && c != '\u{a0}' {
                        if !last_space {
                            out.push(' ');
                            last_space = true;
                        }
                    } else {
                        out.push(if c == '\u{a0}' { ' ' } else { c });
                        last_space = false;
                    }
                }
            }
            Item::Text(_) => {}
        }
        i += 1;
    }
    out.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_of_an_e_book_as_text() {
        let doc = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" dir="rtl">
<head><title>فصل</title><style>p { margin: 0 }</style></head>
<body>
  <h1>الفصل الأول</h1>
  <p>مرحبًا، هذه   فقرة <b>مختلقة</b> فيها English
  وسطر ثانٍ&#160;و&amp;رمز.<sup>1</sup></p>
  <p>Notes<a epub:type="noteref" href="n.xhtml#n1">[2]</a> here &laquo;quoted&raquo;&#x21;</p>
  <script>var x = "<p>not this</p>";</script>
  <!-- <p>nor this</p> -->
  <ul><li>أول</li><li>ثانٍ</li></ul>
</body></html>"#;
        assert_eq!(text(doc), "الفصل الأول\nمرحبًا، هذه فقرة مختلقة فيها English وسطر ثانٍ و&رمز.\nNotes here «quoted»!\nأول\nثانٍ");
    }

    #[test]
    fn tags_and_attributes() {
        let i = items(r#"<item id="c1" href="text/ch%201.xhtml" media-type='application/xhtml+xml' properties="nav"/>"#);
        let Item::Tag(t) = &i[0] else { panic!() };
        assert_eq!((t.name.as_str(), t.attr("href"), t.attr("properties"), t.closing), ("item", Some("text/ch%201.xhtml"), Some("nav"), false));
        let i = items("<dc:title>كتاب &amp; Book</dc:title>");
        let Item::Tag(t) = &i[0] else { panic!() };
        assert_eq!(t.name, "title");
        assert!(matches!(&i[1], Item::Text(x) if decode(x) == "كتاب & Book"));
        assert_eq!(decode("&#1605;&unknown; & &#x41;"), "م&unknown; & A");
    }
}
