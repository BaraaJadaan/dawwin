//! The files 1.8.0 adds to Read aloud: PDF, EPUB and pictures of text, each as parts (pages or
//! chapters) of text to read. PDFium is a tool (`tools::PDFIUM`), downloaded with the first PDF.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use serde::Serialize;

use super::{epub, pdf};
use crate::models::{Event, Finished};
use crate::setup::Paths;
use crate::tools::PDFIUM;

/// Where pdfium.dll lives. Test builds can point elsewhere (`DAWWIN_TEST_PDFIUM`).
pub fn pdfium_dir(paths: &Paths) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var_os("DAWWIN_TEST_PDFIUM") {
        return Some(PathBuf::from(d));
    }
    crate::tools::exe(paths, &PDFIUM).and_then(|e| e.parent().map(Path::to_path_buf))
}

pub fn pdfium_installed(paths: &Paths) -> bool {
    pdfium_dir(paths).is_some_and(|d| d.join("pdfium.dll").is_file())
}

/// Download, check and unpack PDFium. Paused, what arrived stays for the next time.
pub fn install_pdfium(paths: &Paths, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Finished> {
    crate::tools::install(paths, &PDFIUM, stop, on_event)
}

/// PDFium, bound once a run.
static BOUND: OnceLock<pdfium_render::prelude::Pdfium> = OnceLock::new();

fn pdfium(paths: &Paths) -> Result<&'static pdfium_render::prelude::Pdfium> {
    if let Some(p) = BOUND.get() {
        return Ok(p);
    }
    let dir = pdfium_dir(paths).ok_or_else(|| anyhow!("PDFium is not installed"))?;
    let p = pdf::pdfium(&dir)?;
    Ok(BOUND.get_or_init(|| p))
}

/// What Read aloud opens through this file, besides 1.7.0's text formats.
pub const EXTENSIONS: &[&str] = &["pdf", "epub", "png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "jfif", "webp", "heic"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Pdf,
    Epub,
    Picture,
}

pub fn kind(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    Some(match ext.as_str() {
        "pdf" => Kind::Pdf,
        "epub" => Kind::Epub,
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tif" | "tiff" | "jfif" | "webp" | "heic" => Kind::Picture,
        _ => return None,
    })
}

/// A page or a chapter.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Part {
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Opened {
    pub kind: Kind,
    pub title: String,
    pub parts: Vec<Part>,
    /// Some or all of it was read from pictures by Windows' recognition
    pub recognised: bool,
}

/// Why a file could not be opened, for the interface to act on.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum Refused {
    /// PDFs need PDFium, a download of this many bytes
    NeedsPdfium { size: u64 },
    /// Windows cannot read this language in pictures: add it in Windows' settings
    NoLanguage { language: String, message: String },
    Failed { message: String },
}

impl Refused {
    /// As Read aloud's error: "needs-pdfium" asks for the download, the rest is said as it is.
    pub fn for_interface(&self) -> String {
        match self {
            Refused::NeedsPdfium { .. } => "needs-pdfium".into(),
            Refused::NoLanguage { message, .. } | Refused::Failed { message } => message.clone(),
        }
    }
}

/// Lines of a page into paragraphs: lines run on, a word cut at a line's end joined again.
fn run_on(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if out.ends_with('-') && line.chars().next().is_some_and(|c| c.is_lowercase()) {
            out.pop();
        } else if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
    }
    out
}

/// The file as Read aloud's blocks: a book's chapters as headings with their paragraphs, a
/// PDF's pages and a picture's text as paragraphs.
pub fn read(paths: &Paths, path: &Path) -> std::result::Result<crate::reading::formats::Opened, Refused> {
    use crate::reading::formats::{Block, Kind as B, Opened as Read};
    let o = open(paths, path)?;
    let mut blocks = Vec::new();
    match o.kind {
        Kind::Epub => {
            for part in &o.parts {
                let mut lines = part.text.lines().map(str::trim).filter(|l| !l.is_empty()).peekable();
                // a chapter that begins with its own name says it once
                let title = part.title.trim();
                if !title.is_empty() {
                    if lines.peek().is_some_and(|l| *l == title) {
                        lines.next();
                    }
                    blocks.push(Block { kind: B::Heading, text: title.to_string(), speaker: None });
                }
                blocks.extend(lines.map(|l| Block { kind: B::Paragraph, text: l.to_string(), speaker: None }));
            }
        }
        Kind::Pdf | Kind::Picture => {
            for part in &o.parts {
                let text = run_on(&part.text);
                if !text.is_empty() {
                    blocks.push(Block { kind: B::Paragraph, text, speaker: None });
                }
            }
        }
    }
    if blocks.is_empty() {
        return Err(Refused::Failed { message: crate::tr!("There is nothing to read in it.") });
    }
    Ok(Read { title: o.title, blocks })
}

fn recognise(bgra: &[u8], w: i32, h: i32) -> std::result::Result<String, super::ocr::Fail> {
    #[cfg(windows)]
    return super::ocr::read_pixels(bgra, w, h);
    #[cfg(not(windows))]
    {
        let _ = (bgra, w, h);
        Err(super::ocr::Fail::Failed("only on Windows".into()))
    }
}

fn refused(f: super::ocr::Fail) -> Refused {
    match f {
        super::ocr::Fail::NoLanguage(l) => Refused::NoLanguage { language: l.to_string(), message: super::ocr::Fail::NoLanguage(l).message() },
        other => Refused::Failed { message: other.message() },
    }
}

pub fn open(paths: &Paths, path: &Path) -> std::result::Result<Opened, Refused> {
    let failed = |e: anyhow::Error| Refused::Failed { message: format!("{e:#}") };
    let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    match kind(path) {
        Some(Kind::Pdf) => {
            if !pdfium_installed(paths) {
                return Err(Refused::NeedsPdfium { size: PDFIUM.size });
            }
            let p = pdfium(paths).map_err(failed)?;
            let mut ocr_failed = None;
            let pages = pdf::read(p, path, |bgra, w, h| {
                recognise(bgra, w, h).map_err(|f| {
                    ocr_failed = Some(f.clone());
                    anyhow!(f.message())
                })
            });
            let pages = match (pages, ocr_failed) {
                (Ok(p), _) => p,
                (Err(_), Some(f)) => return Err(refused(f)),
                (Err(e), None) => return Err(failed(e)),
            };
            let recognised = pages.iter().any(|p| p.recognised);
            let parts = pages.into_iter().enumerate().map(|(i, p)| Part { title: crate::tr!("Page {}", i + 1), text: p.text }).collect();
            Ok(Opened { kind: Kind::Pdf, title, parts, recognised })
        }
        Some(Kind::Epub) => {
            let b = epub::read(path).map_err(failed)?;
            Ok(Opened { kind: Kind::Epub, title: b.title, parts: b.chapters.into_iter().map(|c| Part { title: c.title, text: c.text }).collect(), recognised: false })
        }
        Some(Kind::Picture) => {
            let bytes = std::fs::read(path).map_err(|e| failed(e.into()))?;
            #[cfg(windows)]
            let text = super::ocr::read_picture(&bytes).map_err(refused)?;
            #[cfg(not(windows))]
            let text = {
                let _ = bytes;
                return Err(Refused::Failed { message: "only on Windows".into() });
            };
            Ok(Opened { kind: Kind::Picture, title, parts: vec![Part { title: String::new(), text }], recognised: true })
        }
        None => Err(Refused::Failed { message: crate::tr!("Dawwin cannot read this kind of file aloud.") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_of_file_by_their_name() {
        assert_eq!(kind(Path::new("كتاب.EPUB")), Some(Kind::Epub));
        assert_eq!(kind(Path::new("a.pdf")), Some(Kind::Pdf));
        assert_eq!(kind(Path::new("scan.JPG")), Some(Kind::Picture));
        assert_eq!(kind(Path::new("notes.txt")), None, "text files are 1.7.0's");
        assert_eq!(kind(Path::new("noext")), None);
    }

    #[test]
    fn an_e_book_opens_as_its_chapters() {
        let dir = std::env::temp_dir().join(format!("dawwin-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let book = dir.join("book.epub");
        super::super::epub::tests::made_epub(&book, true);
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        let o = open(&paths, &book).unwrap();
        assert_eq!((o.kind, o.parts.len(), o.recognised), (Kind::Epub, 2, false));
        assert_eq!(o.parts[0].title, "الفصل الأول");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_book_reads_as_chapters_and_pages_run_on() {
        use crate::reading::formats::Kind as B;
        let dir = std::env::temp_dir().join(format!("dawwin-files-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let book = dir.join("book.epub");
        super::super::epub::tests::made_epub(&book, true);
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        let r = read(&paths, &book).unwrap();
        let kinds: Vec<B> = r.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(kinds, [B::Heading, B::Paragraph, B::Paragraph, B::Heading, B::Paragraph]);
        assert_eq!(r.blocks[0].text, "الفصل الأول");
        assert_eq!(run_on("A word cut at the end of a line, exam-\nple, and\n  the next line."), "A word cut at the end of a line, example, and the next line.");
        assert_eq!(Refused::NeedsPdfium { size: 1 }.for_interface(), "needs-pdfium");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pdf_without_pdfium_asks_for_it() {
        let dir = std::env::temp_dir().join(format!("dawwin-files-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        if std::env::var_os("DAWWIN_TEST_PDFIUM").is_none() {
            assert_eq!(open(&paths, &dir.join("x.pdf")), Err(Refused::NeedsPdfium { size: PDFIUM.size }));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
