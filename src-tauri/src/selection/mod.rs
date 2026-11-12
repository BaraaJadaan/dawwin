//! The text selected in another program, for "read the selection aloud".
//!
//! Two ways, in this order. **UI Automation**: the text pattern of the focused element, or of the
//! nearest ancestor that has one, gives the selection without touching anything. An element with a
//! text pattern and nothing selected means "nothing is selected", and Dawwin stops there: Ctrl+C
//! would copy a whole line in a code editor and stop the running program in a terminal. **Ctrl+C**,
//! only when nothing on the way up has a text pattern: wait for the shortcut's keys to be released,
//! keep what the clipboard holds, send Ctrl+C, read the text, and put the clipboard back unless the
//! user copied something else meanwhile. Never in a terminal, and a setting turns it off.
//!
//! What is read here is never stored and never logged: it goes to the voice and is dropped.
//!
//! This file decides; `win.rs` asks Windows. The decision is tested from recorded answers.

#[cfg(windows)]
mod win;
// the clipboard's own type is for the trial (tools/trials/selection) and the tests
#[cfg(windows)]
#[allow(unused_imports)]
pub use win::{Clipboard, System};

/// The window in front when the shortcut was pressed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Front {
    /// Its window class
    pub class: String,
    /// Its program's file name, in lower case (`code.exe`)
    pub process: String,
    /// It runs with more rights than Dawwin (as administrator): neither way reaches it
    pub higher_rights: bool,
}

/// What UI Automation answered about the focused element and its ancestors.
#[derive(Clone, Debug, PartialEq)]
pub enum Uia {
    /// A text pattern, and this selected text (not yet trimmed)
    Selected(String),
    /// A text pattern, with nothing selected in it (or a password box)
    Empty,
    /// Nothing on the way up has a text pattern that knows a selection
    NoPattern,
    /// No answer in time, or an error
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    pub uia: Uia,
    /// The class names of the focused element and its ancestors, nearest first
    pub classes: Vec<String>,
}

/// What the Ctrl+C way gave.
#[derive(Clone, Debug, PartialEq)]
pub enum Copied {
    Text(String),
    /// The program copied a whole line because nothing was selected (VS Code, Visual Studio,
    /// Notepad++ mark such a copy)
    WholeLine,
    /// The clipboard did not change: nothing was selected
    Nothing,
    /// The shortcut's keys were still held: Ctrl+C would have been another shortcut
    KeysHeld,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Uia,
    Copy,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Text { text: String, by: Method },
    NothingSelected,
    /// The window runs as administrator
    Administrator,
    /// A terminal without a text pattern: Ctrl+C would stop its program
    Terminal,
    /// No text pattern, and the Ctrl+C way is turned off in Settings
    CopyOff,
    KeysHeld,
    Failed(String),
}

/// How Dawwin asks Windows. `System` does it for real; tests answer from recordings.
pub trait Probe {
    /// The window in front, None when there is none
    fn front(&mut self) -> Option<Front>;
    fn ask_uia(&mut self) -> Asked;
    /// Ctrl+C, with the clipboard kept and put back
    fn copy(&mut self) -> Copied;
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The Ctrl+C way may be used
    pub copy: bool,
}

/// The selected text, or why there is none.
pub fn capture(p: &mut impl Probe, options: Options) -> Outcome {
    let Some(front) = p.front() else {
        return Outcome::NothingSelected;
    };
    if front.higher_rights {
        return Outcome::Administrator;
    }
    let asked = p.ask_uia();
    match asked.uia {
        Uia::Selected(t) => {
            return match clean(&t) {
                Some(text) => Outcome::Text { text, by: Method::Uia },
                None => Outcome::NothingSelected,
            }
        }
        Uia::Empty => return Outcome::NothingSelected,
        Uia::NoPattern | Uia::Failed(_) => {}
    }
    if is_terminal(&front, &asked.classes) {
        return Outcome::Terminal;
    }
    if !options.copy {
        return Outcome::CopyOff;
    }
    match p.copy() {
        Copied::Text(t) => match clean(&t) {
            Some(text) => Outcome::Text { text, by: Method::Copy },
            None => Outcome::NothingSelected,
        },
        Copied::WholeLine | Copied::Nothing => Outcome::NothingSelected,
        Copied::KeysHeld => Outcome::KeysHeld,
        Copied::Failed(why) => Outcome::Failed(why),
    }
}

/// Terminals, where Ctrl+C stops the running program. By program, by window class, and by the
/// class of the focused element (the terminal inside VS Code is an xterm.js text area).
pub fn is_terminal(front: &Front, classes: &[String]) -> bool {
    const PROGRAMS: &[&str] = &[
        "windowsterminal.exe",
        "wt.exe",
        "openconsole.exe",
        "conhost.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "mintty.exe",
        "putty.exe",
        "kitty.exe",
        "conemu.exe",
        "conemu64.exe",
        "cmder.exe",
        "alacritty.exe",
        "wezterm-gui.exe",
        "hyper.exe",
        "tabby.exe",
        "mobaxterm.exe",
        "termius.exe",
        "warp.exe",
        "fluentterminal.app.exe",
    ];
    const CLASSES: &[&str] = &["ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "mintty", "PuTTY", "VirtualConsoleClass", "PseudoConsoleWindow"];
    PROGRAMS.contains(&front.process.as_str())
        || CLASSES.iter().any(|c| front.class.eq_ignore_ascii_case(c))
        || classes.iter().any(|c| c.to_ascii_lowercase().split_whitespace().any(|w| w.starts_with("xterm")))
}

/// The text as it will be read: line ends made plain, NULs out, blank means nothing.
pub fn clean(text: &str) -> Option<String> {
    let t = text.replace("\r\n", "\n").replace('\r', "\n").replace('\0', "");
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers recorded from a program, and what was asked of it.
    struct Recorded {
        front: Option<Front>,
        asked: Asked,
        copied: Copied,
        calls: Vec<&'static str>,
    }

    impl Recorded {
        fn new(process: &str, class: &str, uia: Uia, copied: Copied) -> Self {
            Recorded {
                front: Some(Front { class: class.into(), process: process.into(), higher_rights: false }),
                asked: Asked { uia, classes: vec![] },
                copied,
                calls: vec![],
            }
        }
    }

    impl Probe for Recorded {
        fn front(&mut self) -> Option<Front> {
            self.calls.push("front");
            self.front.clone()
        }
        fn ask_uia(&mut self) -> Asked {
            self.calls.push("uia");
            self.asked.clone()
        }
        fn copy(&mut self) -> Copied {
            self.calls.push("copy");
            self.copied.clone()
        }
    }

    const ON: Options = Options { copy: true };
    const OFF: Options = Options { copy: false };

    fn text(t: &str, by: Method) -> Outcome {
        Outcome::Text { text: t.into(), by }
    }

    #[test]
    fn a_text_pattern_with_a_selection_is_read_without_the_clipboard() {
        let mut p = Recorded::new("chrome.exe", "Chrome_WidgetWin_1", Uia::Selected("  مرحبًا، Dawwin!\r\n".into()), Copied::Failed("not asked".into()));
        assert_eq!(capture(&mut p, ON), text("مرحبًا، Dawwin!", Method::Uia));
        assert_eq!(p.calls, ["front", "uia"], "Ctrl+C is never sent when UI Automation answered");
    }

    #[test]
    fn a_text_pattern_with_nothing_selected_stops_there() {
        // VS Code would copy the whole line, a terminal would stop its program
        for (process, class) in [("code.exe", "Chrome_WidgetWin_1"), ("windowsterminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS"), ("winword.exe", "OpusApp")] {
            let mut p = Recorded::new(process, class, Uia::Empty, Copied::Text("the whole line".into()));
            assert_eq!(capture(&mut p, ON), Outcome::NothingSelected, "{process}");
            assert!(!p.calls.contains(&"copy"), "{process}: no Ctrl+C after an empty selection");
        }
        // a selection of only spaces is nothing too
        let mut p = Recorded::new("notepad.exe", "Notepad", Uia::Selected(" \r\n ".into()), Copied::Text("x".into()));
        assert_eq!(capture(&mut p, ON), Outcome::NothingSelected);
        assert!(!p.calls.contains(&"copy"));
    }

    #[test]
    fn without_a_text_pattern_ctrl_c_is_the_fallback() {
        let mut p = Recorded::new("telegram.exe", "Qt51517QWindowIcon", Uia::NoPattern, Copied::Text("نص من برنامج آخر".into()));
        assert_eq!(capture(&mut p, ON), text("نص من برنامج آخر", Method::Copy));
        let mut p = Recorded::new("acrord32.exe", "AcrobatSDIWindow", Uia::Failed("timed out".into()), Copied::Nothing);
        assert_eq!(capture(&mut p, ON), Outcome::NothingSelected, "the clipboard did not change");
        let mut p = Recorded::new("code.exe", "Chrome_WidgetWin_1", Uia::NoPattern, Copied::WholeLine);
        assert_eq!(capture(&mut p, ON), Outcome::NothingSelected, "a whole line copied for an empty selection is not read");
        let mut p = Recorded::new("app.exe", "X", Uia::NoPattern, Copied::KeysHeld);
        assert_eq!(capture(&mut p, ON), Outcome::KeysHeld);
    }

    #[test]
    fn the_ctrl_c_way_turned_off_is_never_used() {
        let mut p = Recorded::new("telegram.exe", "Qt51517QWindowIcon", Uia::NoPattern, Copied::Text("x".into()));
        assert_eq!(capture(&mut p, OFF), Outcome::CopyOff);
        assert!(!p.calls.contains(&"copy"));
    }

    #[test]
    fn terminals_never_get_ctrl_c() {
        for (process, class) in [
            ("windowsterminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS"),
            ("conhost.exe", "ConsoleWindowClass"),
            ("openconsole.exe", "ConsoleWindowClass"),
            ("mintty.exe", "mintty"),
            ("something.exe", "PuTTY"),
        ] {
            let mut p = Recorded::new(process, class, Uia::NoPattern, Copied::Text("x".into()));
            assert_eq!(capture(&mut p, ON), Outcome::Terminal, "{process}");
            assert!(!p.calls.contains(&"copy"), "{process}");
        }
        // the terminal inside VS Code: an xterm.js text area has the focus
        let mut p = Recorded::new("code.exe", "Chrome_WidgetWin_1", Uia::NoPattern, Copied::Text("x".into()));
        p.asked.classes = vec!["xterm-helper-textarea".into(), "xterm-screen".into(), "terminal-wrapper".into()];
        assert_eq!(capture(&mut p, ON), Outcome::Terminal);
        // but a selection there is read: UI Automation needs no Ctrl+C
        let mut p = Recorded::new("windowsterminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS", Uia::Selected("ping 127.0.0.1".into()), Copied::Failed("".into()));
        assert_eq!(capture(&mut p, ON), text("ping 127.0.0.1", Method::Uia));
    }

    #[test]
    fn a_window_run_as_administrator_is_said_so() {
        let mut p = Recorded::new("regedit.exe", "RegEdit_RegEdit", Uia::Selected("x".into()), Copied::Text("x".into()));
        p.front.as_mut().unwrap().higher_rights = true;
        assert_eq!(capture(&mut p, ON), Outcome::Administrator);
        assert_eq!(p.calls, ["front"], "neither way is tried");
    }

    #[test]
    fn no_window_in_front_is_nothing_selected() {
        let mut p = Recorded::new("x.exe", "X", Uia::Selected("x".into()), Copied::Text("x".into()));
        p.front = None;
        assert_eq!(capture(&mut p, ON), Outcome::NothingSelected);
    }

    #[test]
    fn clean_keeps_the_text_and_its_marks() {
        assert_eq!(clean("\u{feff}"), Some("\u{feff}".into()), "only white space is blank");
        assert_eq!(clean("سطر أول\r\nسطر ثانٍ\rthird\0"), Some("سطر أول\nسطر ثانٍ\nthird".into()));
        assert_eq!(clean("\t \n"), None);
        // diacritics and direction marks stay: the reader decides what to do with them
        assert_eq!(clean("بِسْمِ \u{200f}Dawwin"), Some("بِسْمِ \u{200f}Dawwin".into()));
    }
}
