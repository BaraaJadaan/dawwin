//! "Detect the language": which language a recording is in, when the user asks Dawwin to find it
//! rather than say it. The language model (VoxLingua107, in the engine) hears the first 30 s of
//! speech, not of the file: a film's music or a meeting's silent start says nothing. The decision
//! is among the languages Dawwin has a model for; when the model puts little of its probability on
//! those, the recording is likely in another one, which is reported, and its model offered.

use crate::audio::SR;
use crate::engine::Heard;

/// What the spoken-language menu sends for "Detect the language".
pub const AUTO: &str = "auto";
/// How much speech the language model hears: it scores no more than this.
pub const LISTEN_S: f32 = 30.0;
/// Below this share of the probability on the languages with a model, the recording is taken to
/// be in another language.
const ENOUGH: f32 = 0.25;
/// Languages the model mixes up, the second taking the place of the first when it holds this share
/// of what the languages with a model hold: Arabic with English in it is Arabic, which the Arabic
/// model writes, English words and all; Russian is often heard as Belarusian (measured on
/// transcribe.cpp's Russian sample: 65 % Belarusian, 35 % Russian).
const RATHER: &[(&str, &str)] = &[("en", "ar"), ("be", "ru")];
const RATHER_SHARE: f32 = 0.25;

/// What was heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    /// Dawwin's code of the language to transcribe in, or of the one heard when there is no model
    pub language: String,
    /// The language model's name of it (for a language Dawwin does not know by name)
    pub name: String,
    /// There is a model on the disk for it
    pub has_model: bool,
}

/// Dawwin's code for one of the language model's: it uses Whisper's codes, but `iw` for Hebrew.
pub fn dawwin_code(code: &str) -> &str {
    match code {
        "iw" => "he",
        "jv" => "jw",
        "fil" => "tl",
        "nb" => "no",
        other => other,
    }
}

/// The language to transcribe in, from what the language model heard and the languages that have
/// a model on the disk. None when it heard nothing (too little speech).
pub fn decide(heard: &[Heard], with_model: &[&str]) -> Option<Decision> {
    let top = heard.iter().max_by(|a, b| a.p.total_cmp(&b.p))?;
    let allowed: Vec<&Heard> = heard.iter().filter(|h| with_model.contains(&dawwin_code(&h.code))).collect();
    let mass: f32 = allowed.iter().map(|h| h.p).sum();
    if let Some(best) = allowed.iter().max_by(|a, b| a.p.total_cmp(&b.p)).filter(|_| mass >= ENOUGH) {
        for (heard_as, rather) in RATHER {
            if best.code != *heard_as {
                continue;
            }
            if let Some(r) = allowed.iter().find(|h| h.code == *rather).filter(|r| r.p >= RATHER_SHARE * mass) {
                return Some(Decision { language: rather.to_string(), name: r.name.clone(), has_model: true });
            }
        }
        return Some(Decision { language: dawwin_code(&best.code).to_string(), name: best.name.clone(), has_model: true });
    }
    let language = dawwin_code(&top.code).to_string();
    let has_model = with_model.contains(&language.as_str());
    Some(Decision { language, name: top.name.clone(), has_model })
}

/// The first `LISTEN_S` seconds of speech of a recording, as sample ranges: its long silences left out.
pub fn first_speech(samples: &[f32]) -> Vec<(usize, usize)> {
    let mut left = (LISTEN_S * SR as f32) as usize;
    let mut out = Vec::new();
    for (a, b) in crate::audio::speech_spans(samples, 0.7, 0.3) {
        if left == 0 {
            break;
        }
        let take = (b - a).min(left);
        out.push((a, a + take));
        left -= take;
    }
    out
}

/// Those ranges, one after the other: what the language model hears.
pub fn joined(samples: &[f32], ranges: &[(usize, usize)]) -> Vec<f32> {
    crate::audio::join_spans(samples, ranges)
}

/// The longest piece heard at once, and the shortest worth hearing.
const PIECE_S: f32 = 10.0;
const SHORTEST_S: f32 = 1.0;

/// The joined speech (`len` samples) cut into pieces of at most 10 s, each heard on its own: one
/// pass over a whole half minute that holds Arabic and English lets the clearer of the two decide
/// (measured: 20 s of recitation and 10 s of English came out 84 % English), while pieces
/// averaged by their length say how much of each is spoken.
pub fn pieces(len: usize) -> Vec<(usize, usize)> {
    let (piece, shortest) = ((PIECE_S * SR as f32) as usize, (SHORTEST_S * SR as f32) as usize);
    let n = len.div_ceil(piece).max(1);
    let size = len / n;
    (0..n).map(|k| (k * size, if k + 1 == n { len } else { (k + 1) * size })).filter(|(a, b)| b - a >= shortest).collect()
}

/// What the pieces heard, together: each language's probability averaged by the length of the piece.
pub fn average(heard: &[(Vec<Heard>, usize)]) -> Vec<Heard> {
    let total: usize = heard.iter().map(|(_, n)| n).sum();
    let mut out: Vec<Heard> = Vec::new();
    for (list, n) in heard {
        let w = *n as f32 / total.max(1) as f32;
        for h in list {
            match out.iter_mut().find(|o| o.code == h.code) {
                Some(o) => o.p += h.p * w,
                None => out.push(Heard { p: h.p * w, ..h.clone() }),
            }
        }
    }
    out.sort_by(|a, b| b.p.total_cmp(&a.p));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(list: &[(&str, f32)]) -> Vec<Heard> {
        list.iter().map(|(c, p)| Heard { code: c.to_string(), name: format!("name of {c}"), p: *p }).collect()
    }

    #[test]
    fn the_language_is_chosen_among_those_with_a_model_unless_it_is_clearly_another() {
        // (heard, languages with a model, decided, has a model)
        let cases: Vec<(Vec<(&str, f32)>, Vec<&str>, &str, bool)> = vec![
            // Arabic, with the Arabic model (which also does English)
            (vec![("ar", 0.7), ("fa", 0.15), ("ur", 0.1), ("en", 0.05)], vec!["ar", "en"], "ar", true),
            // a dialect heard as partly Persian: still Arabic, as only Arabic and English have a model
            (vec![("fa", 0.45), ("ar", 0.4), ("en", 0.05), ("ur", 0.1)], vec!["ar", "en"], "ar", true),
            // French with only the Arabic model: French is reported, with no model for it
            (vec![("fr", 0.93), ("en", 0.03), ("ar", 0.01), ("it", 0.03)], vec!["ar", "en"], "fr", false),
            // the same with Canary on the disk: French, with its model
            (vec![("fr", 0.93), ("en", 0.03), ("ar", 0.01), ("it", 0.03)], vec!["ar", "en", "fr", "da"], "fr", true),
            // Hebrew: the model says "iw", Dawwin "he"
            (vec![("iw", 0.9), ("ar", 0.1)], vec!["ar", "en"], "he", false),
            (vec![("iw", 0.9), ("ar", 0.1)], vec!["he", "ar"], "he", true),
            // nothing installed: what it heard
            (vec![("en", 0.6), ("de", 0.4)], vec![], "en", false),
            // Arabic with English in it (measured: 20 s of recitation, then 10 s of English): Arabic
            (vec![("en", 0.551), ("ar", 0.352), ("pt", 0.027)], vec!["ar", "en"], "ar", true),
            // the same as the app takes it (the first 30 s of speech of that recording)
            (vec![("en", 0.632), ("ar", 0.278), ("pt", 0.019), ("no", 0.013)], vec!["ar", "en"], "ar", true),
            // Russian heard as Belarusian (measured), with an all-languages model on the disk
            (vec![("be", 0.65), ("ru", 0.35)], vec!["ar", "en", "ru", "be"], "ru", true),
            (vec![("be", 0.95), ("ru", 0.04)], vec!["ar", "en", "ru", "be"], "be", true),
            // English with a few Arabic words is English
            (vec![("en", 0.9), ("ar", 0.05)], vec!["ar", "en"], "en", true),
        ];
        for (n, (h, with, want, has)) in cases.into_iter().enumerate() {
            let d = decide(&heard(&h), &with).unwrap();
            assert_eq!((d.language.as_str(), d.has_model), (want, has), "case {n}");
        }
        assert_eq!(decide(&[], &["ar"]), None, "too little speech: nothing heard");
    }

    #[test]
    fn the_half_minute_is_heard_in_pieces_and_averaged_by_their_length() {
        let s = SR;
        assert_eq!(pieces(30 * s), [(0, 10 * s), (10 * s, 20 * s), (20 * s, 30 * s)]);
        assert_eq!(pieces(12 * s), [(0, 6 * s), (6 * s, 12 * s)], "two even pieces, not 10 s and 2 s");
        assert_eq!(pieces(4 * s), [(0, 4 * s)]);
        assert!(pieces(s / 2).is_empty(), "too short to hear");
        // two pieces of recitation, one of English (as measured on each alone)
        let ar = heard(&[("ar", 0.99), ("en", 0.005)]);
        let en = heard(&[("en", 0.99), ("ar", 0.005)]);
        let all = average(&[(ar.clone(), 10 * s), (ar, 10 * s), (en, 10 * s)]);
        assert_eq!(all[0].code, "ar");
        assert!((all[0].p - 0.6617).abs() < 0.001 && (all[1].p - 0.3333).abs() < 0.001, "{all:?}");
        assert_eq!(decide(&all, &["ar", "en"]).unwrap().language, "ar");
    }

    #[test]
    fn the_language_model_hears_the_first_half_minute_of_speech() {
        // 10 s of silence, 20 s of tone, 5 s of silence, 40 s of tone
        let tone = |s: usize| (0..s * SR).map(|i| (i as f32 * 0.05).sin() * 0.3).collect::<Vec<f32>>();
        let mut samples = vec![0.0f32; 10 * SR];
        samples.extend(tone(20));
        samples.extend(vec![0.0f32; 5 * SR]);
        samples.extend(tone(40));
        let ranges = first_speech(&samples);
        let total: usize = ranges.iter().map(|(a, b)| b - a).sum();
        assert_eq!(total, 30 * SR, "{ranges:?}");
        assert!(ranges[0].0 >= 9 * SR, "the silence at the start is not heard: {ranges:?}");
        assert_eq!(ranges.len(), 2);
        assert_eq!(joined(&samples, &ranges).len(), 30 * SR);
        assert!(first_speech(&vec![0.0; 5 * SR]).is_empty());
    }
}

