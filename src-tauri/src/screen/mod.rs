//! Read anything on the screen (1.8.0): select text in any program, press a shortcut, and Dawwin
//! reads it aloud; Dawwin waits in the notification area for it, and can start with Windows.
//!
//! Everything here hands its text to one seam, `Reader` ("this text, read aloud"), which the
//! reader of 1.7.0 fills. What is read from another program is never stored and never logged.

pub mod app;
pub mod autostart;
pub mod epub;
pub mod files;
pub mod layout;
pub mod markup;
pub mod ocr;
pub mod pdf;
pub mod say;
pub mod shortcut;
pub mod sound;



use serde::{Deserialize, Serialize};

use crate::selection::Outcome;
#[cfg(test)]
use crate::selection::Method;

/// The settings of this part, kept in Dawwin's settings file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The shortcut is on. Only then does Dawwin stay in the notification area when its window
    /// is closed, and only then may it start with Windows.
    pub on: bool,
    /// The keys, as "Ctrl+Alt+R"
    pub shortcut: String,
    /// The Ctrl+C way may be used where a program gives no selection otherwise
    pub copy: bool,
    pub start_with_windows: bool,
    /// "Say it like this"
    pub say: Vec<say::Say>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { on: false, shortcut: shortcut::DEFAULT.into(), copy: true, start_with_windows: false, say: vec![] }
    }
}

/// "This text, read aloud": what 1.8.0 needs from the reader of 1.7.0.
pub trait Reader: Send + Sync {
    /// Read this text from its start, in place of anything being read. It is kept nowhere.
    fn read(&self, text: String) -> anyhow::Result<()>;
    fn stop(&self);
    fn pause(&self, paused: bool);
    /// Back (negative) or forward a number of sentences
    fn skip(&self, sentences: i32);
    fn reading(&self) -> bool;
}

/// A stand-in for tests (debug builds, `DAWWIN_TEST_READER`): it "reads" for a few seconds a
/// text it keeps in memory only, so a test can see what reached the voice and press again to stop.
#[cfg(debug_assertions)]
#[derive(Default)]
pub struct StandIn {
    pub last: std::sync::Mutex<Option<String>>,
    until: std::sync::Mutex<Option<std::time::Instant>>,
}

#[cfg(debug_assertions)]
impl Reader for StandIn {
    fn read(&self, text: String) -> anyhow::Result<()> {
        *self.last.lock().unwrap() = Some(text);
        *self.until.lock().unwrap() = Some(std::time::Instant::now() + std::time::Duration::from_secs(8));
        Ok(())
    }
    fn stop(&self) {
        *self.until.lock().unwrap() = None;
    }
    fn pause(&self, _paused: bool) {}
    fn skip(&self, _sentences: i32) {}
    fn reading(&self) -> bool {
        self.until.lock().unwrap().is_some_and(|t| std::time::Instant::now() < t)
    }
}

/// What the user is told after a press of the shortcut. Never the text itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Note {
    Reading,
    Stopped,
    NothingSelected,
    Administrator,
    Terminal,
    CopyOff,
    KeysHeld,
    Failed,
}

impl Note {
    /// The sentence shown, in the interface's language.
    pub fn message(self) -> String {
        match self {
            Note::Reading => crate::tr!("Reading the selection."),
            Note::Stopped => crate::tr!("Stopped."),
            Note::NothingSelected => crate::tr!("Nothing is selected. Select some text, then press the shortcut."),
            Note::Administrator => crate::tr!("Dawwin cannot read in a window that runs as administrator."),
            Note::Terminal => crate::tr!("This terminal does not give Dawwin its selection, and Ctrl+C would stop its program."),
            Note::CopyOff => crate::tr!("This program does not give Dawwin its selection, and copying it is turned off in Settings."),
            Note::KeysHeld => crate::tr!("The shortcut's keys were still held down. Press it and let go."),
            Note::Failed => crate::tr!("Dawwin could not read the selection."),
        }
        .to_string()
    }
}

/// A press of the shortcut: stop if reading, otherwise read the selection. `capture` gets the
/// selection (the real one asks Windows); what it gives goes to the reader and nowhere else.
pub fn pressed(reader: &dyn Reader, settings: &Settings, capture: impl FnOnce(bool) -> Outcome) -> Note {
    if reader.reading() {
        reader.stop();
        return Note::Stopped;
    }
    match capture(settings.copy) {
        Outcome::Text { text, .. } => {
            let text = say::apply(&text, &settings.say);
            match reader.read(text) {
                Ok(()) => Note::Reading,
                Err(_) => Note::Failed,
            }
        }
        Outcome::NothingSelected => Note::NothingSelected,
        Outcome::Administrator => Note::Administrator,
        Outcome::Terminal => Note::Terminal,
        Outcome::CopyOff => Note::CopyOff,
        Outcome::KeysHeld => Note::KeysHeld,
        Outcome::Failed(_) => Note::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { on: true, say: vec![say::Say { lang: "ar".into(), written: "AI".into(), said: "الذكاء الاصطناعي".into() }], ..Settings::default() }
    }

    #[test]
    fn a_press_reads_the_selection_with_the_list_applied_and_a_second_press_stops() {
        let r = StandIn::default();
        let note = pressed(&r, &settings(), |_| Outcome::Text { text: "يعمل AI هنا.".into(), by: Method::Uia });
        assert_eq!(note, Note::Reading);
        assert_eq!(r.last.lock().unwrap().clone(), Some("يعمل الذكاء الاصطناعي هنا.".into()));
        assert!(r.reading());
        let mut asked = false;
        assert_eq!(pressed(&r, &settings(), |_| {
            asked = true;
            Outcome::NothingSelected
        }), Note::Stopped);
        assert!(!asked, "the second press stops without reading the selection again");
        assert!(!r.reading());
    }

    #[test]
    fn every_outcome_is_said_and_nothing_is_read_without_text() {
        for (o, n) in [
            (Outcome::NothingSelected, Note::NothingSelected),
            (Outcome::Administrator, Note::Administrator),
            (Outcome::Terminal, Note::Terminal),
            (Outcome::CopyOff, Note::CopyOff),
            (Outcome::KeysHeld, Note::KeysHeld),
            (Outcome::Failed("x".into()), Note::Failed),
        ] {
            let r = StandIn::default();
            assert_eq!(pressed(&r, &settings(), |_| o.clone()), n);
            assert!(r.last.lock().unwrap().is_none());
            assert!(!n.message().is_empty());
        }
    }

    #[test]
    fn the_ctrl_c_setting_reaches_the_capture() {
        let r = StandIn::default();
        let off = Settings { copy: false, ..settings() };
        let mut seen = None;
        pressed(&r, &off, |copy| {
            seen = Some(copy);
            Outcome::CopyOff
        });
        assert_eq!(seen, Some(false));
    }

    #[test]
    fn settings_from_before_1_8_get_the_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        assert!(!s.on && s.copy && !s.start_with_windows && s.shortcut == "Ctrl+Shift+Space");
    }
}

/// The Arabic of this part's messages, looked up by `i18n::tr` after its own table.
pub static AR: &[(&str, &str)] = &[
    ("Reading the selection.", "تُقرأ الكتابة المحدَّدة."),
    ("Stopped.", "توقّفت القراءة."),
    ("Nothing is selected. Select some text, then press the shortcut.", "لا شيء محدَّد. حدِّد نصًّا ثم اضغط الاختصار."),
    ("Dawwin cannot read in a window that runs as administrator.", "لا يستطيع دَوِّن القراءة في نافذة تعمل بصلاحيات المسؤول."),
    ("This terminal does not give Dawwin its selection, and Ctrl+C would stop its program.", "هذه الطرفية لا تعطي دَوِّن ما حُدِّد فيها، وCtrl+C يوقف البرنامج الذي يعمل فيها."),
    ("This program does not give Dawwin its selection, and copying it is turned off in Settings.", "هذا البرنامج لا يعطي دَوِّن ما حُدِّد فيه، ونسخُه مُطفأ في الإعدادات."),
    ("The shortcut's keys were still held down. Press it and let go.", "كانت مفاتيح الاختصار ما زالت مضغوطة. اضغطه ثم اتركه."),
    ("Dawwin could not read the selection.", "تعذّر على دَوِّن قراءة النص المحدَّد."),
    ("Open Dawwin", "افتح دَوِّن"),
    ("Stop reading", "أوقف القراءة"),
    ("Quit Dawwin", "أغلق دَوِّن"),
    ("{} is used by another program. Choose other keys.", "يستخدم برنامج آخر {}. اختر مفاتيح أخرى."),
    ("That is not a shortcut Dawwin can use. Use Ctrl, Alt or Win with a letter, a number or F1 to F24.", "هذا ليس اختصارًا يستطيع دَوِّن استخدامه. استخدم Ctrl أو Alt أو Win مع حرف أو رقم أو F1 إلى F24."),
    ("The selection", "النص المحدَّد"),
    ("Save the reading as sound", "احفظ القراءة صوتًا"),
    ("Saving as sound needs ffmpeg. Download it in Settings.", "يحتاج حفظ القراءة صوتًا إلى ffmpeg. نزّله من الإعدادات."),
    // files
    ("Page {}", "الصفحة {}"),
    ("Chapter {}", "الفصل {}"),
    ("This PDF is locked with a password.", "ملف PDF هذا مقفل بكلمة مرور."),
    ("This PDF is damaged, or is not a PDF.", "ملف PDF هذا تالف، أو ليس ملف PDF."),
    ("This PDF could not be opened.", "تعذّر فتح ملف PDF هذا."),
    ("This e-book is damaged, or is not an EPUB.", "هذا الكتاب الإلكتروني تالف، أو ليس ملف EPUB."),
    ("This e-book has no chapters to read.", "لا فصول في هذا الكتاب الإلكتروني لتُقرأ."),
    ("Dawwin cannot read this kind of file aloud.", "لا يستطيع دَوِّن قراءة هذا النوع من الملفات بصوت عالٍ."),
    ("Windows cannot read Arabic in pictures yet. Add Arabic in Windows Settings, Time & language, Language & region, then try again.", "لا يستطيع ويندوز قراءة العربية في الصور بعد. أضف العربية من إعدادات ويندوز، الوقت واللغة، اللغة والمنطقة، ثم حاول مرة أخرى."),
    ("Windows cannot read English in pictures yet. Add English in Windows Settings, Time & language, Language & region, then try again.", "لا يستطيع ويندوز قراءة الإنجليزية في الصور بعد. أضف الإنجليزية من إعدادات ويندوز، الوقت واللغة، اللغة والمنطقة، ثم حاول مرة أخرى."),
    ("The picture could not be read: {}", "تعذّرت قراءة الصورة: {}"),
];
