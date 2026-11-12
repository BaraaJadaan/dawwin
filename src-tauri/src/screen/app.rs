//! In the app: the shortcut's presses, the icon in the notification area, closing the window to
//! it, the small player, and the commands Settings uses.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window};

use super::shortcut::{Keys, Registered, DEFAULT, OTHERS};
use super::{autostart, pressed, say::Say, Note, Reader, Settings};
use crate::queue::Core;

const TRAY: &str = "dawwin-tray";
const PLAYER: &str = "player";

/// Held by the app.
pub struct Screen {
    pub reader: Arc<dyn Reader>,
    #[cfg(windows)]
    hotkey: Option<super::shortcut::Hotkey>,
    registered: Mutex<Registered>,
    /// What the small player shows: a player opened just now asks for it once its page is loaded
    last_note: Mutex<Option<serde_json::Value>>,
}

/// The seam filled with 1.7.0's reader: a selection opens as a document nobody keeps (its place is
/// never remembered), the small player reads it sentence by sentence in the voices chosen in Read
/// aloud, and once it stops the document and the sound made of it are forgotten.
pub struct VoiceReader {
    app: AppHandle,
    doc: Mutex<Option<String>>,
}

impl VoiceReader {
    fn reading_of(&self) -> Arc<crate::reading::Reader> {
        self.app.state::<Arc<crate::reading::Reader>>().inner().clone()
    }
    fn control(&self, action: &str) {
        if let Some(w) = self.app.get_webview_window(PLAYER) {
            let _ = w.emit("player-control", action);
        }
    }
}

impl Reader for VoiceReader {
    fn read(&self, text: String) -> anyhow::Result<()> {
        use crate::reading::formats;
        self.stop();
        let r = self.reading_of();
        r.stop();
        let blocks = if formats::looks_like_markdown(&text) { formats::from_markdown(&text) } else { formats::from_text(&text) };
        let doc = r.open(format!("selection:{}", crate::queue::now_ms()), crate::tr!("The selection"), blocks, Vec::new())?;
        *self.doc.lock().unwrap() = Some(doc.id.clone());
        // Read aloud in the window stops: one voice at a time
        let _ = self.app.emit_to("main", "reading-yield", ());
        let view = serde_json::to_value(r.view(&doc))?;
        show_player_with(&self.app, Note::Reading, Some(view));
        Ok(())
    }
    fn stop(&self) {
        if let Some(id) = self.doc.lock().unwrap().take() {
            self.control("stop");
            self.reading_of().forget(&id);
        }
    }
    fn pause(&self, paused: bool) {
        self.control(if paused { "pause" } else { "resume" });
    }
    fn skip(&self, sentences: i32) {
        self.control(if sentences < 0 { "back" } else { "forward" });
    }
    fn reading(&self) -> bool {
        self.doc.lock().unwrap().is_some()
    }
}

/// The reader of this part: the stand-in when a test asks for it, else 1.7.0's voices.
fn make_reader(app: &AppHandle) -> Arc<dyn Reader> {
    #[cfg(debug_assertions)]
    if std::env::var_os("DAWWIN_TEST_READER").is_some() {
        return Arc::new(super::StandIn::default());
    }
    Arc::new(VoiceReader { app: app.clone(), doc: Mutex::new(None) })
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Arc<Core>>().settings.lock().unwrap().screen.clone()
}

/// Start this part. `hidden`: Windows started Dawwin at sign-in.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    #[cfg(windows)]
    let hotkey = super::shortcut::Hotkey::start(move || {
        let app = handle.clone();
        // the capture waits for the keys to be let go: not on the shortcut's thread
        std::thread::spawn(move || on_press(&app));
    });
    #[cfg(not(windows))]
    let _ = handle;
    app.manage(Screen {
        reader: make_reader(app),
        #[cfg(windows)]
        hotkey,
        registered: Mutex::new(Registered::Off),
        last_note: Mutex::new(None),
    });
    apply(app);
    // Start with Windows follows this copy of Dawwin, wherever it is now
    let s = settings(app);
    if let Ok(exe) = std::env::current_exe() {
        let want = s.on && s.start_with_windows;
        if want != autostart::is_on(&exe) {
            let _ = autostart::set(want, &exe);
        }
    }
}

/// Whether Dawwin should start without its window: started by Windows, with the shortcut on.
pub fn start_hidden(app: &AppHandle) -> bool {
    std::env::args().any(|a| a == autostart::HIDDEN) && settings(app).on
}

/// Register the shortcut and show the icon as the settings say.
fn apply(app: &AppHandle) {
    let s = settings(app);
    let screen = app.state::<Screen>();
    let reg = if !s.on {
        set_keys(&screen, None);
        Registered::Off
    } else {
        match Keys::parse(&s.shortcut) {
            None => {
                set_keys(&screen, None);
                Registered::Invalid { keys: s.shortcut.clone() }
            }
            Some(k) if set_keys(&screen, Some(k)) => Registered::On { keys: k.to_string() },
            Some(k) => Registered::Taken { keys: k.to_string() },
        }
    };
    *screen.registered.lock().unwrap() = reg;
    tray(app, s.on);
}

fn set_keys(screen: &Screen, keys: Option<Keys>) -> bool {
    #[cfg(windows)]
    return screen.hotkey.as_ref().is_some_and(|h| h.set(keys));
    #[cfg(not(windows))]
    {
        let _ = (screen, keys);
        false
    }
}

fn free(screen: &Screen, keys: Keys) -> bool {
    #[cfg(windows)]
    return screen.hotkey.as_ref().is_some_and(|h| h.free(keys));
    #[cfg(not(windows))]
    {
        let _ = (screen, keys);
        false
    }
}

fn on_press(app: &AppHandle) {
    let s = settings(app);
    let screen = app.state::<Screen>();
    let key = Keys::parse(&s.shortcut).map_or(0, |k| k.key);
    let note = pressed(&*screen.reader, &s, |copy| capture(key, copy));
    // a reading shows itself in the player as it starts
    if note != Note::Reading || !screen.reader.reading() {
        show_player(app, note);
    }
}

#[cfg(windows)]
fn capture(key: u16, copy: bool) -> crate::selection::Outcome {
    let mut system = crate::selection::System::new(key);
    crate::selection::capture(&mut system, crate::selection::Options { copy })
}

#[cfg(not(windows))]
fn capture(_key: u16, _copy: bool) -> crate::selection::Outcome {
    crate::selection::Outcome::NothingSelected
}

/// What the small player shows: a note, and the document when it is to read one.
#[derive(Clone, Serialize)]
pub struct PlayerNote {
    note: Note,
    message: String,
    reading: bool,
    dir: &'static str,
    doc: Option<serde_json::Value>,
}

fn show_player(app: &AppHandle, note: Note) {
    show_player_with(app, note, None);
}

/// The small player: on top, without a taskbar entry, and never taking the keyboard from the
/// program the user is in.
fn show_player_with(app: &AppHandle, note: Note, doc: Option<serde_json::Value>) {
    let reading = doc.is_some() || app.try_state::<Screen>().is_some_and(|s| s.reader.reading());
    let payload = PlayerNote { note, message: note.message(), reading, dir: if crate::i18n::lang() == "ar" { "rtl" } else { "ltr" }, doc };
    if let Some(s) = app.try_state::<Screen>() {
        *s.last_note.lock().unwrap() = serde_json::to_value(&payload).ok();
    }
    let window = match app.get_webview_window(PLAYER) {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(app, PLAYER, WebviewUrl::App("player.html".into()))
                .title("Dawwin")
                .inner_size(420.0, 64.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .focusable(false)
                .focused(false)
                .visible(false)
                .build();
            match built {
                Ok(w) => w,
                Err(_) => return,
            }
        }
    };
    // bottom right of the screen the user is on, above the taskbar
    if let Ok(Some(m)) = window.current_monitor().or_else(|_| window.primary_monitor()) {
        let area = m.work_area();
        let scale = m.scale_factor();
        let (w, h) = ((420.0 * scale) as i32, (64.0 * scale) as i32);
        let margin = (16.0 * scale) as i32;
        let _ = window.set_position(tauri::PhysicalPosition::new(area.position.x + area.size.width as i32 - w - margin, area.position.y + area.size.height as i32 - h - margin));
    }
    let _ = window.show();
    let _ = window.emit("player-note", payload);
}

/// The icon in the notification area, while the shortcut is on.
fn tray(app: &AppHandle, on: bool) {
    if !on {
        let _ = app.remove_tray_by_id(TRAY);
        return;
    }
    if app.tray_by_id(TRAY).is_some() {
        return;
    }
    let menu = (|| {
        let open = MenuItemBuilder::with_id("open", crate::tr!("Open Dawwin")).build(app)?;
        let stop = MenuItemBuilder::with_id("stop", crate::tr!("Stop reading")).build(app)?;
        let quit = MenuItemBuilder::with_id("quit", crate::tr!("Quit Dawwin")).build(app)?;
        MenuBuilder::new(app).item(&open).item(&stop).separator().item(&quit).build()
    })();
    let Ok(menu) = menu else { return };
    let mut b = TrayIconBuilder::with_id(TRAY).tooltip(crate::tr!("Dawwin")).menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    let _ = b
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "stop" => app.state::<Screen>().reader.stop(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app);
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Closing the main window while the shortcut is on: to the notification area (a recording goes
/// on). True when that is what happened.
pub fn close_to_tray(window: &Window, api: &tauri::CloseRequestApi) -> bool {
    if window.label() != "main" || !settings(window.app_handle()).on {
        return false;
    }
    api.prevent_close();
    let _ = window.hide();
    true
}

/// Otherwise Dawwin quits as before, unless a recording is asking first: the small player, hidden,
/// must not keep it running.
pub fn quit_on_close(window: &Window) {
    if window.label() == "main" && !crate::queue::meeting::is_recording() {
        window.app_handle().exit(0);
    }
}

// ---- commands ----

#[derive(Serialize)]
pub struct ScreenState {
    settings: Settings,
    shortcut: Registered,
    /// Shortcuts no other program holds, offered when the chosen one is taken
    free: Vec<String>,
    /// What went wrong with the last change, in the interface's language
    error: Option<String>,
}

fn state(app: &AppHandle, error: Option<String>) -> ScreenState {
    let screen = app.state::<Screen>();
    let shortcut = screen.registered.lock().unwrap().clone();
    let free = match &shortcut {
        Registered::Taken { keys } | Registered::Invalid { keys } => std::iter::once(DEFAULT)
            .chain(OTHERS.iter().copied())
            .filter(|k| k != keys)
            .filter_map(Keys::parse)
            .filter(|&k| free(&screen, k))
            .map(|k| k.to_string())
            .take(4)
            .collect(),
        _ => vec![],
    };
    let error = error.or_else(|| match &shortcut {
        Registered::Taken { keys } => Some(crate::tr!("{} is used by another program. Choose other keys.", keys)),
        Registered::Invalid { .. } => Some(crate::tr!("That is not a shortcut Dawwin can use. Use Ctrl, Alt or Win with a letter, a number or F1 to F24.")),
        _ => None,
    });
    ScreenState { settings: settings(app), shortcut, free, error }
}

fn change(app: &AppHandle, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
    app.state::<Arc<Core>>().change_settings(|s| f(&mut s.screen)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn screen_state(app: AppHandle) -> ScreenState {
    state(&app, None)
}

/// The shortcut on or off. Off also ends Start with Windows: without the shortcut there is
/// nothing for a hidden Dawwin to do.
#[tauri::command]
pub fn screen_set_on(app: AppHandle, on: bool) -> Result<ScreenState, String> {
    change(&app, |s| s.on = on)?;
    apply(&app);
    let s = settings(&app);
    if let Ok(exe) = std::env::current_exe() {
        let _ = autostart::set(on && s.start_with_windows, &exe);
    }
    Ok(state(&app, None))
}

/// New keys: kept only if they could be registered; otherwise the ones before stay.
#[tauri::command]
pub fn screen_set_shortcut(app: AppHandle, keys: String) -> Result<ScreenState, String> {
    let Some(k) = Keys::parse(&keys) else {
        return Ok(state(&app, Some(crate::tr!("That is not a shortcut Dawwin can use. Use Ctrl, Alt or Win with a letter, a number or F1 to F24."))));
    };
    let screen = app.state::<Screen>();
    if settings(&app).on && !free(&screen, k) {
        let mut st = state(&app, Some(crate::tr!("{} is used by another program. Choose other keys.", k)));
        if st.free.is_empty() {
            st.free = std::iter::once(DEFAULT).chain(OTHERS.iter().copied()).filter_map(Keys::parse).filter(|&o| o != k && free(&screen, o)).map(|o| o.to_string()).take(4).collect();
        }
        return Ok(st);
    }
    change(&app, |s| s.shortcut = k.to_string())?;
    apply(&app);
    Ok(state(&app, None))
}

#[tauri::command]
pub fn screen_set_copy(app: AppHandle, on: bool) -> Result<ScreenState, String> {
    change(&app, |s| s.copy = on)?;
    Ok(state(&app, None))
}

#[tauri::command]
pub fn screen_set_start_with_windows(app: AppHandle, on: bool) -> Result<ScreenState, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let shortcut_on = settings(&app).on;
    autostart::set(on && shortcut_on, &exe).map_err(|e| format!("{e:#}"))?;
    change(&app, |s| s.start_with_windows = on)?;
    Ok(state(&app, None))
}

#[tauri::command]
pub fn screen_set_say(app: AppHandle, list: Vec<Say>) -> Result<ScreenState, String> {
    let list: Vec<Say> = list.into_iter().filter(|s| !s.written.trim().is_empty()).collect();
    change(&app, |s| s.say = list)?;
    Ok(state(&app, None))
}

/// Test builds: show the small player with a note, as a press of the shortcut does. Async: a
/// window made on the main thread, in a command that blocks it, never loads its page.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn test_player_note(app: AppHandle, note: Note) {
    show_player(&app, note);
}

/// What the small player shows now (its page asks once it is loaded).
#[tauri::command]
pub fn player_last(app: AppHandle) -> Option<serde_json::Value> {
    app.state::<Screen>().last_note.lock().unwrap().take()
}

/// Test builds: read this text as a press of the shortcut would after taking it from another
/// program (the taking itself is the trial's and the unit tests').
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn test_screen_read(app: AppHandle, text: String) -> Note {
    tauri::async_runtime::spawn_blocking(move || {
        let s = settings(&app);
        let screen = app.state::<Screen>();
        let note = pressed(&*screen.reader, &s, |_| crate::selection::Outcome::Text { text, by: crate::selection::Method::Uia });
        if note != Note::Reading || !screen.reader.reading() {
            show_player(&app, note);
        }
        note
    })
    .await
    .unwrap_or(Note::Failed)
}

/// What Read aloud has open, saved as sound: M4A, or M4B with a chapter at each heading for a
/// book. A job in the list with progress and stop; the file is asked for first. Its job's id.
#[tauri::command]
pub async fn reading_save_sound(app: AppHandle, doc: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let r = app.state::<Arc<crate::reading::Reader>>().inner().clone();
    let d = r.doc(&doc).ok_or_else(|| crate::tr!("That text is no longer open."))?;
    let book = d.sentences.iter().filter(|s| s.kind == crate::reading::formats::Kind::Heading).count() >= 2;
    let ext = if book { "m4b" } else { "m4a" };
    let name: String = d.title.chars().map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string();
    let core = app.state::<Arc<Core>>().inner().clone();
    let mut dlg = app
        .dialog()
        .file()
        .set_title(crate::tr!("Save the reading as sound"))
        .set_file_name(format!("{}.{ext}", if name.is_empty() { "Dawwin".to_string() } else { name }))
        .add_filter(crate::tr!("Sound"), &[ext]);
    // a file's reading goes beside it, or to the folder chosen in Settings
    let beside = d.key.strip_prefix("file:").map(std::path::PathBuf::from);
    let folder = match &beside {
        Some(f) => crate::saving::dialog_folder(&core.settings.lock().unwrap(), f),
        None => crate::saving::own_folder(&core.settings.lock().unwrap()),
    };
    if let Some(dir) = folder {
        dlg = dlg.set_directory(dir);
    }
    let Some(dest) = dlg.blocking_save_file() else { return Ok(None) };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    start_saving(&app, d, dest).map(Some)
}

/// Test builds: the same, to a path given (no dialog).
#[cfg(debug_assertions)]
#[tauri::command]
pub fn reading_save_sound_to_path(app: AppHandle, doc: String, path: String) -> Result<String, String> {
    let r = app.state::<Arc<crate::reading::Reader>>().inner().clone();
    let d = r.doc(&doc).ok_or_else(|| crate::tr!("That text is no longer open."))?;
    start_saving(&app, d, path.into())
}

fn start_saving(app: &AppHandle, d: Arc<crate::reading::Doc>, dest: std::path::PathBuf) -> Result<String, String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let ffmpeg = crate::setup::ffmpeg(&core.paths).ok_or_else(|| crate::tr!("Saving as sound needs ffmpeg. Download it in Settings."))?;
    let r = app.state::<Arc<crate::reading::Reader>>().inner().clone();
    let work = core.paths.cache_dir();
    let title = d.title.clone();
    Ok(core.side_job("reading_sound", &title, &dest.display().to_string(), move |cancel, progress| super::sound::save_reading(&r, &d, &ffmpeg, &work, &dest, cancel, progress)))
}

/// A button of the small player.
#[tauri::command]
pub fn player_action(app: AppHandle, action: String) {
    let screen = app.state::<Screen>();
    match action.as_str() {
        "pause" => screen.reader.pause(true),
        "resume" => screen.reader.pause(false),
        "back" => screen.reader.skip(-1),
        "forward" => screen.reader.skip(1),
        // the reading came to its end, or failed: it is forgotten
        "ended" => screen.reader.stop(),
        "close" | "hide" => {
            if action == "close" {
                screen.reader.stop();
            }
            if let Some(w) = app.get_webview_window(PLAYER) {
                let _ = w.hide();
            }
        }
        _ => {}
    }
}

// ---- PDFs in Read aloud ----

/// Download PDFium (about 4 MB), with "pdfium-progress" events of the bytes so far.
#[tauri::command]
pub async fn screen_install_pdfium(app: AppHandle) -> Result<(), String> {
    let paths = app.state::<Arc<Core>>().paths.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let stop = std::sync::atomic::AtomicBool::new(false);
        super::files::install_pdfium(&paths, &stop, &mut |e| {
            if let crate::models::Event::Progress { done } = e {
                let _ = handle.emit("pdfium-progress", (done, crate::tools::PDFIUM.size));
            }
        })
        .map(|_| ())
        .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
