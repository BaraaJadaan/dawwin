//! The minutes as a job in the queue. They are written by the translator's server with a minutes
//! model, on the engine thread, so the speech engine, the translator and the minutes take turns
//! with the GPU's memory.

use serde_json::Value;

use super::*;
use crate::minutes::{self, Draft, Minutes};
use crate::translate::Progress;

/// Room for an answer: the schema keeps one well under this.
const ANSWER_TOKENS: u32 = 4096;
/// What an answer usually takes, to show how far one is.
const USUAL_TOKENS: f64 = 700.0;

impl Core {
    /// Write the minutes of a finished transcript in `language` (its own when `None`), as a job in
    /// the queue. A run that was stopped carries on from the notes it saved. `folder`: the minutes
    /// are also written next to the recording, as a folder run does with transcripts.
    pub fn write_minutes(&self, transcript: &str, language: Option<&str>, folder: bool) -> Result<String> {
        if self.is_live(transcript) {
            return Err(anyhow!(crate::tr!("This transcript is still being written. Write the minutes when it is done.")));
        }
        let t = history::load(&self.paths.history_dir(), transcript)?;
        minutes::ensure_text(&t)?;
        let language = language.filter(|l| !l.is_empty()).unwrap_or(&t.language).to_string();
        if !setup::model_status(&self.paths, &self.settings.lock().unwrap(), Family::Minutes).found {
            return Err(anyhow!(crate::tr!("There is no minutes model yet. Download one in Settings.")));
        }
        if setup::translator_exe(&self.paths).is_none() {
            return Err(anyhow!(crate::tr!("The translator is not downloaded. Download it in Settings.")));
        }
        let running = self.snapshot().queue.into_iter().find(|j| j.kind == "minutes" && j.transcript.as_deref() == Some(transcript) && ACTIVE_JOB.contains(&j.status.as_str()));
        if let Some(j) = running {
            return Ok(j.id);
        }
        // one that stopped before goes from the list: this one carries it on
        self.update(|s| s.queue.retain(|j| !(j.kind == "minutes" && j.transcript.as_deref() == Some(transcript))));
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let id = format!("{}-{:03}", now_ms(), n % 1000);
        self.cancels.lock().unwrap().insert(id.clone(), Arc::new(AtomicBool::new(false)));
        let job = JobView {
            id: id.clone(),
            title: t.title.clone(),
            path: t.source_path.clone(),
            language: t.language.clone(),
            kind: "minutes".into(),
            steps: Steps { transcribe: false, minutes: true, ..Default::default() },
            transcript: Some(transcript.to_string()),
            target: Some(language),
            status: "queued".into(),
            folder,
            ..Default::default()
        };
        self.update(|s| s.queue.push(job));
        self.send(Msg::Minutes(id.clone()));
        Ok(id)
    }

    /// The user's corrections to the minutes: what they changed is marked as theirs.
    pub fn edit_minutes(&self, transcript: &str, new: Minutes) -> Result<Minutes> {
        // a run under way replaces the minutes when it ends: an edit now would be lost without a word
        if self.snapshot().queue.iter().any(|j| j.kind == "minutes" && j.transcript.as_deref() == Some(transcript) && ACTIVE_JOB.contains(&j.status.as_str())) {
            return Err(anyhow!(crate::tr!("The minutes are being written again. Correct them when that is done.")));
        }
        let (_, m) = history::modify_minutes(&self.paths.history_dir(), transcript, |m| {
            let m = m.get_or_insert_with(Minutes::default);
            m.apply_edits(new);
            m.clone()
        })?;
        Ok(m)
    }
}

/// A recording started, or is under way: the minutes give it the engine and the GPU, and wait.
#[derive(Debug)]
pub(super) struct GiveWay;

impl std::fmt::Display for GiveWay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a recording is under way")
    }
}

impl std::error::Error for GiveWay {}

/// Set when the run is stopped or a recording starts: either ends the model's request at once (the
/// notes saved are kept). Watched until it is dropped.
struct Watch {
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl Watch {
    fn new(cancel: Arc<AtomicBool>) -> Self {
        let (stop, done) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (s, d) = (stop.clone(), done.clone());
        std::thread::spawn(move || {
            while !d.load(Ordering::Relaxed) {
                if cancel.load(Ordering::SeqCst) || super::meeting::is_recording() {
                    s.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        Watch { stop, done }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
    }
}

/// The server, with this model and room for the minutes, loaded.
fn ensure_server<'a>(core: &Arc<Core>, slot: &'a mut Option<TranslatorProcess>, model: &str, context: u32, cancel: &AtomicBool) -> Result<&'a mut TranslatorProcess> {
    let cpu_only = core.settings.lock().unwrap().run_on() == "cpu" || core.translator_gpu_failed.load(Ordering::SeqCst);
    if let Some(t) = slot.as_mut() {
        if t.model != model || t.cpu_only != cpu_only || t.context != context || !t.alive() {
            slot.take().unwrap().shutdown();
        }
    }
    if slot.is_none() {
        let exe = setup::translator_exe(&core.paths).ok_or_else(|| anyhow!(crate::tr!("The translator is not downloaded. Download it in Settings.")))?;
        let gpu = core.snapshot().engine.gpu;
        *slot = Some(TranslatorProcess::spawn_with(&exe, &core.paths, model, cpu_only, gpu.as_deref(), context, true)?);
    }
    let t = slot.as_mut().unwrap();
    t.wait_ready(cancel)?;
    Ok(t)
}

/// One request to the model `spec`; `step` of `steps` tells the job how far the whole run is.
#[allow(clippy::too_many_arguments)]
fn ask(core: &Arc<Core>, id: &str, w: &mut TranslatorProcess, spec: &minutes::Spec, a: &minutes::Ask, cancel: &AtomicBool, step: usize, steps: usize, started: Instant) -> Result<Option<Value>> {
    let mut last = Instant::now();
    let mut on = |p: Progress| {
        if last.elapsed() < Duration::from_millis(500) {
            return;
        }
        last = Instant::now();
        let read = if p.prompt_total > 0 { p.prompt_done as f64 / p.prompt_total as f64 } else if p.written > 0 { 1.0 } else { 0.0 };
        let inner = (0.4 * read + 0.6 * (p.written as f64 / USUAL_TOKENS).min(1.0)).min(0.97);
        let done = (step as f64 + inner) / steps as f64;
        let spent = started.elapsed().as_secs_f64();
        core.update_job(id, |j| {
            j.progress = done;
            j.eta_s = (done > 0.05).then(|| spent / done * (1.0 - done));
            j.elapsed_s = spent;
        });
    };
    let reply = w.ask_json(&a.messages, &a.schema, spec.no_thinking, ANSWER_TOKENS, cancel, &mut on)?;
    let answer = serde_json::from_str::<Value>(&reply.content).ok().filter(minutes::is_answer);
    Ok(answer)
}

pub(super) fn run(core: &Arc<Core>, server: &mut Option<TranslatorProcess>, id: &str) -> Result<()> {
    let Some(job) = core.snapshot().queue.into_iter().find(|j| j.id == id) else { return Ok(()) };
    if job.status != "queued" {
        return Ok(());
    }
    let (Some(transcript), Some(language)) = (job.transcript.clone(), job.target.clone()) else { return Ok(()) };
    let cancel = core.cancels.lock().unwrap().get(id).cloned().unwrap_or_default();
    // a recording has the engine's turn: the minutes wait for it (the queue sends them again)
    if super::meeting::is_recording() {
        return Err(anyhow!(GiveWay));
    }
    let watch = Watch::new(cancel.clone());
    let stop: &AtomicBool = &watch.stop;
    let started = Instant::now();
    let _awake = KeepAwake::new();
    core.update_job(id, |j| j.status = "loading".into());

    let status = setup::model_status(&core.paths, &core.settings.lock().unwrap(), Family::Minutes);
    let Some(variant) = models::variant(&status.variant).filter(|v| status.found && v.family == Family::Minutes) else {
        return Err(anyhow!(crate::tr!("There is no minutes model yet. Download one in Settings.")));
    };
    let spec = minutes::model(variant.id);
    let dir = core.paths.history_dir();
    let t = history::load(&dir, &transcript)?;
    minutes::ensure_text(&t)?;
    let lines = minutes::lines(&t, &minutes::speakers(&t));
    // a recorded meeting: the model is told who "Me" and "Them" are
    let note = minutes::who_is_who(&t);
    let from = minutes::fingerprint(&t);
    let parts = minutes::plan(&lines);
    let spans: Vec<[usize; 2]> = parts.iter().map(|&(a, b)| [lines[a].index, lines[b - 1].index]).collect();
    // the notes of a run that stopped, when it was of the same text, language and model
    let mut draft = t
        .minutes
        .as_ref()
        .and_then(|m| m.draft.clone())
        .filter(|d| d.language == language && d.model == variant.label && d.from == from && d.parts == spans)
        .unwrap_or_else(|| Draft { language: language.clone(), model: variant.label.into(), from: from.clone(), parts: spans.clone(), notes: Vec::new() });
    let w = ensure_server(core, server, &status.path, spec.context, stop)?;
    let device = w.placement.label();
    core.update_job(id, |j| {
        j.status = "writing".into();
        j.progress = 0.0;
        j.device = Some(device);
    });
    let _ = core.app.emit("minutes-started", &transcript);

    let source = t.language.clone();
    let steps = if parts.len() == 1 { 1 } else { parts.len() + 1 };
    let answer = if parts.len() == 1 {
        ask(core, id, w, &spec, &minutes::with_note(minutes::ask_whole(&lines, &source, &language), note.as_deref()), stop, 0, 1, started)?
    } else {
        for (n, &(a, b)) in parts.iter().enumerate().skip(draft.notes.len()) {
            let note = ask(core, id, w, &spec, &minutes::with_note(minutes::ask_part(&lines[a..b], n + 1, parts.len(), &source, &language), note.as_deref()), stop, n, steps, started)?;
            // a part the model could not answer still counts, empty, so the rest goes on
            draft.notes.push(note.unwrap_or_else(|| serde_json::json!({ "kind": "other", "summary": "" })));
            let kept = draft.clone();
            history::modify_minutes(&dir, &transcript, |m| m.get_or_insert_with(Minutes::default).draft = Some(kept))?;
        }
        let times: Vec<(f64, f64)> = parts.iter().map(|&(a, b)| (lines[a].start, lines[b - 1].start)).collect();
        let highest = lines.last().map_or(1, |l| l.index + 1);
        let last = steps - 1;
        Some(minutes::combine(draft.notes.clone(), times, |notes, times| {
            ask(core, id, w, &spec, &minutes::with_note(minutes::ask_combine(notes, times, highest, &source, &language), note.as_deref()), stop, last, steps, started)
        })?)
    };
    let Some(answer) = answer else {
        return Err(anyhow!(crate::tr!("The model did not write minutes that Dawwin could read. Try again, or another minutes model.")));
    };
    let mut m = minutes::check(&answer, &t, &lines, language == source);
    m.language = language.clone();
    m.model = variant.label.into();
    m.written_ms = now_ms();
    m.from = from;
    let kept = m.clone();
    let (saved, _) = history::modify_minutes(&dir, &transcript, |m| *m = Some(kept))?;
    let _ = core.app.emit("history-changed", &transcript);
    let _ = core.app.emit("minutes-done", &transcript);

    let mut next_error = None;
    if job.folder {
        if let Err(e) = write_next_to_recording(&saved, &m, &core.settings.lock().unwrap().folder_formats()) {
            next_error = Some(crate::tr!("The minutes are written, but could not be saved next to the recording: {}", e));
        }
    }
    core.update_job(id, |j| {
        j.status = if next_error.is_some() { "failed".into() } else { "done".into() };
        j.error = next_error;
        j.eta_s = None;
        j.elapsed_s = started.elapsed().as_secs_f64();
        j.saved = true;
        j.progress = 1.0;
    });
    Ok(())
}

/// The formats minutes are written in next to a recording: those of the folder run's formats that
/// suit them, else Markdown.
pub(crate) fn minutes_formats(formats: &[String]) -> Vec<String> {
    let chosen: Vec<String> = formats.iter().filter(|f| ["txt", "md", "docx"].contains(&f.as_str())).cloned().collect();
    if chosen.is_empty() {
        vec!["md".into()]
    } else {
        chosen
    }
}

/// `lesson.mp3` → `lesson.minutes.md`, or `lesson (2).minutes.md` when that is taken.
fn minutes_path(recording: &Path, ext: &str) -> PathBuf {
    let first = recording.with_extension(format!("minutes.{ext}"));
    if !first.exists() {
        return first;
    }
    let stem = recording.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    (2..).map(|n| recording.with_file_name(format!("{stem} ({n}).minutes.{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// Whether a recording has none of its minutes files next to it.
pub(crate) fn minutes_missing(recording: &Path, formats: &[String]) -> bool {
    minutes_formats(formats).iter().all(|ext| !recording.with_extension(format!("minutes.{ext}")).exists())
}

fn write_next_to_recording(t: &Transcript, m: &Minutes, formats: &[String]) -> Result<()> {
    let recording = Path::new(&t.source_path);
    for ext in minutes_formats(formats) {
        std::fs::write(minutes_path(recording, &ext), minutes::export(t, m, &ext, false)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_next_to_a_recording_take_the_folder_formats_that_suit_them() {
        let f = |v: &[&str]| minutes_formats(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(f(&["srt", "txt", "docx"]), ["txt", "docx"]);
        assert_eq!(f(&["srt", "vtt"]), ["md"]);
        let dir = std::env::temp_dir().join(format!("dawwin-minutes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("lesson.mp3");
        assert_eq!(minutes_path(&rec, "md"), dir.join("lesson.minutes.md"));
        std::fs::write(dir.join("lesson.minutes.md"), "x").unwrap();
        assert_eq!(minutes_path(&rec, "md"), dir.join("lesson (2).minutes.md"), "nothing already there is overwritten");
        assert!(!minutes_missing(&rec, &["srt".into()]), "lesson.minutes.md is there");
        assert!(minutes_missing(&rec, &["txt".into()]), "lesson.minutes.txt is not");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
