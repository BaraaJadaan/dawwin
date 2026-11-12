//! "Say it like this": the user's list of words the voice gets wrong, each with how to say it, per
//! language. Applied to the text before it is spoken, whole words only. A sentence's language is
//! the script most of its letters are in, so "AI" can be said one way inside Arabic and another
//! inside English. Arabic is matched without its diacritics and tatweel, Latin without case.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Say {
    /// "ar" or "en": the language of the sentences it applies in
    pub lang: String,
    pub written: String,
    pub said: String,
}

fn is_mark(c: char) -> bool {
    matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{0640}' | '\u{06D6}'..='\u{06ED}')
}

fn is_arabic(c: char) -> bool {
    matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{08A0}'..='\u{08FF}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

/// The language most letters of this text are in.
pub fn language(text: &str) -> &'static str {
    let (mut ar, mut latin) = (0, 0);
    for c in text.chars() {
        if is_arabic(c) && !is_mark(c) && c.is_alphabetic() {
            ar += 1;
        } else if c.is_ascii_alphabetic() || ('\u{00C0}'..='\u{024F}').contains(&c) {
            latin += 1;
        }
    }
    if ar >= latin && ar > 0 {
        "ar"
    } else {
        "en"
    }
}

fn same(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || is_mark(c)
}

/// Where `written` starts at `i` in `text` (diacritics in the text skipped): the end, or None.
fn match_at(text: &[char], i: usize, written: &[char]) -> Option<usize> {
    let mut j = i;
    for &w in written {
        while j < text.len() && is_mark(text[j]) && !is_mark(w) {
            j += 1;
        }
        if j >= text.len() || !same(text[j], w) {
            return None;
        }
        j += 1;
    }
    // marks on the last letter belong to the word
    while j < text.len() && is_mark(text[j]) {
        j += 1;
    }
    Some(j)
}

/// The sentences of a text with what ends them, so it can be put back together unchanged.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        // a full stop inside "1.8" ends nothing
        let next_is_space = chars.peek().map_or(true, |(_, n)| n.is_whitespace());
        if matches!(c, '\n' | '؛') || (matches!(c, '.' | '!' | '?' | '؟') && next_is_space) {
            let end = i + c.len_utf8();
            out.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The text with the list applied.
pub fn apply(text: &str, list: &[Say]) -> String {
    if list.is_empty() {
        return text.to_string();
    }
    // the longer first: "Dawwin 1.8" before "Dawwin"
    let mut entries: Vec<(&str, Vec<char>, &str)> = list
        .iter()
        .filter(|s| !s.written.trim().is_empty())
        .map(|s| (s.lang.as_str(), s.written.trim().chars().filter(|&c| !is_mark(c)).collect(), s.said.as_str()))
        .collect();
    entries.sort_by_key(|(_, w, _)| std::cmp::Reverse(w.len()));
    let mut out = String::with_capacity(text.len());
    for sentence in sentences(text) {
        let lang = language(sentence);
        let chars: Vec<char> = sentence.chars().collect();
        let mut i = 0;
        'chars: while i < chars.len() {
            let starts_word = i == 0 || !word_char(chars[i - 1]);
            if starts_word {
                for (l, written, said) in &entries {
                    if *l != lang {
                        continue;
                    }
                    if let Some(end) = match_at(&chars, i, written) {
                        if end == chars.len() || !word_char(chars[end]) {
                            out.push_str(said);
                            i = end;
                            continue 'chars;
                        }
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(lang: &str, written: &str, said: &str) -> Say {
        Say { lang: lang.into(), written: written.into(), said: said.into() }
    }

    #[test]
    fn whole_words_only_in_their_language() {
        let list = [say("ar", "AI", "الذكاء الاصطناعي"), say("en", "AI", "A.I."), say("en", "Dawwin", "Daw-win")];
        assert_eq!(apply("يعمل AI على حاسوبك.", &list), "يعمل الذكاء الاصطناعي على حاسوبك.");
        assert_eq!(apply("The AI runs here. Dawwin's too!", &list), "The A.I. runs here. Daw-win's too!");
        // inside another word: unchanged
        assert_eq!(apply("MAIL and AIR, Dawwins", &list), "MAIL and AIR, Dawwins");
        // a sentence of each language in one text
        assert_eq!(apply("هذا AI. This is AI.", &list), "هذا الذكاء الاصطناعي. This is A.I..");
    }

    #[test]
    fn arabic_is_matched_without_its_marks_and_latin_without_case() {
        let list = [say("ar", "دوين", "دَوِّين"), say("en", "gguf", "G G U F")];
        assert_eq!(apply("برنامج دَوين، ودوين.", &list), "برنامج دَوِّين، ودوين.", "with marks in the text; و joined to it is another word");
        assert_eq!(apply("A GGUF file, a Gguf file.", &list), "A G G U F file, a G G U F file.");
        assert_eq!(apply("دوينـ", &list), "دَوِّين", "tatweel after it is part of the word: matched");
    }

    #[test]
    fn longer_entries_first_and_text_kept_as_it_was() {
        let list = [say("en", "Dawwin", "Dawin"), say("en", "Dawwin 1.8", "Dawin one point eight")];
        assert_eq!(apply("Dawwin 1.8 and Dawwin.", &list), "Dawin one point eight and Dawin.");
        let text = "مرحبًا، هذا نص بلا تغيير!\nAnd a second line?";
        assert_eq!(apply(text, &list), text);
        assert_eq!(apply(text, &[]), text);
    }

    #[test]
    fn the_language_of_a_sentence() {
        assert_eq!(language("يعمل Dawwin على حاسوبك"), "ar");
        assert_eq!(language("Dawwin runs on your PC, كله"), "en");
        assert_eq!(language("1234"), "en");
    }
}
