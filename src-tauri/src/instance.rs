//! One Dawwin per data folder. A second start would delete the first one's audio in the cache,
//! resume its download into the same file and take the GPU from its job; instead it brings the
//! first window forward and quits. The guard is keyed on the data folder, not on the app, so a
//! test run with another `DAWWIN_HOME` lives beside the installed Dawwin.

use std::path::Path;

/// Held by the first Dawwin of a data folder for as long as it runs.
#[derive(Clone, Copy, Debug)]
pub struct Guard {
    /// The event a second start sets to say "come forward"
    #[cfg_attr(not(windows), allow(dead_code))]
    show: usize,
}

pub enum Start {
    /// This is the only Dawwin of its data folder
    First(Guard),
    /// Another one runs on the same data folder, and was told to come forward
    Second,
}

/// The objects' name: the same for every way of writing the same folder.
fn name(data: &Path) -> String {
    let key = data.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    // FNV-1a: stable across runs and versions, unlike the standard library's hasher
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    format!("Local\\Dawwin-{hash:016x}")
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    pub type Handle = *mut c_void;
    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateMutexW(attributes: *mut c_void, initial_owner: i32, name: *const u16) -> Handle;
        pub fn CreateEventW(attributes: *mut c_void, manual_reset: i32, initial: i32, name: *const u16) -> Handle;
        pub fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> Handle;
        pub fn SetEvent(event: Handle) -> i32;
        pub fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
        pub fn CloseHandle(handle: Handle) -> i32;
        pub fn GetLastError() -> u32;
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn AllowSetForegroundWindow(process: u32) -> i32;
    }
    pub const ERROR_ALREADY_EXISTS: u32 = 183;
    pub const EVENT_MODIFY_STATE: u32 = 0x0002;
    pub const INFINITE: u32 = u32::MAX;
    pub const ASFW_ANY: u32 = u32::MAX;

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
}

/// Take the data folder, or find that another Dawwin has it and tell that one to come forward.
#[cfg(windows)]
pub fn claim(data: &Path) -> Start {
    use sys::*;
    let base = name(data);
    let (mutex_name, event_name) = (wide(&format!("{base}-running")), wide(&format!("{base}-show")));
    // SAFETY: plain calls with NUL-terminated names that outlive them; the handles are only
    // closed here or kept for the life of the process.
    unsafe {
        let mutex = CreateMutexW(std::ptr::null_mut(), 0, mutex_name.as_ptr());
        if mutex.is_null() {
            // no guard to be had: run, as versions before it did
            return Start::First(Guard { show: 0 });
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            // the one that runs may take the foreground: the user started this one
            AllowSetForegroundWindow(ASFW_ANY);
            // it makes its event right after its mutex: give it a moment if it is starting too
            for _ in 0..40 {
                let event = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
                if !event.is_null() {
                    SetEvent(event);
                    CloseHandle(event);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            return Start::Second;
        }
        // the mutex stays open, and so held, until this process ends
        let show = CreateEventW(std::ptr::null_mut(), 0, 0, event_name.as_ptr());
        Start::First(Guard { show: show as usize })
    }
}

#[cfg(not(windows))]
pub fn claim(_data: &Path) -> Start {
    Start::First(Guard { show: 0 })
}

/// Call `come_forward` each time Dawwin is started again on the same data folder.
#[cfg(windows)]
pub fn on_second_start(guard: Guard, come_forward: impl Fn() + Send + 'static) {
    if guard.show == 0 {
        return;
    }
    std::thread::Builder::new()
        .name("second-start".into())
        .spawn(move || loop {
            // SAFETY: the event handle stays open for the life of the process.
            let waited = unsafe { sys::WaitForSingleObject(guard.show as sys::Handle, sys::INFINITE) };
            if waited != 0 {
                return;
            }
            come_forward();
        })
        .ok();
}

#[cfg(not(windows))]
pub fn on_second_start(_guard: Guard, _come_forward: impl Fn() + Send + 'static) {}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn the_same_folder_however_written_is_the_same_name() {
        assert_eq!(name(Path::new("C:\\Users\\a\\AppData\\Local\\Dawwin")), name(Path::new("c:/users/a/appdata/local/dawwin/")));
        assert_ne!(name(Path::new("C:\\Users\\a\\AppData\\Local\\Dawwin")), name(Path::new("D:\\test-home")));
    }

    #[test]
    fn a_second_start_on_the_same_folder_brings_the_first_forward() {
        let folder = std::env::temp_dir().join(format!("dawwin-instance-{}", std::process::id()));
        let Start::First(guard) = claim(&folder) else { panic!("nothing else holds this folder") };
        let called = Arc::new(AtomicUsize::new(0));
        let c = called.clone();
        on_second_start(guard, move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        assert!(matches!(claim(&folder), Start::Second), "the same data folder");
        assert!(matches!(claim(&folder.join("another")), Start::First(_)), "another data folder (DAWWIN_HOME) runs beside it");
        for _ in 0..40 {
            if called.load(Ordering::SeqCst) == 1 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("the first one was not told to come forward");
    }
}
