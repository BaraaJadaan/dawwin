//! The commands the Minutes panel calls.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::minutes::{self, Minutes};
use crate::queue::Core;
use crate::{e, safe_name, C, R};

/// Write the minutes, in the queue: the job's id. `language`: the transcript's own when empty.
#[tauri::command]
pub fn write_minutes(core: C, id: String, language: Option<String>) -> R<String> {
    core.write_minutes(&id, language.as_deref(), false).map_err(e)
}

/// The user's corrections: the minutes as they are now.
#[tauri::command]
pub fn edit_minutes(core: C, id: String, minutes: Minutes) -> R<Minutes> {
    core.edit_minutes(&id, minutes).map_err(e)
}

/// Whether the transcript was corrected after its minutes were written.
#[tauri::command]
pub fn minutes_outdated(core: C, id: String) -> R<bool> {
    let t = core.transcript(&id).map_err(e)?;
    // minutes that do not say what they were written from cannot tell
    Ok(t.minutes.as_ref().filter(|m| m.written() && !m.from.is_empty()).is_some_and(|m| m.from != minutes::fingerprint(&t)))
}

#[derive(serde::Serialize)]
pub struct MinutesInfo {
    /// What each minutes model needs, and the languages it writes well
    models: Vec<minutes::Spec>,
    /// This computer's memory, in bytes
    memory: u64,
}

#[tauri::command]
pub fn minutes_info() -> MinutesInfo {
    MinutesInfo { models: minutes::MODELS.to_vec(), memory: memory() }
}

/// The installed memory, from Windows.
#[cfg(windows)]
fn memory() -> u64 {
    #[repr(C)]
    struct MemoryStatus {
        length: u32,
        load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page: u64,
        avail_page: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
    }
    let mut s = MemoryStatus { length: std::mem::size_of::<MemoryStatus>() as u32, load: 0, total_phys: 0, avail_phys: 0, total_page: 0, avail_page: 0, total_virtual: 0, avail_virtual: 0, avail_extended: 0 };
    // SAFETY: a plain call with a valid, sized struct
    if unsafe { GlobalMemoryStatusEx(&mut s) } != 0 {
        s.total_phys
    } else {
        0
    }
}

#[cfg(not(windows))]
fn memory() -> u64 {
    0
}

fn minutes_of(core: &Core, id: &str) -> R<(crate::transcript::Transcript, Minutes)> {
    let t = core.transcript(id).map_err(e)?;
    let m = t.minutes.clone().filter(Minutes::written).ok_or_else(|| crate::tr!("There are no minutes of this transcript yet."))?;
    Ok((t, m))
}

/// Save the minutes, alone or followed by the transcript, as Word, Markdown or text.
#[tauri::command]
pub async fn export_minutes(app: AppHandle, id: String, format: String, with_transcript: bool) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let (t, m) = minutes_of(&core, &id)?;
    let (ext, label) = match format.as_str() {
        "txt" => ("txt", crate::tr!("Text")),
        "md" => ("md", "Markdown".to_string()),
        "docx" => ("docx", crate::tr!("Word document")),
        _ => return Err(format!("unknown format {format}")),
    };
    let name = format!("{} ({})", safe_name(&t.title), if m.language == "ar" { "محضر" } else { "minutes" });
    let mut dlg = app.dialog().file().set_title(crate::tr!("Save the minutes")).set_file_name(format!("{name}.{ext}")).add_filter(label, &[ext]);
    if let Some(dir) = PathBuf::from(&t.source_path).parent().filter(|d| d.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    let Some(dest) = dlg.blocking_save_file() else { return Ok(None) };
    let dest = dest.into_path().map_err(|x| x.to_string())?;
    std::fs::write(&dest, minutes::export(&t, &m, ext, with_transcript).map_err(e)?).map_err(|x| x.to_string())?;
    Ok(Some(dest.display().to_string()))
}

/// The same without asking where (the file name decides the format), for the tests.
#[cfg(debug_assertions)]
#[tauri::command]
pub fn export_minutes_to_path(core: C, id: String, path: String, with_transcript: bool) -> R<String> {
    let (t, m) = minutes_of(&core, &id)?;
    let ext = std::path::Path::new(&path).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    std::fs::write(&path, minutes::export(&t, &m, &ext, with_transcript).map_err(e)?).map_err(|x| x.to_string())?;
    Ok(path)
}
