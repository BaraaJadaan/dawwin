//! Dawwin: local speech-to-text, Arabic first. The commands the UI calls.

mod audio;
mod captions;
mod engine;
mod folder;
mod history;
pub mod i18n;
mod models;
mod queue;
mod setup;
mod timing;
mod tools;
mod transcript;
mod translate;
mod updates;
mod video;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::queue::{Core, Snapshot};
use crate::transcript::{Transcript, View};

type C<'a> = State<'a, Arc<Core>>;
type R<T> = Result<T, String>;

fn e(err: anyhow::Error) -> String {
    format!("{err:#}")
}

#[tauri::command]
fn get_state(core: C) -> Snapshot {
    core.snapshot()
}

#[tauri::command]
fn refresh_setup(core: C) -> Snapshot {
    core.refresh_setup();
    core.snapshot()
}

#[tauri::command]
fn download_model(core: C, id: String) {
    core.inner().download_model(&id);
}

#[tauri::command]
fn stop_download(core: C) {
    core.stop_download();
}

#[tauri::command]
fn use_model(core: C, id: String) -> R<Snapshot> {
    core.use_model(&id).map_err(e)?;
    Ok(core.snapshot())
}

#[tauri::command]
fn delete_model(core: C, id: String) -> R<Snapshot> {
    core.delete_model(&id).map_err(e)?;
    Ok(core.snapshot())
}

/// How much the Python engine of older versions left behind, in bytes.
#[tauri::command]
async fn old_engine_size(app: AppHandle) -> u64 {
    let core = app.state::<Arc<Core>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.paths.old_engine_dirs().iter().map(|d| setup::folder_size(d)).sum())
        .await
        .unwrap_or(0)
}

#[tauri::command]
async fn remove_old_engine(app: AppHandle) -> R<()> {
    let core = app.state::<Arc<Core>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        for d in core.paths.old_engine_dirs() {
            std::fs::remove_dir_all(&d).map_err(|x| format!("{}: {x}", d.display()))?;
        }
        Ok(())
    })
    .await
    .map_err(|x| x.to_string())?
}

#[tauri::command]
async fn choose_download_dir(app: AppHandle) -> R<Snapshot> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let Some(dir) = app.dialog().file().set_title(crate::tr!("Where should the model be saved?")).blocking_pick_folder() else {
        return Ok(core.snapshot());
    };
    let dir = dir.into_path().map_err(|x| x.to_string())?;
    core.set_download_dir(&dir).map_err(e)?;
    Ok(core.snapshot())
}

/// Use a model file the user already has.
#[tauri::command]
async fn import_model(app: AppHandle) -> R<Snapshot> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let Some(file) = app
        .dialog()
        .file()
        .set_title(crate::tr!("Choose the model file (.gguf)"))
        .add_filter("GGUF", &["gguf"])
        .blocking_pick_file()
    else {
        return Ok(core.snapshot());
    };
    let file = file.into_path().map_err(|x| x.to_string())?;
    core.import_model(&file).map_err(e)?;
    Ok(core.snapshot())
}

/// The same, for a file or folder dropped on the window.
#[tauri::command]
fn import_model_path(core: C, path: String) -> R<Snapshot> {
    core.import_model(std::path::Path::new(&path.replace('/', "\\"))).map_err(e)?;
    Ok(core.snapshot())
}

/// The interface language: messages from Rust and the worker, dialog titles, the window title.
#[tauri::command]
fn set_ui_language(app: AppHandle, lang: String) -> R<Snapshot> {
    let core = app.state::<Arc<Core>>();
    core.set_ui_lang(&lang).map_err(e)?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&crate::tr!("Dawwin · Speech to text"));
    }
    Ok(core.snapshot())
}

/// Ask GitHub whether a newer version is out.
#[tauri::command]
async fn check_updates() -> R<updates::UpdateInfo> {
    tauri::async_runtime::spawn_blocking(updates::check).await.map_err(|x| x.to_string())?.map_err(e)
}

#[tauri::command]
fn set_check_updates(core: C, on: bool) -> R<Snapshot> {
    core.change_settings(|s| s.check_updates = on).map_err(e)?;
    Ok(core.snapshot())
}

#[tauri::command]
fn set_run_on(core: C, value: String) -> R<Snapshot> {
    core.set_run_on(&value).map_err(e)?;
    Ok(core.snapshot())
}

/// Open a Hugging Face page in the browser.
#[tauri::command]
fn open_url(url: String) -> R<()> {
    // the two sites, and writing to the maker
    let mail = url == "mailto:braajad1@gmail.com" || url.starts_with("mailto:braajad1@gmail.com?");
    if !url.starts_with("https://huggingface.co/") && !url.starts_with("https://github.com/") && !mail {
        return Err("not an allowed link".into());
    }
    std::process::Command::new("explorer.exe").arg(&url).spawn().map_err(|x| x.to_string())?;
    Ok(())
}


#[tauri::command]
async fn pick_files(app: AppHandle) -> Vec<String> {
    app.dialog()
        .file()
        .set_title(crate::tr!("Choose recordings to transcribe"))
        .add_filter(crate::tr!("Audio and video"), folder::MEDIA_EXT)
        .add_filter(crate::tr!("All files"), &["*"])
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
        .collect()
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title(crate::tr!("Choose a folder to transcribe"))
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
}

fn spoken(language: &str) -> String {
    if models::known_language(language) { language } else { "ar" }.to_string()
}

#[derive(serde::Serialize)]
struct SpokenLanguage {
    code: &'static str,
    /// In English; the interface asks Windows for the name in its own language first
    name: &'static str,
    right_to_left: bool,
    /// The models that can transcribe it, the best first
    models: Vec<&'static str>,
}

/// The languages recordings can be in.
#[tauri::command]
fn spoken_languages() -> Vec<SpokenLanguage> {
    models::languages()
        .map(|code| SpokenLanguage {
            code,
            name: models::language_name(code),
            right_to_left: models::is_right_to_left(code),
            models: models::models_for(code).iter().map(|v| v.id).collect(),
        })
        .collect()
}

#[tauri::command]
fn enqueue(core: C, paths: Vec<String>, language: String, steps: Option<queue::Steps>) -> Vec<String> {
    core.enqueue(paths, spoken(&language), steps.unwrap_or_default())
}

#[tauri::command]
fn enqueue_folder(core: C, path: String, language: String, steps: Option<queue::Steps>) -> queue::FolderScan {
    core.enqueue_folder(std::path::Path::new(&path.replace('/', "\\")), spoken(&language), steps.unwrap_or_default())
}

/// What a dropped path is: a model to import, a folder to transcribe, or something else.
#[tauri::command]
fn path_kind(path: String) -> &'static str {
    let p = PathBuf::from(path.replace('/', "\\"));
    if p.is_dir() {
        // a folder of recordings stays a folder of recordings, even with a model in it
        if folder::recordings_in(&p).is_empty() && models::find_in_folder(&p, None).is_some() {
            "model"
        } else {
            "folder"
        }
    } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("gguf")) {
        "model"
    } else {
        "other"
    }
}

#[tauri::command]
fn set_folder_formats(core: C, formats: Vec<String>) -> R<Snapshot> {
    let formats = formats.into_iter().filter(|f| folder::FORMATS.contains(&f.as_str())).collect();
    core.change_settings(|s| s.folder_formats = Some(formats)).map_err(e)?;
    Ok(core.snapshot())
}

#[tauri::command]
fn cancel_all_jobs(core: C) {
    core.cancel_all();
}

#[tauri::command]
fn cancel_job(core: C, id: String) {
    core.cancel(&id);
}

#[tauri::command]
fn dismiss_job(core: C, id: String) {
    core.dismiss(&id);
}

#[tauri::command]
fn list_history(core: C) -> Vec<history::Summary> {
    history::list(&core.paths.history_dir())
}

#[tauri::command]
fn get_transcript(core: C, id: String) -> R<Transcript> {
    core.transcript(&id).map_err(e)
}

fn edit(core: &Core, id: &str, f: impl FnOnce(&mut Transcript)) -> R<Transcript> {
    if core.is_live(id) {
        return Err(crate::tr!("This transcript is still being written. Edit it when it is done."));
    }
    history::modify(&core.paths.history_dir(), id, f).map(|(t, _)| t).map_err(e)
}

#[tauri::command]
fn update_segment(core: C, id: String, index: usize, text: String) -> R<()> {
    edit(&core, &id, |t| {
        if let Some(s) = t.segments.get_mut(index) {
            if s.text != text {
                s.text = text;
                s.edited = true;
                // the old times no longer fit the text: matched again with what was heard, if anything
                s.words.clear();
                timing::apply(s);
            }
        }
    })?;
    Ok(())
}

/// Place the words of a transcript in time again (see `timing`): a job in the queue.
#[tauri::command]
fn fix_timing(core: C, id: String) -> R<String> {
    core.fix_timing(&id).map_err(e)
}

#[derive(serde::Serialize)]
struct TranslationTarget {
    code: &'static str,
    /// In English; the interface asks Windows for the name in its own language first
    name: &'static str,
    right_to_left: bool,
}

/// The languages a transcript can be translated into.
#[tauri::command]
fn translation_targets() -> Vec<TranslationTarget> {
    models::translation_targets()
        .map(|(code, name)| TranslationTarget { code, name, right_to_left: models::is_right_to_left(code) })
        .collect()
}

/// Translate a finished transcript, in the queue: the job's id.
#[tauri::command]
fn translate(core: C, id: String, language: String) -> R<String> {
    core.translate(&id, &language).map_err(e)
}

#[tauri::command]
fn update_translation(core: C, id: String, language: String, index: usize, text: String) -> R<()> {
    edit(&core, &id, |t| {
        let count = t.segments.len();
        let from = t.segments.get(index).map(|s| s.text.clone());
        if let (Some(tr), Some(from)) = (t.translations.iter_mut().find(|x| x.language == language), from) {
            tr.lines.resize(count, Default::default());
            let line = &mut tr.lines[index];
            if line.text != text {
                line.text = text;
                line.edited = true;
                // written by hand for the line as it is now
                line.from = from;
            }
        }
    })?;
    Ok(())
}

#[tauri::command]
fn delete_translation(core: C, id: String, language: String) -> R<()> {
    edit(&core, &id, |t| t.translations.retain(|x| x.language != language)).map(|_| ())
}

#[tauri::command]
fn rename_transcript(core: C, id: String, title: String) -> R<()> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(crate::tr!("A title cannot be empty."));
    }
    edit(&core, &id, |t| t.title = title).map(|_| ())
}

#[tauri::command]
fn delete_transcript(core: C, id: String) -> R<()> {
    history::delete(&core.paths.history_dir(), &id).map_err(e)
}

/// The text to copy: the transcript, its translation, or both (`view` "translation" | "both", into `language`).
#[tauri::command]
fn transcript_text(core: C, id: String, stamps: bool, view: Option<String>, language: Option<String>) -> R<String> {
    let t = core.transcript(&id).map_err(e)?;
    let bytes = transcript::export_view(&t, "txt", stamps, &View::parse(view.as_deref(), language.as_deref())).map_err(e)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn safe_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let n = n.trim().trim_end_matches('.').to_string();
    if n.is_empty() { "transcript".into() } else { n }
}

#[tauri::command]
async fn export_transcript(app: AppHandle, id: String, format: String, stamps: bool, view: Option<String>, language: Option<String>) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = core.transcript(&id).map_err(e)?;
    let view = View::parse(view.as_deref(), language.as_deref());
    // "Meeting (English).docx" for a translation, so it does not take the place of the transcript
    let name = match &view {
        View::Original => safe_name(&t.title),
        View::Translation(l) | View::Both(l) => format!("{} ({})", safe_name(&t.title), models::language_name(l)),
    };
    let (ext, label) = match format.as_str() {
        "txt" => ("txt", crate::tr!("Text")),
        "md" => ("md", "Markdown".to_string()),
        "srt" => ("srt", crate::tr!("SubRip subtitles")),
        "vtt" => ("vtt", crate::tr!("WebVTT subtitles")),
        "docx" => ("docx", crate::tr!("Word document")),
        "csv" => ("csv", "CSV".to_string()),
        "tsv" => ("tsv", "TSV".to_string()),
        "json" => ("json", "JSON".to_string()),
        _ => return Err(format!("unknown format {format}")),
    };
    let mut dlg = app
        .dialog()
        .file()
        .set_title(crate::tr!("Export transcript"))
        .set_file_name(format!("{name}.{ext}"))
        .add_filter(label, &[ext]);
    if let Some(dir) = PathBuf::from(&t.source_path).parent().filter(|d| d.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    let Some(dest) = dlg.blocking_save_file() else { return Ok(None) };
    let dest = dest.into_path().map_err(|x| x.to_string())?;
    let t = if matches!(ext, "srt" | "vtt") { timed(&core, &id).await? } else { t };
    write_export(&t, ext, stamps, &view, &dest)?;
    Ok(Some(dest.display().to_string()))
}

/// The transcript with its words placed in time, for subtitles: a timing job first when they are
/// not (see `Core::timed_transcript`), off the interface's thread.
async fn timed(core: &Arc<Core>, id: &str) -> R<Transcript> {
    let (core, id) = (core.clone(), id.to_string());
    tauri::async_runtime::spawn_blocking(move || core.timed_transcript(&id)).await.map_err(|x| x.to_string())?.map_err(e)
}

/// Export without asking where (the file name decides the format).
#[tauri::command]
async fn export_to_path(app: AppHandle, id: String, path: String, stamps: bool, view: Option<String>, language: Option<String>) -> R<String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let dest = PathBuf::from(&path);
    let ext = dest.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    let t = if matches!(ext.as_str(), "srt" | "vtt") { timed(&core, &id).await? } else { core.transcript(&id).map_err(e)? };
    write_export(&t, &ext, stamps, &View::parse(view.as_deref(), language.as_deref()), &dest)?;
    Ok(path)
}

fn write_export(t: &Transcript, ext: &str, stamps: bool, view: &View, dest: &std::path::Path) -> R<()> {
    let bytes = transcript::export_view(t, ext, stamps, view).map_err(e)?;
    std::fs::write(dest, bytes).map_err(|x| x.to_string())
}

/// Save the sound of a transcript's recording to `dest`, with or without its long silences. The
/// file written, whose extension may differ (a WAV when there is no ffmpeg).
fn save_sound_of(t: &Transcript, without_silences: bool, dest: &std::path::Path, ffmpeg: Option<PathBuf>) -> R<PathBuf> {
    let source = PathBuf::from(&t.source_path);
    if !source.is_file() {
        return Err(crate::tr!("The recording is not at {} any more.", source.display()));
    }
    if std::fs::canonicalize(&source).ok() == std::fs::canonicalize(dest).ok() {
        return Err(crate::tr!("Choose another name: the recording itself cannot be written over."));
    }
    let stop = std::sync::atomic::AtomicBool::new(false);
    let samples = audio::decode(&source, ffmpeg.as_ref(), &stop).map_err(e)?;
    let spans = without_silences.then(|| audio::speech_spans(&samples, 0.7, 0.3));
    if spans.as_ref().is_some_and(|s| s.is_empty()) {
        return Err(crate::tr!("There is no speech in this recording."));
    }
    // the save dialog already asked before replacing a file
    let _ = std::fs::remove_file(dest);
    audio::save_sound(&source, dest, spans.as_deref(), &samples, ffmpeg.as_ref(), &stop).map_err(e)
}

/// The recording's sound as a file of its own: from a video, or without its silences.
#[tauri::command]
async fn export_audio(app: AppHandle, id: String, without_silences: bool) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = core.transcript(&id).map_err(e)?;
    let source = PathBuf::from(&t.source_path);
    let ffmpeg = setup::ffmpeg(&core.paths);
    let ext = if ffmpeg.is_some() { "m4a" } else { "wav" };
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| safe_name(&t.title));
    let name = if without_silences { format!("{} ({}).{ext}", safe_name(&stem), crate::tr!("without silences")) } else { format!("{}.{ext}", safe_name(&stem)) };
    let mut dlg = app.dialog().file().set_title(crate::tr!("Save the sound")).set_file_name(name).add_filter(crate::tr!("Sound"), &[ext]);
    if let Some(dir) = source.parent().filter(|d| d.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    let Some(dest) = dlg.blocking_save_file() else { return Ok(None) };
    let dest = dest.into_path().map_err(|x| x.to_string())?;
    let _ = app.emit("audio-saving", dest.display().to_string());
    let saved = tauri::async_runtime::spawn_blocking(move || save_sound_of(&t, without_silences, &dest, ffmpeg)).await.map_err(|x| x.to_string())??;
    Ok(Some(saved.display().to_string()))
}

/// The same, to `path` without asking where.
#[tauri::command]
async fn export_audio_to_path(app: AppHandle, id: String, path: String, without_silences: bool) -> R<String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = core.transcript(&id).map_err(e)?;
    let ffmpeg = setup::ffmpeg(&core.paths);
    let saved = tauri::async_runtime::spawn_blocking(move || save_sound_of(&t, without_silences, std::path::Path::new(&path), ffmpeg)).await.map_err(|x| x.to_string())??;
    Ok(saved.display().to_string())
}

/// What adding subtitles to a transcript's video needs: the video, ffmpeg, and the tracks (see `video`).
fn video_job(core: &Core, t: &Transcript, view: Option<&str>, language: Option<&str>) -> R<(PathBuf, PathBuf, Vec<video::Track>)> {
    let source = PathBuf::from(&t.source_path);
    if !video::is_video(&source) {
        return Err(crate::tr!("That recording is not a video."));
    }
    if !source.is_file() {
        return Err(crate::tr!("The video is not at {} any more.", source.display()));
    }
    let ffmpeg = setup::ffmpeg(&core.paths).ok_or_else(|| crate::tr!("Adding subtitles to a video needs ffmpeg. Download it in Settings."))?;
    let tracks = video::tracks(t, &View::parse(view, language))?;
    Ok((source, ffmpeg, tracks))
}
/// The same, written to `path` without asking where.
#[tauri::command]
async fn export_video_to_path(app: AppHandle, id: String, path: String, view: Option<String>, language: Option<String>) -> R<String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = timed(&core, &id).await?;
    let (source, ffmpeg, tracks) = video_job(&core, &t, view.as_deref(), language.as_deref())?;
    let (dest, scratch) = (PathBuf::from(&path), core.paths.cache_dir());
    tauri::async_runtime::spawn_blocking(move || video::add_subtitles(&ffmpeg, &source, &tracks, &dest, &scratch))
        .await
        .map_err(|x| x.to_string())?
        .map_err(e)?;
    Ok(path)
}

#[tauri::command]
async fn export_video(app: AppHandle, id: String, view: Option<String>, language: Option<String>) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = core.transcript(&id).map_err(e)?;
    let (source, _, tracks) = video_job(&core, &t, view.as_deref(), language.as_deref())?;
    let ext = video::output_ext(&source);
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| safe_name(&t.title));
    let names = tracks.iter().map(|x| x.title.clone()).collect::<Vec<_>>().join(", ");
    let mut dlg = app
        .dialog()
        .file()
        .set_title(crate::tr!("Save the video with its subtitles"))
        .set_file_name(format!("{} ({}).{ext}", safe_name(&stem), safe_name(&names)))
        .add_filter(crate::tr!("Video"), &[ext]);
    if let Some(dir) = source.parent().filter(|d| d.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    let Some(dest) = dlg.blocking_save_file() else { return Ok(None) };
    let dest = dest.into_path().map_err(|x| x.to_string())?;
    let dest = if dest.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) { dest } else { dest.with_extension(ext) };
    let t = timed(&core, &id).await?;
    let (source, ffmpeg, tracks) = video_job(&core, &t, view.as_deref(), language.as_deref())?;
    let _ = app.emit("video-saving", dest.display().to_string());
    let scratch = core.paths.cache_dir();
    let out = dest.clone();
    tauri::async_runtime::spawn_blocking(move || video::add_subtitles(&ffmpeg, &source, &tracks, &out, &scratch))
        .await
        .map_err(|x| x.to_string())?
        .map_err(e)?;
    Ok(Some(dest.display().to_string()))
}

/// The video with its subtitles burned into the picture: a job in the list, since it takes a while.
/// The file it will be, or none when the user did not choose one.
#[tauri::command]
async fn export_burned(app: AppHandle, id: String, view: Option<String>, language: Option<String>) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = core.transcript(&id).map_err(e)?;
    let source = PathBuf::from(&t.source_path);
    if !video::is_video(&source) {
        return Err(crate::tr!("That recording is not a video."));
    }
    if setup::burner(&core.paths).is_none() {
        return Err(crate::tr!("Burning subtitles in needs the video ffmpeg. Download it in Settings."));
    }
    let view = View::parse(view.as_deref(), language.as_deref());
    let tracks = video::tracks(&t, &view)?;
    let name = video::burned_dest(&source, &tracks).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut dlg = app.dialog().file().set_title(crate::tr!("Save the video with its subtitles")).set_file_name(name).add_filter(crate::tr!("Video"), &["mp4"]);
    if let Some(dir) = source.parent().filter(|d| d.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    let Some(dest) = dlg.blocking_save_file() else { return Ok(None) };
    let dest = dest.into_path().map_err(|x| x.to_string())?.with_extension("mp4");
    timed(&core, &id).await?;
    core.burn(&id, &view, dest.clone()).map_err(e)?;
    Ok(Some(dest.display().to_string()))
}

/// The same, to `path` without asking where: the job's id.
#[tauri::command]
async fn export_burned_to_path(app: AppHandle, id: String, path: String, view: Option<String>, language: Option<String>) -> R<String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    timed(&core, &id).await?;
    core.burn(&id, &View::parse(view.as_deref(), language.as_deref()), PathBuf::from(path)).map_err(e)
}

/// The recording to play next to a transcript, allowed for the webview's asset protocol.
#[tauri::command]
fn audio_path(app: AppHandle, id: String) -> R<Option<String>> {
    let core = app.state::<Arc<Core>>();
    let t = core.transcript(&id).map_err(e)?;
    let p = PathBuf::from(&t.source_path);
    if !p.is_file() {
        return Ok(None);
    }
    app.asset_protocol_scope().allow_file(&p).map_err(|x| x.to_string())?;
    Ok(Some(p.display().to_string()))
}

/// A WAV copy, for formats the webview cannot play.
#[tauri::command]
async fn audio_preview(app: AppHandle, id: String) -> R<String> {
    let core = app.state::<Arc<Core>>().inner().clone();
    let t = core.transcript(&id).map_err(e)?;
    let out = core.paths.cache_dir().join(format!("{id}.wav"));
    if !out.is_file() {
        let ff = setup::ffmpeg(&core.paths);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let samples = audio::decode(PathBuf::from(&t.source_path).as_path(), ff.as_ref(), &stop).map_err(e)?;
        audio::write_wav(&out, &samples).map_err(e)?;
    }
    app.asset_protocol_scope().allow_file(&out).map_err(|x| x.to_string())?;
    Ok(out.display().to_string())
}

#[tauri::command]
fn reveal(path: String) {
    let p = PathBuf::from(&path);
    let mut cmd = std::process::Command::new("explorer.exe");
    if p.is_file() {
        cmd.arg(format!("/select,{}", p.display()));
    } else {
        cmd.arg(&p);
    }
    let _ = cmd.spawn();
}

#[tauri::command]
fn open_log(core: C) {
    let _ = std::process::Command::new("explorer.exe").arg(core.paths.logs_dir()).spawn();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let core = Core::start(app.handle().clone())?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_setup,
            download_model,
            stop_download,
            use_model,
            delete_model,
            old_engine_size,
            remove_old_engine,
            choose_download_dir,
            import_model,
            import_model_path,
            set_run_on,
            set_ui_language,
            check_updates,
            set_check_updates,
            export_to_path,
            pick_folder,
            enqueue_folder,
            spoken_languages,
            path_kind,
            set_folder_formats,
            cancel_all_jobs,
            open_url,
            pick_files,
            enqueue,
            cancel_job,
            dismiss_job,
            list_history,
            get_transcript,
            update_segment,
            fix_timing,
            translation_targets,
            translate,
            update_translation,
            delete_translation,
            rename_transcript,
            delete_transcript,
            transcript_text,
            export_transcript,
            export_video,
            export_video_to_path,
            export_burned,
            export_burned_to_path,
            export_audio,
            export_audio_to_path,
            audio_path,
            audio_preview,
            reveal,
            open_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dawwin");
}
