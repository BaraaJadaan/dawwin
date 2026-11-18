//! Moving the models to another folder, or another disk, when the models folder changes: every
//! model Dawwin downloaded (speech, translation, minutes, speakers, language), what there is of an
//! unfinished download, and the voices of Read aloud. Nothing is lost on the way: on one disk the
//! files are renamed; to another they are copied first, and the old ones go only once every copy
//! is whole. A move that fails puts everything back as it was.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::models;

/// A file of Dawwin's in the models folder, and its size.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub bytes: u64,
}

/// What Dawwin keeps in the models folder `dir`: its models (finished or not) and the voices.
pub fn items(dir: &Path) -> Vec<Item> {
    let mut out = Vec::new();
    let mut add = |p: PathBuf| {
        if let Ok(m) = std::fs::metadata(&p) {
            if m.is_file() {
                out.push(Item { path: p, bytes: m.len() });
            }
        }
    };
    for v in models::VARIANTS {
        let file = dir.join(v.file);
        add(models::part_path(&file));
        add(models::marker_path(&file));
        add(file);
    }
    let mut stack = vec![dir.join("voices")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                add(p);
            }
        }
    }
    out
}

/// How a move goes: bytes done of the total.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// Move `items` from `from` to `to`, keeping where each is under it.
pub fn move_all(from: &Path, to: &Path, items: &[Item], progress: Progress) -> Result<()> {
    if same(from, to) {
        return Ok(());
    }
    let total: u64 = items.iter().map(|i| i.bytes).sum();
    let mut done = 0u64;
    // (where it was, where it is now, copied rather than renamed)
    let mut moved: Vec<(PathBuf, PathBuf, bool)> = Vec::new();
    let result = (|| -> Result<()> {
        for it in items {
            let rel = it.path.strip_prefix(from).map_err(|_| anyhow!("{} is not in {}", it.path.display(), from.display()))?;
            let dest = to.join(rel);
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p)?;
            }
            if dest.exists() {
                // a file of the same name there: the same one (a move done before) is taken, any other
                // stays where it is and this one is not moved over it
                if std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) == it.bytes {
                    moved.push((it.path.clone(), dest, true));
                    done += it.bytes;
                    progress(done, total);
                    continue;
                }
                return Err(anyhow!(crate::tr!("{} is already there, and is another file.", dest.display())));
            }
            if std::fs::rename(&it.path, &dest).is_ok() {
                moved.push((it.path.clone(), dest, false));
                done += it.bytes;
                progress(done, total);
                continue;
            }
            // another disk: copied, then the copy takes the name
            let tmp = dest.with_file_name(format!("{}.moving", dest.file_name().unwrap_or_default().to_string_lossy()));
            let copied = copy(&it.path, &tmp, &mut |n| progress(done + n, total));
            if let Err(e) = copied.and_then(|_| Ok(std::fs::rename(&tmp, &dest)?)) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            moved.push((it.path.clone(), dest, true));
            done += it.bytes;
            progress(done, total);
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            // every copy is whole: the old files go
            for (old, _, copied) in &moved {
                if *copied {
                    let _ = std::fs::remove_file(old);
                }
            }
            remove_empty(&from.join("voices"));
            Ok(())
        }
        Err(e) => {
            // back as it was
            for (old, new, copied) in moved.iter().rev() {
                if *copied {
                    if old.exists() {
                        let _ = std::fs::remove_file(new);
                    }
                } else {
                    let _ = std::fs::rename(new, old);
                }
            }
            Err(e)
        }
    }
}

fn copy(from: &Path, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<()> {
    let mut r = std::fs::File::open(from)?;
    let mut w = std::fs::File::create(to)?;
    let mut buf = vec![0u8; 4 << 20];
    let mut n = 0u64;
    loop {
        let k = r.read(&mut buf)?;
        if k == 0 {
            break;
        }
        w.write_all(&buf[..k])?;
        n += k as u64;
        progress(n);
    }
    w.sync_all()?;
    Ok(())
}

/// Folders left empty by the move go too.
fn remove_empty(dir: &Path) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            remove_empty(&e.path());
        }
    }
    let _ = std::fs::remove_dir(dir);
}

/// The same folder, however it is written.
pub fn same(a: &Path, b: &Path) -> bool {
    crate::folder::same_file_key(&a.display().to_string()) == crate::folder::same_file_key(&b.display().to_string())
}

/// A path under `from` as it is under `to` once moved; any other stays as it is.
pub fn moved_path(p: &Path, from: &Path, to: &Path) -> PathBuf {
    let key = crate::folder::same_file_key(&p.display().to_string());
    let base = crate::folder::same_file_key(&from.display().to_string());
    let base = if base.ends_with('\\') { base } else { format!("{base}\\") };
    if key.starts_with(&base) {
        if let Ok(rel) = p.strip_prefix(from) {
            return to.join(rel);
        }
        // the same folder written another way: the rest of the path counts
        let rest: PathBuf = p.components().skip(from.components().count()).collect();
        return to.join(rest);
    }
    p.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dawwin-move-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn models_and_voices_move_and_nothing_else_does() {
        let d = dir("all");
        let (from, to) = (d.join("old"), d.join("new"));
        std::fs::create_dir_all(from.join("voices/supertonic")).unwrap();
        let model = from.join(models::VARIANTS[0].file);
        std::fs::write(&model, vec![7u8; 1000]).unwrap();
        std::fs::write(from.join("voices/supertonic/a.gguf"), b"voice").unwrap();
        std::fs::write(from.join("notes.txt"), b"the user's own").unwrap();
        let list = items(&from);
        assert_eq!(list.len(), 2, "{list:?}");
        let mut last = (0, 0);
        move_all(&from, &to, &list, &mut |a, b| last = (a, b)).unwrap();
        assert_eq!(last, (1005, 1005));
        assert_eq!(std::fs::read(to.join(models::VARIANTS[0].file)).unwrap().len(), 1000);
        assert_eq!(std::fs::read(to.join("voices/supertonic/a.gguf")).unwrap(), b"voice");
        assert!(!model.exists() && !from.join("voices").exists(), "the old ones are gone");
        assert!(from.join("notes.txt").exists(), "what is not Dawwin's stays");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_move_that_fails_puts_everything_back() {
        let d = dir("fail");
        let (from, to) = (d.join("old"), d.join("new"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        let a = from.join(models::VARIANTS[0].file);
        let b = from.join(models::VARIANTS[1].file);
        std::fs::write(&a, b"first").unwrap();
        std::fs::write(&b, b"second").unwrap();
        // another file of that name in the new folder
        std::fs::write(to.join(models::VARIANTS[1].file), b"someone else's").unwrap();
        let list = items(&from);
        assert!(move_all(&from, &to, &list, &mut |_, _| {}).is_err());
        assert_eq!(std::fs::read(&a).unwrap(), b"first");
        assert_eq!(std::fs::read(&b).unwrap(), b"second");
        assert!(!to.join(models::VARIANTS[0].file).exists());
        assert_eq!(std::fs::read(to.join(models::VARIANTS[1].file)).unwrap(), b"someone else's");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_chosen_model_in_the_old_folder_is_found_in_the_new_one() {
        let (from, to) = (Path::new(r"C:\Users\x\Dawwin\models"), Path::new(r"D:\Models"));
        assert_eq!(moved_path(&from.join("q8.gguf"), from, to), to.join("q8.gguf"));
        assert_eq!(moved_path(Path::new(r"E:\mine\m.gguf"), from, to), PathBuf::from(r"E:\mine\m.gguf"));
        assert_eq!(moved_path(Path::new(r"C:\Users\x\Dawwin\models2\m.gguf"), from, to), PathBuf::from(r"C:\Users\x\Dawwin\models2\m.gguf"), "a folder whose name begins the same");
    }
}
