//! Past transcripts, one JSON file each in %APPDATA%\Dawwin\history.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::minutes::Minutes;
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
    Ok(())
}

/// One change to a transcript's file at a time.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Read a transcript, change it and write it back, with no other change in between: an edit in
/// the window and a translation running in the background both change the same file.
pub fn modify<T>(dir: &Path, id: &str, f: impl FnOnce(&mut Transcript) -> T) -> Result<(Transcript, T)> {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut t = load(dir, id)?;
    let out = f(&mut t);
    save(dir, &t)?;
    Ok((t, out))
}

/// Change only a transcript's minutes, and leave the rest of its file as it is, fields this version
/// does not know included (a newer version's speakers, say): the minutes are written in the
/// background, and they are all that changes.
pub fn modify_minutes<T>(dir: &Path, id: &str, f: impl FnOnce(&mut Option<Minutes>) -> T) -> Result<(Transcript, T)> {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = file(dir, id)?;
    let mut raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).with_context(|| format!("no transcript {id}"))?)?;
    let mut t: Transcript = serde_json::from_value(raw.clone())?;
    let out = f(&mut t.minutes);
    let Some(fields) = raw.as_object_mut() else { bail!("transcript {id} is not an object") };
    match &t.minutes {
        Some(m) => {
            fields.insert("minutes".into(), serde_json::to_value(m)?);
        }
        None => {
            fields.remove("minutes");
        }
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&raw)?)?;
    std::fs::rename(&tmp, &path)?;
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
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn list(dir: &Path) -> Vec<Summary> {
    let mut out: Vec<Summary> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice::<Transcript>(&std::fs::read(e.path()).ok()?).ok())
        .map(|t| {
            let preview: String = t
                .segments
                .iter()
                .map(|s| s.text.trim())
                .find(|s| !s.is_empty())
                .unwrap_or("")
                .chars()
                .take(90)
                .collect();
            Summary {
                id: t.id,
                title: t.title,
                created_ms: t.created_ms,
                duration_s: t.duration_s,
                partial: t.partial,
                preview,
                source_path: t.source_path,
            }
        })
        .collect();
    out.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    out
}
