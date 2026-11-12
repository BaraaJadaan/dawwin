//! The commands of what came with 1.4: who said what, words and names, how subtitles name the
//! speaker. Kept apart from `lib.rs`, which has the others.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::history;
use crate::look::Look;
use crate::queue::{Core, Snapshot};
use crate::transcript::Transcript;
use crate::words::{self, Entry};

type C<'a> = State<'a, Arc<Core>>;
type R<T> = Result<T, String>;

fn e(err: anyhow::Error) -> String {
    format!("{err:#}")
}

/// Change a transcript that is not being written, and give it back as it is now.
fn edit(core: &Core, id: &str, f: impl FnOnce(&mut Transcript)) -> R<Transcript> {
    if core.is_live(id) {
        return Err(crate::tr!("This transcript is still being written. Edit it when it is done."));
    }
    history::modify(&core.paths.history_dir(), id, f).map(|(t, _)| t).map_err(e)
}

// ---------------------------------------------------------------- speakers

/// "Find the speakers" on a finished transcript: a job in the queue.
#[tauri::command]
pub fn find_speakers(core: C, id: String) -> R<String> {
    core.find_speakers(&id).map_err(e)
}

#[tauri::command]
pub fn rename_speaker(core: C, id: String, speaker: u32, name: String) -> R<Transcript> {
    edit(&core, &id, |t| crate::speakers::rename(t, speaker, &name))
}

/// One person taken for two: `from`'s lines become `into`'s.
#[tauri::command]
pub fn merge_speakers(core: C, id: String, from: u32, into: u32) -> R<Transcript> {
    edit(&core, &id, |t| crate::speakers::merge(t, from, into))
}

/// Give line `index` to `speaker`, or to someone new when there is none.
#[tauri::command]
pub fn give_line(core: C, id: String, index: usize, speaker: Option<u32>) -> R<Transcript> {
    edit(&core, &id, |t| {
        crate::speakers::give_line(t, index, speaker);
    })
}

// ---------------------------------------------------------------- words and names

/// What the settings hold besides what the state shows: the words, and the names in subtitles.
#[derive(serde::Serialize)]
pub struct Extras {
    words: Vec<Entry>,
    names_in_srt: bool,
    names_in_vtt: bool,
    /// How many speakers the model tells apart
    max_speakers: usize,
    look: Look,
}

#[tauri::command]
pub fn get_extras(core: C) -> Extras {
    let st = core.settings.lock().unwrap();
    Extras { words: st.words.clone(), names_in_srt: st.names_in_srt, names_in_vtt: st.names_in_vtt.unwrap_or(true), max_speakers: crate::speakers::MAX_SPEAKERS, look: st.look.clone() }
}

/// The whole list, as the settings page edits it.
#[tauri::command]
pub fn set_words(core: C, words: Vec<Entry>) -> R<Extras> {
    let words: Vec<Entry> = words
        .into_iter()
        .map(|w| Entry { write: w.write.trim().to_string(), heard: w.heard.into_iter().map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect() })
        .filter(|w| !w.write.is_empty())
        .collect();
    core.change_settings(|s| s.words = words).map_err(e)?;
    Ok(get_extras(core))
}

/// "Always write it like this…": `heard` (the words chosen in a transcript) is to be written as
/// `write`, in this transcript now and in every new one. The transcript as it is now.
#[tauri::command]
pub fn always_write(core: C, id: String, heard: String, write: String) -> R<Transcript> {
    core.change_settings(|s| words::add(&mut s.words, &write, &heard)).map_err(e)?;
    apply_words(core, id)
}

/// "Apply my words" on a transcript: every line written with the list.
#[tauri::command]
pub fn apply_words(core: C, id: String) -> R<Transcript> {
    let list = core.settings.lock().unwrap().words.clone();
    edit(&core, &id, |t| {
        for s in t.segments.iter_mut() {
            let text = words::apply(&s.text, &list);
            if text != s.text {
                s.text = text;
                s.words.clear(); // they no longer fit the text
            }
        }
    })
}

/// Save the list to a text file, to keep or to take to another PC.
#[tauri::command]
pub async fn save_words(app: AppHandle) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let text = words::to_text(&core.settings.lock().unwrap().words);
    let Some(dest) = app
        .dialog()
        .file()
        .set_title(crate::tr!("Save the words and names"))
        .set_file_name(format!("{}.txt", crate::tr!("Dawwin words")))
        .add_filter(crate::tr!("Text"), &["txt"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = dest.into_path().map_err(|x| x.to_string())?;
    std::fs::write(&dest, format!("\u{feff}{text}")).map_err(|x| x.to_string())?;
    Ok(Some(dest.display().to_string()))
}

/// Load a list saved before: its words join the ones here.
#[tauri::command]
pub async fn load_words(app: AppHandle) -> R<Option<Extras>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let Some(file) = app.dialog().file().set_title(crate::tr!("Load words and names")).add_filter(crate::tr!("Text"), &["txt"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|x| x.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|_| crate::tr!("That file could not be read."))?;
    let loaded = words::from_text(&text);
    core.change_settings(|s| {
        for w in &loaded {
            if w.heard.is_empty() {
                words::add(&mut s.words, &w.write, "");
            }
            for h in &w.heard {
                words::add(&mut s.words, &w.write, h);
            }
        }
    })
    .map_err(e)?;
    Ok(Some(get_extras(app.state::<Arc<Core>>())))
}

// ---------------------------------------------------------------- names in subtitles

#[tauri::command]
pub fn set_subtitle_names(core: C, srt: bool, vtt: bool) -> R<Snapshot> {
    core.change_settings(|s| {
        s.names_in_srt = srt;
        s.names_in_vtt = Some(vtt);
    })
    .map_err(e)?;
    Ok(core.snapshot())
}

// ---------------------------------------------------------------- subtitle look

#[tauri::command]
pub fn set_look(core: C, look: Look) -> R<Extras> {
    core.change_settings(|s| s.look = look).map_err(e)?;
    Ok(get_extras(core))
}

/// One second of a video of the user's with a line of its subtitles drawn in `look`, burned by
/// Dawwin's own ffmpeg: the file, for the page to show. The video is the transcript's `id`, or
/// the newest transcript of a video that is still there.
#[tauri::command]
pub async fn preview_look(app: AppHandle, id: Option<String>, look: Look) -> R<String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let ffmpeg = crate::setup::burner(&core.paths).ok_or_else(|| crate::tr!("Burning subtitles in needs the video ffmpeg. Download it in Settings."))?;
    let is_there = |p: &str| crate::video::is_video(std::path::Path::new(p)) && std::path::Path::new(p).is_file();
    let id = id
        .filter(|i| core.transcript(i).is_ok_and(|t| is_there(&t.source_path)))
        .or_else(|| history::list(&core.paths.history_dir()).into_iter().filter(|h| is_there(&h.source_path)).max_by_key(|h| h.created_ms).map(|h| h.id))
        .ok_or_else(|| crate::tr!("The preview shows a video of yours: transcribe one first."))?;
    let t = core.transcript(&id).map_err(e)?;
    // the first subtitle of the transcript, at its own moment
    let cue = crate::captions::cues(&t).into_iter().find(|c| !c.text().trim().is_empty());
    let (at, text) = match cue {
        Some(c) => (c.start, c.text()),
        None => ((t.duration_s / 2.0).min(5.0), t.title.clone()),
    };
    let srt = format!("1
00:00:00,000 --> 00:00:01,000
{text}

");
    let scratch = core.paths.cache_dir();
    // a new name each time, so the page does not show the one before
    for old in std::fs::read_dir(&scratch).into_iter().flatten().flatten() {
        if old.file_name().to_string_lossy().starts_with("look-preview-") {
            let _ = std::fs::remove_file(old.path());
        }
    }
    let dest = scratch.join(format!("look-preview-{}.mp4", crate::queue::now_ms()));
    let video = std::path::PathBuf::from(&t.source_path);
    let out = dest.clone();
    tauri::async_runtime::spawn_blocking(move || crate::video::burn_clip(&ffmpeg, &video, &srt, at, &look, &out, &scratch))
        .await
        .map_err(|x| x.to_string())?
        .map_err(e)?;
    app.asset_protocol_scope().allow_file(&dest).map_err(|x| x.to_string())?;
    Ok(dest.display().to_string())
}

/// The fonts subtitles can be burned in on this PC (see `fonts`).
#[tauri::command]
pub async fn subtitle_fonts() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(crate::fonts::for_subtitles).await.unwrap_or_default()
}
