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
        ];
        for (n, (h, with, want, has)) in cases.into_iter().enumerate() {
            let d = decide(&heard(&h), &with).unwrap();
            assert_eq!((d.language.as_str(), d.has_model), (want, has), "case {n}");
        }
        assert_eq!(decide(&[], &["ar"]), None, "too little speech: nothing heard");
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
