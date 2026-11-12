//! Trial 2 of Dawwin 1.8.0: Arabic text out of PDFs. PDFium (through pdfium-render) against
//! pdf_oxide on PDFs made the ways real ones are made (make_pdfs.py, make_word.ps1), scored against
//! the text they were made from; and for the scan, Windows' own OCR on the page drawn by PDFium and
//! by Windows' own PDF renderer.
//!
//!   trial-pdf <folder with the PDFs and expected.json> <folder with pdfium.dll>

mod layout;

use std::path::{Path, PathBuf};
use std::time::Instant;

use unicode_normalization::UnicodeNormalization;

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let (dir, lib) = (&args[0], &args[1]);
    let expected: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    let mut blocks: Vec<String> = vec![expected["title"].as_str().unwrap().to_string()];
    blocks.extend(expected["paragraphs"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()));

    let pdfium = pdfium::open(lib);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "pdf")).collect();
    files.sort();
    println!("| File | Library | ms | Paragraphs whole | Errors (letters and marks) | Errors (letters only) | Start of the text |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let mut want = blocks.clone();
        if name == "word.pdf" {
            want.extend(["العمود الأول", "Second column", "خلية فيها 42 كلمة", "a cell, with عربي"].map(String::from));
        }
        let dump = dir.join("extracted");
        let _ = std::fs::create_dir_all(&dump);
        for (lib_name, got) in [("PDFium bounded", pdfium::text(&pdfium, f)), ("PDFium GetText", pdfium::get_text(&pdfium, f)), ("pdf_oxide", oxide::text(f))] {
            if let Ok((t, _)) = &got {
                let _ = std::fs::write(dump.join(format!("{name}.{}.txt", lib_name.replace(' ', "-"))), t);
            }
            match got {
                Ok((text, ms)) => println!("| {name} | {lib_name} | {ms} | {} | {} | {} | {} |", whole(&want, &text), cer(&want.join(""), &text, false), cer(&want.join(""), &text, true), start(&text)),
                Err(e) => println!("| {name} | {lib_name} | | | | | failed: {} |", start(&e)),
            }
        }
        use crate::layout::Rules;
        for (label, r) in [
            ("positions: gaps", Rules { ligatures: false, tight_lines: false, pdfium_spaces: false, gap_spaces: true }),
            ("positions: gaps + ligatures", Rules { ligatures: true, tight_lines: false, pdfium_spaces: false, gap_spaces: true }),
            ("positions: gaps + tight lines", Rules { ligatures: false, tight_lines: true, pdfium_spaces: false, gap_spaces: true }),
            ("positions: PDFium spaces", Rules { ligatures: false, tight_lines: false, pdfium_spaces: true, gap_spaces: false }),
            ("positions: PDFium spaces + ligatures", Rules { ligatures: true, tight_lines: false, pdfium_spaces: true, gap_spaces: false }),
        ] {
            if let Ok((text, ms)) = pdfium::positions(&pdfium, f, r) {
                let _ = std::fs::write(dump.join(format!("{name}.{}.txt", label.replace([' ', ':', '+'], "-"))), &text);
                println!("| {name} | PDFium, {label} | {ms} | {} | {} | {} | {} |", whole(&want, &text), cer(&want.join(""), &text, false), cer(&want.join(""), &text, true), start(&text));
            }
        }
    }

    // the scan: no text in it, so the page is drawn and read by Windows' OCR
    println!();
    println!("OCR languages on this PC: {}", ocr::languages().join(", "));
    let scan = dir.join("scan.pdf");
    if scan.is_file() {
        println!("| Drawn by | OCR language | ms | Paragraphs whole | Errors (letters and marks) | Errors (letters only) | Start of the text |");
        println!("| --- | --- | --- | --- | --- | --- | --- |");
        let drawn = [("PDFium", pdfium::draw(&pdfium, &scan, 2000)), ("Windows.Data.Pdf", ocr::draw_with_windows(&scan, 2000))];
        for (by, page) in drawn {
            let Ok((w, h, bgra)) = page else {
                println!("| {by} | | | | | | failed to draw |");
                continue;
            };
            for lang in ["ar", "en"] {
                let t = Instant::now();
                match ocr::read(&bgra, w, h, lang) {
                    Ok(text) => {
                        let ms = t.elapsed().as_millis();
                        let fixed: String = text.lines().map(|l| ocr::logical(l) + "\n").collect();
                        let _ = std::fs::write(dir.join("extracted").join(format!("ocr.{by}.{lang}.txt")), format!("{text}\n---\n{fixed}"));
                        println!("| {by} | {lang} | {ms} | {} | {} | {} | {} |", whole(&blocks, &text), cer(&blocks.join(""), &text, false), cer(&blocks.join(""), &text, true), start(&text));
                        println!("| {by} | {lang}, words put in reading order | {ms} | {} | {} | {} | {} |", whole(&blocks, &fixed), cer(&blocks.join(""), &fixed, false), cer(&blocks.join(""), &fixed, true), start(&fixed));
                    }
                    Err(e) => println!("| {by} | {lang} | | | | | {e} |"),
                }
            }
        }
    }
}

/// The text as compared: presentation forms to letters (NFKC), no tatweel, no direction marks,
/// no white space (lines break differently); and without diacritics when `letters`.
fn norm(s: &str, letters: bool) -> Vec<char> {
    s.nfkc()
        .filter(|c| !c.is_whitespace())
        .filter(|c| !matches!(*c, '\u{0640}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{00AD}'))
        .filter(|c| !letters || !matches!(*c, '\u{064B}'..='\u{0652}' | '\u{0670}'))
        .collect()
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Character errors against the expected text, in per cent.
fn cer(want: &str, got: &str, letters: bool) -> String {
    let (w, g) = (norm(want, letters), norm(got, letters));
    format!("{:.1} %", 100.0 * levenshtein(&w, &g) as f64 / w.len().max(1) as f64)
}

/// How many paragraphs come out whole and in their own order of letters.
fn whole(want: &[String], got: &str) -> String {
    let g: String = norm(got, false).into_iter().collect();
    let n = want.iter().filter(|p| g.contains(&norm(p, false).into_iter().collect::<String>())).count();
    format!("{n} of {}", want.len())
}

fn start(t: &str) -> String {
    let one = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = one.chars().take(70).collect();
    s.replace('|', "/")
}

mod pdfium {
    use super::*;
    use pdfium_render::prelude::*;

    pub fn open(lib: &Path) -> Pdfium {
        Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(lib)).expect("pdfium.dll"))
    }

    pub fn text(p: &Pdfium, f: &Path) -> Result<(String, u128), String> {
        let t = Instant::now();
        let doc = p.load_pdf_from_file(f, None).map_err(|e| e.to_string())?;
        let mut out = String::new();
        for page in doc.pages().iter() {
            out.push_str(&page.text().map_err(|e| e.to_string())?.all());
            out.push('\n');
        }
        Ok((out, t.elapsed().as_millis()))
    }

    /// The text through FPDFText_GetText, which orders the characters the way PDFium's own text
    /// page does, instead of by their boxes.
    pub fn get_text(p: &Pdfium, f: &Path) -> Result<(String, u128), String> {
        let t = Instant::now();
        let b = p.bindings();
        let mut out = String::new();
        unsafe {
            let doc = b.FPDF_LoadDocument(&f.display().to_string(), None);
            if doc.is_null() {
                return Err("could not open".into());
            }
            for i in 0..b.FPDF_GetPageCount(doc) {
                let page = b.FPDF_LoadPage(doc, i);
                let tp = b.FPDFText_LoadPage(page);
                let n = b.FPDFText_CountChars(tp);
                let mut buf = vec![0u16; n.max(0) as usize + 1];
                let got = b.FPDFText_GetText(tp, 0, n, buf.as_mut_ptr());
                out.push_str(&String::from_utf16_lossy(&buf[..(got.max(1) - 1) as usize]));
                out.push('\n');
                b.FPDFText_ClosePage(tp);
                b.FPDF_ClosePage(page);
            }
            b.FPDF_CloseDocument(doc);
        }
        Ok((out, t.elapsed().as_millis()))
    }

    /// Each character with its box, put in order by `layout`.
    pub fn positions(p: &Pdfium, f: &Path, r: crate::layout::Rules) -> Result<(String, u128), String> {
        let t = Instant::now();
        let b = p.bindings();
        let mut out = String::new();
        unsafe {
            let doc = b.FPDF_LoadDocument(&f.display().to_string(), None);
            if doc.is_null() {
                return Err("could not open".into());
            }
            for i in 0..b.FPDF_GetPageCount(doc) {
                let page = b.FPDF_LoadPage(doc, i);
                let tp = b.FPDFText_LoadPage(page);
                let mut glyphs = Vec::new();
                let mut space = false;
                for c in 0..b.FPDFText_CountChars(tp) {
                    let Some(ch) = char::from_u32(b.FPDFText_GetUnicode(tp, c)) else { continue };
                    if b.FPDFText_IsGenerated(tp, c) == 1 {
                        space |= ch.is_whitespace();
                        continue;
                    }
                    let space_before = std::mem::replace(&mut space, ch.is_whitespace());
                    // the loose box: the glyph's advance and the font's height, the same for every letter
                    let mut rect = FS_RECTF { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };
                    if b.FPDFText_GetLooseCharBox(tp, c, &mut rect) == 0 {
                        continue;
                    }
                    glyphs.push(crate::layout::Glyph { ch, left: rect.left as f64, right: rect.right as f64, bottom: rect.bottom as f64, top: rect.top as f64, space_before });
                }
                out.push_str(&crate::layout::text(&glyphs, r));
                out.push('\n');
                b.FPDFText_ClosePage(tp);
                b.FPDF_ClosePage(page);
            }
            b.FPDF_CloseDocument(doc);
        }
        Ok((out, t.elapsed().as_millis()))
    }

    /// The first page as BGRA pixels, `width` wide.
    pub fn draw(p: &Pdfium, f: &Path, width: i32) -> Result<(i32, i32, Vec<u8>), String> {
        let doc = p.load_pdf_from_file(f, None).map_err(|e| e.to_string())?;
        let page = doc.pages().get(0).map_err(|e| e.to_string())?;
        let bitmap = page.render_with_config(&PdfRenderConfig::new().set_target_width(width)).map_err(|e| e.to_string())?;
        let (w, h) = (bitmap.width() as i32, bitmap.height() as i32);
        Ok((w, h, bitmap.as_raw_bytes()))
    }
}

mod oxide {
    use super::*;

    pub fn text(f: &Path) -> Result<(String, u128), String> {
        let t = Instant::now();
        let doc = pdf_oxide::PdfDocument::open(f).map_err(|e| e.to_string())?;
        let mut out = String::new();
        for i in 0..doc.page_count().map_err(|e| e.to_string())? {
            out.push_str(&doc.extract_text(i).map_err(|e| e.to_string())?);
            out.push('\n');
        }
        Ok((out, t.elapsed().as_millis()))
    }
}

mod ocr {
    use windows::core::HSTRING;
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Storage::StorageFile;
    use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};

    /// Windows' OCR gives the words of a right-to-left line from left to right. Put them back in
    /// reading order: the line's runs reversed, a run of left-to-right words (Latin, numbers)
    /// kept in its own order.
    pub fn logical(line: &str) -> String {
        let arabic = |w: &str| w.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c) || ('\u{0750}'..='\u{077F}').contains(&c));
        let words: Vec<&str> = line.split_whitespace().collect();
        if !words.iter().any(|w| arabic(w)) {
            return words.join(" ");
        }
        let mut runs: Vec<Vec<&str>> = Vec::new();
        for w in words {
            match runs.last_mut() {
                Some(r) if !arabic(w) && !arabic(r[0]) => r.push(w),
                _ => runs.push(vec![w]),
            }
        }
        runs.reverse();
        runs.into_iter().map(|r| r.join(" ")).collect::<Vec<_>>().join(" ")
    }

    pub fn languages() -> Vec<String> {
        OcrEngine::AvailableRecognizerLanguages().map(|v| v.into_iter().map(|l| l.LanguageTag().map(|t| t.to_string()).unwrap_or_default()).collect()).unwrap_or_default()
    }

    pub fn read(bgra: &[u8], w: i32, h: i32, lang: &str) -> windows::core::Result<String> {
        let buffer = CryptographicBuffer::CreateFromByteArray(bgra)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, w, h)?;
        let engine = OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from(lang))?)?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        // a line each, as the page has them
        let mut out = String::new();
        for line in result.Lines()? {
            out.push_str(&line.Text()?.to_string());
            out.push('\n');
        }
        Ok(out)
    }

    /// The first page drawn by Windows' own PDF renderer, as BGRA pixels.
    pub fn draw_with_windows(f: &std::path::Path, width: u32) -> Result<(i32, i32, Vec<u8>), String> {
        (|| -> windows::core::Result<(i32, i32, Vec<u8>)> {
            let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(f.as_os_str()))?.join()?;
            let doc = PdfDocument::LoadFromFileAsync(&file)?.join()?;
            let page = doc.GetPage(0)?;
            let stream = InMemoryRandomAccessStream::new()?;
            let options = PdfPageRenderOptions::new()?;
            options.SetDestinationWidth(width)?;
            page.RenderWithOptionsToStreamAsync(&stream, &options)?.join()?;
            let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
            let bitmap = decoder.GetSoftwareBitmapAsync()?.join()?;
            let bitmap = SoftwareBitmap::ConvertWithAlpha(&bitmap, BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied)?;
            let (w, h) = (bitmap.PixelWidth()?, bitmap.PixelHeight()?);
            let buffer = windows::Storage::Streams::Buffer::Create((w * h * 4) as u32)?;
            bitmap.CopyToBuffer(&buffer)?;
            let reader = DataReader::FromBuffer(&buffer)?;
            let mut bytes = vec![0u8; buffer.Length()? as usize];
            reader.ReadBytes(&mut bytes)?;
            Ok((w, h, bytes))
        })()
        .map_err(|e| e.to_string())
    }
}
