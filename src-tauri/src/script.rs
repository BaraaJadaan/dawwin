//! How the writing systems of the transcripts put their words down: where a word starts and ends,
//! and how wide it is on a line of subtitles.
//!
//! Chinese, Japanese, Thai, Lao, Khmer and Burmese are written without spaces between words. There
//! each character is a word, together with what belongs to it: the marks written above, below or
//! around it (Thai vowels and tones, Khmer subscripts, Japanese voicing marks), the vowel written
//! before it, the opening bracket or quote in front of it and the closing punctuation after it. So
//! a line never breaks inside a character, after an opening bracket or before a full stop.

/// Characters of scripts written without spaces between words.
pub fn unspaced(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF            // Hiragana, Katakana
        | 0x31F0..=0x31FF          // Katakana extensions
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F // Han
        | 0xFF66..=0xFF9F          // half-width Katakana
        | 0x0E00..=0x0E7F          // Thai
        | 0x0E80..=0x0EFF          // Lao
        | 0x1780..=0x17FF          // Khmer
        | 0x1000..=0x109F          // Myanmar
    )
}

/// Marks that are written with the character before them and take no place of their own.
pub fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F            // combining accents
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E // Thai vowels above and below, tones
        | 0x0EB1 | 0x0EB4..=0x0EBC | 0x0EC8..=0x0ECE // Lao
        | 0x17B4..=0x17D3 | 0x17DD // Khmer
        | 0x102B..=0x103E | 0x1056..=0x1059 | 0x105E..=0x1060 | 0x1062..=0x1064 | 0x1067..=0x106D
        | 0x1071..=0x1074 | 0x1082..=0x108D | 0x108F | 0x109A..=0x109D // Myanmar
        | 0x3099 | 0x309A          // Japanese voicing marks
        | 0xFE00..=0xFE0F          // variation selectors
        | 0x200D                   // zero-width joiner
    )
}

/// After these, the next consonant is written under the one before (Khmer, Myanmar), or the two
/// characters are joined: they stay together.
fn joins_next(c: char) -> bool {
    matches!(c, '\u{17D2}' | '\u{1039}' | '\u{200D}')
}

/// Thai and Lao vowels written before the consonant they are said after.
fn leading_vowel(c: char) -> bool {
    matches!(c as u32, 0x0E40..=0x0E44 | 0x0EC0..=0x0EC4)
}

/// Brackets and quotes that open: a line never ends on one.
pub fn opens(c: char) -> bool {
    matches!(c, '「' | '『' | '（' | '《' | '〈' | '【' | '〔' | '［' | '｛' | '“' | '‘' | '(' | '[' | '«')
}

/// Characters a full width wide: Chinese, Japanese and Korean letters, and their punctuation.
pub fn wide(c: char) -> bool {
    unspaced(c) && !matches!(c as u32, 0x0E00..=0x109F | 0x1780..=0x17FF | 0xFF66..=0xFF9F)
        || matches!(c as u32, 0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F | 0x3000..=0x303F | 0xFF01..=0xFF60 | 0xFFE0..=0xFFE6)
}

/// The words of a text, as character ranges: what spaces separate, and in scripts without spaces,
/// each character with what belongs to it (see the top of this file).
pub fn tokens(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let at_unspaced = |mut i: usize| {
        while i < n && (opens(chars[i]) || leading_vowel(chars[i])) {
            i += 1;
        }
        i < n && unspaced(chars[i])
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let a = i;
        if at_unspaced(i) {
            while i < n && (opens(chars[i]) || leading_vowel(chars[i])) {
                i += 1;
            }
            i += 1; // the character itself
            loop {
                if i < n && is_mark(chars[i]) {
                    i += 1;
                } else if i < n && joins_next(chars[i - 1]) && !chars[i].is_whitespace() {
                    i += 1;
                } else {
                    break;
                }
            }
            // a full stop, a comma, a closing bracket: with the character before them
            while i < n && !chars[i].is_whitespace() && !chars[i].is_alphanumeric() && !opens(chars[i]) && !unspaced(chars[i]) {
                i += 1;
            }
            out.push((a, i));
            continue;
        }
        while i < n && !chars[i].is_whitespace() && !(i > a && at_unspaced(i)) {
            i += 1;
        }
        out.push((a, i));
    }
    out
}

/// How a script's characters are counted on a line of subtitles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Count {
    /// One a character
    Chars,
    /// Chinese, Japanese and Korean: one a full-width character, half one for Latin letters,
    /// digits, spaces and punctuation (Netflix's guides for those languages)
    FullWidth,
    /// Thai, Lao, Khmer, Burmese: marks above and below the line do not count (Netflix's Thai guide)
    Bases,
}

impl Count {
    pub fn for_language(language: &str) -> Count {
        match language {
            "zh" | "zh-Hant" | "yue" | "ja" | "ko" => Count::FullWidth,
            "th" | "lo" | "km" | "my" => Count::Bases,
            _ => Count::Chars,
        }
    }

    /// How wide `text` is, in characters as this script counts them.
    pub fn width(self, text: &str) -> f64 {
        match self {
            Count::Chars => text.chars().count() as f64,
            Count::FullWidth => text.chars().map(|c| if wide(c) { 1.0 } else { 0.5 }).sum(),
            Count::Bases => text.chars().filter(|c| !is_mark(*c)).count() as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        tokens(text).into_iter().map(|(a, b)| chars[a..b].iter().collect()).collect()
    }

    #[test]
    fn spaced_scripts_split_at_spaces() {
        assert_eq!(words("صباح الخير، يا Sami."), ["صباح", "الخير،", "يا", "Sami."]);
    }

    #[test]
    fn chinese_and_japanese_split_by_character_with_their_punctuation() {
        assert_eq!(words("我们今天，开会。"), ["我", "们", "今", "天，", "开", "会。"]);
        assert_eq!(words("他说「好的」。"), ["他", "说", "「好", "的」。"]);
        assert_eq!(words("the API 很好"), ["the", "API", "很", "好"]);
        assert_eq!(words("ガ\u{3099}ラス"), ["ガ\u{3099}", "ラ", "ス"]);
        assert_eq!(words("会議はOKです"), ["会", "議", "は", "OK", "で", "す"]);
    }

    #[test]
    fn thai_keeps_its_vowels_and_tones_with_their_letter() {
        // เมื่อวาน: the leading vowel เ, ม with the vowel ื and the tone ่ above it, then อ, ว, า, น
        assert_eq!(words("เมื่อวาน"), ["เมื่", "อ", "ว", "า", "น"]);
        assert_eq!(Count::Bases.width("เมื่อวาน"), 6.0);
    }

    #[test]
    fn khmer_keeps_a_subscript_with_its_letter() {
        // ក្រុម: ក, the subscript ្រ under it, then ុ, then ម
        assert_eq!(words("ក្រុម"), ["ក្រុ", "ម"]);
    }

    #[test]
    fn full_width_characters_count_one_and_latin_half() {
        assert_eq!(Count::FullWidth.width("会議はOK"), 4.0);
        assert_eq!(Count::FullWidth.width("회의 OK"), 3.5);
        assert_eq!(Count::Chars.width("مرحبا"), 5.0);
    }
}
