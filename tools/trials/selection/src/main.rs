//! Trial 1 of Dawwin 1.8.0: does the shortcut get the selected text out of real programs?
//!
//! It runs the app's own capture code (`src-tauri/src/selection`) behind a real system-wide
//! shortcut, on windows this trial opens itself, holding made-up text. Nothing else is read: a
//! press of the shortcut in any other window is ignored, keys go only to the trial's own windows,
//! each program runs in a throwaway profile, and the clipboard the user had is put back at the end.
//! It waits while the user is typing or moving the mouse, and gives the window back afterwards.
//!
//!   trial-selection run [target ...]   targets: testwin chrome edge chrome-pdf edge-pdf firefox word
//!                                      vscode antigravity wt conhost
//!   trial-selection watch              by hand: select text in any program, press Ctrl+Alt+Q; it
//!                                      prints the way that worked, never the text
//!   trial-selection hotkeys            which shortcuts other programs already hold
//!   trial-selection testwin            the small test window alone

#[path = "../../../../src-tauri/src/selection/mod.rs"]
mod selection;
mod testwin;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use selection::{capture, Asked, Clipboard, Copied, Front, Method, Options, Outcome, Probe, System, Uia};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TOKEN: &str = "DAWWIN-TRIAL";
const LINE1: &str = "مرحبًا، هذه جملة مختلقة للتجربة في Dawwin 1.8.0: هل يُقرأ «النص المحدد» جيدًا؟";
const LINE2: &str = "The second line is made up too, with one Arabic word: تجربة.";
const BOX_P2: &str = "A paragraph in a box that takes the focus (tabindex), with Arabic inside: كلمة واحدة.";
const AREA: &str = "نص في مربع كتابة، with English words, 42%.";
const HOTKEY_ID: i32 = 0x4441;
const KEY: u16 = 'Q' as u16;
const WM_TRIAL_DONE: u32 = WM_APP + 1;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("testwin") => testwin::run(),
        Some("watch") => watch(),
        Some("run") => run(args[1..].to_vec()),
        Some("hotkeys") => {
            // which shortcuts another program already holds (each is let go at once)
            let list: [(&str, HOT_KEY_MODIFIERS, u32); 14] = [
                ("Ctrl+Alt+R", MOD_CONTROL | MOD_ALT, 'R' as u32),
                ("Ctrl+Alt+S", MOD_CONTROL | MOD_ALT, 'S' as u32),
                ("Ctrl+Alt+Q", MOD_CONTROL | MOD_ALT, 'Q' as u32),
                ("Ctrl+Alt+E", MOD_CONTROL | MOD_ALT, 'E' as u32),
                ("Ctrl+Alt+L", MOD_CONTROL | MOD_ALT, 'L' as u32),
                ("Ctrl+Alt+Space", MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32),
                ("Ctrl+Shift+Space", MOD_CONTROL | MOD_SHIFT, VK_SPACE.0 as u32),
                ("Alt+Shift+R", MOD_ALT | MOD_SHIFT, 'R' as u32),
                ("Ctrl+Shift+R", MOD_CONTROL | MOD_SHIFT, 'R' as u32),
                ("Ctrl+Alt+Shift+R", MOD_CONTROL | MOD_ALT | MOD_SHIFT, 'R' as u32),
                ("Win+Alt+R", MOD_WIN | MOD_ALT, 'R' as u32),
                ("Win+Shift+R", MOD_WIN | MOD_SHIFT, 'R' as u32),
                ("Ctrl+Alt+F9", MOD_CONTROL | MOD_ALT, VK_F9.0 as u32),
                ("Alt+Shift+S", MOD_ALT | MOD_SHIFT, 'S' as u32),
            ];
            for (name, m, vk) in list {
                let r = unsafe { RegisterHotKey(None, 0x100, m | MOD_NOREPEAT, vk) };
                println!("{name:18} {}", if r.is_ok() { "free" } else { "taken" });
                if r.is_ok() {
                    unsafe {
                        let _ = UnregisterHotKey(None, 0x100);
                    }
                }
            }
        }
        _ => eprintln!("trial-selection run [target ...] | testwin"),
    }
}

/// The capture, as the app does it, behind a gate: only the trial's own windows are asked.
struct Gated {
    inner: System,
    force_copy: bool,
    ignored: bool,
    asked: Option<Asked>,
}

impl Probe for Gated {
    fn front(&mut self) -> Option<Front> {
        let title = window_text(unsafe { GetForegroundWindow() });
        if !title.contains(TOKEN) {
            self.ignored = true;
            return None;
        }
        self.inner.front()
    }
    fn ask_uia(&mut self) -> Asked {
        let a = if self.force_copy { Asked { uia: Uia::NoPattern, classes: vec![] } } else { self.inner.ask_uia() };
        self.asked = Some(a.clone());
        a
    }
    fn copy(&mut self) -> Copied {
        self.inner.copy()
    }
}

struct Pressed {
    outcome: Outcome,
    ms: u128,
    ignored: bool,
    asked: Option<Asked>,
}

/// What the shortcut's thread should do on the next press.
#[derive(Clone, Copy)]
struct Mode {
    force_copy: bool,
    copy: bool,
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn run(targets: Vec<String>) {
    let dir = std::env::var("TRIAL_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("dawwin-trial-selection"));
    std::fs::create_dir_all(&dir).unwrap();
    let targets = if targets.is_empty() { ["testwin", "chrome", "edge", "firefox", "word", "vscode", "wt", "conhost"].map(String::from).to_vec() } else { targets };

    // the shortcut: Ctrl+Alt+R is the spec's default, but another program holds it on the
    // maintainer's PC (see `hotkeys`), so the trial uses Ctrl+Alt+Q
    let main_thread = unsafe { GetCurrentThreadId() };
    let mods = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    let free = unsafe { RegisterHotKey(None, HOTKEY_ID, mods, KEY as u32) };
    println!("shortcut Ctrl+Alt+{}: {}", KEY as u8 as char, if free.is_ok() { "free, registered" } else { "TAKEN by another program" });
    if free.is_err() {
        return;
    }
    // a second registration of the same keys fails, as when another program has them
    let again = unsafe { RegisterHotKey(None, HOTKEY_ID + 1, mods, KEY as u32) };
    println!("registering it a second time: {}", match again {
        Ok(()) => "succeeded (unexpected)".to_string(),
        Err(e) => format!("refused: {e}"),
    });

    let mode = Arc::new(Mutex::new(Mode { force_copy: false, copy: true }));
    let (tx, rx) = mpsc::channel::<Pressed>();
    let runner_mode = mode.clone();
    let runner = std::thread::spawn(move || {
        let report = scenarios(&dir, &targets, &runner_mode, &rx);
        unsafe {
            let _ = PostThreadMessageW(main_thread, WM_TRIAL_DONE, WPARAM(0), LPARAM(0));
        }
        report
    });

    // the shortcut's messages
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if msg.message == WM_TRIAL_DONE {
            break;
        }
        if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
            let m = *mode.lock().unwrap();
            let tx = tx.clone();
            let start = Instant::now();
            std::thread::spawn(move || {
                let mut p = Gated { inner: System::new(KEY), force_copy: m.force_copy, ignored: false, asked: None };
                let outcome = capture(&mut p, Options { copy: m.copy });
                let _ = tx.send(Pressed { outcome, ms: start.elapsed().as_millis(), ignored: p.ignored, asked: p.asked });
            });
        }
    }
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
    let report = runner.join().unwrap();
    println!("\n{report}");
}

// ---- input, politely ----

fn tick() -> u32 {
    unsafe { GetTickCount() }
}

fn last_input() -> u32 {
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        let _ = GetLastInputInfo(&mut li);
    }
    li.dwTime
}

/// Waits until nobody has touched the keyboard or mouse for two seconds (input the trial sent
/// itself does not count). False after two minutes of someone working.
fn wait_idle(own_input: u32) -> bool {
    let start = Instant::now();
    loop {
        let li = last_input();
        let user_quiet = li <= own_input.wrapping_add(30) || tick().wrapping_sub(li) >= 2000;
        if user_quiet {
            return true;
        }
        if start.elapsed() > Duration::from_secs(120) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn extended(vk: VIRTUAL_KEY) -> bool {
    [VK_HOME, VK_END, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_PRIOR, VK_NEXT, VK_INSERT, VK_DELETE].contains(&vk)
}

fn input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

struct Keys {
    target: HWND,
    sent: u32,
}

impl Keys {
    /// A chord (modifiers then a key), only while the trial's window is in front.
    fn chord(&mut self, mods: &[VIRTUAL_KEY], key: VIRTUAL_KEY) -> bool {
        if unsafe { GetForegroundWindow() } != self.target {
            return false;
        }
        let mut v: Vec<INPUT> = mods.iter().map(|&m| input(m, false)).collect();
        v.push(input(key, false));
        v.push(input(key, true));
        v.extend(mods.iter().rev().map(|&m| input(m, true)));
        unsafe { SendInput(&v, std::mem::size_of::<INPUT>() as i32) };
        self.sent = tick();
        std::thread::sleep(Duration::from_millis(120));
        true
    }
    fn key(&mut self, key: VIRTUAL_KEY) -> bool {
        self.chord(&[], key)
    }
    /// The shortcut as a person presses it: held a moment, then let go.
    fn shortcut(&mut self) -> bool {
        if unsafe { GetForegroundWindow() } != self.target {
            return false;
        }
        let down = [input(VK_CONTROL, false), input(VK_MENU, false), input(VIRTUAL_KEY(KEY), false)];
        let up = [input(VIRTUAL_KEY(KEY), true), input(VK_MENU, true), input(VK_CONTROL, true)];
        unsafe { SendInput(&down, std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(Duration::from_millis(180));
        unsafe { SendInput(&up, std::mem::size_of::<INPUT>() as i32) };
        self.sent = tick();
        true
    }
}

/// Bring one of the trial's windows to the front, without keys.
fn bring_front(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        for _ in 0..3 {
            if GetForegroundWindow() == hwnd {
                return true;
            }
            let fg = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg, None);
            let me = GetCurrentThreadId();
            let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            if attached {
                let _ = AttachThreadInput(me, fg_thread, false);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        GetForegroundWindow() == hwnd
    }
}

fn find_window(token: &str, within: Duration) -> Option<HWND> {
    let start = Instant::now();
    while start.elapsed() < within {
        let mut found: Vec<(HWND, String)> = Vec::new();
        unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> windows::core::BOOL {
            let v = &mut *(lp.0 as *mut Vec<(HWND, String)>);
            if IsWindowVisible(hwnd).as_bool() {
                v.push((hwnd, window_text(hwnd)));
            }
            true.into()
        }
        unsafe {
            let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut _ as isize));
        }
        if let Some((h, _)) = found.into_iter().find(|(_, t)| t.contains(token)) {
            return Some(h);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    None
}

// ---- the clipboard ----

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(|u| u.to_le_bytes()).collect()
}

fn picture() -> Vec<u8> {
    let mut h = 40u32.to_le_bytes().to_vec();
    h.extend_from_slice(&8i32.to_le_bytes());
    h.extend_from_slice(&4i32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&32u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 24]);
    for i in 0..32u8 {
        h.extend_from_slice(&[i * 8, 255 - i * 8, 90, 0]);
    }
    h
}

fn html_format() -> u32 {
    let w: Vec<u16> = "HTML Format".encode_utf16().chain(Some(0)).collect();
    unsafe { windows::Win32::System::DataExchange::RegisterClipboardFormatW(windows::core::PCWSTR(w.as_ptr())) }
}

/// Made-up text, a made-up picture and made-up HTML, as someone's clipboard.
fn made_up_clipboard() -> Option<Clipboard> {
    let mut html = "Version:0.9\r\nStartHTML:0000000000\r\n<html><body><b>made up</b> نص</body></html>".as_bytes().to_vec();
    html.push(0);
    Clipboard::set(&[(13, utf16("ما نسخه المستخدم: made-up text, 7%")), (8, picture()), (html_format(), html)]);
    Clipboard::save()
}

fn sequence() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

// ---- processes of the trial ----

fn ps(script: &str) -> String {
    let out = Command::new("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// Processes whose command line holds this text (the trial puts its folder or a token in each).
fn running_with(text: &str) -> usize {
    let script = format!("@(Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains('{}') -and $_.Name -ne 'powershell.exe' -or ($_.Name -eq 'powershell.exe' -and $_.CommandLine.Contains('{}') -and -not $_.CommandLine.Contains('Get-CimInstance')) }}).Count", text.replace('\'', "''"), text.replace('\'', "''"));
    ps(&script).parse().unwrap_or(0)
}

fn kill_with(text: &str) {
    // only what the trial started: its folder or its token is in the command line
    let dir = std::env::var("TRIAL_DIR").unwrap_or_default();
    if text.is_empty() || !(text.contains(TOKEN) || (!dir.is_empty() && text.contains(&dir))) {
        return;
    }
    let script = format!("Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains('{}') -and -not $_.CommandLine.Contains('Get-CimInstance') }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}", text.replace('\'', "''"));
    ps(&script);
}

// ---- the programs ----

#[derive(Clone, Copy, PartialEq)]
enum Sel {
    Keys(&'static [(&'static [VIRTUAL_KEY], VIRTUAL_KEY)]),
    /// Select this text through UI Automation (terminals)
    Find(&'static str),
    None,
}

struct Case {
    name: &'static str,
    select: Sel,
    /// The text the selection must hold (part of it)
    expect: Option<&'static str>,
}

struct Target {
    name: &'static str,
    window: String,
    /// Something only this program's processes have in their command line, to close them
    marker: String,
    cases: Vec<Case>,
    /// A program that must keep running (terminals): its marker
    running: Option<String>,
    note: String,
}

const C: VIRTUAL_KEY = VK_CONTROL;
const S: VIRTUAL_KEY = VK_SHIFT;
const LINE_ONE: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[C], VK_HOME), (&[S], VK_END)];
const NO_SELECTION: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[C], VK_HOME)];
const ALL: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[C], VIRTUAL_KEY('A' as u16))];

fn page(dir: &Path) -> PathBuf {
    let p = dir.join("page.html");
    let html = format!(
        r#"<!doctype html><html lang="ar" dir="rtl"><head><meta charset="utf-8"><title>{TOKEN} page</title>
<style>body{{font:20px "Segoe UI";margin:30px}} #box{{border:1px solid #888;padding:8px}}</style></head><body>
<p id="p1">{LINE1}</p>
<div id="box" tabindex="0"><p id="p2" dir="ltr">{BOX_P2}</p></div>
<p><textarea id="ta" rows="3" cols="50">{AREA}</textarea></p>
<script>
document.addEventListener('keydown', e => {{
  const s = getSelection(), pick = el => {{ s.removeAllRanges(); const r = document.createRange(); r.selectNodeContents(el); s.addRange(r); }};
  if (e.key === 'F2') {{ document.activeElement.blur(); pick(p1); }}
  else if (e.key === 'F3') {{ document.activeElement.blur(); s.removeAllRanges(); }}
  else if (e.key === 'F4') {{ box.focus(); pick(p2); }}
  else if (e.key === 'F6') {{ ta.focus(); ta.setSelectionRange(0, 21); }}
  else return;
  e.preventDefault();
}});
</script></body></html>"#
    );
    std::fs::write(&p, html).unwrap();
    p
}

fn page_cases() -> Vec<Case> {
    const F2: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F2)];
    const F3: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F3)];
    const F4: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F4)];
    const F6: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F6)];
    vec![
        Case { name: "paragraph", select: Sel::Keys(F2), expect: Some("جملة مختلقة") },
        Case { name: "nothing selected", select: Sel::Keys(F3), expect: None },
        Case { name: "focused box (tabindex)", select: Sel::Keys(F4), expect: Some("tabindex") },
        Case { name: "text area", select: Sel::Keys(F6), expect: Some("نص في مربع") },
    ]
}

fn browser(name: &'static str, exe: &str, dir: &Path, extra: &[&str], url: String) -> Target {
    let profile = dir.join(format!("{name}-profile"));
    let mut args: Vec<String> = vec![format!("--user-data-dir={}", profile.display()), "--no-first-run".into(), "--no-default-browser-check".into(), "--new-window".into(), "--window-size=900,650".into()];
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push(url);
    let _ = Command::new(exe).args(&args).spawn();
    Target { name, window: TOKEN.into(), marker: profile.display().to_string(), cases: page_cases(), running: None, note: String::new() }
}

fn sample_text(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(format!("{TOKEN}-{name}.txt"));
    std::fs::write(&p, format!("{LINE1}\r\n{LINE2}\r\n")).unwrap();
    p
}

fn launch(name: &str, dir: &Path) -> Option<Target> {
    let me = std::env::current_exe().unwrap();
    let edit_cases = || {
        vec![
            Case { name: "first line", select: Sel::Keys(LINE_ONE), expect: Some("جملة مختلقة") },
            Case { name: "nothing selected", select: Sel::Keys(NO_SELECTION), expect: None },
        ]
    };
    Some(match name {
        "testwin" => {
            let _ = Command::new(&me).args(["testwin", TOKEN]).spawn();
            const F2: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F2)];
            const F3: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F3)];
            const F4: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F4)];
            const F5: &[(&[VIRTUAL_KEY], VIRTUAL_KEY)] = &[(&[], VK_F5)];
            Target {
                name: "test window",
                window: format!("{TOKEN} testwin"),
                marker: format!("testwin {TOKEN}"),
                cases: vec![
                    Case { name: "edit box, first line", select: Sel::Keys(F2), expect: Some("جملة مختلقة") },
                    Case { name: "edit box, nothing selected", select: Sel::Keys(F3), expect: None },
                    Case { name: "box without a text pattern", select: Sel::Keys(F4), expect: Some("بلا نمط") },
                    Case { name: "box without a text pattern, nothing selected", select: Sel::Keys(F5), expect: None },
                ],
                running: None,
                note: String::new(),
            }
        }
        "chrome" => browser("chrome", r"C:\Program Files\Google\Chrome\Application\chrome.exe", dir, &[], url(&page(dir))),
        "edge" => browser("edge", r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", dir, &[], url(&page(dir))),
        "chrome-pdf" | "edge-pdf" => {
            let pdf = dir.join(format!("{TOKEN}-sample.pdf"));
            if !pdf.is_file() {
                return None;
            }
            let (n, exe) = if name == "chrome-pdf" { ("chrome-pdf", r"C:\Program Files\Google\Chrome\Application\chrome.exe") } else { ("edge-pdf", r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe") };
            let mut t = browser(n, exe, dir, &[], url(&pdf));
            t.cases = vec![Case { name: "PDF, all selected (Ctrl+A)", select: Sel::Keys(ALL), expect: Some("مختلقة") }];
            t
        }
        "firefox" => {
            let profile = dir.join("firefox-profile");
            std::fs::create_dir_all(&profile).unwrap();
            std::fs::write(
                profile.join("user.js"),
                [
                    r#"user_pref("browser.shell.checkDefaultBrowser", false);"#,
                    r#"user_pref("browser.startup.homepage_override.mstone", "ignore");"#,
                    r#"user_pref("browser.aboutwelcome.enabled", false);"#,
                    r#"user_pref("datareporting.policy.dataSubmissionPolicyBypassNotification", true);"#,
                    r#"user_pref("datareporting.healthreport.uploadEnabled", false);"#,
                    r#"user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);"#,
                    r#"user_pref("app.update.auto", false);"#,
                    r#"user_pref("browser.startup.page", 0);"#,
                ]
                .join("\n"),
            )
            .unwrap();
            let _ = Command::new(r"C:\Program Files\Mozilla Firefox\firefox.exe").args(["-no-remote", "-new-instance", "-profile"]).arg(&profile).arg(url(&page(dir))).spawn();
            Target { name: "firefox", window: TOKEN.into(), marker: profile.display().to_string(), cases: page_cases(), running: None, note: String::new() }
        }
        "word" => {
            let doc = dir.join(format!("{TOKEN}-word.docx"));
            let script = format!(
                "$w = New-Object -ComObject Word.Application; $d = $w.Documents.Add(); $d.Content.Text = \"{LINE1}`r{LINE2}\"; $d.Paragraphs(1).ReadingOrder = 0; $d.SaveAs2('{}'); $w.Visible = $true; $d.Activate()",
                doc.display()
            );
            ps(&script);
            // Word may hold the user's own documents too: only the trial's window is closed
            Target { name: "word", window: TOKEN.into(), marker: String::new(), cases: edit_cases(), running: None, note: String::new() }
        }
        "vscode" | "antigravity" => {
            let exe = if name == "vscode" { format!(r"{}\Programs\Microsoft VS Code\Code.exe", std::env::var("LOCALAPPDATA").unwrap()) } else { format!(r"{}\Programs\antigravity\Antigravity.exe", std::env::var("LOCALAPPDATA").unwrap()) };
            let data = dir.join(format!("{name}-data"));
            let file = sample_text(dir, name);
            let _ = Command::new(exe)
                .arg("--new-window")
                .arg("--user-data-dir")
                .arg(&data)
                .arg("--extensions-dir")
                .arg(dir.join(format!("{name}-ext")))
                .args(["--disable-extensions", "--disable-workspace-trust", "--skip-welcome", "--skip-release-notes"])
                .arg(&file)
                .spawn();
            Target { name: if name == "vscode" { "vscode" } else { "antigravity" }, window: TOKEN.into(), marker: data.display().to_string(), cases: edit_cases(), running: None, note: String::new() }
        }
        "wt" | "conhost" => {
            let running = format!("{TOKEN}-RUNNING-{name}");
            let script = format!("$Host.UI.RawUI.WindowTitle = '{TOKEN} {name}'; Write-Host '{LINE1}'; Write-Host '{LINE2}'; $marker = '{running}'; while ($true) {{ Start-Sleep -Milliseconds 300 }}");
            if name == "wt" {
                let _ = Command::new("wt.exe").args(["-w", "new", "new-tab", "--title", &format!("{TOKEN} wt"), "--suppressApplicationTitle", "powershell", "-NoProfile", "-NoExit", "-Command", &script]).spawn();
            } else {
                let _ = Command::new("conhost.exe").args(["powershell", "-NoProfile", "-NoExit", "-Command", &script]).spawn();
            }
            Target {
                name: if name == "wt" { "windows terminal" } else { "console (conhost)" },
                window: format!("{TOKEN} {name}"),
                marker: running.clone(),
                cases: vec![
                    Case { name: "a line selected", select: Sel::Find("The second line"), expect: Some("second line") },
                    Case { name: "nothing selected, a program running", select: Sel::None, expect: None },
                ],
                running: Some(running),
                note: String::new(),
            }
        }
        _ => return None,
    })
}

fn url(p: &Path) -> String {
    format!("file:///{}", p.display().to_string().replace('\\', "/"))
}

/// Select text through UI Automation in the trial's own window (terminals have no keys for it).
fn uia_select(hwnd: HWND, needle: &str) -> bool {
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Accessibility::*;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(uia) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) else { return false };
        let Ok(root) = uia.ElementFromHandle(hwnd) else { return false };
        let Ok(walker) = uia.RawViewWalker() else { return false };
        // depth first, a few hundred elements at most
        let mut stack = vec![root];
        let mut seen = 0;
        while let Some(e) = stack.pop() {
            seen += 1;
            if seen > 400 {
                break;
            }
            if let Ok(tp) = e.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                if let Ok(doc) = tp.DocumentRange() {
                    let n: Vec<u16> = needle.encode_utf16().collect();
                    if let Ok(found) = doc.FindText(&windows::core::BSTR::from_wide(&n), false, false) {
                        if found.Select().is_ok() {
                            return true;
                        }
                    }
                }
            }
            if let Ok(mut c) = walker.GetFirstChildElement(&e) {
                loop {
                    stack.push(c.clone());
                    match walker.GetNextSiblingElement(&c) {
                        Ok(n) => c = n,
                        Err(_) => break,
                    }
                }
            }
        }
        false
    }
}

fn describe(o: &Outcome) -> String {
    match o {
        Outcome::Text { text, by } => format!("text by {}: «{}»", if *by == Method::Uia { "UI Automation" } else { "Ctrl+C" }, short(text)),
        other => format!("{other:?}"),
    }
}

fn short(t: &str) -> String {
    let one: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 60 {
        format!("{}…", one.chars().take(60).collect::<String>())
    } else {
        one
    }
}

/// Every case of every target, three ways: as the app does it, with the Ctrl+C way forced, and
/// (where there is no text pattern) with the Ctrl+C way turned off.
fn scenarios(dir: &Path, names: &[String], mode: &Arc<Mutex<Mode>>, rx: &mpsc::Receiver<Pressed>) -> String {
    let mut rows = vec!["| Program | Case | Way | Result | ms | Clipboard after |".to_string(), "| --- | --- | --- | --- | --- | --- |".to_string()];
    let theirs = Clipboard::save();
    let user_window = unsafe { GetForegroundWindow() };
    let mut own_sequence = sequence();
    let mut own_input = tick();
    let mut aborted = None;
    'targets: for name in names {
        let Some(mut target) = launch(name, dir) else {
            rows.push(format!("| {name} | – | – | not launched | | |"));
            continue;
        };
        let Some(hwnd) = find_window(&target.window, Duration::from_secs(40)) else {
            rows.push(format!("| {} | – | – | its window did not appear | | |", target.name));
            kill_with(&target.marker);
            continue;
        };
        std::thread::sleep(Duration::from_millis(2500));
        target.note = window_text(hwnd);
        for case in &target.cases {
            for way in ["app", "Ctrl+C forced", "Ctrl+C off"] {
                if way == "Ctrl+C off" && case.expect.is_none() {
                    continue;
                }
                if !wait_idle(own_input) {
                    rows.push(format!("| {} | {} | {way} | skipped: the PC was in use | | |", target.name, case.name));
                    continue;
                }
                // the user copied something during the trial: theirs is the newer, stop here
                if sequence() != own_sequence {
                    aborted = Some("the clipboard changed outside the trial: stopped, and the user's new clipboard kept");
                    break 'targets;
                }
                if !bring_front(hwnd) {
                    rows.push(format!("| {} | {} | {way} | could not bring its window to the front | | |", target.name, case.name));
                    continue;
                }
                let mut keys = Keys { target: hwnd, sent: own_input };
                let selected = match case.select {
                    Sel::Keys(chords) => chords.iter().all(|(m, k)| keys.chord(m, *k)),
                    Sel::Find(needle) => uia_select(hwnd, needle),
                    Sel::None => {
                        // move the selection away: click-free, the terminal's own Escape
                        keys.key(VK_ESCAPE)
                    }
                };
                if !selected {
                    rows.push(format!("| {} | {} | {way} | could not set up the selection | | |", target.name, case.name));
                    continue;
                }
                std::thread::sleep(Duration::from_millis(300));
                let Some(before) = made_up_clipboard() else {
                    rows.push(format!("| {} | {} | {way} | the clipboard was busy | | |", target.name, case.name));
                    continue;
                };
                *mode.lock().unwrap() = Mode { force_copy: way != "app", copy: way != "Ctrl+C off" };
                while rx.try_recv().is_ok() {}
                if !keys.shortcut() {
                    rows.push(format!("| {} | {} | {way} | the window lost the front | | |", target.name, case.name));
                    continue;
                }
                let pressed = rx.recv_timeout(Duration::from_secs(6));
                std::thread::sleep(Duration::from_millis(150));
                let after = Clipboard::save();
                own_sequence = sequence();
                own_input = keys.sent;
                let clip = match (&after, &pressed) {
                    (Some(a), _) if a.sequence == before.sequence => "untouched".to_string(),
                    (Some(a), _) if a.essentials() == before.essentials() => "put back, every format".to_string(),
                    (Some(a), _) => format!("DIFFERENT ({} formats, was {})", a.essentials().len(), before.essentials().len()),
                    (None, _) => "busy".to_string(),
                };
                let (result, ms) = match pressed {
                    Ok(p) if p.ignored => ("ignored: not the trial's window".to_string(), p.ms),
                    Ok(p) => {
                        let mut r = describe(&p.outcome);
                        let good = match (&p.outcome, case.expect) {
                            (Outcome::Text { text, .. }, Some(e)) => text.contains(e),
                            (Outcome::NothingSelected, None) => true,
                            (Outcome::Terminal, None) => true,
                            (Outcome::CopyOff, Some(_)) => way == "Ctrl+C off",
                            (Outcome::Terminal, Some(_)) => way != "app",
                            _ => false,
                        };
                        r = format!("{} {r}", if good { "✓" } else { "✗" });
                        if let Some(a) = p.asked {
                            if way == "app" {
                                r.push_str(&format!(" (UIA: {}{})", match a.uia {
                                    Uia::Selected(_) => "selection",
                                    Uia::Empty => "empty selection",
                                    Uia::NoPattern => "no text pattern",
                                    Uia::Failed(ref e) => e.as_str(),
                                }, a.classes.first().map(|c| format!(", focus class `{c}`")).unwrap_or_default()));
                            }
                        }
                        (r, p.ms)
                    }
                    Err(_) => ("✗ the shortcut gave no answer".to_string(), 0),
                };
                let mut result = result;
                if let Some(running) = &target.running {
                    let alive = running_with(running) > 0;
                    result.push_str(if alive { "; its program still runs" } else { "; ITS PROGRAM WAS STOPPED" });
                }
                rows.push(format!("| {} | {} | {way} | {result} | {ms} | {clip} |", target.name, case.name));
                println!("{}", rows.last().unwrap());
            }
        }
        // close what this target opened
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        std::thread::sleep(Duration::from_millis(1500));
        kill_with(&target.marker);
        if !user_window.is_invalid() {
            bring_front(user_window);
        }
        own_input = tick();
    }
    // the user's clipboard, as it was
    let back = match (&theirs, aborted) {
        (_, Some(why)) => why.to_string(),
        (Some(c), None) => {
            let owner_ok = restore_theirs(c);
            let same = Clipboard::save().map(|n| n.essentials() == c.essentials()).unwrap_or(false);
            format!("the user's clipboard put back: {} ({} formats, compared byte for byte: {})", if owner_ok { "yes" } else { "FAILED" }, c.essentials().len(), if same { "the same" } else { "DIFFERENT" })
        }
        (None, None) => "the user's clipboard could not be read at the start, so it was not touched".into(),
    };
    if !user_window.is_invalid() {
        bring_front(user_window);
    }
    format!("{}\n\n{back}\n", rows.join("\n"))
}

fn restore_theirs(c: &Clipboard) -> bool {
    use windows::core::w;
    unsafe {
        let owner = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None).unwrap_or_default();
        let ok = c.restore(owner);
        let _ = DestroyWindow(owner);
        ok
    }
}

/// By hand: the user selects text in a program and presses the shortcut. Printed: the program,
/// the way that worked, how many characters, how long, and whether the clipboard came back. Never
/// the text itself.
fn watch() {
    let mods = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    if unsafe { RegisterHotKey(None, HOTKEY_ID, mods, KEY as u32) }.is_err() {
        println!("Ctrl+Alt+{} is taken by another program", KEY as u8 as char);
        return;
    }
    println!("Select text in a program, press Ctrl+Alt+{}. Close this window to stop.", KEY as u8 as char);
    println!("| Program | Window class | Result | Way | Characters | ms | Clipboard after | Focus class |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if msg.message != WM_HOTKEY {
            continue;
        }
        let start = Instant::now();
        let before = Clipboard::save();
        let mut sys = System::new(KEY);
        let front = sys.front();
        // the same decision the app makes, with what UI Automation said kept for the report
        struct Seen<'a> {
            sys: &'a mut System,
            front: Option<Front>,
            asked: Option<Asked>,
        }
        impl Probe for Seen<'_> {
            fn front(&mut self) -> Option<Front> {
                self.front.clone()
            }
            fn ask_uia(&mut self) -> Asked {
                let a = self.sys.ask_uia();
                self.asked = Some(a.clone());
                a
            }
            fn copy(&mut self) -> Copied {
                self.sys.copy()
            }
        }
        let mut seen = Seen { sys: &mut sys, front: front.clone(), asked: None };
        let outcome = capture(&mut seen, Options { copy: true });
        let ms = start.elapsed().as_millis();
        std::thread::sleep(Duration::from_millis(150));
        let after = Clipboard::save();
        let clip = match (&before, &after) {
            (Some(b), Some(a)) if a.sequence == b.sequence => "untouched",
            (Some(b), Some(a)) if a.essentials() == b.essentials() => "put back",
            (Some(_), Some(_)) => "DIFFERENT",
            _ => "busy",
        };
        let (result, way, chars) = match &outcome {
            Outcome::Text { text, by } => ("text", if *by == Method::Uia { "UI Automation" } else { "Ctrl+C" }, text.chars().count()),
            Outcome::NothingSelected => ("nothing selected", "", 0),
            Outcome::Administrator => ("administrator window", "", 0),
            Outcome::Terminal => ("terminal, no Ctrl+C", "", 0),
            Outcome::CopyOff => ("Ctrl+C off", "", 0),
            Outcome::KeysHeld => ("keys held", "", 0),
            Outcome::Failed(_) => ("failed", "", 0),
        };
        let f = front.unwrap_or_default();
        let focus = seen.asked.as_ref().and_then(|a| a.classes.first().cloned()).unwrap_or_default();
        let uia = match seen.asked.as_ref().map(|a| &a.uia) {
            Some(Uia::Selected(_)) => "UIA selection",
            Some(Uia::Empty) => "UIA empty",
            Some(Uia::NoPattern) => "UIA no pattern",
            Some(Uia::Failed(_)) => "UIA failed",
            None => "UIA not asked",
        };
        println!("| {} | {} | {result} ({uia}) | {way} | {chars} | {ms} | {clip} | {focus} |", f.process, f.class);
    }
}
