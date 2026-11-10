//! The programs Dawwin downloads only when they are wanted, so the installer stays small: the
//! translator (llama.cpp's server), which comes with the first translation model, and ffmpeg in
//! two sizes, one that reads and saves sound and adds subtitle tracks, and a larger one that also
//! burns subtitles into the picture.
//!
//! Each is a `.tar.xz` on the release (`tools/pack.py` makes them, the Tools workflow uploads them),
//! downloaded like a model, checked against its SHA-256, and unpacked into a folder of its own
//! under `%LOCALAPPDATA%\Dawwin\tools`.

use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, bail, Context, Result};

use crate::models::{self, Event, Finished};
use crate::setup::Paths;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tool {
    pub id: &'static str,
    /// The file on the release
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// The folder it is unpacked into, and the program in it
    pub folder: &'static str,
    pub exe: &'static str,
}

/// The release the tools are files of. A newer version of the app keeps pointing here until a
/// tool itself changes.
const RELEASE: &str = "v1.3.0";

pub const TRANSLATOR: Tool = Tool {
    id: "translator",
    file: "dawwin-translator-llama.cpp-b12237.tar.xz",
    size: 0,
    sha256: "",
    folder: "translate",
    exe: "llama-server.exe",
};

pub const FFMPEG_AUDIO: Tool = Tool {
    id: "ffmpeg-audio",
    file: "dawwin-ffmpeg-audio-8.1.2.tar.xz",
    size: 0,
    sha256: "",
    folder: "ffmpeg-audio",
    exe: "ffmpeg.exe",
};

pub const FFMPEG_VIDEO: Tool = Tool {
    id: "ffmpeg-video",
    file: "dawwin-ffmpeg-video-8.1.2.tar.xz",
    size: 0,
    sha256: "",
    folder: "ffmpeg-video",
    exe: "ffmpeg.exe",
};

pub const TOOLS: [Tool; 3] = [TRANSLATOR, FFMPEG_AUDIO, FFMPEG_VIDEO];

pub fn tool(id: &str) -> Option<Tool> {
    TOOLS.into_iter().find(|t| t.id == id)
}

pub fn url(t: &Tool) -> String {
    // for trying the downloads against a server of one's own; not there in release builds
    if cfg!(debug_assertions) {
        if let Some(base) = std::env::var("DAWWIN_TOOLS_URL").ok().filter(|b| !b.is_empty()) {
            return format!("{}/{}", base.trim_end_matches('/'), t.file);
        }
    }
    format!("https://github.com/{}/releases/download/{RELEASE}/{}", crate::updates::REPO, t.file)
}

fn root(paths: &Paths) -> PathBuf {
    paths.local.join("tools")
}

/// Where the download is saved before it is unpacked.
fn archive(paths: &Paths, t: &Tool) -> PathBuf {
    root(paths).join(t.file)
}

/// The tool's program, when it is installed.
pub fn exe(paths: &Paths, t: &Tool) -> Option<PathBuf> {
    let exe = root(paths).join(t.folder).join(t.exe);
    exe.is_file().then_some(exe)
}

/// How much of a download that is not finished is there.
pub fn partial(paths: &Paths, t: &Tool) -> u64 {
    models::downloaded(&archive(paths, t))
}

/// Download, check and unpack a tool. Paused, what arrived stays for the next time.
pub fn install(paths: &Paths, t: &Tool, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Finished> {
    let dest = archive(paths, t);
    let job = models::Download { url: url(t), dest: dest.clone(), size: t.size, sha256: t.sha256.to_string() };
    if models::download(&job, stop, on_event)? == Finished::Paused {
        return Ok(Finished::Paused);
    }
    let unpacked = unpack(&dest, &root(paths).join(t.folder));
    let _ = std::fs::remove_file(&dest);
    unpacked?;
    if exe(paths, t).is_none() {
        bail!("{} has no {}", t.file, t.exe);
    }
    Ok(Finished::Complete)
}

/// Unpack a `.tar.xz` into `dir`, in place of what was there. Only plain files at its top level
/// are taken, so nothing in it can land outside the folder.
pub fn unpack(archive: &Path, dir: &Path) -> Result<()> {
    let fresh = dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh)?;
    let result = (|| -> Result<()> {
        let mut input = BufReader::new(std::fs::File::open(archive)?);
        let mut tar = Vec::new();
        lzma_rs::xz_decompress(&mut input, &mut tar).map_err(|e| anyhow!("{}: {e:?}", archive.display()))?;
        let mut files = tar::Archive::new(Cursor::new(tar));
        for entry in files.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            let mut parts = path.components().filter(|c| !matches!(c, std::path::Component::CurDir));
            let (Some(std::path::Component::Normal(name)), None) = (parts.next(), parts.next()) else { continue };
            if entry.header().entry_type().is_file() {
                entry.unpack(fresh.join(name)).with_context(|| format!("{}", path.display()))?;
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|_| {
            let _ = std::fs::remove_dir_all(&fresh);
            anyhow!(crate::tr!("{} is in use. Close what uses it and try again.", dir.display()))
        })?;
    }
    std::fs::rename(&fresh, dir)?;
    Ok(())
}

/// Delete a tool, and what is there of an unfinished download of it.
pub fn remove(paths: &Paths, t: &Tool) -> Result<()> {
    models::discard(&archive(paths, t));
    let dir = root(paths).join(t.folder);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|_| anyhow!(crate::tr!("{} is in use. Close what uses it and try again.", dir.display())))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dawwin-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A .tar.xz holding these files, by name.
    fn tar_xz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, *data).unwrap();
        }
        let tar = b.into_inner().unwrap();
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut Cursor::new(tar), &mut xz).unwrap();
        xz
    }

    #[test]
    fn a_tool_unpacks_into_its_folder_in_place_of_the_old_one() {
        let dir = folder("unpack");
        let file = dir.join("t.tar.xz");
        std::fs::write(&file, tar_xz(&[("ffmpeg.exe", b"new"), ("./LICENSE.txt", b"lgpl")])).unwrap();
        let into = dir.join("ffmpeg-audio");
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(into.join("old.dll"), b"old").unwrap();
        unpack(&file, &into).unwrap();
        assert_eq!(std::fs::read(into.join("ffmpeg.exe")).unwrap(), b"new");
        assert_eq!(std::fs::read(into.join("LICENSE.txt")).unwrap(), b"lgpl");
        assert!(!into.join("old.dll").exists(), "what the older version had is gone");
        assert!(!dir.join("ffmpeg-audio.new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_in_an_archive_lands_outside_its_folder() {
        let dir = folder("escape");
        let file = dir.join("t.tar.xz");
        // the tar builder refuses ".." itself, so the header is written by hand
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        h.as_old_mut().name[..10].copy_from_slice(b"../out.exe");
        h.set_cksum();
        let mut b = tar::Builder::new(Vec::new());
        b.append(&h, &b"x"[..]).unwrap();
        b.append_data(&mut tar_header(2), "sub/inner.dll", &b"yy"[..]).unwrap();
        b.append_data(&mut tar_header(2), "ok.exe", &b"ok"[..]).unwrap();
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut Cursor::new(b.into_inner().unwrap()), &mut xz).unwrap();
        std::fs::write(&file, xz).unwrap();
        let into = dir.join("tool");
        unpack(&file, &into).unwrap();
        assert!(!dir.join("out.exe").exists());
        assert!(!into.join("sub").exists() && !into.join("inner.dll").exists());
        assert!(into.join("ok.exe").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tar_header(size: u64) -> tar::Header {
        let mut h = tar::Header::new_gnu();
        h.set_size(size);
        h.set_mode(0o644);
        h.set_cksum();
        h
    }

    #[test]
    fn a_damaged_archive_leaves_the_installed_tool_as_it_was() {
        let dir = folder("damaged");
        let file = dir.join("t.tar.xz");
        let mut xz = tar_xz(&[("ffmpeg.exe", b"new")]);
        xz.truncate(xz.len() / 2);
        std::fs::write(&file, xz).unwrap();
        let into = dir.join("ffmpeg-audio");
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(into.join("ffmpeg.exe"), b"old").unwrap();
        assert!(unpack(&file, &into).is_err());
        assert_eq!(std::fs::read(into.join("ffmpeg.exe")).unwrap(), b"old");
        assert!(!dir.join("ffmpeg-audio.new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_a_tool_takes_its_folder_and_any_unfinished_download() {
        let dir = folder("remove");
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        let t = FFMPEG_VIDEO;
        std::fs::create_dir_all(root(&paths).join(t.folder)).unwrap();
        std::fs::write(root(&paths).join(t.folder).join(t.exe), b"x").unwrap();
        std::fs::write(models::part_path(&archive(&paths, &t)), b"half").unwrap();
        assert!(exe(&paths, &t).is_some());
        remove(&paths, &t).unwrap();
        assert!(exe(&paths, &t).is_none());
        assert!(!models::part_path(&archive(&paths, &t)).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Any archive packed by tools/pack.py (DAWWIN_TEST_ARCHIVE): what it holds comes out whole.
    #[test]
    #[ignore]
    fn a_packed_archive_unpacks() {
        let file = PathBuf::from(std::env::var("DAWWIN_TEST_ARCHIVE").expect("DAWWIN_TEST_ARCHIVE"));
        let dir = folder("packed");
        let started = std::time::Instant::now();
        unpack(&file, &dir.join("out")).unwrap();
        let files: Vec<_> = std::fs::read_dir(dir.join("out")).unwrap().flatten().collect();
        let bytes: u64 = files.iter().map(|f| f.metadata().unwrap().len()).sum();
        println!("{} files, {bytes} bytes, in {:.1} s", files.len(), started.elapsed().as_secs_f64());
        if let Some(same) = std::env::var_os("DAWWIN_TEST_ARCHIVE_FROM").map(PathBuf::from) {
            for f in &files {
                assert_eq!(std::fs::read(f.path()).unwrap(), std::fs::read(same.join(f.file_name())).unwrap(), "{:?}", f.file_name());
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real files of the release: DAWWIN_TEST_TOOLS is a folder holding them. Each unpacks,
    /// and its program starts.
    #[test]
    #[ignore]
    fn real_tools_unpack_and_start() {
        let from = PathBuf::from(std::env::var("DAWWIN_TEST_TOOLS").expect("DAWWIN_TEST_TOOLS"));
        let dir = folder("real");
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        for t in TOOLS {
            let file = from.join(t.file);
            assert!(file.is_file(), "no {}", file.display());
            let started = std::time::Instant::now();
            unpack(&file, &root(&paths).join(t.folder)).unwrap();
            let exe = exe(&paths, &t).unwrap_or_else(|| panic!("{} has no {}", t.file, t.exe));
            let flag = if t.id == "translator" { "--version" } else { "-version" };
            let out = std::process::Command::new(&exe).arg(flag).output().unwrap();
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            println!("{}: unpacked in {:.1} s, {}", t.id, started.elapsed().as_secs_f64(), text.lines().next().unwrap_or(""));
            assert!(out.status.success(), "{} does not start: {text}", t.id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
