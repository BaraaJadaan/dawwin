//! Past transcripts, one JSON file each in %APPDATA%\Dawwin\history.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::transcript::Transcript;

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub duration_s: f64,
    pub partial: bool,
    pub preview: String,
    pub source_path: String,
    /// The files Dawwin wrote for it: a folder run never takes them for recordings
    #[serde(skip)]
    pub outputs: Vec<String>,
    /// Those files are listed (1.3.2 on), rather than to be told by their names
    #[serde(skip)]
    pub outputs_listed: bool,
}

fn file(dir: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("bad transcript id");
    }
    Ok(dir.join(format!("{id}.json")))
}

pub fn save(dir: &Path, t: &Transcript) -> Result<()> {
    let path = file(dir, &t.id)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(t)?)?;
    std::fs::rename(&tmp, &path)?;
    remember(&path, t);
    Ok(())
}

/// Read a transcript, change it and write it back, with no other change in between: an edit in
/// the window and a translation running in the background both change the same file.
pub fn modify<T>(dir: &Path, id: &str, f: impl FnOnce(&mut Transcript) -> T) -> Result<(Transcript, T)> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut t = load(dir, id)?;
    let out = f(&mut t);
    save(dir, &t)?;
    Ok((t, out))
}

pub fn load(dir: &Path, id: &str) -> Result<Transcript> {
    let path = file(dir, id)?;
    let bytes = std::fs::read(&path).with_context(|| format!("no transcript {id}"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn delete(dir: &Path, id: &str) -> Result<()> {
    let path = file(dir, id)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    forget(&path);
    Ok(())
}

/// What the list of past transcripts shows of each, kept in memory by file: a file is read again
/// only when its size or time changed, and the app's own saves update it directly. Hundreds of
/// transcripts list in milliseconds, and nothing is read again after every job.
type Index = std::collections::HashMap<PathBuf, (Option<std::time::SystemTime>, u64, Summary)>;
static INDEX: std::sync::Mutex<Option<Index>> = std::sync::Mutex::new(None);

fn summary(t: &Transcript) -> Summary {
    let preview: String = t.segments.iter().map(|s| s.text.trim()).find(|s| !s.is_empty()).unwrap_or("").chars().take(90).collect();
    Summary {
        id: t.id.clone(),
        title: t.title.clone(),
        created_ms: t.created_ms,
        duration_s: t.duration_s,
        partial: t.partial,
        preview,
        source_path: t.source_path.clone(),
        outputs: t.outputs.clone(),
        outputs_listed: t.outputs_listed,
    }
}

fn stamp(path: &Path) -> (Option<std::time::SystemTime>, u64) {
    std::fs::metadata(path).map(|m| (m.modified().ok(), m.len())).unwrap_or((None, 0))
}

fn remember(path: &Path, t: &Transcript) {
    let (time, len) = stamp(path);
    if let Some(index) = INDEX.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        index.insert(path.to_path_buf(), (time, len, summary(t)));
    }
}

fn forget(path: &Path) {
    if let Some(index) = INDEX.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        index.remove(path);
    }
}

pub fn list(dir: &Path) -> Vec<Summary> {
    let mut guard = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let index = guard.get_or_insert_with(Default::default);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = e.path();
        if !path.extension().is_some_and(|x| x == "json") {
            continue;
        }
        let (time, len) = e.metadata().map(|m| (m.modified().ok(), m.len())).unwrap_or((None, 0));
        seen.insert(path.clone());
        match index.get(&path) {
            Some((t, l, s)) if *t == time && *l == len && time.is_some() => out.push(s.clone()),
            _ => {
                let Some(t) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Transcript>(&b).ok()) else { continue };
                let s = summary(&t);
                index.insert(path, (time, len, s.clone()));
                out.push(s);
            }
        }
    }
    index.retain(|p, _| seen.contains(p) || !p.starts_with(dir));
    out.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript(id: &str, title: &str) -> Transcript {
        Transcript {
            id: id.into(),
            title: title.into(),
            source_path: "C:/x.m4a".into(),
            created_ms: 5,
            duration_s: 1.0,
            language: "ar".into(),
            elapsed_s: 0.0,
            device: String::new(),
            model: String::new(),
            partial: false,
            timed: false,
            outputs: vec![],
            outputs_listed: false,
            segments: vec![],
            translations: vec![],
            speakers: vec![],
            detected: None,
        }
    }

    #[test]
    fn the_list_follows_saves_edits_by_others_and_deletions() {
        let dir = std::env::temp_dir().join(format!("dawwin-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        save(&dir, &transcript("a", "one")).unwrap();
        assert_eq!(list(&dir)[0].title, "one");
        modify(&dir, "a", |t| t.title = "two".into()).unwrap();
        assert_eq!(list(&dir)[0].title, "two", "a save of the app's own");
        // written by something else: a longer file, so its size tells
        let mut t = transcript("a", "three, from another program");
        t.duration_s = 2.0;
        std::fs::write(dir.join("a.json"), serde_json::to_vec_pretty(&t).unwrap()).unwrap();
        assert_eq!(list(&dir)[0].title, "three, from another program");
        delete(&dir, "a").unwrap();
        assert!(list(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
