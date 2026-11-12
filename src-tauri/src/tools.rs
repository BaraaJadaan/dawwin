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
    /// Every file it needs to run: it counts as installed only when all are there, so a folder a
    /// cleaner emptied in part is offered again, not run
    pub files: &'static [&'static str],
    /// Which build of it this is: one installed in an older version is offered as an update
    pub version: u32,
}

/// The release the tools are files of. A newer version of the app keeps pointing here until a
/// tool itself changes.
const RELEASE: &str = "v1.4.0";

pub const TRANSLATOR: Tool = Tool {
    id: "translator",
    file: "dawwin-translator-llama.cpp-b12237.tar.xz",
    size: 14_117_968,
    sha256: "98f19e4bc87fc78e40bca8ec752d0a513d26fa04f464e77c015afaa67de02665",
    folder: "translate",
    exe: "llama-server.exe",
    // what the pack holds, but its two license files
    files: &[
        "llama-server.exe", "llama-server-impl.dll", "llama-common.dll", "llama.dll", "mtmd.dll", "libomp.dll",
        "ggml.dll", "ggml-base.dll", "ggml-vulkan.dll", "ggml-cpu-x64.dll", "ggml-cpu-sse42.dll",
        "ggml-cpu-sandybridge.dll", "ggml-cpu-ivybridge.dll", "ggml-cpu-piledriver.dll", "ggml-cpu-haswell.dll",
        "ggml-cpu-skylakex.dll", "ggml-cpu-cannonlake.dll", "ggml-cpu-cascadelake.dll", "ggml-cpu-icelake.dll",
        "ggml-cpu-cooperlake.dll", "ggml-cpu-alderlake.dll", "ggml-cpu-sapphirerapids.dll", "ggml-cpu-zen4.dll",
    ],
    version: 1,
};

pub const FFMPEG_AUDIO: Tool = Tool {
    id: "ffmpeg-audio",
    file: "dawwin-ffmpeg-audio-8.1.2.tar.xz",
    size: 1_335_556,
    sha256: "48a2b244b1815cdfe2f85a411812369e9efd480e67b42ef939d74e0719be4659",
    folder: "ffmpeg-audio",
    exe: "ffmpeg.exe",
    files: &["ffmpeg.exe"],
    version: 1,
};

pub const FFMPEG_VIDEO: Tool = Tool {
    id: "ffmpeg-video",
    file: "dawwin-ffmpeg-video-8.1.2.tar.xz",
    size: 3_847_048,
    sha256: "464b379577c87fb1952093cde7ff0112488f4e43502735e02d821629bb41feaf",
    folder: "ffmpeg-video",
    exe: "ffmpeg.exe",
    files: &["ffmpeg.exe"],
    // 2 (on the release of 1.4.0) reads AV1 (`video::reads_av1`)
    version: 2,
};

/// The voice for reading aloud: Dawwin's dawwin-voice.exe (voice/) with audio.cpp's official library
/// for the processor (with the Microsoft runtime it needs). Comes with the first voice.
pub const VOICE: Tool = Tool {
    id: "voice",
    file: "dawwin-voice-audio.cpp-v0.9.4.tar.xz",
    // from Tools run 43178594170, whose packs a release can upload with from_run
    size: 8_652_532,
    sha256: "aa6ae390bce0762f4d814deb6b6d5a883bee9bcc5ad716c99b920ddd6fd33a52",
    folder: "voice",
    exe: "dawwin-voice.exe",
    files: &[
        "dawwin-voice.exe", "audiocpp.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "msvcp140_atomic_wait.dll",
        "msvcp140_codecvt_ids.dll", "vcomp140.dll", "vcruntime140.dll", "vcruntime140_1.dll", "vcruntime140_threads.dll",
        // VoxCPM 2's ready-made voices (voice/refs)
        "voxcpm2-ar-f.wav", "voxcpm2-ar-m.wav", "voxcpm2-en-f.wav", "voxcpm2-en-m.wav",
    ],
    version: 1,
};

/// The GPU build of audio.cpp's library (Vulkan, for every brand), for the voices that want a
/// graphics card. The voice program loads it from here; where it cannot (no Vulkan driver), it
/// takes the processor build beside itself.
pub const VOICE_GPU: Tool = Tool {
    id: "voice-gpu",
    file: "dawwin-voice-gpu-audio.cpp-v0.9.4.tar.xz",
    size: 0,
    sha256: "",
    folder: "voice-gpu",
    exe: "audiocpp.dll",
    files: &[
        "audiocpp.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "msvcp140_atomic_wait.dll",
        "msvcp140_codecvt_ids.dll", "vcomp140.dll", "vcruntime140.dll", "vcruntime140_1.dll", "vcruntime140_threads.dll",
    ],
    version: 1,
};

pub const TOOLS: [Tool; 5] = [TRANSLATOR, FFMPEG_AUDIO, FFMPEG_VIDEO, VOICE, VOICE_GPU];

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

/// The tool's program, when it is installed: every file it needs is there.
pub fn exe(paths: &Paths, t: &Tool) -> Option<PathBuf> {
    let dir = root(paths).join(t.folder);
    t.files.iter().all(|f| dir.join(f).is_file()).then(|| dir.join(t.exe))
}

/// The file in a tool's folder that says which build of it is there.
const VERSION_FILE: &str = "dawwin-version.txt";

/// Which build of the tool is installed: none, or the one its folder says (1 for the builds of
/// 1.3, which did not say).
pub fn installed_version(paths: &Paths, t: &Tool) -> Option<u32> {
    exe(paths, t)?;
    let said = std::fs::read_to_string(root(paths).join(t.folder).join(VERSION_FILE)).ok();
    Some(said.and_then(|v| v.trim().parse().ok()).unwrap_or(1))
}

/// Installed, but an older build than the one the app now pins: offered as an update.
pub fn outdated(paths: &Paths, t: &Tool) -> bool {
    installed_version(paths, t).is_some_and(|v| v < t.version)
}

/// Tools unpacked by Dawwin 1.3.0 carry the date 1970-01-01, which cleaners and the antivirus take
/// for junk. Their files get today's date, so they keep working without a new download.
pub fn refresh_dates(paths: &Paths) {
    let year_2000 = std::time::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
    let now = std::time::SystemTime::now();
    for t in TOOLS {
        for entry in std::fs::read_dir(root(paths).join(t.folder)).into_iter().flatten().flatten() {
            let old = entry.metadata().and_then(|m| m.modified()).is_ok_and(|m| m < year_2000);
            if old {
                if let Ok(f) = std::fs::File::options().write(true).open(entry.path()) {
                    let _ = f.set_modified(now);
                }
            }
        }
    }
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
    std::fs::write(root(paths).join(t.folder).join(VERSION_FILE), t.version.to_string())?;
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
                // dated when unpacked: the packs say 1970, and cleaners delete files that old
                entry.set_preserve_mtime(false);
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
    fn every_tool_is_pinned_and_has_a_name_of_its_own() {
        for t in TOOLS {
            assert!(t.size > 0 && t.sha256.len() == 64, "{} has no size or checksum", t.id);
            assert!(t.file.ends_with(".tar.xz") && url(&t).starts_with("https://github.com/BaraaJadaan/dawwin/releases/download/"));
            assert_eq!(TOOLS.iter().filter(|o| o.folder == t.folder || o.file == t.file).count(), 1);
            assert_eq!(tool(t.id), Some(t));
        }
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
        // the archive says 1970, as the packs of 1.3.0 do: the files are dated now instead
        let dated = std::fs::metadata(into.join("ffmpeg.exe")).unwrap().modified().unwrap();
        assert!(dated > std::time::SystemTime::now() - std::time::Duration::from_secs(3600), "{dated:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tool_missing_a_file_is_not_installed_and_old_dates_are_made_new() {
        let dir = folder("files");
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        let t = TRANSLATOR;
        let into = root(&paths).join(t.folder);
        std::fs::create_dir_all(&into).unwrap();
        for f in t.files {
            std::fs::write(into.join(f), b"x").unwrap();
        }
        assert_eq!(exe(&paths, &t), Some(into.join("llama-server.exe")));
        // what 1.3.0 left: files dated 1970
        let epoch = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
        std::fs::File::options().write(true).open(into.join("ggml.dll")).unwrap().set_modified(epoch).unwrap();
        refresh_dates(&paths);
        let dated = std::fs::metadata(into.join("ggml.dll")).unwrap().modified().unwrap();
        assert!(dated > std::time::SystemTime::now() - std::time::Duration::from_secs(3600), "{dated:?}");
        // the builds of 1.3 did not say their version: the first
        assert_eq!(installed_version(&paths, &t), Some(1));
        let newer = Tool { version: 2, ..t };
        assert!(outdated(&paths, &newer) && !outdated(&paths, &t), "a newer pin is offered as an update");
        std::fs::write(into.join(VERSION_FILE), "2").unwrap();
        assert!(!outdated(&paths, &newer));
        // a cleaner took one of its files: not installed, so it is offered again
        std::fs::remove_file(into.join("ggml-base.dll")).unwrap();
        assert_eq!(exe(&paths, &t), None);
        assert_eq!(installed_version(&paths, &t), None);
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
    /// and its program starts. With DAWWIN_TEST_TOOLS_OUT, what they unpack to stays there, in the
    /// folders the app uses, for other tests to run.
    #[test]
    #[ignore]
    fn real_tools_unpack_and_start() {
        let from = PathBuf::from(std::env::var("DAWWIN_TEST_TOOLS").expect("DAWWIN_TEST_TOOLS"));
        let keep = std::env::var_os("DAWWIN_TEST_TOOLS_OUT").map(PathBuf::from);
        let dir = keep.clone().unwrap_or_else(|| folder("real"));
        let paths = Paths { local: dir.clone(), roaming: dir.clone() };
        for t in TOOLS {
            let file = from.join(t.file);
            assert!(file.is_file(), "no {}", file.display());
            let started = std::time::Instant::now();
            unpack(&file, &root(&paths).join(t.folder)).unwrap();
            let exe = exe(&paths, &t).unwrap_or_else(|| panic!("{} has no {}", t.file, t.exe));
            // a library is not a program: that it unpacked whole is what can be checked here
            if t.id == "voice-gpu" {
                println!("{}: unpacked in {:.1} s", t.id, started.elapsed().as_secs_f64());
                continue;
            }
            let flag = if t.id == "translator" || t.id == "voice" { "--version" } else { "-version" };
            let out = std::process::Command::new(&exe).arg(flag).output().unwrap();
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            println!("{}: unpacked in {:.1} s, {}", t.id, started.elapsed().as_secs_f64(), text.lines().next().unwrap_or(""));
            assert!(out.status.success(), "{} does not start: {text}", t.id);
        }
        if keep.is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
