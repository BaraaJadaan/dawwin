//! Asking Windows: the window in front and its rights, UI Automation, the clipboard and Ctrl+C.

use std::ffi::c_void;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{DeleteEnhMetaFile, GetEnhMetaFileBits, SetEnhMetaFileBits, HENHMETAFILE};
use windows::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern, SupportedTextSelection_None, UIA_TextPatternId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LWIN, VK_MENU,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};

use super::{Asked, Copied, Front, Probe, Uia};

/// Asks the real Windows.
pub struct System {
    /// The key of the shortcut besides its modifiers: Ctrl+C waits until it is released too
    pub shortcut_key: u16,
    /// How long UI Automation may take before Ctrl+C is tried
    pub uia_wait: Duration,
    /// The window that was in front when the shortcut was pressed
    front: HWND,
}

impl System {
    pub fn new(shortcut_key: u16) -> Self {
        System { shortcut_key, uia_wait: Duration::from_millis(1500), front: HWND::default() }
    }
}

// SAFETY (for the whole file): plain Win32 and COM calls on handles that are checked and kept no
// longer than the calls that use them.

impl Probe for System {
    fn front(&mut self) -> Option<Front> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return None;
        }
        self.front = hwnd;
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let (process, level) = process_info(pid);
        Some(Front { class: class_name(hwnd), process, higher_rights: level.map_or(level_unknown_is_higher(pid), |l| l > own_level()) })
    }

    fn ask_uia(&mut self) -> Asked {
        // on a thread of its own: a program that hangs must not hang Dawwin
        let (tx, rx) = mpsc::channel();
        let wait = self.uia_wait;
        let spawned = std::thread::Builder::new().name("selection-uia".into()).spawn(move || {
            let _ = tx.send(uia_selection(wait));
        });
        if spawned.is_err() {
            return Asked { uia: Uia::Failed("no thread".into()), classes: vec![] };
        }
        rx.recv_timeout(wait + Duration::from_millis(200)).unwrap_or(Asked { uia: Uia::Failed("UI Automation did not answer in time".into()), classes: vec![] })
    }

    fn copy(&mut self) -> Copied {
        if !keys_released(self.shortcut_key, Duration::from_millis(2000)) {
            return Copied::KeysHeld;
        }
        // the user may have moved on while the keys were held
        if unsafe { GetForegroundWindow() } != self.front {
            return Copied::Failed("another window came to the front".into());
        }
        copy_selection()
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The program's file name in lower case, and its integrity level (None when it cannot be read).
fn process_info(pid: u32) -> (String, Option<u32>) {
    let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return (String::new(), None);
    };
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let name = match unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len) } {
        Ok(()) => {
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()
        }
        Err(_) => String::new(),
    };
    let level = integrity_level(h);
    unsafe {
        let _ = CloseHandle(h);
    }
    (name, level)
}

/// A process that does not even let Dawwin read its name is taken as one with more rights,
/// unless it is gone.
fn level_unknown_is_higher(pid: u32) -> bool {
    pid != 0 && unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.map(|h| unsafe { CloseHandle(h) }).is_err()
}

fn own_level() -> u32 {
    integrity_level(unsafe { GetCurrentProcess() }).unwrap_or(0x2000)
}

/// The process's mandatory integrity level: 0x2000 medium, 0x3000 high (administrator).
fn integrity_level(process: HANDLE) -> Option<u32> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len);
        let mut buf = vec![0u8; len.max(64) as usize];
        let got = GetTokenInformation(token, TokenIntegrityLevel, Some(buf.as_mut_ptr() as *mut c_void), buf.len() as u32, &mut len);
        let _ = CloseHandle(token);
        got.ok()?;
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        Some(*GetSidSubAuthority(sid, count.checked_sub(1)? as u32))
    }
}

/// The selection of the focused element, or of the nearest ancestor with a text pattern.
fn uia_selection(wait: Duration) -> Asked {
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED);
        let asked = uia_selection_inner(wait).unwrap_or_else(|e| Asked { uia: Uia::Failed(format!("{e}")), classes: vec![] });
        if com.is_ok() {
            CoUninitialize();
        }
        asked
    }
}

unsafe fn uia_selection_inner(wait: Duration) -> windows::core::Result<Asked> {
    let uia: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
    if let Ok(uia2) = uia.cast::<IUIAutomation2>() {
        let ms = wait.as_millis() as u32;
        let _ = uia2.SetConnectionTimeout(ms);
        let _ = uia2.SetTransactionTimeout(ms);
    }
    let walker = uia.RawViewWalker()?;
    let mut element: IUIAutomationElement = uia.GetFocusedElement()?;
    let pid = element.CurrentProcessId().unwrap_or(0);
    let mut classes = Vec::new();
    for _ in 0..24 {
        classes.push(element.CurrentClassName().map(|s| s.to_string()).unwrap_or_default());
        if element.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false) {
            // never read a password box
            return Ok(Asked { uia: Uia::Empty, classes });
        }
        if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
            if pattern.SupportedTextSelection().map_or(false, |s| s != SupportedTextSelection_None) {
                let ranges = pattern.GetSelection()?;
                let mut text = String::new();
                for i in 0..ranges.Length().unwrap_or(0) {
                    if let Ok(t) = ranges.GetElement(i).and_then(|r| r.GetText(-1)) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&t.to_string());
                    }
                }
                let uia = if text.trim().is_empty() { Uia::Empty } else { Uia::Selected(text) };
                return Ok(Asked { uia, classes });
            }
        }
        // up, but not out of the program
        match walker.GetParentElement(&element) {
            Ok(parent) if parent.CurrentProcessId().unwrap_or(-1) == pid => element = parent,
            _ => break,
        }
    }
    Ok(Asked { uia: Uia::NoPattern, classes })
}

fn key_down(vk: u16) -> bool {
    (unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000) != 0
}

/// Wait until the modifiers and the shortcut's key are up.
fn keys_released(key: u16, within: Duration) -> bool {
    let keys = [VK_SHIFT.0, VK_CONTROL.0, VK_MENU.0, VK_LWIN.0, VK_RWIN.0, key];
    let start = Instant::now();
    loop {
        if !keys.iter().any(|&k| k != 0 && key_down(k)) {
            return true;
        }
        if start.elapsed() > within {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    }
}

/// Send Ctrl+C to the window in front, read the text it copies, and put the clipboard back.
fn copy_selection() -> Copied {
    let owner = Owner::new();
    let Some(saved) = Clipboard::save() else {
        return Copied::Failed("the clipboard is busy".into());
    };
    let before = saved.sequence;
    let keys = [key(VK_CONTROL, false), key(VK_C, false), key(VK_C, true), key(VK_CONTROL, true)];
    if unsafe { SendInput(&keys, std::mem::size_of::<INPUT>() as i32) } != keys.len() as u32 {
        return Copied::Failed("Windows did not let Dawwin press Ctrl+C".into());
    }
    // wait for the copy, then for the program to finish writing all its formats
    let start = Instant::now();
    let mut seen = before;
    let mut settled = None;
    while start.elapsed() < Duration::from_millis(500) {
        std::thread::sleep(Duration::from_millis(10));
        let now = unsafe { GetClipboardSequenceNumber() };
        if now != seen {
            seen = now;
            settled = Some(Instant::now());
        } else if settled.is_some_and(|t| t.elapsed() >= Duration::from_millis(40)) {
            break;
        }
    }
    if seen == before {
        return Copied::Nothing;
    }
    let read = read_copied_text();
    let after_read = unsafe { GetClipboardSequenceNumber() };
    // the user's own copy since then wins over what Dawwin kept
    if after_read == seen {
        saved.restore(owner.0);
    }
    read
}

/// The text just copied, or that it was a whole line copied for an empty selection.
fn read_copied_text() -> Copied {
    if !open_clipboard(None) {
        return Copied::Failed("the clipboard is busy".into());
    }
    let marks = ["MSDEVLineSelect", "VisualStudioEditorOperationsLineCutCopyClipboardTag"].map(|n| format_id(n));
    let vscode = format_id("vscode-editor-data");
    let result = unsafe {
        if marks.iter().any(|&f| f != 0 && GetClipboardData(f).is_ok()) {
            Copied::WholeLine
        } else if vscode != 0 && read_global(vscode).is_some_and(|b| String::from_utf8_lossy(&b).replace(' ', "").contains("\"isFromEmptySelection\":true")) {
            Copied::WholeLine
        } else {
            match read_global(13) {
                Some(bytes) => Copied::Text(utf16z(&bytes)),
                None => Copied::Nothing,
            }
        }
    };
    unsafe {
        let _ = CloseClipboard();
    }
    result
}

fn utf16z(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    String::from_utf16_lossy(&units)
}

fn format_id(name: &str) -> u32 {
    let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) }
}

fn open_clipboard(owner: Option<HWND>) -> bool {
    for _ in 0..25 {
        if unsafe { OpenClipboard(owner) }.is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// The bytes of an HGLOBAL format; the clipboard must be open.
unsafe fn read_global(format: u32) -> Option<Vec<u8>> {
    let h = GetClipboardData(format).ok()?;
    let g = HGLOBAL(h.0);
    let size = GlobalSize(g);
    let p = GlobalLock(g) as *const u8;
    if p.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(p, size).to_vec();
    let _ = GlobalUnlock(g);
    Some(bytes)
}

/// A window of Dawwin's own to own the clipboard while it is put back: with none, Windows takes
/// the data from nobody and SetClipboardData fails.
struct Owner(HWND);

impl Owner {
    fn new() -> Self {
        let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
        let hwnd = unsafe { CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()), PCWSTR::null(), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None) };
        Owner(hwnd.unwrap_or_default())
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

/// Everything the clipboard held, to be put back as it was.
#[derive(Clone, Debug, PartialEq)]
pub struct Clipboard {
    /// Each format with its bytes, in the order the clipboard listed them
    pub formats: Vec<(u32, Data)>,
    /// The clipboard's change counter when it was read
    pub sequence: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Bytes(Vec<u8>),
    /// An enhanced metafile (Word and Excel copy one), as its bits
    Metafile(Vec<u8>),
}

const CF_BITMAP: u32 = 2;
const CF_METAFILEPICT: u32 = 3;
const CF_PALETTE: u32 = 9;
const CF_ENHMETAFILE: u32 = 14;
const CF_OWNERDISPLAY: u32 = 0x80;
const CF_DSPBITMAP: u32 = 0x82;
const CF_DSPMETAFILEPICT: u32 = 0x83;
const CF_DSPENHMETAFILE: u32 = 0x8E;

/// Formats that are not memory Dawwin can copy: Windows makes the bitmap and the palette from
/// the DIB again, the others belong to their program.
fn skipped(format: u32) -> bool {
    matches!(format, CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_OWNERDISPLAY | CF_DSPBITMAP | CF_DSPMETAFILEPICT | CF_DSPENHMETAFILE) || (0x200..=0x3FF).contains(&format)
}

impl Clipboard {
    /// Read every format the clipboard holds. None when it stays busy.
    pub fn save() -> Option<Clipboard> {
        if !open_clipboard(None) {
            return None;
        }
        let mut formats = Vec::new();
        unsafe {
            let sequence = GetClipboardSequenceNumber();
            let mut f = EnumClipboardFormats(0);
            while f != 0 {
                if f == CF_ENHMETAFILE {
                    if let Ok(h) = GetClipboardData(f) {
                        let emf = HENHMETAFILE(h.0);
                        let n = GetEnhMetaFileBits(emf, None);
                        if n > 0 {
                            let mut bits = vec![0u8; n as usize];
                            GetEnhMetaFileBits(emf, Some(&mut bits));
                            formats.push((f, Data::Metafile(bits)));
                        }
                    }
                } else if !skipped(f) {
                    if let Some(bytes) = read_global(f) {
                        formats.push((f, Data::Bytes(bytes)));
                    }
                }
                f = EnumClipboardFormats(f);
            }
            let _ = CloseClipboard();
            Some(Clipboard { formats, sequence })
        }
    }

    /// Put it back, with `owner` as the clipboard's owner. The copy is kept out of clipboard
    /// history and cloud sync, where the original already is (or was kept out by its program).
    pub fn restore(&self, owner: HWND) -> bool {
        if !open_clipboard(Some(owner)) {
            return false;
        }
        let mut ok = unsafe { EmptyClipboard() }.is_ok();
        let put = |format: u32, data: &Data| unsafe {
            match data {
                Data::Bytes(bytes) => {
                    let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else { return false };
                    let p = GlobalLock(g) as *mut u8;
                    if p.is_null() {
                        let _ = GlobalFree(Some(g));
                        return false;
                    }
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                    let _ = GlobalUnlock(g);
                    if SetClipboardData(format, Some(HANDLE(g.0))).is_err() {
                        let _ = GlobalFree(Some(g));
                        return false;
                    }
                    true
                }
                Data::Metafile(bits) => {
                    let emf = SetEnhMetaFileBits(bits);
                    if emf.is_invalid() {
                        return false;
                    }
                    if SetClipboardData(format, Some(HANDLE(emf.0))).is_err() {
                        let _ = DeleteEnhMetaFile(Some(emf));
                        return false;
                    }
                    true
                }
            }
        };
        for (f, data) in &self.formats {
            ok &= put(*f, data);
        }
        if !self.formats.is_empty() {
            for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
                let f = format_id(name);
                if f != 0 && !self.formats.iter().any(|(g, _)| *g == f) {
                    put(f, &Data::Bytes(0u32.to_le_bytes().to_vec()));
                }
            }
        }
        unsafe {
            let _ = CloseClipboard();
        }
        ok
    }

    /// Put these formats on the clipboard (tests and the trial), out of clipboard history.
    pub fn set(formats: &[(u32, Vec<u8>)]) -> bool {
        let owner = Owner::new();
        let c = Clipboard { formats: formats.iter().map(|(f, b)| (*f, Data::Bytes(b.clone()))).collect(), sequence: 0 };
        let mut with_marks = c.clone();
        let exclude = format_id("ExcludeClipboardContentFromMonitorProcessing");
        with_marks.formats.push((exclude, Data::Bytes(vec![0])));
        with_marks.restore(owner.0)
    }

    /// The formats Windows makes up from others (text in other code pages, bitmaps from DIBs),
    /// and the history marks, left out when two clipboards are compared.
    pub fn essentials(&self) -> Vec<(u32, Data)> {
        let marks = ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard", "ExcludeClipboardContentFromMonitorProcessing"].map(format_id);
        self.formats.iter().filter(|(f, _)| ![1, 7, 16, 17].contains(f) && !marks.contains(f)).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CF_UNICODETEXT: u32 = 13;
    const CF_DIB: u32 = 8;

    fn dib() -> Vec<u8> {
        // a made-up 4×2 picture, 32 bits a pixel
        let mut h = 40u32.to_le_bytes().to_vec();
        h.extend_from_slice(&4i32.to_le_bytes());
        h.extend_from_slice(&2i32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&32u16.to_le_bytes());
        h.extend_from_slice(&[0u8; 24]);
        for i in 0..8u8 {
            h.extend_from_slice(&[i * 30, 255 - i * 30, 128, 0]);
        }
        h
    }

    fn text(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(Some(0)).flat_map(|u| u.to_le_bytes()).collect()
    }

    /// The real clipboard of this session: kept and put back around the test, and only made-up
    /// data goes on it. Run by hand: `cargo test clipboard -- --ignored`.
    #[test]
    #[ignore]
    fn the_clipboard_is_put_back_with_every_format() {
        let theirs = Clipboard::save().expect("clipboard");
        let html = format_id("HTML Format");
        let ours = [(CF_UNICODETEXT, text("نص مختلق، with English: 42%")), (CF_DIB, dib()), (html, b"Version:0.9\r\n<b>made up</b>\0".to_vec())];
        assert!(Clipboard::set(&ours));
        let before = Clipboard::save().unwrap();
        // something else is copied, as Ctrl+C would
        assert!(Clipboard::set(&[(CF_UNICODETEXT, text("the selection"))]));
        let owner = Owner::new();
        assert!(before.restore(owner.0));
        let after = Clipboard::save().unwrap();
        assert_eq!(before.essentials(), after.essentials(), "every format back, byte for byte");
        assert!(after.formats.iter().any(|(f, _)| *f == CF_DIB), "the picture too");
        // and the user's own clipboard, as it was
        theirs.restore(owner.0);
        assert_eq!(theirs.essentials(), Clipboard::save().unwrap().essentials());
    }
}
