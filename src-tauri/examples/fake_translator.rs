//! A stand-in for llama.cpp's server, to try Dawwin's translation without a model or a GPU:
//! `cargo build --example fake_translator`, then start a debug build of the app with
//! `DAWWIN_TEST_TRANSLATOR` set to the .exe it made. It answers as llama-server does: `/health`
//! (loading for a moment first), and `/completion`, with the line in brackets after the target.
//!
//! `FAKE_TRANSLATOR_DELAY_MS`: how long a line takes (400 ms). `FAKE_TRANSLATOR_CRASH_AFTER`: exit
//! after that many lines, unless started with `--device none`, as a GPU driver might.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn answer(s: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

fn main() {
    if std::env::args().any(|a| a == "--list-devices") {
        println!("Available devices:\n  Vulkan0: Fake GPU (8000 MiB, 7000 MiB free)");
        return;
    }
    let port = arg("--port").unwrap_or_else(|| "8080".into());
    let key = arg("--api-key").unwrap_or_default();
    let device = arg("--device").unwrap_or_default();
    let cpu_only = device == "none";
    let delay = std::env::var("FAKE_TRANSLATOR_DELAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(400);
    let crash_after: Option<usize> = std::env::var("FAKE_TRANSLATOR_CRASH_AFTER").ok().and_then(|v| v.parse().ok()).filter(|_| !cpu_only);
    eprintln!("device: {device}");
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("port");
    let started = Instant::now();
    let mut lines = 0usize;
    for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let mut reader = BufReader::new(s.try_clone().unwrap());
        let mut request = String::new();
        if reader.read_line(&mut request).is_err() {
            continue;
        }
        let (mut length, mut auth) = (0usize, String::new());
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                break;
            }
            let lower = h.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
            if lower.starts_with("authorization:") {
                auth = h["authorization:".len()..].trim().to_string();
            }
        }
        let mut body = vec![0u8; length];
        let _ = reader.read_exact(&mut body);
        if request.starts_with("GET /health") {
            if started.elapsed() < Duration::from_millis(1500) {
                answer(&mut s, "503 Service Unavailable", r#"{"error":{"message":"Loading model"}}"#);
            } else {
                answer(&mut s, "200 OK", r#"{"status":"ok"}"#);
            }
            continue;
        }
        if !request.starts_with("POST /completion") {
            answer(&mut s, "404 Not Found", "{}");
            continue;
        }
        if auth != format!("Bearer {key}") {
            answer(&mut s, "401 Unauthorized", r#"{"error":{"message":"Invalid API Key"}}"#);
            continue;
        }
        let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
        let prompt = req["prompt"].as_str().unwrap_or_default();
        // "Translate this from Arabic to English:\nArabic: <line>\nEnglish:"
        let parts: Vec<&str> = prompt.split('\n').collect();
        let text = parts.get(1).and_then(|l| l.split_once(": ")).map_or("", |(_, t)| t);
        let target = parts.get(2).map_or("", |l| l.trim_end_matches(':'));
        std::thread::sleep(Duration::from_millis(delay));
        lines += 1;
        if crash_after.is_some_and(|n| lines > n) {
            eprintln!("fake GPU driver crash");
            std::process::exit(3);
        }
        let reply = serde_json::json!({ "content": format!(" [{target}] {text}"), "stop_type": "eos", "tokens_predicted": text.len() / 3 });
        answer(&mut s, "200 OK", &reply.to_string());
    }
}
