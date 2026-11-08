//! The speech models: which ones there are, where they live, and how they get here.
//!
//! A download survives anything: the connection dropping, the app closing or crashing, the power
//! going out. It writes to `<file>.part`, and `<file>.part.ok` records how much of that is known to
//! be on the disk. A restart continues from there with an HTTP range request, and the file only
//! takes its real name after its checksum matches.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};

const REPOSITORY: &str = "handy-computer/cohere-transcribe-arabic-07-2026-gguf";
/// The revision these checksums belong to: it cannot change under the app.
const REVISION: &str = "11a54d6895f94cce78d0ece142b8e552e7822539";
/// Fall back on the disk at most this much after a power cut, instead of the whole download.
const SYNC_EVERY: u64 = 8 * 1024 * 1024;
const BUFFER: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub id: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub const VARIANTS: [Variant; 2] = [
    Variant {
        id: "q8",
        file: "cohere-transcribe-arabic-07-2026-Q8_0.gguf",
        sha256: "910de5c9c57f9fd8a280e1701f9cd96f63768878c1ca9e4ecf23638a5e0fef16",
        size: 2_410_655_136,
    },
    Variant {
        id: "q4",
        file: "cohere-transcribe-arabic-07-2026-Q4_K_M.gguf",
        sha256: "4671080cd10ffc6166e1e1b36216f9b51e1455f170d55c59cff7b81332ad4621",
        size: 1_558_162_848,
    },
];

pub fn variant(id: &str) -> Option<Variant> {
    VARIANTS.into_iter().find(|v| v.id == id)
}

/// Which of ours this file is, by name.
pub fn variant_of(path: &Path) -> Option<Variant> {
    let name = path.file_name()?.to_str()?;
    VARIANTS.into_iter().find(|v| v.file.eq_ignore_ascii_case(name))
}

pub fn url(v: Variant) -> String {
    format!("https://huggingface.co/{REPOSITORY}/resolve/{REVISION}/{}", v.file)
}

/// Where to download a model by hand.
pub fn files_page() -> String {
    format!("https://huggingface.co/{REPOSITORY}/tree/{REVISION}")
}

pub fn part_path(dest: &Path) -> PathBuf {
    with_suffix(dest, ".part")
}

fn marker_path(dest: &Path) -> PathBuf {
    with_suffix(dest, ".part.ok")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// How much of this download is safely on the disk.
pub fn downloaded(dest: &Path) -> u64 {
    let part = std::fs::metadata(part_path(dest)).map(|m| m.len()).unwrap_or(0);
    let marker = std::fs::read_to_string(marker_path(dest)).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    part.min(marker)
}

/// Remove a download that is not finished.
pub fn discard(dest: &Path) {
    let _ = std::fs::remove_file(part_path(dest));
    let _ = std::fs::remove_file(marker_path(dest));
}

/// Take an unfinished download along to another folder. On another drive it stays behind: moving
/// gigabytes is not worth making the user wait for.
pub fn move_unfinished(from: &Path, to: &Path) {
    if from != to && std::fs::rename(part_path(from), part_path(to)).is_ok() {
        let _ = std::fs::rename(marker_path(from), marker_path(to));
    }
}

/// A finished model file that the engine can try to load. Our own files must have their exact
/// size, so a file cut short by a copy is not taken for a model.
pub fn check_file(path: &Path) -> Result<u64> {
    let meta = std::fs::metadata(path).map_err(|_| anyhow!(crate::tr!("That file could not be read.")))?;
    if !meta.is_file() {
        bail!(crate::tr!("That is not a model file."));
    }
    if let Some(v) = variant_of(path) {
        if meta.len() != v.size {
            bail!(crate::tr!("That model file is incomplete."));
        }
    }
    let mut head = vec![0u8; 256 * 1024];
    let mut f = std::fs::File::open(path)?;
    let n = f.read(&mut head)?;
    head.truncate(n);
    if !head.starts_with(b"GGUF") {
        bail!(crate::tr!("That is not a model file."));
    }
    if !head.windows(10).any(|w| w == b"cohere_asr") {
        bail!(crate::tr!("That file holds a different model. Dawwin needs Cohere Transcribe Arabic."));
    }
    Ok(meta.len())
}

/// A model file in this folder, the larger of ours first.
pub fn find_in_folder(dir: &Path) -> Option<PathBuf> {
    let ours = VARIANTS.iter().map(|v| dir.join(v.file)).find(|p| check_file(p).is_ok());
    ours.or_else(|| {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("gguf")))
            .find(|p| check_file(p).is_ok())
    })
}

// ---------------------------------------------------------------- download

pub struct Download {
    pub url: String,
    pub dest: PathBuf,
    pub size: u64,
    pub sha256: String,
}

pub enum Event {
    Progress { done: u64 },
    /// The connection is gone; trying again after this many seconds
    Retrying { wait_s: u64, attempt: u64 },
    /// All of it is here; reading it back to check it
    Verifying { done: u64 },
}

#[derive(Debug, PartialEq)]
pub enum Finished {
    Complete,
    Paused,
}

enum Fetch {
    Complete,
    Stopped,
    /// Try again later
    Retry,
    /// The file on the server does not continue what we have
    StartOver,
}

pub fn download(d: &Download, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Finished> {
    if let Some(dir) = d.dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        // a connection that sends nothing for this long is dead, however slow it was before
        .timeout_read(Duration::from_secs(60))
        .tls_connector(std::sync::Arc::new(native_tls::TlsConnector::new()?))
        .build();
    let mut attempt = 0u64;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(Finished::Paused);
        }
        let before = downloaded(&d.dest);
        if before < d.size {
            need_space(d.dest.parent().unwrap_or(Path::new(".")), d.size - before)?;
        }
        match fetch(&agent, d, before, stop, on_event)? {
            Fetch::Complete => break,
            Fetch::Stopped => return Ok(Finished::Paused),
            Fetch::StartOver => {
                discard(&d.dest);
                attempt = 0;
            }
            Fetch::Retry => {
                // what counts is whether it is getting anywhere, not how often it dropped
                attempt = if downloaded(&d.dest) > before { 1 } else { attempt + 1 };
                let wait_s = [5, 10, 20, 30, 60].get(attempt as usize - 1).copied().unwrap_or(60);
                on_event(Event::Retrying { wait_s, attempt });
                for _ in 0..wait_s * 5 {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(Finished::Paused);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    }
    if !verify(d, stop, on_event)? {
        return if stop.load(Ordering::SeqCst) {
            Ok(Finished::Paused)
        } else {
            discard(&d.dest);
            bail!(crate::tr!("The download arrived damaged, so it was removed. Please start it again."))
        };
    }
    std::fs::rename(part_path(&d.dest), &d.dest)?;
    let _ = std::fs::remove_file(marker_path(&d.dest));
    Ok(Finished::Complete)
}

fn fetch(agent: &ureq::Agent, d: &Download, offset: u64, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<Fetch> {
    if offset >= d.size {
        return Ok(Fetch::Complete);
    }
    let response = agent
        .get(&d.url)
        .set("User-Agent", concat!("Dawwin/", env!("CARGO_PKG_VERSION")))
        .set("Range", &format!("bytes={offset}-"))
        .call();
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(416, _)) => return Ok(Fetch::StartOver),
        Err(ureq::Error::Status(code, _)) if code == 429 || code >= 500 => return Ok(Fetch::Retry),
        Err(ureq::Error::Status(code, _)) => bail!(crate::tr!("The server refused the download (error {}).", code)),
        Err(ureq::Error::Transport(_)) => return Ok(Fetch::Retry),
    };
    // a server may answer a range request with the whole file: then that is what we write
    let mut at = if response.status() == 206 { offset } else { 0 };
    if response.status() == 206 && content_range_start(&response) != Some(offset) {
        return Ok(Fetch::StartOver);
    }

    let mut file = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(part_path(&d.dest))?;
    file.set_len(at)?; // what was written after the last sync may not have reached the disk
    file.seek(SeekFrom::Start(at))?;
    write_marker(&d.dest, at)?;

    let mut body = response.into_reader();
    let mut buf = vec![0u8; BUFFER];
    let mut unsynced = 0u64;
    let mut told = Instant::now();
    on_event(Event::Progress { done: at });
    let end = loop {
        if stop.load(Ordering::SeqCst) {
            break Fetch::Stopped;
        }
        let n = match body.read(&mut buf) {
            Ok(0) => break if at >= d.size { Fetch::Complete } else { Fetch::Retry },
            Ok(n) => n,
            Err(_) => break Fetch::Retry,
        };
        if at + n as u64 > d.size {
            // longer than the file we are after: it is not that file
            break Fetch::StartOver;
        }
        file.write_all(&buf[..n])?;
        at += n as u64;
        unsynced += n as u64;
        if unsynced >= SYNC_EVERY {
            file.sync_data()?;
            write_marker(&d.dest, at)?;
            unsynced = 0;
        }
        if told.elapsed() >= Duration::from_millis(250) {
            on_event(Event::Progress { done: at });
            told = Instant::now();
        }
        if at == d.size {
            break Fetch::Complete;
        }
    };
    file.sync_data()?;
    write_marker(&d.dest, at)?;
    on_event(Event::Progress { done: at });
    Ok(end)
}

fn content_range_start(r: &ureq::Response) -> Option<u64> {
    r.header("Content-Range")?.trim().strip_prefix("bytes ")?.split('-').next()?.trim().parse().ok()
}

/// Replaced in one step, so that it never reads as less than what is really there.
fn write_marker(dest: &Path, len: u64) -> Result<()> {
    let tmp = with_suffix(dest, ".part.ok.tmp");
    std::fs::write(&tmp, len.to_string())?;
    std::fs::rename(&tmp, marker_path(dest))?;
    Ok(())
}

fn verify(d: &Download, stop: &AtomicBool, on_event: &mut dyn FnMut(Event)) -> Result<bool> {
    let mut f = std::fs::File::open(part_path(&d.dest))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut done = 0u64;
    let mut told = Instant::now();
    on_event(Event::Verifying { done: 0 });
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        done += n as u64;
        if told.elapsed() >= Duration::from_millis(250) {
            on_event(Event::Verifying { done });
            told = Instant::now();
        }
    }
    Ok(format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&d.sha256))
}

#[cfg(windows)]
fn free_space(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, free: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` ends in a NUL and outlives the call; the other two outputs may be null.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
fn free_space(_dir: &Path) -> Option<u64> {
    None
}

fn need_space(dir: &Path, bytes: u64) -> Result<()> {
    let margin = 200 * 1024 * 1024;
    if let Some(free) = free_space(dir) {
        if free < bytes + margin {
            bail!(crate::tr!(
                "Not enough free space: the model needs {} GB more, and {} GB is free. Choose another folder or free some space.",
                format!("{:.1}", bytes as f64 / 1e9),
                format!("{:.1}", free as f64 / 1e9)
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Copy, PartialEq)]
    enum Serve {
        Normal,
        /// Closes the first connection after this many bytes
        DropAfter(usize),
        /// Answers every range request with the whole file
        IgnoresRanges,
        NotFound,
        /// The first answer is a server error
        BusyOnce,
    }

    struct Server {
        url: String,
        ranges: Arc<Mutex<Vec<Option<u64>>>>,
    }

    /// A small web server that speaks just enough HTTP, and can misbehave on request.
    fn serve(body: Vec<u8>, how: Serve) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.gguf", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = ranges.clone();
        std::thread::spawn(move || {
            let mut request_no = 0;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                request_no += 1;
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_lowercase().strip_prefix("range: bytes=") {
                        range = v.trim().trim_end_matches('-').parse::<u64>().ok();
                    }
                }
                seen.lock().unwrap().push(range);
                if how == Serve::NotFound {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                if how == Serve::BusyOnce && request_no == 1 {
                    let _ = stream.write_all(b"HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let from = match (range, how) {
                    (_, Serve::IgnoresRanges) | (None, _) => None,
                    (Some(r), _) => Some(r as usize),
                };
                let part = &body[from.unwrap_or(0).min(body.len())..];
                let head = match from {
                    Some(r) => format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n", part.len(), r, body.len() - 1, body.len()),
                    None => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len()),
                };
                let _ = stream.write_all(head.as_bytes());
                match how {
                    Serve::DropAfter(n) if request_no == 1 => {
                        let _ = stream.write_all(&part[..n.min(part.len())]);
                    }
                    _ => {
                        let _ = stream.write_all(part);
                    }
                }
            }
        });
        Server { url, ranges }
    }

    fn payload() -> Vec<u8> {
        (0..700_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dawwin-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(server: &Server, dir: &Path, body: &[u8]) -> Download {
        Download { url: server.url.clone(), dest: dir.join("model.gguf"), size: body.len() as u64, sha256: sha(body) }
    }

    #[test]
    fn downloads_a_whole_file_and_checks_it() {
        let (body, dir) = (payload(), folder("whole"));
        let s = serve(body.clone(), Serve::Normal);
        let d = job(&s, &dir, &body);
        let end = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(end, Finished::Complete);
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
        assert!(!part_path(&d.dest).exists() && !marker_path(&d.dest).exists());
    }

    #[test]
    fn pausing_keeps_the_bytes_and_resuming_asks_only_for_the_rest() {
        let (body, dir) = (payload(), folder("pause"));
        let s = serve(body.clone(), Serve::Normal);
        let d = job(&s, &dir, &body);
        let stop = AtomicBool::new(false);
        let end = download(&d, &stop, &mut |e| {
            if let Event::Progress { .. } = e {
                stop.store(true, Ordering::SeqCst);
            }
        });
        // the very first progress event comes before any bytes: pausing there keeps nothing
        assert_eq!(end.unwrap(), Finished::Paused);
        assert!(!d.dest.exists(), "a paused download is not a model");

        // pretend it stopped half way
        std::fs::write(part_path(&d.dest), &body[..300_000]).unwrap();
        write_marker(&d.dest, 300_000).unwrap();
        assert_eq!(downloaded(&d.dest), 300_000);
        let end = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(end, Finished::Complete);
        assert_eq!(s.ranges.lock().unwrap().last().unwrap(), &Some(300_000), "asked for the rest only");
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_crash_falls_back_to_what_was_known_to_be_on_the_disk() {
        let (body, dir) = (payload(), folder("crash"));
        let s = serve(body.clone(), Serve::Normal);
        let d = job(&s, &dir, &body);
        // 400 000 bytes in the file, but only 100 000 were synced; the rest may be garbage
        let mut part = body[..100_000].to_vec();
        part.extend(std::iter::repeat_n(0xAAu8, 300_000));
        std::fs::write(part_path(&d.dest), part).unwrap();
        write_marker(&d.dest, 100_000).unwrap();
        assert_eq!(downloaded(&d.dest), 100_000);
        assert_eq!(download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap(), Finished::Complete);
        assert_eq!(s.ranges.lock().unwrap()[0], Some(100_000));
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_dropped_connection_carries_on_from_where_it_stopped() {
        let (body, dir) = (payload(), folder("drop"));
        let s = serve(body.clone(), Serve::DropAfter(250_000));
        let d = job(&s, &dir, &body);
        let mut retried = false;
        let end = download(&d, &AtomicBool::new(false), &mut |e| {
            if let Event::Retrying { .. } = e {
                retried = true;
            }
        })
        .unwrap();
        assert_eq!(end, Finished::Complete);
        assert!(retried);
        let ranges = s.ranges.lock().unwrap();
        assert_eq!(ranges[0], Some(0));
        assert_eq!(ranges[1], Some(250_000));
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_server_that_ignores_ranges_still_gives_the_right_file() {
        let (body, dir) = (payload(), folder("ignore"));
        let s = serve(body.clone(), Serve::IgnoresRanges);
        let d = job(&s, &dir, &body);
        std::fs::write(part_path(&d.dest), &body[..300_000]).unwrap();
        write_marker(&d.dest, 300_000).unwrap();
        assert_eq!(download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap(), Finished::Complete);
        assert_eq!(std::fs::read(&d.dest).unwrap(), body);
    }

    #[test]
    fn a_busy_server_is_tried_again() {
        let (body, dir) = (payload(), folder("busy"));
        let s = serve(body.clone(), Serve::BusyOnce);
        let d = job(&s, &dir, &body);
        assert_eq!(download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap(), Finished::Complete);
    }

    #[test]
    fn a_missing_file_is_an_error_and_not_a_retry() {
        let (body, dir) = (payload(), folder("missing"));
        let s = serve(body.clone(), Serve::NotFound);
        let d = job(&s, &dir, &body);
        let err = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(err.contains("404"), "{err}");
    }

    #[test]
    fn a_damaged_download_is_thrown_away() {
        let (body, dir) = (payload(), folder("damaged"));
        let s = serve(body.clone(), Serve::Normal);
        let mut d = job(&s, &dir, &body);
        d.sha256 = sha(b"something else");
        assert!(download(&d, &AtomicBool::new(false), &mut |_| {}).is_err());
        assert!(!d.dest.exists());
        assert!(!part_path(&d.dest).exists(), "so the next try starts clean");
    }

    #[test]
    fn pausing_during_the_retry_wait_stops_at_once() {
        let (body, dir) = (payload(), folder("wait"));
        let s = serve(body.clone(), Serve::BusyOnce);
        let d = job(&s, &dir, &body);
        let stop = AtomicBool::new(false);
        let started = Instant::now();
        let end = download(&d, &stop, &mut |e| {
            if let Event::Retrying { .. } = e {
                stop.store(true, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert_eq!(end, Finished::Paused);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn only_real_model_files_pass() {
        let dir = folder("check");
        let good = dir.join("mine.gguf");
        let mut bytes = b"GGUF\x03\0\0\0general.architecture\x0a\0\0\0cohere_asr".to_vec();
        bytes.extend(vec![0u8; 1000]);
        std::fs::write(&good, &bytes).unwrap();
        assert!(check_file(&good).is_ok());
        assert_eq!(find_in_folder(&dir), Some(good));

        let other = dir.join("whisper.gguf");
        std::fs::write(&other, b"GGUF\x03\0\0\0general.architecture\x07\0\0\0whisper").unwrap();
        assert!(check_file(&other).is_err());

        let text = dir.join("notes.gguf");
        std::fs::write(&text, b"not a model").unwrap();
        assert!(check_file(&text).is_err());

        // our own file name, cut short by a copy
        let short = dir.join(VARIANTS[1].file);
        std::fs::write(&short, &bytes).unwrap();
        assert!(check_file(&short).is_err());
    }

    /// The real thing, over the real network: `cargo test real_download -- --ignored --nocapture`.
    /// It pauses part way, then resumes, and the checksum has to match at the end.
    #[test]
    #[ignore]
    fn real_download() {
        let v = variant(&std::env::var("DAWWIN_TEST_MODEL").unwrap_or_else(|_| "q4".into())).unwrap();
        let dir = PathBuf::from(std::env::var("DAWWIN_TEST_DIR").expect("DAWWIN_TEST_DIR"));
        let d = Download { url: url(v), dest: dir.join(v.file), size: v.size, sha256: v.sha256.into() };
        let stop = AtomicBool::new(false);
        let first = download(&d, &stop, &mut |e| {
            if let Event::Progress { done } = e {
                if done > 20_000_000 {
                    stop.store(true, Ordering::SeqCst);
                }
            }
        })
        .unwrap();
        assert_eq!(first, Finished::Paused);
        let kept = downloaded(&d.dest);
        println!("paused with {kept} bytes kept");
        assert!(kept > 20_000_000);
        let started = Instant::now();
        let second = download(&d, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(second, Finished::Complete);
        println!("resumed and finished in {:.0} s", started.elapsed().as_secs_f32());
        assert_eq!(std::fs::metadata(&d.dest).unwrap().len(), v.size);
    }
}
