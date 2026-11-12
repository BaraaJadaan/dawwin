//! A stand-in for llama.cpp's server, to try Dawwin's translation without a model or a GPU:
//! `cargo build --example fake_translator`, then start a debug build of the app with
//! `DAWWIN_TEST_TRANSLATOR` set to the .exe it made. It answers as llama-server does: `/health`
//! (loading for a moment first), and `/completion`, with the line in brackets after the target.
//! It also writes minutes, as a minutes model would through `/v1/chat/completions` (streamed or
//! not): a line that says "decided"/"agreed" (or "اتفقنا"/"قررنا") is a decision, one with " will "
//! (or "رح "/"سوف") an action of its speaker, due after "by" (or "قبل"), one that ends in "?" an open
//! question, every tenth line a topic, and a recording with none of the first two a lecture. From
//! notes on parts it keeps the parts' items. Each request's task and the context the server was
//! started with go to its log.
//!
//! `FAKE_TRANSLATOR_DELAY_MS`: how long a line takes (400 ms). `FAKE_MINUTES_DELAY_MS`: how long each
//! piece of a minutes answer takes (60 ms, about 30 pieces). `FAKE_TRANSLATOR_CRASH_AFTER`: exit
//! after that many lines or minutes requests, unless started with `--device none`, as a GPU driver might.

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

/// The minutes for a request: the lines of a transcript, or the items of notes, turned into JSON.
fn minutes(user: &str) -> serde_json::Value {
    use serde_json::json;
    let arabic = user.contains(" in Arabic");
    let (mut topics, mut decisions, mut actions, mut questions) = (vec![], vec![], vec![], vec![]);
    let mut count = 0;
    for line in user.lines() {
        // notes on parts: "Decision [3, 4]: text", "Action [5]: who: X; what: Y; due: Z"
        let item = |prefix: &str| -> Option<(Vec<u64>, String)> {
            let rest = line.strip_prefix(prefix)?;
            let (nums, text) = rest.split_once("]: ")?;
            let nums = nums.trim_start_matches('[').split(',').filter_map(|n| n.trim().parse().ok()).collect();
            Some((nums, text.to_string()))
        };
        if let Some((n, text)) = item("Topic ") {
            topics.push(json!({ "title": text, "line": n.first().copied().unwrap_or(1) }));
            continue;
        }
        if let Some((n, text)) = item("Decision ") {
            decisions.push(json!({ "text": text, "lines": n }));
            continue;
        }
        if let Some((n, text)) = item("Open question ") {
            questions.push(json!({ "text": text, "lines": n }));
            continue;
        }
        if let Some((n, text)) = item("Action ") {
            let field = |k: &str| text.split("; ").find_map(|p| p.strip_prefix(k)).unwrap_or("").trim_matches('-').trim().to_string();
            actions.push(json!({ "who": field("who: "), "what": field("what: "), "due": field("due: "), "lines": n }));
            continue;
        }
        // a transcript line: "[12] 03:41 Sara: text" or "[12] 03:41 text"
        let Some(rest) = line.strip_prefix('[') else { continue };
        let Some((n, rest)) = rest.split_once("] ") else { continue };
        let Ok(n) = n.parse::<u64>() else { continue };
        let (time, said) = rest.split_once(' ').unwrap_or((rest, ""));
        let (who, text) = match said.split_once(": ") {
            Some((w, t)) if w.split_whitespace().count() <= 3 => (w.to_string(), t.to_string()),
            _ => (String::new(), said.to_string()),
        };
        let lower = text.to_lowercase();
        if count % 10 == 0 {
            topics.push(json!({ "title": format!("{} {time}", if arabic { "موضوع عند" } else { "Topic at" }), "line": n }));
        }
        count += 1;
        if ["decided", "agreed", "اتفقنا", "قررنا"].iter().any(|k| lower.contains(k)) {
            decisions.push(json!({ "text": text, "lines": [n] }));
        }
        if [" will ", "رح ", "سوف"].iter().any(|k| lower.contains(k)) {
            let due = ["by ", "قبل "]
                .iter()
                .find_map(|k| lower.split_once(k).map(|(_, d)| d.trim_end_matches(['.', '؟', '?', '!']).to_string()))
                .unwrap_or_default();
            actions.push(json!({ "who": who, "what": text, "due": due, "lines": [n] }));
        }
        if text.trim_end().ends_with('?') || text.trim_end().ends_with('؟') {
            questions.push(json!({ "text": text, "lines": [n] }));
        }
    }
    let meeting = !decisions.is_empty() || !actions.is_empty() || user.contains("), meeting:");
    let summary = if arabic {
        format!("محضر تجريبي: {} قرارات و{} مهام.", decisions.len(), actions.len())
    } else {
        format!("Stand-in minutes: {} decisions and {} actions.", decisions.len(), actions.len())
    };
    json!({
        "kind": if meeting { "meeting" } else { "lecture" },
        "topics": topics,
        "decisions": if meeting { decisions } else { vec![] },
        "actions": if meeting { actions } else { vec![] },
        "open_questions": questions,
        "summary": summary,
    })
}

/// The minutes streamed as llama-server streams them: the prompt read, then the answer in pieces.
fn stream_minutes(s: &mut TcpStream, answer: &str, piece_ms: u64) -> std::io::Result<()> {
    write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n")?;
    let read = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": null }], "prompt_progress": { "total": 100, "processed": 100 } });
    write!(s, "data: {read}\n\n")?;
    let chars: Vec<char> = answer.chars().collect();
    let size = (chars.len() / 30).max(8);
    for piece in chars.chunks(size) {
        std::thread::sleep(Duration::from_millis(piece_ms));
        let text: String = piece.iter().collect();
        write!(s, "data: {}\n\n", serde_json::json!({ "choices": [{ "delta": { "content": text }, "finish_reason": null }] }))?;
        s.flush()?;
    }
    let end = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }], "timings": { "prompt_n": 100, "predicted_n": chars.len() / size + 1 } });
    write!(s, "data: {end}\n\ndata: [DONE]\n\n")?;
    s.flush()
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
    let piece_ms = std::env::var("FAKE_MINUTES_DELAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    eprintln!("device: {device}");
    eprintln!("context: {}", arg("--ctx-size").unwrap_or_default());
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
        let chat = request.starts_with("POST /v1/chat/completions");
        if !request.starts_with("POST /completion") && !chat {
            answer(&mut s, "404 Not Found", "{}");
            continue;
        }
        // started without a key (as the minutes benchmark starts llama-server), it asks for none
        if !key.is_empty() && auth != format!("Bearer {key}") {
            answer(&mut s, "401 Unauthorized", r#"{"error":{"message":"Invalid API Key"}}"#);
            continue;
        }
        if chat {
            let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let user = req["messages"].as_array().and_then(|m| m.last()).and_then(|m| m["content"].as_str()).unwrap_or_default().to_string();
            let schema_ok = req["response_format"]["json_schema"]["schema"]["properties"]["kind"].is_object();
            let task = user.lines().rev().find(|l| l.starts_with("Write ")).unwrap_or("").to_string();
            eprintln!("minutes request: {task} (schema: {schema_ok})");
            lines += 1;
            if crash_after.is_some_and(|n| lines > n) {
                eprintln!("fake GPU driver crash");
                std::process::exit(3);
            }
            let text = minutes(&user).to_string();
            if req["stream"].as_bool() == Some(true) {
                let _ = stream_minutes(&mut s, &text, piece_ms);
            } else {
                let reply = serde_json::json!({ "choices": [{ "message": { "content": text }, "finish_reason": "stop" }] });
                answer(&mut s, "200 OK", &reply.to_string());
            }
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
