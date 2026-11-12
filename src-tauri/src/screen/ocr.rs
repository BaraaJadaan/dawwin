//! Pictures of text, through Windows' own recognition (`Windows.Media.Ocr`): nothing to download,
//! and Arabic once the Arabic language (with its text recognition) is added to Windows. Nothing is
//! shipped for it in 1.8.0. The language of a picture is found by reading it: Arabic first when
//! Windows has it, English when what came back is not Arabic.

/// Why a picture could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// Windows has no recognition for this language: the user adds it in Windows' settings
    NoLanguage(&'static str),
    Failed(String),
}

impl Fail {
    pub fn message(&self) -> String {
        match self {
            Fail::NoLanguage("ar") => crate::tr!("Windows cannot read Arabic in pictures yet. Add Arabic in Windows Settings, Time & language, Language & region, then try again."),
            Fail::NoLanguage(_) => crate::tr!("Windows cannot read English in pictures yet. Add English in Windows Settings, Time & language, Language & region, then try again."),
            Fail::Failed(e) => crate::tr!("The picture could not be read: {}", e),
        }
    }
}

fn arabic_letter(c: char) -> bool {
    matches!(c, '\u{0620}'..='\u{064A}' | '\u{066E}'..='\u{06D3}' | '\u{0750}'..='\u{077F}')
}

/// How much of the text's letters are Arabic.
pub fn arabic_share(text: &str) -> f64 {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    if letters == 0 {
        return 0.0;
    }
    text.chars().filter(|&c| arabic_letter(c)).count() as f64 / letters as f64
}

/// What English recognition makes of a picture of Arabic: bits of letters glued to signs.
pub fn looks_misread(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }
    let clean = words
        .iter()
        .filter(|w| {
            let core = w.trim_matches(|c: char| c.is_ascii_punctuation());
            !core.is_empty() && (core.chars().all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-') || core.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '%')))
        })
        .count();
    (clean as f64) < 0.6 * words.len() as f64
}

/// Windows' recognition gives the words of a right-to-left line from left to right: the line's
/// runs back in reading order, a run of left-to-right words (Latin, numbers) kept as it is.
pub fn logical(line: &str) -> String {
    let arabic = |w: &str| w.chars().any(arabic_letter);
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

/// Choose between what each language read: Arabic when it found Arabic, English otherwise.
/// `ar` and `en` are None when Windows has no recognition for that language.
pub fn choose(ar: Option<String>, en: Option<String>) -> Result<String, Fail> {
    let arabic = |a: String| a.lines().map(logical).collect::<Vec<_>>().join("\n");
    match (ar, en) {
        (Some(a), None) => Ok(arabic(a)),
        (Some(a), Some(e)) => Ok(if arabic_share(&a) >= 0.3 { arabic(a) } else { e }),
        (None, Some(e)) if looks_misread(&e) => Err(Fail::NoLanguage("ar")),
        (None, Some(e)) => Ok(e),
        (None, None) => Err(Fail::NoLanguage("en")),
    }
}

#[cfg(windows)]
mod sys {
    use super::*;
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    fn has(lang: &str) -> bool {
        Language::CreateLanguage(&HSTRING::from(lang)).and_then(|l| OcrEngine::IsLanguageSupported(&l)).unwrap_or(false)
    }

    fn recognise(bitmap: &SoftwareBitmap, lang: &str) -> windows::core::Result<String> {
        let engine = OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from(lang))?)?;
        let result = engine.RecognizeAsync(bitmap)?.join()?;
        let mut out = Vec::new();
        for line in result.Lines()? {
            out.push(line.Text()?.to_string());
        }
        Ok(out.join("\n"))
    }

    fn read_bitmap(bitmap: &SoftwareBitmap) -> Result<String, Fail> {
        let f = |e: windows::core::Error| Fail::Failed(e.message());
        let ar = if has("ar") { Some(recognise(bitmap, "ar").map_err(f)?) } else { None };
        // English only when Arabic did not find Arabic, or is not there
        let en = if has("en") && ar.as_deref().map_or(true, |a| arabic_share(a) < 0.3) { Some(recognise(bitmap, "en").map_err(f)?) } else { None };
        choose(ar, en)
    }

    /// BGRA pixels, as PDFium draws a page.
    pub fn read_pixels(bgra: &[u8], width: i32, height: i32) -> Result<String, Fail> {
        let f = |e: windows::core::Error| Fail::Failed(e.message());
        let buffer = CryptographicBuffer::CreateFromByteArray(bgra).map_err(f)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width, height).map_err(f)?;
        read_bitmap(&bitmap)
    }

    /// A picture file (PNG, JPEG, BMP, GIF, TIFF, and what else Windows decodes).
    pub fn read_picture(bytes: &[u8]) -> Result<String, Fail> {
        let f = |e: windows::core::Error| Fail::Failed(e.message());
        let bitmap = (|| -> windows::core::Result<SoftwareBitmap> {
            let stream = InMemoryRandomAccessStream::new()?;
            let writer = DataWriter::CreateDataWriter(&stream)?;
            writer.WriteBytes(bytes)?;
            writer.StoreAsync()?.join()?;
            writer.FlushAsync()?.join()?;
            writer.DetachStream()?;
            stream.Seek(0)?;
            let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
            let (w, h) = (decoder.OrientedPixelWidth()?, decoder.OrientedPixelHeight()?);
            let max = OcrEngine::MaxImageDimension()?;
            let transform = BitmapTransform::new()?;
            if w.max(h) > max {
                let scale = max as f64 / w.max(h) as f64;
                transform.SetScaledWidth((w as f64 * scale) as u32)?;
                transform.SetScaledHeight((h as f64 * scale) as u32)?;
                transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
            }
            decoder
                .GetSoftwareBitmapTransformedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied, &transform, ExifOrientationMode::RespectExifOrientation, ColorManagementMode::DoNotColorManage)?
                .join()
        })()
        .map_err(f)?;
        read_bitmap(&bitmap)
    }
}

#[cfg(windows)]
#[allow(unused_imports)]
pub use sys::{read_picture, read_pixels};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arabic_line_comes_back_in_reading_order() {
        // as Windows' recognition gave it in trial 2
        assert_eq!(logical("PDF قراءة تجربة"), "تجربة قراءة PDF");
        assert_eq!(logical("(offline)، نفسه الحاسوب على Dawwin برنامج يعمل"), "يعمل برنامج Dawwin على الحاسوب نفسه (offline)،");
        assert_eq!(logical("من 1.8.0 النسخة Windows 11 الأول"), "الأول Windows 11 النسخة 1.8.0 من");
        assert_eq!(logical("An English line"), "An English line");
    }

    #[test]
    fn the_language_of_a_picture_is_the_one_that_read_it() {
        let ar = "PDF قراءة تجربة\nللتجربة مختلقة فقرة هذه";
        assert_eq!(choose(Some(ar.into()), None).unwrap(), "تجربة قراءة PDF\nهذه فقرة مختلقة للتجربة");
        assert_eq!(choose(Some("a few odd أ letters".into()), Some("A clean English page.".into())).unwrap(), "A clean English page.");
        // no Arabic in Windows, and English made nonsense of the picture
        let misread = "PDF (offline) (_,d.c Dawwin J.-o.n-J :äæp-i-LJ .2026 J14J 1,200 0/025.";
        assert!(looks_misread(misread));
        assert_eq!(choose(None, Some(misread.into())), Err(Fail::NoLanguage("ar")));
        assert_eq!(choose(None, Some("Plain English, read well: 42 words.".into())).unwrap(), "Plain English, read well: 42 words.");
        assert_eq!(choose(None, None), Err(Fail::NoLanguage("en")));
    }
}
