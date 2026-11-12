//! How burned-in subtitles look: a setting, not a question at each export. A few sizes rather
//! than a number, a font that draws the subtitle's language, white or yellow, an outline or a dark
//! box behind the text, at the bottom or the top; and, for a video under 1080p, the picture
//! enlarged first so the text is drawn sharp (a 480p video's text is soft otherwise, whatever the
//! bitrate: the player enlarges the letters along with the picture).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Look {
    /// "small" | "medium" | "large" | "very-large"
    pub size: String,
    /// A font installed on the PC; empty for Segoe UI, which draws every language Dawwin writes
    pub font: String,
    /// "white" | "yellow"
    pub color: String,
    /// "outline" | "box"
    pub back: String,
    /// "bottom" | "top"
    pub place: String,
    /// Enlarge a video under 1080p to 1080p before drawing the text
    pub sharp: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look { size: "medium".into(), font: String::new(), color: "white".into(), back: "outline".into(), place: "bottom".into(), sharp: true }
    }
}

/// The font when none is chosen: Windows' Arial draws ث before لا as a box.
pub const DEFAULT_FONT: &str = "Segoe UI";
/// The height the sharp-text switch enlarges a smaller video to.
pub const SHARP_HEIGHT: u64 = 1080;

/// A font name as libass's style takes it: commas, quotes and colons would end the style.
fn safe_font(name: &str) -> String {
    let n: String = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '&')).collect();
    let n = n.trim().to_string();
    if n.is_empty() { DEFAULT_FONT.to_string() } else { n }
}

impl Look {
    /// The font size in libass's units (a 288-line picture): the medium one is what 1.3 had.
    fn font_size(&self) -> u32 {
        match self.size.as_str() {
            "small" => 14,
            "large" => 23,
            "very-large" => 28,
            _ => 18,
        }
    }

    /// The style libass draws the subtitles in. `Encoding=-1` lets libass find each line's
    /// direction itself: without it a right-to-left line is laid out from the left, its full stop
    /// lands at the start and the Arabic around an English word swaps.
    pub fn style(&self) -> String {
        // &HAABBGGRR: yellow is red and green
        let colour = if self.color == "yellow" { "&H0000FFFF" } else { "&H00FFFFFF" };
        let back = if self.back == "box" {
            // an opaque box in the outline's colour, a little see-through
            "BorderStyle=3,OutlineColour=&H40000000,Outline=3,Shadow=0"
        } else {
            "BorderStyle=1,OutlineColour=&H00000000,Outline=1.6,Shadow=0.8"
        };
        let align = if self.place == "top" { 8 } else { 2 };
        format!(
            "FontName={},FontSize={},PrimaryColour={colour},{back},Alignment={align},MarginV=18,Encoding=-1",
            safe_font(if self.font.trim().is_empty() { DEFAULT_FONT } else { &self.font }),
            self.font_size()
        )
    }

    /// Whether a video whose shorter side is `short_side` pixels is enlarged before the text is drawn.
    pub fn sharpens(&self, short_side: u64) -> bool {
        self.sharp && short_side > 0 && short_side < SHARP_HEIGHT
    }

    /// The picture's filters: enlarged first when sharpened (its shorter side to 1080, upright or
    /// lying), then the subtitles from the file `subtitles`.
    pub fn filter(&self, subtitles: &str, short_side: u64) -> String {
        let draw = format!("subtitles={subtitles}:force_style='{}'", self.style());
        if self.sharpens(short_side) {
            format!("scale='if(gt(iw,ih),-2,{h})':'if(gt(iw,ih),{h},-2)':flags=lanczos,{draw}", h = SHARP_HEIGHT)
        } else {
            draw
        }
    }
}

static CURRENT: Mutex<Option<Look>> = Mutex::new(None);

/// The look burning uses from now on (from the settings).
pub fn set(look: Look) {
    *CURRENT.lock().unwrap() = Some(look);
}

pub fn current() -> Look {
    CURRENT.lock().unwrap().clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_look_is_the_one_before_and_each_choice_shows_in_the_style() {
        let d = Look::default();
        assert_eq!(d.style(), "FontName=Segoe UI,FontSize=18,PrimaryColour=&H00FFFFFF,BorderStyle=1,OutlineColour=&H00000000,Outline=1.6,Shadow=0.8,Alignment=2,MarginV=18,Encoding=-1");
        let mine = Look { size: "very-large".into(), font: "Noto Naskh Arabic".into(), color: "yellow".into(), back: "box".into(), place: "top".into(), sharp: true };
        let s = mine.style();
        for part in ["FontName=Noto Naskh Arabic,", "FontSize=28,", "PrimaryColour=&H0000FFFF,", "BorderStyle=3,", "Alignment=8,", "Encoding=-1"] {
            assert!(s.contains(part), "{part} in {s}");
        }
        // a name that would end the style early cannot
        let odd = Look { font: "Evil',Font:Name".into(), ..Look::default() };
        assert!(odd.style().starts_with("FontName=EvilFontName,"), "{}", odd.style());
        assert!(Look { font: "  ".into(), ..Look::default() }.style().starts_with("FontName=Segoe UI,"));
    }

    #[test]
    fn small_videos_are_enlarged_before_the_text_is_drawn_when_asked() {
        let on = Look::default();
        let off = Look { sharp: false, ..Look::default() };
        // (shorter side, enlarged)
        for (short, want) in [(480, true), (720, true), (1080, false), (2160, false), (0, false)] {
            assert_eq!(on.sharpens(short), want, "{short}");
            assert!(!off.sharpens(short));
        }
        let f = on.filter("burn-1.srt", 480);
        assert!(f.starts_with("scale='if(gt(iw,ih),-2,1080)':'if(gt(iw,ih),1080,-2)':flags=lanczos,subtitles=burn-1.srt:force_style='FontName=Segoe UI,"), "{f}");
        assert!(on.filter("burn-1.srt", 1080).starts_with("subtitles=burn-1.srt:"));
        // settings saved before the look existed, or with only some of it
        let partial: Look = serde_json::from_str(r#"{"size":"large"}"#).unwrap();
        assert_eq!(partial, Look { size: "large".into(), ..Look::default() });
    }
}
