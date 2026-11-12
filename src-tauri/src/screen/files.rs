//! The files 1.8.0 adds to Read aloud: PDF, EPUB and pictures of text, each as parts (pages or
//! chapters) of text to read. PDFium comes as a tool, downloaded with the first PDF: the official
//! Windows build of pdfium-binaries, pinned by size and SHA-256, unpacked with its licenses.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use super::{epub, pdf};
use crate::models::{self, Event, Finished};
use crate::setup::Paths;

/// pdfium-binaries, release chromium/8152 (2026-11-10): BSD-3-Clause, with the notices of the
/// libraries inside it.
pub const PDFIUM_URL: &str = "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8152/pdfium-win-x64.tgz";
pub const PDFIUM_SIZE: u64 = 4_037_307;
pub const PDFIUM_SHA256: &str = "cee2b0e9990bc67c0721774932961b9dbf2766046c10eec925acd4cfba6fdfe3";

/// Where pdfium.dll lives. Test builds can point elsewhere (`DAWWIN_TEST_PDFIUM`).
pub fn pdfium_dir(paths: &Paths) -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var_os("DAWWIN_TEST_PDFIUM") {
        return PathBuf::from(d);
    }
    paths.local.join("tools").join("pdfium")
}

pub fn pdfium_installed(paths: &Paths) -> bool {
    pdfium_dir(paths).join("pdfium.dll").is_file()
}

/// Download, check and unpack PDFium. Paused, what arrived stays for the next time.
pub fn install_pdfium(paths: &Paths, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Finished> {
    let dir = pdfium_dir(paths);
    let archive = dir.with_extension("tgz");
    let job = models::Download { url: PDFIUM_URL.into(), dest: archive.clone(), size: PDFIUM_SIZE, sha256: PDFIUM_SHA256.into() };
    if models::download(&job, stop, on_event)? == Finished::Paused {
        return Ok(Finished::Paused);
    }
    let unpacked = unpack_pdfium(&archive, &dir);
    let _ = std::fs::remove_file(&archive);
    unpacked?;
    Ok(Finished::Complete)
}

/// From the .tgz: `bin/pdfium.dll`, `LICENSE` and `licenses/*`, nothing else, as plain files.
fn unpack_pdfium(archive: &Path, dir: &Path) -> Result<()> {
    let fresh = dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(fresh.join("licenses"))?;
    let result = (|| -> Result<()> {
        let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(std::fs::File::open(archive)?));
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries()? {
            let mut entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry.path()?.to_string_lossy().replace('\\', "/");
            let path = path.trim_start_matches("./");
            let to = match path.split('/').collect::<Vec<_>>().as_slice() {
                ["bin", "pdfium.dll"] => fresh.join("pdfium.dll"),
                ["LICENSE"] => fresh.join("LICENSE.txt"),
                ["licenses", name] if !name.is_empty() && !name.contains("..") => fresh.join("licenses").join(name),
                _ => continue,
            };
            entry.set_preserve_mtime(false);
            entry.unpack(&to).with_context(|| path.to_string())?;
        }
        if !fresh.join("pdfium.dll").is_file() {
            return Err(anyhow!("the download has no pdfium.dll"));
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|_| anyhow!(crate::tr!("{} is in use. Close what uses it and try again.", dir.display())))?;
    }
    std::fs::rename(&fresh, dir)?;
    Ok(())
}

static PDFIUM: OnceLock<pdfium_render::prelude::Pdfium> = OnceLock::new();

fn pdfium(paths: &Paths) -> Result<&'static pdfium_render::prelude::Pdfium> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let p = pdf::pdfium(&pdfium_dir(paths))?;
    Ok(PDFIUM.get_or_init(|| p))
}

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
                return Err(Refused::NeedsPdfium { size: PDFIUM_SIZE });
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

    /// A .tgz laid out as pdfium-binaries' is: only the DLL and the licenses are taken.
    #[test]
    fn pdfium_is_unpacked_with_its_licenses_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("dawwin-pdfium-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tgz = dir.join("pdfium.tgz");
        {
            let gz = flate2::write::GzEncoder::new(std::fs::File::create(&tgz).unwrap(), flate2::Compression::fast());
            let mut t = tar::Builder::new(gz);
            for (name, body) in [("bin/pdfium.dll", "dll"), ("LICENSE", "BSD"), ("licenses/icu.txt", "ICU"), ("include/fpdfview.h", "h"), ("../evil.txt", "x")] {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o644);
                h.set_entry_type(tar::EntryType::Regular);
                // written raw: the builder refuses ".." itself, a hostile archive would not
                let bytes = name.as_bytes();
                h.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
                h.set_cksum();
                t.append(&h, body.as_bytes()).unwrap();
            }
            t.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.join("pdfium");
        unpack_pdfium(&tgz, &out).unwrap();
        let mut names: Vec<String> = walk(&out);
        names.sort();
        assert_eq!(names, ["LICENSE.txt", "licenses/icu.txt", "pdfium.dll"]);
        assert!(!dir.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn walk(d: &Path) -> Vec<String> {
        let mut v = Vec::new();
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() {
                v.extend(walk(&e.path()).into_iter().map(|x| format!("{n}/{x}")));
            } else {
                v.push(n);
            }
        }
        v
    }

    #[test]
    fn a_pdf_without_pdfium_asks_for_it() {
        let dir = std::env::temp_dir().join(format!("dawwin-files-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        if std::env::var_os("DAWWIN_TEST_PDFIUM").is_none() {
            assert_eq!(open(&paths, &dir.join("x.pdf")), Err(Refused::NeedsPdfium { size: PDFIUM_SIZE }));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
