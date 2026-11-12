# Trial 2 for 1.8.0: Arabic text out of PDFs

Which library gets Arabic out of a PDF in reading order, PDFium (through `pdfium-render`) or
`pdf_oxide`, and what reads a scan. Run on 2026-11-12, on PDFs of made-up Arabic with punctuation,
diacritics, Arabic-Indic digits and English words inside, made the ways real ones are made.

```bash
python make_pdfs.py <out>                      # Chrome's print to PDF, an old visual-order file, a naive one, a scan
powershell -File make_word.ps1 <out>           # Word's own Save as PDF, with a table (a hidden Word of its own)
cargo run --release -- <out> <folder of pdfium.dll>
```

`make_print.ps1` ("Microsoft Print to PDF") could not run: that printer is not installed on the
maintainer's PC. No InDesign or LibreOffice file was tried: neither program is here.

## Results

Errors are character errors against the text the PDF was made from, white space ignored (lines
break differently), in per cent; "letters only" leaves the diacritics out. "Positions" is PDFium's
characters put in order by where they sit on the page (`src/layout.rs`), with PDFium's own word
spaces.

| PDF | PDFium as it comes | PDFium, positions | pdf_oxide 0.3.78 |
| --- | --- | --- | --- |
| Chrome, print to PDF | 40.2 % (letters only 40.0 %) | **5.5 % (5.2 %)** | 67.3 % (64.9 %) |
| Word, Save as PDF, with a table | 38.1 % (35.3 %) | **22.5 % (22.0 %)** | 70.4 % (69.0 %) |
| Old file in visual order, presentation forms | 8.3 % (5.2 %) | **7.8 % (4.7 %)** | 71.6 % (70.9 %) |
| Logical order, letters not joined (looks wrong on screen too) | 69.6 % | 69.8 % | 77.1 % |
| Scan (a picture only) | no text | no text | no text |

The scan, drawn at 2000 pixels wide and read by Windows' own OCR (`Windows.Media.Ocr`, Arabic):

| Drawn by | As OCR gives it | Words put back in reading order |
| --- | --- | --- |
| PDFium | 66.8 % | **14.5 %** letters only, 0.3 s |
| Windows.Data.Pdf (built into Windows) | 67.1 % | 18.4 % |

OCR languages on the maintainer's PC: ar-SA, en-GB, en-US. The English recogniser on the same page
gives nonsense for the Arabic (74 %): the language must be chosen per picture.

## What this says

- **PDFium.** It is ahead on every file, and `pdf_oxide` 0.3.78 is not usable for Arabic: words and
  lines come out reversed or jumbled on all four kinds. (Its October 2026 Arabic fixes are not in a
  release yet.)
- **PDFium's own order is not enough.** Chrome writes some words' glyphs in drawing order, and
  PDFium gives them back reversed ("ةبرجت" for "تجربة"). Ordering each line's characters by their
  place on the page and then from visual into reading order fixes that (40 % to 5 %) and halves the
  damage in Word's file. What stays wrong in Word's file is in the file itself: its character map
  says "عىل" for "على" and "واإلنجليزية" for "والإنجليزية", which Acrobat copies the same way.
  So the app uses PDFium with the position ordering of `src/layout.rs`.
- **Ligatures kept as one** (`Rules::ligatures`) helps the visual-order file and hurts Word's: off.
- **A scan** needs its words turned round per line after OCR (`ocr::logical`): Windows' OCR gives an
  Arabic line's words from left to right. PDFium draws the page slightly better than Windows' own
  renderer for OCR; since the app has PDFium anyway, it draws with PDFium.
- **Size**: `pdfium.dll` is 7.8 MB unpacked, 4.0 MB as `pdfium-win-x64.tgz` (chromium/8152,
  sha256 `cee2b0e9…6fdfe3`). It comes as a tool, downloaded with the first PDF. Its license is
  BSD-3-Clause, with notices for the libraries inside it (FreeType under its FTL, ICU, zlib, libpng,
  libjpeg-turbo, OpenJPEG, LittleCMS, HarfBuzz, Abseil, Highway, AGG 2.3, simdutf, fast_float).
