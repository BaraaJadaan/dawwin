//! A small window of made-up text: a standard edit box (UI Automation gives it a text pattern) and
//! a box of its own that has none but copies on Ctrl+C, as many programs do. Keys, sent only to
//! this window: F2 selects the edit box's first line, F3 selects nothing in it, F4 selects the
//! whole of the other box, F5 nothing in it.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, DrawTextW, EndPaint, FillRect, GetSysColorBrush, COLOR_HIGHLIGHT, COLOR_WINDOW, DT_RTLREADING, DT_WORDBREAK, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_F2, VK_F3, VK_F4, VK_F5};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const EDIT_TEXT: &str = "مرحبًا، هذه جملة مختلقة للتجربة في Dawwin 1.8.0: هل يُقرأ «النص المحدد» جيدًا؟\r\nThe second line is made up too, with one Arabic word: تجربة.";
pub const EDIT_FIRST_LINE: &str = "مرحبًا، هذه جملة مختلقة للتجربة في Dawwin 1.8.0: هل يُقرأ «النص المحدد» جيدًا؟";
pub const BOX_TEXT: &str = "نص في صندوق بلا نمط نصّي (no text pattern), copied by Ctrl+C: 3.5% فقط!";

static SELECTED: AtomicBool = AtomicBool::new(false);
static mut EDIT: HWND = HWND(std::ptr::null_mut());
static mut BOX: HWND = HWND(std::ptr::null_mut());

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Copy as an ordinary program does: its own window owns the clipboard, plain text only.
unsafe fn copy_text(owner: HWND, text: &str) {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    let units = wide(text);
    if OpenClipboard(Some(owner)).is_err() {
        return;
    }
    let _ = EmptyClipboard();
    if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) {
        let p = GlobalLock(g) as *mut u16;
        std::ptr::copy_nonoverlapping(units.as_ptr(), p, units.len());
        let _ = GlobalUnlock(g);
        let _ = SetClipboardData(13, Some(HANDLE(HGLOBAL::from(g).0)));
    }
    let _ = CloseClipboard();
}

unsafe extern "system" fn box_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let _ = SetFocus(Some(hwnd));
            LRESULT(0)
        }
        WM_GETDLGCODE => LRESULT(DLGC_WANTALLKEYS as isize),
        WM_KEYDOWN if wp.0 == 'C' as usize && GetKeyState(VK_CONTROL.0 as i32) < 0 => {
            if SELECTED.load(Ordering::SeqCst) {
                // what most programs do on Ctrl+C: the selection, as text, on the clipboard
                copy_text(hwnd, BOX_TEXT);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            let brush = GetSysColorBrush(if SELECTED.load(Ordering::SeqCst) { COLOR_HIGHLIGHT } else { COLOR_WINDOW });
            FillRect(dc, &r, brush);
            let mut text = wide(BOX_TEXT);
            text.pop();
            DrawTextW(dc, &mut text, &mut r, DT_WORDBREAK | DT_RTLREADING);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn run() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let main_class = w!("DawwinTrialTestWindow");
        let box_class = w!("DawwinTrialNoPattern");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(main_proc), hInstance: instance.into(), lpszClassName: main_class, hbrBackground: GetSysColorBrush(COLOR_WINDOW), ..Default::default() });
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(box_proc), hInstance: instance.into(), lpszClassName: box_class, ..Default::default() });
        let main = CreateWindowExW(WINDOW_EX_STYLE(0), main_class, w!("DAWWIN-TRIAL testwin"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 120, 120, 640, 360, None, None, Some(instance.into()), None).unwrap();
        let text = wide(EDIT_TEXT);
        EDIT = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL) as u32),
            10,
            10,
            600,
            150,
            Some(main),
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        BOX = CreateWindowExW(WS_EX_CLIENTEDGE, box_class, w!(""), WS_CHILD | WS_VISIBLE | WS_TABSTOP, 10, 170, 600, 130, Some(main), None, Some(instance.into()), None).unwrap();
        let _ = SetFocus(Some(EDIT));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_KEYDOWN {
                let first = EDIT_FIRST_LINE.encode_utf16().count();
                let handled = match msg.wParam.0 as u16 {
                    k if k == VK_F2.0 => {
                        let _ = SetFocus(Some(EDIT));
                        SendMessageW(EDIT, 0x00B1, Some(WPARAM(0)), Some(LPARAM(first as isize)));
                        true
                    }
                    k if k == VK_F3.0 => {
                        let _ = SetFocus(Some(EDIT));
                        SendMessageW(EDIT, 0x00B1, Some(WPARAM(0)), Some(LPARAM(0)));
                        true
                    }
                    k if k == VK_F4.0 || k == VK_F5.0 => {
                        SELECTED.store(k == VK_F4.0, Ordering::SeqCst);
                        let _ = SetFocus(Some(BOX));
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(BOX), None, true);
                        true
                    }
                    _ => false,
                };
                if handled {
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
