//! A stand-in for `dawwin-engine.exe`, to try how the app copes with an engine that goes wrong,
//! without a model or a GPU: `cargo build --example fake_engine`, then start a debug build of the
//! app with `DAWWIN_TEST_ENGINE` set to the .exe it made. It talks the engine's JSON lines.
//!
//! It finds a "Fake GPU". Every piece of audio reads "مرحبا بكم في الاجتماع". With
//! `FAKE_ENGINE_HANG_LOAD`, loading on the GPU never ends, as with a graphics driver that hangs
//! (with `--cpu` it loads at once). With `FAKE_ENGINE_HANG_TRANSCRIBE`, transcribing never ends.

use std::io::{BufRead, Write};

fn main() {
    let cpu = std::env::args().any(|a| a == "--cpu");
    let hang_load = std::env::var_os("FAKE_ENGINE_HANG_LOAD").is_some() && !cpu;
    let hang_transcribe = std::env::var_os("FAKE_ENGINE_HANG_TRANSCRIBE").is_some();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let hang = || loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    };
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(req) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let id = req["id"].clone();
        let reply = match req["cmd"].as_str() {
            Some("probe") => serde_json::json!({ "id": id, "ok": true, "cpu_only": cpu, "gpu": if cpu { None } else { Some("Fake GPU") }, "vram_total_gb": 8.0 }),
            Some("load") => {
                if hang_load {
                    eprintln!("loading on the fake GPU… for ever");
                    hang();
                }
                serde_json::json!({ "id": id, "ok": true, "device": if cpu { "cpu" } else { "gpu" }, "gpu": if cpu { None } else { Some("Fake GPU") }, "batch": 1 })
            }
            Some("transcribe") => {
                if hang_transcribe {
                    eprintln!("transcribing… for ever");
                    hang();
                }
                let n = req["ranges"].as_array().map_or(0, |r| r.len());
                serde_json::json!({ "id": id, "ok": true, "texts": vec!["مرحبا بكم في الاجتماع"; n], "stopped": vec![false; n] })
            }
            Some("quit") => {
                let _ = writeln!(out, "{}", serde_json::json!({ "id": id, "ok": true }));
                return;
            }
            _ => serde_json::json!({ "id": id, "ok": false, "error": "unknown command" }),
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            return;
        }
    }
}
