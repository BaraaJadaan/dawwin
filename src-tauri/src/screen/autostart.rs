//! "Start with Windows": a value under the user's Run key that starts Dawwin hidden, in the
//! notification area. Off until the user turns it on. Test builds can point it elsewhere
//! (`DAWWIN_TEST_AUTOSTART_KEY`) so that a test never touches the real Run key.

use std::path::Path;

use anyhow::{bail, Result};

pub const HIDDEN: &str = "--hidden";
const NAME: &str = "Dawwin";

fn key() -> String {
    #[cfg(debug_assertions)]
    if let Ok(k) = std::env::var("DAWWIN_TEST_AUTOSTART_KEY") {
        return k;
    }
    r"Software\Microsoft\Windows\CurrentVersion\Run".into()
}

/// What the Run key holds for this program.
pub fn command(exe: &Path) -> String {
    format!("\"{}\" {HIDDEN}", exe.display())
}

#[cfg(windows)]
mod sys {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn read() -> Option<String> {
        let (k, n) = (wide(&key()), wide(NAME));
        let mut buf = vec![0u16; 2048];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: the buffers outlive the call, and `len` is their size in bytes
        let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len)) };
        if r.is_err() {
            return None;
        }
        let units = (len as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..units]))
    }

    pub fn write(value: Option<&str>) -> Result<()> {
        let (k, n) = (wide(&key()), wide(NAME));
        // SAFETY: as above; the value is NUL-terminated and its size given in bytes with the NUL
        let r = unsafe {
            match value {
                Some(v) => {
                    let v = wide(v);
                    RegSetKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr()), REG_SZ.0, Some(v.as_ptr() as *const _), (v.len() * 2) as u32)
                }
                None => {
                    let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr()));
                    // already gone is fine
                    if r.0 == 2 {
                        return Ok(());
                    }
                    r
                }
            }
        };
        if r.is_err() {
            bail!("Windows refused to change the Run key ({})", r.0);
        }
        Ok(())
    }
}

/// Whether Windows starts this Dawwin when the user signs in.
pub fn is_on(exe: &Path) -> bool {
    #[cfg(windows)]
    return sys::read().is_some_and(|v| v.eq_ignore_ascii_case(&command(exe)));
    #[cfg(not(windows))]
    {
        let _ = exe;
        false
    }
}

pub fn set(on: bool, exe: &Path) -> Result<()> {
    #[cfg(windows)]
    return sys::write(on.then(|| command(exe)).as_deref());
    #[cfg(not(windows))]
    {
        let _ = (on, exe);
        bail!("only on Windows")
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn start_with_windows_writes_and_removes_one_value() {
        // a key of the test's own, never the real Run key
        let k = format!(r"Software\Dawwin-test-{}\Run", std::process::id());
        std::env::set_var("DAWWIN_TEST_AUTOSTART_KEY", &k);
        let exe = Path::new(r"C:\Program Files\Dawwin\dawwin.exe");
        assert!(!is_on(exe));
        set(true, exe).unwrap();
        assert!(is_on(exe));
        assert_eq!(sys::read().unwrap(), "\"C:\\Program Files\\Dawwin\\dawwin.exe\" --hidden");
        assert!(!is_on(Path::new(r"D:\other\dawwin.exe")), "another copy of Dawwin is not this one");
        set(false, exe).unwrap();
        assert!(!is_on(exe));
        set(false, exe).unwrap();
        let _ = std::process::Command::new("reg").args(["delete", &format!(r"HKCU\Software\Dawwin-test-{}", std::process::id()), "/f"]).output();
    }
}
