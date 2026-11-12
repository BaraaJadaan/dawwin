//! A stand-in for Dawwin's voice program, to try reading aloud without a voice model or a GPU:
//! `cargo build --example fake_voice`, then start a debug build of the app with
//! `DAWWIN_TEST_VOICE` set to the .exe it made. It answers as `dawwin-voice.exe` does, one JSON
//! line for each, and speaks a tone for each request instead of words: 220 Hz for Arabic and
//! 330 Hz for English, a little higher for a woman's voice, as long as the text needs.
//!
//! `FAKE_VOICE_DELAY_MS`: how long a request takes (300 ms). `FAKE_VOICE_CRASH_AFTER`: exit after
//! that many requests. `FAKE_VOICE_CRASH_TIMES` with `FAKE_VOICE_CRASH_FILE`: the first that many
//! processes exit on their first request (the file counts them). `FAKE_VOICE_LOG`: a file where
//! each request is written, one JSON a line.

use std::io::{BufRead, Write};
use std::time::Duration;

fn write_wav(path: &str, rate: u32, samples: &[f32]) -> std::io::Result<()> {
    let data = samples.len() * 2;
    let mut b = Vec::with_capacity(44 + data);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    for v in [16u32] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data as u32).to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b)
}

fn main() {
    let delay = std::env::var("FAKE_VOICE_DELAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let crash_after: Option<usize> = std::env::var("FAKE_VOICE_CRASH_AFTER").ok().and_then(|v| v.parse().ok());
    let log = std::env::var("FAKE_VOICE_LOG").ok().filter(|l| !l.is_empty());
    let rate = 24_000u32;
    let mut done = 0usize;
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines().map_while(|l| l.ok()) {
        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("not JSON: {e}");
                continue;
            }
        };
        if let Some(path) = &log {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{line}");
            }
        }
        let reply = match req["cmd"].as_str() {
            Some("load") => {
                eprintln!("loading {} on {}", req["model"], req["backend"]);
                std::thread::sleep(Duration::from_millis(400));
                serde_json::json!({ "ok": true, "device": if req["backend"] == "vulkan" { "Stand-in GPU" } else { "CPU" } })
            }
            Some("speak") => {
                std::thread::sleep(Duration::from_millis(delay));
                done += 1;
                if crash_after.is_some_and(|n| done > n) {
                    eprintln!("stand-in voice: crashing as asked");
                    std::process::exit(3);
                }
                if let (Some(times), Ok(file)) = (std::env::var("FAKE_VOICE_CRASH_TIMES").ok().and_then(|v| v.parse::<u32>().ok()), std::env::var("FAKE_VOICE_CRASH_FILE")) {
                    let count: u32 = std::fs::read_to_string(&file).ok().and_then(|c| c.trim().parse().ok()).unwrap_or(0);
                    if count < times {
                        let _ = std::fs::write(&file, (count + 1).to_string());
                        eprintln!("stand-in voice: crashing as asked ({})", count + 1);
                        std::process::exit(3);
                    }
                }
                let text = req["text"].as_str().unwrap_or("");
                let woman = req["voice_id"].as_str().unwrap_or("").contains('f');
                let base = if req["language"] == "ar" { 220.0 } else { 330.0 };
                let freq = if woman { base * 1.5 } else { base };
                let seconds = (text.chars().count() as f32 * 0.06).clamp(0.4, 20.0);
                let n = (seconds * rate as f32) as usize;
                let samples: Vec<f32> = (0..n)
                    .map(|i| {
                        let t = i as f32 / rate as f32;
                        let fade = (t / 0.02).min(1.0).min((seconds - t) / 0.02).max(0.0);
                        0.2 * fade * (2.0 * std::f32::consts::PI * freq * t).sin()
                    })
                    .collect();
                match req["out"].as_str().map(|p| write_wav(p, rate, &samples)) {
                    Some(Ok(())) => serde_json::json!({ "id": req["id"], "ok": true, "seconds": seconds }),
                    Some(Err(e)) => serde_json::json!({ "id": req["id"], "ok": false, "error": e.to_string() }),
                    None => serde_json::json!({ "id": req["id"], "ok": false, "error": "no out" }),
                }
            }
            _ => serde_json::json!({ "ok": false, "error": "unknown command" }),
        };
        let mut out = stdout.lock();
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
}
