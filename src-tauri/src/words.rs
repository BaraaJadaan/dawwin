//! Words and names: how to write the words Dawwin keeps getting wrong. Each entry is the right form
//! and, if the user knows them, the wrong forms it is heard as. Every new transcript has them put
//! right as whole words, with Arabic letters folded the way the search folds them (أ إ آ ا, ة ه,
//! ى ئ ي, ؤ و, no diacritics) and Latin letters in any case; "Apply my words" does the same to an
//! older one. A Whisper model also gets the right forms as hints, so it hears them right at once.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// How to write it
    pub write: String,
    /// What it comes out as, wrongly; it is also found written in another case or with other hamzas
    #[serde(default)]
    pub heard: Vec<String>,
}

/// The diacritics and the tatweel: left out when comparing (the Arabic digits between them are not).
fn ignored(c: char) -> bool {
    matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}' | '\u{0640}')
}

fn fold(c: char) -> char {
    match c {
        'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
        'ة' => 'ه',
        'ى' | 'ئ' => 'ي',
        'ؤ' => 'و',
        c => {
            let mut l = c.to_lowercase();
            match (l.next(), l.next()) {
                (Some(x), None) => x,
                _ => c,
            }
        }
    }
}

/// The text folded, and for each folded character where it is in the text.
fn folded(chars: &[char]) -> (Vec<char>, Vec<usize>) {
    let mut out = Vec::with_capacity(chars.len());
    let mut at = Vec::with_capacity(chars.len());
    for (i, c) in chars.iter().enumerate() {
        if !ignored(*c) {
            out.push(fold(*c));
            at.push(i);
        }
    }
    (out, at)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || ignored(c)
}

/// The forms to look for, folded, the longest first, each with what to write instead.
fn forms(entries: &[Entry]) -> Vec<(Vec<char>, &str)> {
    let mut out: Vec<(Vec<char>, &str)> = Vec::new();
    for e in entries {
        let write = e.write.trim();
        if write.is_empty() {
            continue;
        }
        for form in std::iter::once(write).chain(e.heard.iter().map(|h| h.trim())) {
            let f: Vec<char> = form.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !ignored(*c)).map(fold).collect();
            if !f.is_empty() && !out.iter().any(|(g, _)| *g == f) {
                out.push((f, write));
            }
        }
    }
    out.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    out
}

/// `text` with every whole-word form of the entries written the right way.
pub fn apply(text: &str, entries: &[Entry]) -> String {
    let forms = forms(entries);
    if forms.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (norm, at) = folded(&chars);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0; // characters of `chars` already in `out`
    let mut k = 0; // position in `norm`
    'scan: while k < norm.len() {
        let start = at[k];
        // a word starts here: nothing of a word right before it
        if start == 0 || !is_word_char(chars[start - 1]) {
            for (form, write) in &forms {
                let n = form.len();
                if k + n <= norm.len() && norm[k..k + n] == form[..] {
                    // and ends here, with any diacritics of its last letter
                    let mut end = at[k + n - 1] + 1;
                    while end < chars.len() && ignored(chars[end]) {
                        end += 1;
                    }
                    if end == chars.len() || !is_word_char(chars[end]) {
                        out.extend(&chars[copied..start]);
                        out.push_str(write);
                        copied = end;
                        k += n;
                        while k < norm.len() && at[k] < end {
                            k += 1;
                        }
                        continue 'scan;
                    }
                }
            }
        }
        k += 1;
    }
    out.extend(&chars[copied..]);
    out
}

/// The right forms, for a model that takes hints (Whisper), the order of the list.
pub fn hints(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|e| e.write.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

/// The list as a text file to keep or take to another PC: one word a line, "right = wrong, wrong".
pub fn to_text(entries: &[Entry]) -> String {
    let mut out = String::from("# Dawwin: words and names. One a line: how to write it = what it comes out as, separated by commas.\r\n");
    for e in entries {
        if e.heard.is_empty() {
            out.push_str(&format!("{}\r\n", e.write.trim()));
        } else {
            out.push_str(&format!("{} = {}\r\n", e.write.trim(), e.heard.iter().map(|h| h.trim()).collect::<Vec<_>>().join(", ")));
        }
    }
    out
}

/// A list saved by `to_text` (or written by hand the same way).
pub fn from_text(text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (write, heard) = line.split_once('=').unwrap_or((line, ""));
        let write = write.trim().to_string();
        if write.is_empty() {
            continue;
        }
        let heard: Vec<String> = heard.split([',', '،']).map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect();
        match out.iter_mut().find(|e| e.write == write) {
            Some(e) => e.heard.extend(heard),
            None => out.push(Entry { write, heard }),
        }
    }
    out
}

/// Add a word, or more wrong forms of one already in the list.
pub fn add(entries: &mut Vec<Entry>, write: &str, heard: &str) {
    let write = write.split_whitespace().collect::<Vec<_>>().join(" ");
    if write.is_empty() {
        return;
    }
    let heard = heard.split_whitespace().collect::<Vec<_>>().join(" ");
    let e = match entries.iter_mut().position(|e| e.write == write) {
        Some(i) => &mut entries[i],
        None => {
            entries.push(Entry { write: write.clone(), heard: Vec::new() });
            entries.last_mut().unwrap()
        }
    };
    if !heard.is_empty() && heard != write && !e.heard.contains(&heard) {
        e.heard.push(heard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Entry> {
        vec![
            Entry { write: "Thmanyah".into(), heard: vec!["ثمانية".into(), "تمانيه".into()] },
            Entry { write: "OpenAI".into(), heard: vec!["open ai".into()] },
            Entry { write: "سامي الأحمد".into(), heard: vec!["سامي الاحمد".into()] },
        ]
    }

    #[test]
    fn words_are_put_right_as_whole_words() {
        // (as heard, as written)
        let cases = [
            ("نشتغل في ثمانية من سنتين", "نشتغل في Thmanyah من سنتين"),
            // other letters for the same word, a diacritic, punctuation right next to it
            ("في ثمانيه، قالوا", "في Thmanyah، قالوا"),
            ("ثمانيةٌ؟ نعم", "Thmanyah؟ نعم"),
            ("(تمانيه)", "(Thmanyah)"),
            // part of a longer word: left alone
            ("الثمانية أيام", "الثمانية أيام"),
            ("ثمانيةعشر", "ثمانيةعشر"),
            // Latin in any case, the right form itself in the wrong case, two words as one form
            ("we use Open AI and thmanyah.", "we use OpenAI and Thmanyah."),
            ("THMANYAH, openai", "Thmanyah, OpenAI"),
            ("opening ai", "opening ai"),
            // a name of two words, with another hamza
            ("مع سامي الاحمد اليوم", "مع سامي الأحمد اليوم"),
            ("مع سامي إلاحمد", "مع سامي الأحمد"),
            ("", ""),
        ];
        for (heard, want) in cases {
            assert_eq!(apply(heard, &list()), want, "{heard}");
        }
        assert_eq!(apply("ثمانية", &[]), "ثمانية");
    }

    #[test]
    fn the_list_goes_to_a_file_and_comes_back() {
        let text = to_text(&list());
        assert!(text.contains("Thmanyah = ثمانية, تمانيه\r\n"), "{text}");
        assert_eq!(from_text(&text), list());
        // written by hand: Arabic commas, a word with no wrong forms, blank lines and comments
        let mine = from_text("\u{feff}# mine\n\nThmanyah = ثمانية، تمانيه\nDawwin\n  = nothing\n");
        assert_eq!(mine, vec![Entry { write: "Thmanyah".into(), heard: vec!["ثمانية".into(), "تمانيه".into()] }, Entry { write: "Dawwin".into(), heard: vec![] }]);
        assert_eq!(hints(&mine), ["Thmanyah", "Dawwin"]);
    }

    #[test]
    fn a_word_chosen_in_a_transcript_is_added_once() {
        let mut l = Vec::new();
        add(&mut l, " Thmanyah ", "ثمانية");
        add(&mut l, "Thmanyah", "ثمانية");
        add(&mut l, "Thmanyah", "تمانيه");
        add(&mut l, "", "x");
        assert_eq!(l, vec![Entry { write: "Thmanyah".into(), heard: vec!["ثمانية".into(), "تمانيه".into()] }]);
    }
}
