//! The fonts burned-in subtitles can be drawn in: the PC's installed fonts that hold the letters
//! of Arabic and of English, and that libass draws Arabic with correctly.
//!
//! A font file is read as far as its family name (`name`) and which characters it has (`cmap`).
//! Holding the letters is not enough: with libass, many of Windows' fonts draw a box before every
//! lam-alef ("ث□لا"), though HarfBuzz shapes them as it should. The sentence "ثلاث لا ثلاثاء، الـ
//! login." was burned with libass 0.17 on 2026-11-12 in each Windows font that has Arabic letters:
//! it came out right only in those of `DRAW_ARABIC_RIGHT`; Arial, Times New Roman, Courier New,
//! Sakkal Majalla, Arabic Typesetting, Aldhabi, Urdu Typesetting, Dubai, Microsoft Uighur,
//! Palestine and KacstFarsi broke. Dawwin's own ffmpeg cannot hand a picture back, so the app
//! cannot burn the sentence on each PC to see: only fonts seen right are offered.

use std::path::PathBuf;
use std::sync::Mutex;

/// Fonts libass draws the test sentence right in.
const DRAW_ARABIC_RIGHT: &[&str] = &[
    "Segoe UI", "Tahoma", "Microsoft Sans Serif", "Calibri", "Traditional Arabic", "Simplified Arabic", "Simplified Arabic Fixed", "Andalus",
];

/// The letters a font must have: of the test sentence, and of English.
const LETTERS: &str = "ثلاءابتسمو،؟ـAZaz.";

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Where each font of a file starts: one, or each of a collection's (`ttcf`).
fn faces(b: &[u8]) -> Vec<usize> {
    if b.get(0..4) == Some(b"ttcf") {
        let n = u32_at(b, 8).unwrap_or(0).min(64) as usize;
        (0..n).filter_map(|i| u32_at(b, 12 + 4 * i).map(|o| o as usize)).collect()
    } else {
        vec![0]
    }
}

/// A table of the font at `face`: where it starts and how long it is.
fn table(b: &[u8], face: usize, tag: &[u8; 4]) -> Option<(usize, usize)> {
    let n = u16_at(b, face + 4)? as usize;
    (0..n).find_map(|i| {
        let rec = face + 12 + 16 * i;
        (b.get(rec..rec + 4)? == tag).then(|| Some((u32_at(b, rec + 8)? as usize, u32_at(b, rec + 12)? as usize)))?
    })
}

/// The family name, in English, as Windows' own font list gives it (name 16, else 1).
fn family(b: &[u8], face: usize) -> Option<String> {
    let (t, len) = table(b, face, b"name")?;
    let n = u16_at(b, t + 2)? as usize;
    let strings = t + u16_at(b, t + 4)? as usize;
    let mut best: Option<(u8, String)> = None;
    for i in 0..n {
        let r = t + 6 + 12 * i;
        let (platform, encoding, lang, id) = (u16_at(b, r)?, u16_at(b, r + 2)?, u16_at(b, r + 4)?, u16_at(b, r + 6)?);
        let (size, offset) = (u16_at(b, r + 8)? as usize, u16_at(b, r + 10)? as usize);
        if platform != 3 || !(encoding == 1 || encoding == 0) || (id != 1 && id != 16) || strings + offset + size > t + len {
            continue;
        }
        let units: Vec<u16> = b.get(strings + offset..strings + offset + size)?.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        let name = String::from_utf16_lossy(&units).trim().to_string();
        // the typographic family before the legacy one, English before any other language
        let rank = (if id == 16 { 2 } else { 0 }) + u8::from(lang == 0x409);
        if !name.is_empty() && best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, name));
        }
    }
    best.map(|(_, n)| n)
}

/// The glyph of `c` in the font's Unicode character map (format 4 or 12): whether it has one.
fn has_char(b: &[u8], face: usize, c: char) -> bool {
    let Some((t, _)) = table(b, face, b"cmap") else { return false };
    let n = u16_at(b, t + 2).unwrap_or(0) as usize;
    let cp = c as u32;
    for i in 0..n {
        let r = t + 4 + 8 * i;
        let (Some(platform), Some(encoding), Some(offset)) = (u16_at(b, r), u16_at(b, r + 2), u32_at(b, r + 4)) else { continue };
        if !(platform == 3 && (encoding == 1 || encoding == 10)) && platform != 0 {
            continue;
        }
        let sub = t + offset as usize;
        match u16_at(b, sub) {
            Some(4) if cp <= 0xFFFF => {
                let segs = u16_at(b, sub + 6).unwrap_or(0) as usize / 2;
                let (ends, starts) = (sub + 14, sub + 16 + 2 * segs);
                let (deltas, ranges) = (starts + 2 * segs, starts + 4 * segs);
                for k in 0..segs {
                    let (Some(end), Some(start)) = (u16_at(b, ends + 2 * k), u16_at(b, starts + 2 * k)) else { break };
                    if (start as u32..=end as u32).contains(&cp) {
                        let delta = u16_at(b, deltas + 2 * k).unwrap_or(0);
                        let range = u16_at(b, ranges + 2 * k).unwrap_or(0) as usize;
                        let glyph = if range == 0 {
                            (cp as u16).wrapping_add(delta)
                        } else {
                            let at = ranges + 2 * k + range + 2 * (cp as usize - start as usize);
                            match u16_at(b, at) {
                                Some(0) | None => 0,
                                Some(g) => g.wrapping_add(delta),
                            }
                        };
                        if glyph != 0 {
                            return true;
                        }
                    }
                }
            }
            Some(12) => {
                let groups = u32_at(b, sub + 12).unwrap_or(0) as usize;
                for k in 0..groups.min(100_000) {
                    let g = sub + 16 + 12 * k;
                    if let (Some(a), Some(z)) = (u32_at(b, g), u32_at(b, g + 4)) {
                        if (a..=z).contains(&cp) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// The families of a font file that hold every letter of `LETTERS`.
fn usable_families(bytes: &[u8]) -> Vec<String> {
    faces(bytes)
        .into_iter()
        .filter(|&f| LETTERS.chars().all(|c| has_char(bytes, f, c)))
        .filter_map(|f| family(bytes, f))
        .collect()
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(w) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(w).join("Fonts"));
    }
    if let Some(l) = dirs::data_local_dir() {
        dirs.push(l.join("Microsoft").join("Windows").join("Fonts"));
    }
    dirs
}

/// The fonts subtitles can be burned in on this PC, by family name, sorted.
pub fn for_subtitles() -> Vec<String> {
    static FOUND: Mutex<Option<Vec<String>>> = Mutex::new(None);
    if let Some(f) = FOUND.lock().unwrap().clone() {
        return f;
    }
    let mut names: Vec<String> = Vec::new();
    for dir in font_dirs() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|x| x.to_str()).map(|x| x.to_lowercase()).unwrap_or_default();
            // the large ones are Chinese, Japanese and Korean fonts, without Arabic
            if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") || entry.metadata().map_or(true, |m| m.len() > 30_000_000) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&p) {
                names.extend(usable_families(&bytes));
            }
        }
    }
    names.retain(|n| DRAW_ARABIC_RIGHT.iter().any(|b| b.eq_ignore_ascii_case(n)));
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    *FOUND.lock().unwrap() = Some(names.clone());
    names
}

/// Whether `path` is a font file this reads (for tests).
#[cfg(test)]
fn families_of(path: &std::path::Path) -> Vec<String> {
    std::fs::read(path).map(|b| usable_families(&b)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_fonts_are_kept_to_those_that_draw_arabic_right() {
        let fonts = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
        let segoe = fonts.join("segoeui.ttf");
        if !segoe.is_file() {
            return; // not Windows
        }
        assert_eq!(families_of(&segoe), ["Segoe UI"]);
        assert_eq!(families_of(&fonts.join("tahoma.ttf")), ["Tahoma"]);
        // it has the letters, but libass draws a box before لا in it
        assert_eq!(families_of(&fonts.join("cour.ttf")), ["Courier New"]);
        let list = for_subtitles();
        assert!(list.iter().any(|n| n == "Segoe UI"), "{list:?}");
        assert!(!list.iter().any(|n| n == "Arial" || n == "Times New Roman"), "they draw a box before لا: {list:?}");
        assert!(!list.iter().any(|n| n == "Courier New" || n == "Palestine"), "{list:?}");
        println!("{} fonts: {list:?}", list.len());
    }

    #[test]
    fn a_broken_file_is_no_font() {
        assert!(usable_families(b"").is_empty());
        assert!(usable_families(b"ttcf\0\0\0\0\xff\xff\xff\xff").is_empty());
        assert!(usable_families(&[0u8; 300]).is_empty());
    }
}
