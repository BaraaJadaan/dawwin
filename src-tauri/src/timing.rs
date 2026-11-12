//! When each word of a line is said.
//!
//! The speech models give text and no times: Cohere and Canary none at all, Whisper only for
//! whole phrases (transcribe.cpp 0.4.1). A line is a piece of up to 30 s, and inside it the words
//! used to be placed by their share of the text over the speech. When the speaker slows down,
//! speeds up or stops in the middle of a sentence, that guess put words seconds early, before a
//! pause they were said after, and their subtitle was gone by the time they were said.
//!
//! The timing pass: each stretch of a line between two pauses is transcribed again on its own,
//! only to learn which words fall in it (`Segment::heard`). Those rough texts are matched word by
//! word against the line's own text, which is never changed, so every pause falls at the right
//! word. Inside a stretch the words are placed by their share of its text, which over a few
//! seconds of speech without a stop is close. The match is made again from `heard` whenever the
//! line is edited, without the audio.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use crate::audio::SR;
use crate::engine::{Audio, Engine};
use crate::script::tokens;
use crate::transcript::Segment;

#[cfg(test)]
mod bench;

/// A pause at least this long cuts a line into stretches,
const MIN_CUT_S: f64 = 0.3;
/// and a stretch has at least this much in it: shorter ones are too little for the model to read.
const MIN_STRETCH_S: f64 = 1.0;
/// Sound on each side of a stretch the model hears too, so its first and last words are whole.
const PAD_S: f64 = 0.15;

/// A word as it is compared: in lower case, without punctuation, and Arabic without its vowel
/// marks and with the letters people write either way made one (أ إ آ ا, ى ي, ة ه).
pub fn fold(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for c in word.chars().flat_map(char::to_lowercase) {
        match c {
            '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{0640}' => {}
            'أ' | 'إ' | 'آ' | 'ٱ' => out.push('ا'),
            'ى' | 'ئ' => out.push('ي'),
            'ة' => out.push('ه'),
            'ؤ' => out.push('و'),
            '٠'..='٩' => out.push(char::from(b'0' + (c as u32 - '٠' as u32) as u8)),
            '۰'..='۹' => out.push(char::from(b'0' + (c as u32 - '۰' as u32) as u8)),
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// How alike two folded words are, from 0 to 1 (one minus the edit distance over the longer).
fn similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diag = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let up = row[j];
            row[j] = if a[i - 1] == b[j - 1] { diag } else { 1 + diag.min(up).min(row[j - 1]) };
            diag = up;
        }
    }
    1.0 - row[b.len()] as f64 / a.len().max(b.len()) as f64
}

/// Which word of `rough` each word of `accurate` is, where they match: a global alignment, in
/// which a pair scores by how alike its words are and a word left out costs a little. Words
/// missing from either side, extra words and repeated phrases are all skipped over.
pub fn align(accurate: &[String], rough: &[String]) -> Vec<Option<usize>> {
    const GAP: f64 = -0.4;
    let (n, m) = (accurate.len(), rough.len());
    let mut score = vec![vec![0.0f64; m + 1]; n + 1];
    // 0: a pair, 1: an accurate word left out, 2: a rough word left out
    let mut back = vec![vec![0u8; m + 1]; n + 1];
    for i in 1..=n {
        score[i][0] = GAP * i as f64;
        back[i][0] = 1;
    }
    for j in 1..=m {
        score[0][j] = GAP * j as f64;
        back[0][j] = 2;
    }
    for i in 1..=n {
        for j in 1..=m {
            let pair = score[i - 1][j - 1] + 2.0 * similarity(&accurate[i - 1], &rough[j - 1]) - 1.0;
            let up = score[i - 1][j] + GAP;
            let left = score[i][j - 1] + GAP;
            (score[i][j], back[i][j]) = if pair >= up && pair >= left {
                (pair, 0)
            } else if up >= left {
                (up, 1)
            } else {
                (left, 2)
            };
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        match back[i][j] {
            0 => {
                if similarity(&accurate[i - 1], &rough[j - 1]) >= 0.5 {
                    out[i - 1] = Some(j - 1);
                }
                i -= 1;
                j -= 1;
            }
            1 => i -= 1,
            _ => j -= 1,
        }
    }
    out
}

/// The stretches of a line between the pauses it is cut at: (speech starts, speech ends), in
/// seconds. Only pauses of at least `MIN_CUT_S` cut, and only where both sides keep at least
/// `MIN_STRETCH_S`.
pub fn stretches(s: &Segment) -> Vec<(f64, f64)> {
    let (first, last) = match (s.voice.first(), s.voice.last()) {
        (Some(a), Some(b)) if s.voice.len() >= 2 && b > a => (*a as f64, *b as f64),
        _ => (s.start, s.end),
    };
    let mut out = Vec::new();
    let mut from = first;
    for p in &s.pauses {
        let (a, b) = (p[1] as f64, p[2] as f64);
        if b - a < MIN_CUT_S || a <= from || b >= last {
            continue;
        }
        if a - from >= MIN_STRETCH_S {
            out.push((from, a));
            from = b;
        }
    }
    out.push((from, last));
    if out.len() >= 2 && out[out.len() - 1].1 - out[out.len() - 1].0 < MIN_STRETCH_S {
        let (_, end) = out.pop().unwrap();
        out.last_mut().unwrap().1 = end;
    }
    out
}

/// Which stretch each word of the line was said in, from what the model heard in each; None
/// when nothing it heard matches the line.
/// Which stretch each word of the line was said in, and which stretches had any of the line's
/// words heard in them; None when nothing heard matches the line.
fn stretch_of_words(accurate: &[String], heard: &[(f32, f32, String)]) -> Option<(Vec<usize>, Vec<bool>)> {
    let mut rough = Vec::new();
    let mut tag = Vec::new();
    for (k, h) in heard.iter().enumerate() {
        for (a, b) in tokens(&h.2) {
            let w: String = h.2.chars().skip(a).take(b - a).collect();
            rough.push(fold(&w));
            tag.push(k);
        }
    }
    if rough.is_empty() || accurate.is_empty() {
        return None;
    }
    let matched = align(accurate, &rough);
    let mut matching = vec![false; heard.len()];
    for j in matched.iter().flatten() {
        matching[tag[*j]] = true;
    }
    // the words that matched, and one before the first and one after the last of both sides
    let mut anchors: Vec<(f64, f64)> = vec![(-1.0, -1.0)];
    anchors.extend(matched.iter().enumerate().filter_map(|(i, j)| j.map(|j| (i as f64, j as f64))));
    if anchors.len() == 1 {
        return None;
    }
    anchors.push((accurate.len() as f64, rough.len() as f64));
    // every word gets a place among the rough words: its own, or in between the matched ones
    let mut out = Vec::with_capacity(accurate.len());
    let mut k = 0;
    for i in 0..accurate.len() {
        let x = i as f64;
        while anchors[k + 1].0 < x {
            k += 1;
        }
        let ((x0, y0), (x1, y1)) = (anchors[k], anchors[k + 1]);
        let y = if x1 > x0 { y0 + (x - x0) * (y1 - y0) / (x1 - x0) } else { y0 };
        out.push(tag[(y.round().max(0.0) as usize).min(rough.len() - 1)]);
    }
    Some((out, matching))
}

/// How fast people say a line's characters, about: for words placed in a stretch that holds
/// something else besides.
const CHARS_A_SECOND: f64 = 14.0;

/// `along` seconds of speech after `from`, the pauses in `inner` skipped.
fn skip_pauses(from: f64, mut along: f64, inner: &[(f64, f64)]) -> f64 {
    let mut t = from;
    for &(a, b) in inner {
        if a < t {
            continue;
        }
        if t + along < a {
            break;
        }
        along -= a - t;
        t = b;
    }
    t + along
}

/// When each word of the line starts, as (character offset, seconds), from what was heard in
/// each of its stretches. None for a line the timing pass did not do, or where nothing matched.
pub fn word_starts(s: &Segment) -> Option<Vec<(u32, f32)>> {
    if s.heard.is_empty() {
        return None;
    }
    let chars: Vec<char> = s.text.chars().collect();
    let toks = tokens(&s.text);
    let accurate: Vec<String> = toks.iter().map(|&(a, b)| fold(&chars[a..b].iter().collect::<String>())).collect();
    let (tags, matching) = stretch_of_words(&accurate, &s.heard)?;
    let mut out = Vec::with_capacity(toks.len());
    let mut k = 0;
    while k < toks.len() {
        let st = tags[k];
        let mut e = k;
        while e < toks.len() && tags[e] == st {
            e += 1;
        }
        let (from, to) = (s.heard[st].0 as f64, s.heard[st].1 as f64);
        let inner: Vec<(f64, f64)> = s.pauses.iter().map(|p| (p[1] as f64, p[2] as f64)).filter(|&(a, b)| a > from && b < to).collect();
        let mut speech = ((to - from) - inner.iter().map(|(a, b)| b - a).sum::<f64>()).max(0.0);
        let (c0, c1) = (toks[k].0, toks[e - 1].1);
        let len = (c1 - c0).max(1) as f64;
        // none of the line's words was heard in this stretch: it holds mostly something the line
        // does not (another voice in another language, which the model left out), and the words
        // placed in it were said next to the speech that follows, at its end; or, with nothing
        // heard after it, next to what came before, at its start. At the pace people speak.
        let mut lead = 0.0;
        if !matching[st] {
            let needed = (len / CHARS_A_SECOND).min(speech);
            if matching[st + 1..].iter().any(|m| *m) {
                lead = speech - needed;
            }
            speech = needed;
        }
        for tok in &toks[k..e] {
            let share = (tok.0 - c0) as f64 / len;
            let t = skip_pauses(from, lead + share * speech, &inner);
            out.push((tok.0 as u32, ((t * 100.0).round() / 100.0) as f32));
        }
        k = e;
    }
    Some(out)
}

/// Set a line's word times from what was heard in it, or clear them when that says nothing.
pub fn apply(s: &mut Segment) {
    if let Some(w) = word_starts(s) {
        s.words = w;
    } else if !s.heard.is_empty() {
        s.words.clear();
    }
}

/// Whether the timing pass has anything to learn from this line: one with a single stretch has
/// no pause for it to place.
pub fn needs_pass(s: &Segment) -> bool {
    stretches(s).len() >= 2
}

/// The timing pass over the lines of a transcript: `audio` holds the whole recording, on the
/// same clock as the lines. `progress` is told how far it got (0 to 1).
pub fn pass(w: &mut dyn Engine, audio: &Audio, segments: &mut [Segment], language: &str, cancel: &AtomicBool, mut progress: impl FnMut(f64)) -> Result<()> {
    let total: f64 = segments.iter().map(|s| s.end - s.start).sum::<f64>().max(1e-9);
    let mut done = 0.0;
    for s in segments.iter_mut() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        done += s.end - s.start;
        if !needs_pass(s) || s.text.trim().is_empty() {
            s.heard.clear();
            continue;
        }
        let parts = stretches(s);
        let len = audio.samples.len();
        let ranges: Vec<(usize, usize)> = parts
            .iter()
            .enumerate()
            .map(|(k, &(a, b))| {
                // into the pauses on each side, up to half of them
                let before = if k == 0 { PAD_S } else { ((a - parts[k - 1].1) / 2.0).min(PAD_S) };
                let after = if k + 1 == parts.len() { PAD_S } else { ((parts[k + 1].0 - b) / 2.0).min(PAD_S) };
                let x = ((a - before).max(0.0) * SR as f64) as usize;
                let y = (((b + after) * SR as f64) as usize).min(len);
                (x.min(y), y)
            })
            .collect();
        let texts = w.transcribe(audio, &ranges, language)?;
        s.heard = parts.iter().zip(texts).map(|(&(a, b), r)| (a as f32, b as f32, r.text)).collect();
        apply(s);
        progress(done / total);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(fold).collect()
    }

    #[test]
    fn arabic_spelt_either_way_is_the_same_word() {
        assert_eq!(fold("إلى"), fold("الى"));
        assert_eq!(fold("مدرسة،"), fold("مدرسه"));
        assert_eq!(fold("أكيدًا"), fold("اكيدا"));
        assert_eq!(fold("الـAPI"), fold("الapi"));
        assert_eq!(fold("٢٠٢٦"), "2026");
        assert_eq!(fold("Meaning,"), "meaning");
    }

    #[test]
    fn words_the_rough_text_leaves_out_or_adds_are_skipped() {
        let accurate = words("we will go through them with accounting on monday and come back");
        let rough = words("we go through them with the accounting monday and uh come back");
        let m = align(&accurate, &rough);
        assert_eq!(m[0], Some(0));
        assert_eq!(m[1], None, "\"will\" was not heard");
        assert_eq!(m[2], Some(1));
        assert_eq!(m[6], Some(6), "\"accounting\" after the extra \"the\"");
        assert_eq!(m[7], None, "\"on\" was not heard");
        assert_eq!(m[11], Some(11), "after the extra \"uh\"");
    }

    #[test]
    fn a_repeated_phrase_is_matched_in_order() {
        let accurate = words("thank you thank you very much thank you");
        let rough = words("thank you thank you very much thank you");
        assert_eq!(align(&accurate, &rough), (0..8).map(Some).collect::<Vec<_>>());
        let rough = words("thank you very much thank you");
        let m = align(&accurate, &rough);
        let found: Vec<usize> = m.iter().flatten().copied().collect();
        assert!(found.windows(2).all(|w| w[0] < w[1]), "in order: {m:?}");
        assert_eq!(m[7], Some(5));
    }

    #[test]
    fn arabic_spelling_variants_match() {
        let accurate = words("أكيد، بس بحتاج الـ API الجديد من فريق الـ backend يوم الأحد على الأكثر.");
        let rough = words("اكيد بس بحتاج ال api الجديد من فريق الباك اند يوم الاحد على الاكثر");
        let m = align(&accurate, &rough);
        assert_eq!(m[0], Some(0));
        assert_eq!(m[4], Some(4), "API");
        assert_eq!(m[10], Some(10), "يوم: {m:?}");
        assert_eq!(m[13], Some(13), "الأكثر.");
    }

    fn line(text: &str, voice: (f32, f32), pauses: Vec<[f32; 3]>) -> Segment {
        Segment { start: 0.0, end: 20.0, text: text.into(), voice: vec![voice.0, voice.1], pauses, ..Default::default() }
    }

    #[test]
    fn a_line_is_cut_at_its_longer_pauses_only() {
        // pauses: a short one, one at 4 s, and one too near the end to leave a stretch after it
        let s = line("x", (1.0, 12.0), vec![[0.1, 2.0, 2.2], [0.3, 4.0, 5.0], [0.9, 11.5, 11.8]]);
        assert_eq!(stretches(&s), vec![(1.0, 4.0), (5.0, 12.0)]);
        let s = line("x", (1.0, 12.0), vec![]);
        assert_eq!(stretches(&s), vec![(1.0, 12.0)]);
        assert!(!needs_pass(&s));
    }

    #[test]
    fn words_said_after_a_pause_start_after_it() {
        // "meaning" ends the first stretch; most of the text comes in the second, short one
        let mut s = line(
            "The world, for the realist, is mind independent, meaning it's not pegged on or subject to",
            (1.0, 9.0),
            vec![[0.6, 5.0, 6.0]],
        );
        s.heard = vec![
            (1.0, 5.0, "the world for the realist is mind independent meaning".into()),
            (6.0, 9.0, "its not pegged on or subject to".into()),
        ];
        let w = word_starts(&s).unwrap();
        assert_eq!(w.len(), 16);
        let text: Vec<char> = s.text.chars().collect();
        let at = |k: usize| text[w[k].0 as usize..].iter().take_while(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(at(9), "it's");
        assert!((w[9].1 - 6.0).abs() < 0.01, "on when the speaker goes on: {:?}", w[9]);
        assert!(w[8].1 < 5.0, "\"meaning\" before the pause: {:?}", w[8]);
        assert!(w[15].1 > 8.0 && w[15].1 < 9.0, "\"to\" near the end: {:?}", w[15]);
        assert!(w.windows(2).all(|p| p[0].1 <= p[1].1), "in order: {w:?}");
    }

    #[test]
    fn an_edited_line_is_matched_again_without_the_audio() {
        let mut s = line("one two three four five six", (0.0, 6.0), vec![[0.5, 3.0, 3.5]]);
        s.heard = vec![(0.0, 3.0, "one two three".into()), (3.5, 6.0, "four five six".into())];
        apply(&mut s);
        assert_eq!(s.words[3].1, 3.5);
        s.text = "one two three and four five six".into();
        apply(&mut s);
        assert_eq!(s.words.len(), 7);
        assert!(s.words[4].1 >= 3.5, "\"four\" still starts after the pause: {:?}", s.words);
        assert!(s.words[2].1 < 3.0, "and \"three\" before it: {:?}", s.words);
    }

    #[test]
    fn words_left_out_of_a_stretch_of_another_voice_go_at_its_end() {
        // English, then twelve seconds of recitation and a little English, then English: the
        // recitation is not in the line, and hearing that stretch again gives only the recitation
        let mut s = line("the realist is mind independent meaning pegged on or subject to what any of us believes", (36.0, 58.0), vec![[0.3, 39.2, 40.0], [0.8, 52.1, 52.6]]);
        s.heard = vec![
            (36.0, 39.2, "the realist is mind independent meaning".into()),
            (40.0, 52.1, "كذلك أوحينا إليك قرآنا عربيا لتنذر أم القرى".into()),
            (52.6, 58.0, "what any of us believes".into()),
        ];
        let w = word_starts(&s).unwrap();
        let text: Vec<char> = s.text.chars().collect();
        let at = |k: usize| text[w[k].0 as usize..].iter().take_while(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(at(6), "pegged");
        assert!(w[6].1 > 49.5 && w[6].1 < 52.1, "said just before the English that follows, not over the recitation: {:?}", w[6]);
        assert!(w[10].1 < 52.1, "\"to\" still before the pause: {:?}", w[10]);
        assert!(w.windows(2).all(|p| p[0].1 <= p[1].1), "in order: {w:?}");
    }

    #[test]
    fn nothing_heard_that_matches_keeps_no_times() {
        let mut s = line("one two three four", (0.0, 6.0), vec![[0.5, 3.0, 3.5]]);
        s.heard = vec![(0.0, 3.0, "".into()), (3.5, 6.0, "".into())];
        s.words = vec![(0, 0.5)];
        apply(&mut s);
        assert!(s.words.is_empty());
    }

    // ------------------------------------------------------------ the timing bench

    /// How far off the words and subtitles are, with each way of timing them, on recordings whose
    /// every word time is known (`tools/timing`, which says how to run it).
    #[test]
    #[ignore]
    fn bench() {
        super::bench::run();
    }

    #[test]
    fn chinese_counts_each_character_as_a_word() {
        assert_eq!(tokens("我们今天，开会。"), vec![(0, 1), (1, 2), (2, 3), (3, 5), (5, 6), (6, 8)]);
        assert_eq!(tokens("the API 很好"), vec![(0, 3), (4, 7), (8, 9), (9, 10)]);
    }
}
