//! The system-wide shortcut: written as "Ctrl+Alt+R", registered on a thread of its own, and a
//! refusal (another program holds the keys) reported, never ignored.

use std::fmt;

/// A combination of keys: at least one of Ctrl, Alt, Win, then one key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keys {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// The Windows virtual-key code of the key
    pub key: u16,
}

/// The default the spec proposes, then free-looking others to offer when it is taken. Ctrl+Shift+R
/// is not among them: browsers reload with it.
pub const DEFAULT: &str = "Ctrl+Alt+R";
pub const OTHERS: &[&str] = &["Ctrl+Alt+S", "Ctrl+Alt+Q", "Ctrl+Shift+Space", "Ctrl+Alt+Shift+R", "Alt+Shift+S", "Ctrl+Alt+F9"];

fn key_name(vk: u16) -> Option<String> {
    Some(match vk {
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x20 => "Space".into(),
        0x13 => "Pause".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0xC0 => "`".into(),
        _ => return None,
    })
}

fn key_code(name: &str) -> Option<u16> {
    let n = name.trim();
    let up = n.to_ascii_uppercase();
    if up.len() == 1 {
        let c = up.as_bytes()[0];
        if c.is_ascii_alphanumeric() {
            return Some(c as u16);
        }
    }
    if let Some(f) = up.strip_prefix('F').and_then(|d| d.parse::<u16>().ok()) {
        if (1..=24).contains(&f) {
            return Some(0x6F + f);
        }
    }
    (0u16..=0xFF).find(|&vk| key_name(vk).is_some_and(|k| k.eq_ignore_ascii_case(n)))
}

impl Keys {
    pub fn parse(s: &str) -> Option<Keys> {
        let mut k = Keys { ctrl: false, alt: false, shift: false, win: false, key: 0 };
        for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => k.ctrl = true,
                "alt" => k.alt = true,
                "shift" => k.shift = true,
                "win" | "windows" | "super" | "meta" => k.win = true,
                _ if k.key == 0 => k.key = key_code(part)?,
                _ => return None,
            }
        }
        // a key alone, or with Shift alone, would take a key from typing
        (k.key != 0 && (k.ctrl || k.alt || k.win)).then_some(k)
    }

    /// The modifiers as RegisterHotKey wants them, with MOD_NOREPEAT: held down, it fires once.
    pub fn modifiers(&self) -> u32 {
        (self.alt as u32) | (self.ctrl as u32) << 1 | (self.shift as u32) << 2 | (self.win as u32) << 3 | 0x4000
    }
}

impl fmt::Display for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                parts.push(name.to_string());
            }
        }
        parts.push(key_name(self.key).unwrap_or_else(|| format!("#{}", self.key)));
        write!(f, "{}", parts.join("+"))
    }
}

/// What registering the shortcut gave.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Registered {
    Off,
    On { keys: String },
    /// Another program holds these keys
    Taken { keys: String },
    /// Not a combination Dawwin can use
    Invalid { keys: String },
}

#[cfg(windows)]
pub use sys::Hotkey;

#[cfg(windows)]
mod sys {
    use super::*;
    use std::sync::mpsc;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY, WM_QUIT};
    use windows::Win32::Foundation::{LPARAM, WPARAM};

    const ID: i32 = 0x0D17;
    const PROBE: i32 = 0x0D18;
    const WM_DO: u32 = WM_APP + 0x31;

    enum Ask {
        Set(Option<Keys>, mpsc::Sender<bool>),
        Probe(Keys, mpsc::Sender<bool>),
    }

    /// The thread that holds the shortcut. A shortcut belongs to the thread that registered it,
    /// and its presses arrive in that thread's messages.
    pub struct Hotkey {
        thread: u32,
        asks: mpsc::Sender<Ask>,
    }

    impl Hotkey {
        /// `pressed` runs on the shortcut's thread at each press: keep it short.
        pub fn start(pressed: impl Fn() + Send + 'static) -> Option<Hotkey> {
            let (tx, rx) = mpsc::channel::<Ask>();
            let (id_tx, id_rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("shortcut".into())
                .spawn(move || unsafe {
                    let mut msg = MSG::default();
                    // make the message queue before anyone posts to it
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                    let _ = id_tx.send(GetCurrentThreadId());
                    let mut current: Option<Keys> = None;
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        match msg.message {
                            WM_HOTKEY if msg.wParam.0 as i32 == ID => pressed(),
                            WM_DO => {
                                while let Ok(ask) = rx.try_recv() {
                                    match ask {
                                        Ask::Set(keys, reply) => {
                                            let register = |k: Keys| RegisterHotKey(None, ID, HOT_KEY_MODIFIERS(k.modifiers()), k.key as u32).is_ok();
                                            let before = current.take();
                                            if before.is_some() {
                                                let _ = UnregisterHotKey(None, ID);
                                            }
                                            let ok = keys.map_or(true, register);
                                            current = if ok {
                                                keys
                                            } else {
                                                // the keys before stay, if they can
                                                before.filter(|&k| register(k))
                                            };
                                            let _ = reply.send(ok);
                                        }
                                        Ask::Probe(k, reply) => {
                                            let free = Some(k) == current || RegisterHotKey(None, PROBE, HOT_KEY_MODIFIERS(k.modifiers()), k.key as u32).is_ok();
                                            if free && Some(k) != current {
                                                let _ = UnregisterHotKey(None, PROBE);
                                            }
                                            let _ = reply.send(free);
                                        }
                                    }
                                }
                            }
                            WM_QUIT => break,
                            _ => {}
                        }
                    }
                })
                .ok()?;
            let thread = id_rx.recv().ok()?;
            Some(Hotkey { thread, asks: tx })
        }

        fn ask(&self, a: Ask, reply: mpsc::Receiver<bool>) -> bool {
            if self.asks.send(a).is_err() {
                return false;
            }
            unsafe {
                let _ = PostThreadMessageW(self.thread, WM_DO, WPARAM(0), LPARAM(0));
            }
            reply.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or(false)
        }

        /// Register these keys in place of the ones before (None lets go). False: another program
        /// holds them, and the keys before stay registered.
        pub fn set(&self, keys: Option<Keys>) -> bool {
            let (tx, rx) = mpsc::channel();
            self.ask(Ask::Set(keys, tx), rx)
        }

        /// Whether no other program holds these keys (Dawwin's own count as free).
        pub fn free(&self, keys: Keys) -> bool {
            let (tx, rx) = mpsc::channel();
            self.ask(Ask::Probe(keys, tx), rx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_read_and_written_the_same_way() {
        for s in ["Ctrl+Alt+R", "Ctrl+Shift+Space", "Alt+Shift+S", "Ctrl+Alt+F9", "Alt+Win+7", "Ctrl+Alt+Shift+R"] {
            assert_eq!(Keys::parse(s).unwrap().to_string(), s);
        }
        assert_eq!(Keys::parse(" ctrl + alt + r ").unwrap().to_string(), "Ctrl+Alt+R");
        assert_eq!(Keys::parse("Alt+Ctrl+r").unwrap().to_string(), "Ctrl+Alt+R", "written in one order");
        let k = Keys::parse(DEFAULT).unwrap();
        assert_eq!((k.key, k.modifiers()), (b'R' as u16, 0x4000 | 1 | 2));
    }

    #[test]
    fn a_key_alone_or_with_shift_only_is_refused() {
        for s in ["R", "Shift+R", "Ctrl+Alt", "Ctrl+Alt+R+S", "Ctrl+Alt+ر", "", "Ctrl+Alt+F25"] {
            assert_eq!(Keys::parse(s), None, "{s}");
        }
        for s in OTHERS {
            assert!(Keys::parse(s).is_some(), "{s}");
        }
    }

    /// Registers real shortcuts in this session: run by hand, `cargo test shortcut -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn a_shortcut_another_program_holds_is_reported() {
        let first = Hotkey::start(|| {}).unwrap();
        let second = Hotkey::start(|| {}).unwrap();
        let keys = Keys::parse("Ctrl+Alt+Shift+F11").unwrap();
        assert!(first.set(Some(keys)));
        assert!(first.free(keys), "its own shortcut counts as free");
        assert!(!second.free(keys), "held by another thread, as by another program");
        assert!(!second.set(Some(keys)));
        assert!(first.set(None));
        assert!(second.set(Some(keys)), "free again once let go");
        assert!(second.set(None));
    }
}
