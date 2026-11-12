//! E-books (EPUB 2 and 3): the chapters in reading order (the spine), each named from the table
//! of contents (the EPUB 3 navigation page, or the EPUB 2 `toc.ncx`), else from its first heading.
//! Read with the `zip` Dawwin already has and `markup`; no GPL code (the `epub` crate is GPL).

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use super::markup::{self, Item};

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub title: String,
    /// The book's language, as it says ("ar", "en-GB"), if it says
    pub language: Option<String>,
    pub chapters: Vec<Chapter>,
}

struct Archive {
    zip: zip::ZipArchive<std::fs::File>,
}

impl Archive {
    fn read(&mut self, name: &str) -> Result<String> {
        let name = name.trim_start_matches('/');
        // some books write names %-encoded in the manifest
        let index = self.zip.index_for_name(name).or_else(|| self.zip.index_for_name(&percent_decode(name))).with_context(|| format!("{name} is missing from the book"))?;
        let mut f = self.zip.by_index(index)?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `href` from a file in `base_dir`, as a name in the archive.
fn join(base_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let mut parts: Vec<&str> = if base_dir.is_empty() { vec![] } else { base_dir.split('/').collect() };
    for p in href.split('/') {
        match p {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    percent_decode(&parts.join("/"))
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

pub fn read(file: &Path) -> Result<Book> {
    let f = std::fs::File::open(file)?;
    let mut a = Archive { zip: zip::ZipArchive::new(f).map_err(|_| anyhow!("{}", crate::tr!("This e-book is damaged, or is not an EPUB.")))? };
    // META-INF/container.xml names the package file
    let container = a.read("META-INF/container.xml").map_err(|_| anyhow!("{}", crate::tr!("This e-book is damaged, or is not an EPUB.")))?;
    let opf_path = markup::items(&container)
        .into_iter()
        .find_map(|i| match i {
            Item::Tag(t) if t.name == "rootfile" => t.attr("full-path").map(str::to_string),
            _ => None,
        })
        .ok_or_else(|| anyhow!("{}", crate::tr!("This e-book is damaged, or is not an EPUB.")))?;
    let opf = a.read(&opf_path)?;
    let base = dir_of(&opf_path).to_string();

    let (mut title, mut language) = (String::new(), None);
    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new(); // id -> (path, media type, properties)
    let mut spine: Vec<String> = Vec::new();
    let mut toc_id = None;
    let items = markup::items(&opf);
    for (k, i) in items.iter().enumerate() {
        let Item::Tag(t) = i else { continue };
        if t.closing {
            continue;
        }
        let next_text = || match items.get(k + 1) {
            Some(Item::Text(x)) => markup::decode(x).trim().to_string(),
            _ => String::new(),
        };
        match t.name.as_str() {
            "title" if title.is_empty() => title = next_text(),
            "language" if language.is_none() => language = Some(next_text()).filter(|l| !l.is_empty()),
            "item" => {
                if let (Some(id), Some(href)) = (t.attr("id"), t.attr("href")) {
                    manifest.insert(id.to_string(), (join(&base, href), t.attr("media-type").unwrap_or("").to_string(), t.attr("properties").unwrap_or("").to_string()));
                }
            }
            "spine" => toc_id = t.attr("toc").map(str::to_string),
            "itemref" => {
                if t.attr("linear") != Some("no") {
                    if let Some(id) = t.attr("idref") {
                        spine.push(id.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if spine.is_empty() {
        bail!("{}", crate::tr!("This e-book has no chapters to read."));
    }

    // chapter names, by the file they start in
    let mut names: HashMap<String, String> = HashMap::new();
    if let Some((nav, _, _)) = manifest.values().find(|(_, _, p)| p.split_whitespace().any(|w| w == "nav")) {
        let nav = nav.clone();
        if let Ok(doc) = a.read(&nav) {
            let nav_dir = dir_of(&nav).to_string();
            let its = markup::items(&doc);
            let mut in_toc = false;
            let mut k = 0;
            while k < its.len() {
                if let Item::Tag(t) = &its[k] {
                    if t.name == "nav" && !t.closing {
                        in_toc = t.attr("type").map_or(true, |v| v.split_whitespace().any(|w| w == "toc"));
                    } else if t.name == "nav" {
                        in_toc = false;
                    } else if in_toc && t.name == "a" && !t.closing {
                        if let Some(href) = t.attr("href") {
                            // the link's text, up to its end
                            let mut label = String::new();
                            let mut j = k + 1;
                            while j < its.len() && !matches!(&its[j], Item::Tag(e) if e.name == "a" && e.closing) {
                                if let Item::Text(x) = &its[j] {
                                    label.push_str(&markup::decode(x));
                                }
                                j += 1;
                            }
                            let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
                            names.entry(join(&nav_dir, href)).or_insert(label);
                        }
                    }
                }
                k += 1;
            }
        }
    } else if let Some((ncx, _, _)) = toc_id.and_then(|id| manifest.get(&id)).or_else(|| manifest.values().find(|(_, m, _)| m == "application/x-dtbncx+xml")) {
        let ncx = ncx.clone();
        if let Ok(doc) = a.read(&ncx) {
            let ncx_dir = dir_of(&ncx).to_string();
            let its = markup::items(&doc);
            let mut label: Option<String> = None;
            for (k, i) in its.iter().enumerate() {
                match i {
                    Item::Tag(t) if t.name == "text" && !t.closing && label.is_none() => {
                        if let Some(Item::Text(x)) = its.get(k + 1) {
                            label = Some(markup::decode(x).split_whitespace().collect::<Vec<_>>().join(" "));
                        }
                    }
                    Item::Tag(t) if t.name == "content" => {
                        if let (Some(src), Some(l)) = (t.attr("src"), label.take()) {
                            names.entry(join(&ncx_dir, src)).or_insert(l);
                        }
                    }
                    Item::Tag(t) if t.name == "navpoint" && !t.closing => label = None,
                    _ => {}
                }
            }
        }
    }

    let mut chapters = Vec::new();
    for id in &spine {
        let Some((path, media, _)) = manifest.get(id) else { continue };
        if !(media.contains("html") || media.is_empty()) {
            continue;
        }
        let Ok(doc) = a.read(path) else { continue };
        let text = markup::text(&doc);
        if text.trim().is_empty() {
            continue;
        }
        let title = names.get(path).cloned().filter(|n| !n.is_empty()).unwrap_or_else(|| {
            // the first line, if short, else "Chapter n"
            let first = text.lines().next().unwrap_or("");
            if first.chars().count() <= 80 {
                first.to_string()
            } else {
                crate::tr!("Chapter {}", chapters.len() + 1)
            }
        });
        chapters.push(Chapter { title, text });
    }
    if chapters.is_empty() {
        bail!("{}", crate::tr!("This e-book has no chapters to read."));
    }
    if title.is_empty() {
        title = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    }
    Ok(Book { title, language, chapters })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;

    /// A small EPUB of made-up text: `nav` an EPUB 3 navigation page, else an EPUB 2 toc.ncx.
    pub fn made_epub(path: &Path, nav: bool) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, body: &str, o| {
            z.start_file(name, o).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        };
        put("mimetype", "application/epub+zip", stored);
        put("META-INF/container.xml", r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#, deflated);
        let toc_item = if nav { r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"# } else { r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"# };
        put(
            "OEBPS/content.opf",
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>كتاب مختلق &amp; Made-up</dc:title><dc:language>ar</dc:language></metadata><manifest>{toc_item}<item id="c1" href="text/one.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/two%20b.xhtml" media-type="application/xhtml+xml"/><item id="cover" href="text/cover.xhtml" media-type="application/xhtml+xml"/></manifest><spine toc="ncx"><itemref idref="cover" linear="no"/><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#
            ),
            deflated,
        );
        put("OEBPS/text/cover.xhtml", "<html><body><p>Cover</p></body></html>", deflated);
        put("OEBPS/text/one.xhtml", r#"<html xmlns="http://www.w3.org/1999/xhtml" dir="rtl"><head><title>x</title></head><body><h1>البداية</h1><p>مرحبًا، هذا فصل مختلق فيه كلمة English؛ ونهاية.</p></body></html>"#, deflated);
        put("OEBPS/text/two b.xhtml", "<html><body><p>The second chapter, in English, with عربي inside.</p></body></html>", deflated);
        if nav {
            put("OEBPS/nav.xhtml", r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="landmarks"><ol><li><a href="text/cover.xhtml">Landmark</a></li></ol></nav><nav epub:type="toc"><ol><li><a href="text/one.xhtml">الفصل <b>الأول</b></a></li><li><a href="text/two%20b.xhtml#start">Chapter two</a></li></ol></nav></body></html>"#, deflated);
        } else {
            put("OEBPS/toc.ncx", r#"<ncx><navMap><navPoint id="p1"><navLabel><text>الفصل الأول</text></navLabel><content src="text/one.xhtml"/></navPoint><navPoint id="p2"><navLabel><text>Chapter two</text></navLabel><content src="text/two%20b.xhtml"/></navPoint></navMap></ncx>"#, deflated);
        }
        z.finish().unwrap();
    }

    #[test]
    fn an_e_book_gives_its_chapters_in_reading_order_with_their_names() {
        let dir = std::env::temp_dir().join(format!("dawwin-epub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for nav in [true, false] {
            let p = dir.join(format!("book-{nav}.epub"));
            made_epub(&p, nav);
            let b = read(&p).unwrap();
            assert_eq!(b.title, "كتاب مختلق & Made-up");
            assert_eq!(b.language.as_deref(), Some("ar"));
            assert_eq!(b.chapters.len(), 2, "the cover is not in the reading order");
            assert_eq!(b.chapters[0].title, "الفصل الأول");
            assert_eq!(b.chapters[0].text, "البداية\nمرحبًا، هذا فصل مختلق فيه كلمة English؛ ونهاية.");
            assert_eq!(b.chapters[1].title, "Chapter two");
            assert_eq!(b.chapters[1].text, "The second chapter, in English, with عربي inside.");
        }
        std::fs::write(dir.join("not.epub"), b"not a zip").unwrap();
        assert!(read(&dir.join("not.epub")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_inside_the_book() {
        assert_eq!(join("OEBPS", "text/two%20b.xhtml#x"), "OEBPS/text/two b.xhtml");
        assert_eq!(join("OEBPS/text", "../images/a.png"), "OEBPS/images/a.png");
        assert_eq!(join("", "./ch.xhtml"), "ch.xhtml");
    }
}
