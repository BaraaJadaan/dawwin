//! Text before it is spoken: cleaned of what a voice should not see, cut into sentences, and each
//! sentence into runs of one language, by script.

/// Arabic or English: the two languages Dawwin reads aloud.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Ar,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ar => "ar",
            Lang::En => "en",
        }
    }
}

/// A Latin run with fewer words than this, inside an Arabic sentence, stays with the Arabic voice:
/// a product name or a term ("GitHub", "Read aloud") must not switch voices mid-sentence.
pub const MIN_LATIN_WORDS: usize = 3;

/// A sentence longer than this is cut again at a comma or a semicolon: one request per sentence,
/// and the first sound must come soon.
pub const MAX_SENTENCE_CHARS: usize = 260;

/// The base letter of each Arabic presentation form from U+FE80 on, with how many forms it has.
const FORMS_B: &[(&str, u32)] = &[
    ("ء", 1), ("آ", 2), ("أ", 2), ("ؤ", 2), ("إ", 2), ("ئ", 4), ("ا", 2), ("ب", 4), ("ة", 2), ("ت", 4),
    ("ث", 4), ("ج", 4), ("ح", 4), ("خ", 4), ("د", 2), ("ذ", 2), ("ر", 2), ("ز", 2), ("س", 4), ("ش", 4),
    ("ص", 4), ("ض", 4), ("ط", 4), ("ظ", 4), ("ع", 4), ("غ", 4), ("ف", 4), ("ق", 4), ("ك", 4), ("ل", 4),
    ("م", 4), ("ن", 4), ("ه", 4), ("و", 2), ("ى", 2), ("ي", 4), ("لآ", 2), ("لأ", 2), ("لإ", 2), ("لا", 2),
];

fn presentation_form(c: char) -> Option<&'static str> {
    let cp = c as u32;
    match cp {
        0xFE70 | 0xFE71 => Some("\u{064B}"),
        0xFE72 => Some("\u{064C}"),
        0xFE74 => Some("\u{064D}"),
        0xFE76 | 0xFE77 => Some("\u{064E}"),
        0xFE78 | 0xFE79 => Some("\u{064F}"),
        0xFE7A | 0xFE7B => Some("\u{0650}"),
        0xFE7C | 0xFE7D => Some("\u{0651}"),
        0xFE7E | 0xFE7F => Some("\u{0652}"),
        0xFDF2 => Some("الله"),
        0xFDFA => Some("صلى الله عليه وسلم"),
        0xFDFB => Some("جل جلاله"),
        0xFE80..=0xFEFC => {
            let mut at = 0xFE80;
            for (letter, n) in FORMS_B {
                if cp < at + n {
                    return Some(letter);
                }
                at += n;
            }
            None
        }
        _ => None,
    }
}

/// What a voice should read: presentation forms back to letters, no tatweel and no direction or
/// joining marks, Arabic-Indic digits as 0-9, every kind of space a plain one. Diacritics stay.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let digit_around = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(|d| d.is_ascii_digit() || ('\u{0660}'..='\u{0669}').contains(d) || ('\u{06F0}'..='\u{06F9}').contains(d));
        match c {
            '\u{0640}' => {}
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{061C}' | '\u{FEFF}' | '\u{00AD}' => {}
            '\u{0660}'..='\u{0669}' => out.push(char::from(b'0' + (c as u32 - 0x0660) as u8)),
            '\u{06F0}'..='\u{06F9}' => out.push(char::from(b'0' + (c as u32 - 0x06F0) as u8)),
            '\u{066B}' if digit_around(i.checked_sub(1)) && digit_around(Some(i + 1)) => out.push('.'),
            '\u{066C}' if digit_around(i.checked_sub(1)) && digit_around(Some(i + 1)) => out.push(','),
            '\u{066A}' => out.push('%'),
            '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\t' => out.push(' '),
            c => match presentation_form(c) {
                Some(s) => out.push_str(s),
                None => out.push(c),
            },
        }
    }
    // runs of spaces as one, within a line
    let mut tidy = String::with_capacity(out.len());
    let mut space = false;
    for c in out.chars() {
        if c == ' ' {
            if !space {
                tidy.push(' ');
            }
            space = true;
        } else {
            tidy.push(c);
            space = false;
        }
    }
    tidy.trim().to_string()
}

pub fn is_arabic(c: char) -> bool {
    matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{08A0}'..='\u{08FF}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
        && !matches!(c, '\u{060C}' | '\u{061B}' | '\u{061F}' | '\u{0660}'..='\u{066D}' | '\u{06D4}' | '\u{06F0}'..='\u{06F9}')
}

pub fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '\u{00C0}'..='\u{024F}') && c != '\u{00D7}' && c != '\u{00F7}'
}

fn script(c: char) -> Option<Lang> {
    if is_arabic(c) {
        Some(Lang::Ar)
    } else if is_latin(c) {
        Some(Lang::En)
    } else {
        None
    }
}

/// The language most of the text's letters are in; Arabic when there are none.
pub fn main_lang(s: &str) -> Lang {
    let (mut ar, mut en) = (0usize, 0usize);
    for c in s.chars() {
        match script(c) {
            Some(Lang::Ar) => ar += 1,
            Some(Lang::En) => en += 1,
            None => {}
        }
    }
    if en > ar {
        Lang::En
    } else {
        Lang::Ar
    }
}

const ENDS: &[char] = &['.', '!', '?', '؟', '…', '۔'];
const CLOSERS: &[char] = &['"', '\'', '”', '’', '»', ')', ']', '}', '›'];
/// Words a period follows without ending the sentence, whatever comes next: "Dr. Smith", "No. 7".
const TITLES: &[&str] = &["mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "e.g", "i.e", "no", "fig", "vol", "approx", "dept", "mt", "cf"];

/// Where a sentence may end at `i` (a period, a question mark…): not inside a number, an
/// abbreviation, an initial or "a.m." in the middle of a sentence.
fn ends_here(chars: &[char], i: usize) -> bool {
    let c = chars[i];
    if !ENDS.contains(&c) {
        return false;
    }
    // the next thing after the closing quotes and the spaces
    let mut j = i + 1;
    while j < chars.len() && (ENDS.contains(&chars[j]) || CLOSERS.contains(&chars[j])) {
        j += 1;
    }
    if j < chars.len() && !chars[j].is_whitespace() {
        return false; // 3.5, e.g, example.com
    }
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    let Some(&next) = chars.get(j) else { return true };
    if c != '.' {
        return true;
    }
    // the word the period follows
    let mut k = i;
    while k > 0 && !chars[k - 1].is_whitespace() && !CLOSERS.contains(&chars[k - 1]) && chars[k - 1] != '(' {
        k -= 1;
    }
    let word: String = chars[k..i].iter().collect::<String>().to_lowercase();
    let latin_word = word.chars().any(is_latin);
    if latin_word {
        // "J. K. Rowling", "Dr. Smith"
        if word.chars().count() == 1 || TITLES.contains(&word.as_str()) {
            return false;
        }
        // "10:30 a.m. in room 7", "etc. and more": an English sentence goes on in small letters
        if next.is_lowercase() {
            return false;
        }
    }
    true
}

/// Cut a paragraph into sentences, each with its end punctuation and closing quotes.
pub fn sentences(paragraph: &str) -> Vec<String> {
    let chars: Vec<char> = paragraph.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' || ends_here(&chars, i) {
            let mut end = i + 1;
            if chars[i] != '\n' {
                while end < chars.len() && (ENDS.contains(&chars[end]) || CLOSERS.contains(&chars[end])) {
                    end += 1;
                }
            }
            let s: String = chars[start..end].iter().collect();
            push_sentence(&mut out, s.trim());
            start = end;
            i = end;
            continue;
        }
        i += 1;
    }
    let rest: String = chars[start..].iter().collect();
    push_sentence(&mut out, rest.trim());
    out
}

fn push_sentence(out: &mut Vec<String>, s: &str) {
    if !s.chars().any(|c| c.is_alphanumeric()) {
        // only punctuation: it belongs to the sentence before
        if let Some(last) = out.last_mut() {
            if !s.is_empty() {
                last.push_str(s);
            }
        }
        return;
    }
    for piece in cut_long(s) {
        out.push(piece);
    }
}

/// A sentence too long for one request, cut at the clause mark nearest its middle, again and
/// again; at a space when it has none.
fn cut_long(s: &str) -> Vec<String> {
    let n = s.chars().count();
    if n <= MAX_SENTENCE_CHARS {
        return vec![s.to_string()];
    }
    let chars: Vec<char> = s.chars().collect();
    let middle = n / 2;
    let marks = ['؛', ';', '،', ',', ':', '—', '–'];
    let near = |pred: &dyn Fn(char) -> bool| -> Option<usize> {
        (0..n).filter(|&i| pred(chars[i]) && i > 20 && i + 20 < n && chars.get(i + 1).is_some_and(|c| c.is_whitespace())).min_by_key(|&i| i.abs_diff(middle))
    };
    let at = near(&|c| marks.contains(&c)).map(|i| i + 1).or_else(|| near(&|c| c == ' ')).unwrap_or(middle);
    let (a, b): (String, String) = (chars[..at].iter().collect(), chars[at..].iter().collect());
    let mut out = cut_long(a.trim());
    out.extend(cut_long(b.trim()));
    out
}

/// A piece of a sentence in one language.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Run {
    pub lang: Lang,
    pub text: String,
}

/// A sentence in runs of one script each. Digits, spaces and punctuation belong to the run they
/// are in; a short Latin run inside an Arabic sentence stays Arabic; Arabic inside English always
/// gets the Arabic voice, which is the one that can read it.
pub fn runs(sentence: &str, default: Lang) -> Vec<Run> {
    let mut raw: Vec<(Option<Lang>, String)> = Vec::new();
    for c in sentence.chars() {
        let s = script(c);
        match raw.last_mut() {
            // neutral characters join the run in progress
            Some((cur, text)) if s.is_none() || *cur == s || cur.is_none() => {
                if cur.is_none() {
                    *cur = s;
                }
                text.push(c);
            }
            _ => raw.push((s, c.to_string())),
        }
    }
    // a sentence without letters is in the language of the text around it
    let main = if sentence.chars().any(|c| script(c).is_some()) { main_lang(sentence) } else { default };
    let mut runs: Vec<Run> = raw.into_iter().map(|(l, text)| Run { lang: l.unwrap_or(default), text }).collect();
    if main == Lang::Ar {
        for r in runs.iter_mut() {
            if r.lang == Lang::En && r.text.split_whitespace().filter(|w| w.chars().any(is_latin)).count() < MIN_LATIN_WORDS {
                r.lang = Lang::Ar;
            }
        }
    }
    // neighbours in the same language as one
    let mut out: Vec<Run> = Vec::new();
    for r in runs {
        match out.last_mut() {
            Some(last) if last.lang == r.lang => last.text.push_str(&r.text),
            _ => out.push(r),
        }
    }
    // a run that is only punctuation or spaces goes to the run before it
    let mut tidy: Vec<Run> = Vec::new();
    for r in out {
        let has_word = r.text.chars().any(|c| c.is_alphanumeric());
        match tidy.last_mut() {
            Some(last) if !has_word => last.text.push_str(&r.text),
            Some(last) if last.lang == r.lang => last.text.push_str(&r.text),
            _ => tidy.push(r),
        }
    }
    for r in tidy.iter_mut() {
        r.text = r.text.trim().to_string();
    }
    tidy.retain(|r| !r.text.is_empty());
    tidy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_forms_tatweel_and_marks_are_cleaned_and_diacritics_kept() {
        // "سلام" typed in presentation forms, as text copied from old PDFs is
        assert_eq!(normalize("\u{FEB3}\u{FEFC}\u{FEE1}"), "سلام");
        assert_eq!(normalize("جمـــيل"), "جميل");
        assert_eq!(normalize("\u{200F}مرحبا\u{200E} Hello\u{202B}"), "مرحبا Hello");
        assert_eq!(normalize("كَتَبَ الدَّرْسَ"), "كَتَبَ الدَّرْسَ");
        assert_eq!(normalize("عام ٢٠٢٦ و۱۲"), "عام 2026 و12");
        assert_eq!(normalize("٣٫٥ و١٬٢٥٠ و٪١٥"), "3.5 و1,250 و%15");
        assert_eq!(normalize("a\u{00A0}\u{00A0}b\t c"), "a b c");
        assert_eq!(normalize("\u{FDF2}"), "الله");
        // every letter of the table lands on a letter, none on the next one's
        assert_eq!(normalize("\u{FE8D}\u{FE8E}\u{FE8F}\u{FEF2}\u{FEF4}\u{FEFB}"), "اابييلا");
    }

    #[test]
    fn arabic_is_cut_into_sentences() {
        let s = sentences("كيف حالك؟ أنا بخير. وأنت! هل وصلت الرسالة…");
        assert_eq!(s, vec!["كيف حالك؟", "أنا بخير.", "وأنت!", "هل وصلت الرسالة…"]);
        // a decimal and a comma do not end one
        assert_eq!(sentences("ارتفع السعر 3.5 بالمئة، ثم توقف."), vec!["ارتفع السعر 3.5 بالمئة، ثم توقف."]);
    }

    #[test]
    fn english_is_cut_into_sentences_but_not_at_abbreviations() {
        assert_eq!(sentences("Hello there. How are you? Fine!"), vec!["Hello there.", "How are you?", "Fine!"]);
        assert_eq!(sentences("Dr. Smith met Mr. Jones at 10:30 a.m. in room 7. They talked."), vec!["Dr. Smith met Mr. Jones at 10:30 a.m. in room 7.", "They talked."]);
        assert_eq!(sentences("J. K. Rowling wrote it, e.g. the first one. Then more."), vec!["J. K. Rowling wrote it, e.g. the first one.", "Then more."]);
        assert_eq!(sentences("He said \"Go home.\" Then he left."), vec!["He said \"Go home.\"", "Then he left."]);
        assert_eq!(sentences("Visit example.com today. It costs $3.50."), vec!["Visit example.com today.", "It costs $3.50."]);
        assert_eq!(sentences("Wait... what? Yes... Go."), vec!["Wait... what?", "Yes...", "Go."]);
    }

    #[test]
    fn a_new_line_ends_a_sentence_and_lone_punctuation_joins_the_one_before() {
        assert_eq!(sentences("Title\nThe text goes on"), vec!["Title", "The text goes on"]);
        assert_eq!(sentences("Done. ..."), vec!["Done...."]);
    }

    #[test]
    fn a_very_long_sentence_is_cut_at_a_comma_near_its_middle() {
        let long = format!("{}، {}", "كلمة ".repeat(40).trim(), "أخرى ".repeat(40).trim());
        let s = sentences(&long);
        assert_eq!(s.len(), 2);
        assert!(s[0].ends_with('،'));
        assert!(s.iter().all(|x| x.chars().count() <= MAX_SENTENCE_CHARS));
        let no_marks = "word ".repeat(120);
        assert!(sentences(&no_marks).iter().all(|x| x.chars().count() <= MAX_SENTENCE_CHARS));
    }

    #[test]
    fn runs_follow_the_script_and_short_english_stays_with_the_arabic_voice() {
        let r = runs("رفعنا النسخة على GitHub أمس، وأضفنا ميزة اسمها Read aloud.", Lang::Ar);
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].lang, Lang::Ar);
        let r = runs("وفي نهاية الاجتماع قال المدير: we will ship it next week, no matter what.", Lang::Ar);
        assert_eq!(r, vec![
            Run { lang: Lang::Ar, text: "وفي نهاية الاجتماع قال المدير:".into() },
            Run { lang: Lang::En, text: "we will ship it next week, no matter what.".into() },
        ]);
        // digits go with the words around them
        let r = runs("يعمل على Windows 11 منذ 2024", Lang::Ar);
        assert_eq!(r.len(), 1);
        // Arabic inside English: the Arabic voice reads it
        let r = runs("The word سلام means peace.", Lang::En);
        assert_eq!(r.iter().map(|x| x.lang).collect::<Vec<_>>(), vec![Lang::En, Lang::Ar, Lang::En]);
        assert_eq!(r[1].text, "سلام");
        // only digits: the document's language
        assert_eq!(runs("2026", Lang::En), vec![Run { lang: Lang::En, text: "2026".into() }]);
        assert_eq!(main_lang("Hello مرحبا بكم جميعا"), Lang::Ar);
    }
}
