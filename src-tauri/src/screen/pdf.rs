//! PDFs, through PDFium (trial 2 chose it: tools/trials/pdf). Each page's characters are put in
//! reading order by where they sit (`layout`), so Arabic comes out right whatever order the file
//! keeps them in. A page without text (a scan) is drawn and read by Windows' recognition.

use std::path::Path;

use anyhow::{bail, Result};
use pdfium_render::prelude::*;

use super::layout::{self, Glyph, Rules};

/// One page's text, and how it was found.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub text: String,
    /// Read from a picture of the page
    pub recognised: bool,
}

/// Bind PDFium from the folder its DLL is in. Bound once per run.
pub fn pdfium(dir: &Path) -> Result<Pdfium> {
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir)).map_err(|e| anyhow::anyhow!("PDFium: {e}"))?;
    Ok(Pdfium::new(bindings))
}

/// Letters on a page: fewer than this and it is taken as a picture of text.
const SCANNED_BELOW: usize = 8;

/// The pages' text, in reading order. `recognise` reads a page drawn as BGRA pixels; it is
/// called only for pages without text.
pub fn read(p: &Pdfium, file: &Path, mut recognise: impl FnMut(&[u8], i32, i32) -> Result<String>) -> Result<Vec<Page>> {
    let b = p.bindings();
    let mut pages = Vec::new();
    // SAFETY: PDFium's own calls, on handles opened here and closed before returning
    unsafe {
        let doc = b.FPDF_LoadDocument(&file.display().to_string(), None);
        if doc.is_null() {
            match b.FPDF_GetLastError() as u32 {
                4 => bail!("{}", crate::tr!("This PDF is locked with a password.")),
                3 => bail!("{}", crate::tr!("This PDF is damaged, or is not a PDF.")),
                _ => bail!("{}", crate::tr!("This PDF could not be opened.")),
            }
        }
        for i in 0..b.FPDF_GetPageCount(doc) {
            let page = b.FPDF_LoadPage(doc, i);
            if page.is_null() {
                continue;
            }
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
                // the loose box: the glyph's advance and the font's height
                let mut r = FS_RECTF { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };
                if b.FPDFText_GetLooseCharBox(tp, c, &mut r) == 0 {
                    continue;
                }
                glyphs.push(Glyph { ch, left: r.left as f64, right: r.right as f64, bottom: r.bottom as f64, top: r.top as f64, space_before });
            }
            b.FPDFText_ClosePage(tp);
            b.FPDF_ClosePage(page);
            let text = layout::text(&glyphs, Rules::CHOSEN);
            if text.chars().filter(|c| c.is_alphabetic()).count() >= SCANNED_BELOW {
                pages.push(Page { text, recognised: false });
                continue;
            }
            // a scan: the page drawn 2000 pixels wide, as trial 2 read it best
            let drawn = (|| -> Result<(i32, i32, Vec<u8>)> {
                let pdf = p.load_pdf_from_file(file, None)?;
                let page = pdf.pages().get(i)?;
                let bitmap = page.render_with_config(&PdfRenderConfig::new().set_target_width(2000))?;
                Ok((bitmap.width() as i32, bitmap.height() as i32, bitmap.as_raw_bytes()))
            })();
            match drawn {
                Ok((w, h, bgra)) => match recognise(&bgra, w, h) {
                    Ok(text) => pages.push(Page { text, recognised: true }),
                    Err(e) => {
                        b.FPDF_CloseDocument(doc);
                        return Err(e);
                    }
                },
                Err(_) => pages.push(Page { text, recognised: false }),
            }
        }
        b.FPDF_CloseDocument(doc);
    }
    Ok(pages)
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF of made-up text written by hand: a standard font whose character map says which
    /// letter each code is, and each glyph placed on its own, in the order given (the order a
    /// producer keeps them in) at the place given (where it is drawn).
    pub fn made_pdf(glyphs: &[(char, f64, f64)]) -> Vec<u8> {
        let mut letters: Vec<char> = glyphs.iter().map(|g| g.0).collect();
        letters.sort();
        letters.dedup();
        let code = |c: char| letters.iter().position(|&l| l == c).unwrap() + 0x41;
        let mut content = String::from("BT /F1 12 Tf\n");
        for &(c, x, y) in glyphs {
            content += &format!("1 0 0 1 {x} {y} Tm <{:02X}> Tj\n", code(c));
        }
        content += "ET";
        let cmap = format!(
            "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Adobe-Identity-UCS def /CMapType 2 def 1 begincodespacerange <00> <FF> endcodespacerange {} beginbfchar {} endbfchar endcmap CMapName currentdict /CMap defineresource pop end end",
            letters.len(),
            letters.iter().enumerate().map(|(i, c)| format!("<{:02X}> <{:04X}>", i + 0x41, *c as u32)).collect::<Vec<_>>().join(" ")
        );
        let widths: String = letters.iter().map(|_| "600 ").collect();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 400] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
            format!("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /FirstChar 65 /LastChar {} /Widths [{widths}] /ToUnicode 7 0 R >>", 0x40 + letters.len()),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len() + 1),
            "<< >>".to_string(),
            format!("<< /Length {} >>\nstream\n{cmap}\nendstream", cmap.len() + 1),
        ];
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for o in offsets {
            pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        pdf
    }

    /// A line drawn as a page shows it (left to right), its glyphs kept in `order`.
    fn line(visual: &str, y: f64, order: impl Fn(usize) -> usize) -> Vec<(char, f64, f64)> {
        let chars: Vec<(usize, char)> = visual.chars().enumerate().filter(|(_, c)| *c != ' ').collect();
        let mut out: Vec<(char, f64, f64)> = chars.iter().map(|&(i, c)| (c, 40.0 + i as f64 * 7.2, y)).collect();
        let n = out.len();
        let idx: Vec<usize> = (0..n).map(|k| order(k) % n).collect();
        let mut seen = vec![false; n];
        let mut ordered = Vec::new();
        for k in idx.into_iter().chain(0..n) {
            if !seen[k] {
                seen[k] = true;
                ordered.push(out[k]);
            }
        }
        out = ordered;
        out
    }

    /// Needs pdfium.dll: `DAWWIN_TEST_PDFIUM=<its folder> cargo test pdf -- --ignored`.
    #[test]
    #[ignore]
    fn arabic_comes_out_in_reading_order_whatever_order_the_file_keeps() {
        let dir = std::env::var("DAWWIN_TEST_PDFIUM").expect("DAWWIN_TEST_PDFIUM: the folder of pdfium.dll");
        let p = pdfium(Path::new(&dir)).unwrap();
        let tmp = std::env::temp_dir().join(format!("dawwin-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // "مرحبا، هذا Dawwin 1.8 للتجربة." as the page shows it, from left to right
        let visual = ".ةبرجتلل Dawwin 1.8 اذه ،ابحرم";
        for (name, order) in [("drawing order", 0usize), ("reading order", 1), ("jumbled", 2)] {
            let mut glyphs = line(visual, 300.0, |k| match order {
                0 => k,
                1 => usize::MAX - k,
                _ => k * 7 + 3,
            });
            // an English line with an Arabic word, which the page shows reversed
            glyphs.extend(line("An English line, with يبرع inside.", 270.0, |k| k));
            let f = tmp.join(format!("{order}.pdf"));
            std::fs::write(&f, made_pdf(&glyphs)).unwrap();
            let pages = read(&p, &f, |_, _, _| Ok("(drawn for recognition)".into())).unwrap();
            assert!(!pages[0].recognised, "{name}: a page with text is not drawn ({})", f.display());
            assert_eq!(pages.len(), 1);
            let lines: Vec<&str> = pages[0].text.lines().collect();
            assert_eq!(lines[0], "مرحبا، هذا Dawwin 1.8 للتجربة.", "{name}: {:?}", pages[0].text);
            assert_eq!(lines[1], "An English line, with عربي inside.", "{name}");
        }
        // a page without text goes to recognition
        let f = tmp.join("blank.pdf");
        std::fs::write(&f, made_pdf(&[('.', 10.0, 10.0)])).unwrap();
        let pages = read(&p, &f, |bgra, w, h| {
            assert_eq!(bgra.len(), (w * h * 4) as usize);
            Ok("read from the picture".into())
        })
        .unwrap();
        assert_eq!(pages, vec![Page { text: "read from the picture".into(), recognised: true }]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
