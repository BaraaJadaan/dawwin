//! Trial for Dawwin 1.5.0 (issue 4): record the microphone ("me") and what the PC plays
//! ("them") as two tracks on one clock, into a WAV that is still readable after a kill.
//!
//!   record-trial devices
//!   record-trial record --out F.wav [--seconds N] [--them process|endpoint|none]
//!                       [--me default|none] [--keepalive] [--self-play F.wav] [--no-audio]
//!   record-trial play F.wav [--gain-db G]
//!   record-trial check F.wav        what the header says against what the file holds
//!   record-trial analyse F.wav      levels, and how the microphone hears the speakers
//!
//! "them" is process loopback in exclude mode on this process's own tree (everything the
//! PC plays but this program), or loopback of the default output device (`endpoint`),
//! optionally with a silent stream played to it so that packets keep coming.

mod timeline;
mod wav;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use timeline::{index_of, RateFit, Track, RATE};
use wasapi::*;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn qpc() -> u64 {
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut c, mut f) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    (c as i128 * 10_000_000 / f as i128) as u64
}

fn cpu_seconds() -> f64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut a, mut b, mut k, mut u) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u);
    }
    let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 1e7;
    t(k) + t(u)
}

fn fmt16k() -> WaveFormat {
    WaveFormat::new(32, 32, SampleType::Float, RATE as usize, 1, None)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    Mic,
    ProcessLoopback,
    EndpointLoopback,
}

struct Packet {
    track: usize,
    ts: u64,
    arrival: u64,
    discontinuity: bool,
    silent: bool,
    ts_error: bool,
    data: Vec<f32>,
}

enum Msg {
    Packet(Packet),
    Note(String),
}

fn open(src: Source, mic: &Option<String>) -> Res<(AudioClient, AudioCaptureClient, Handle, String)> {
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 2_000_000 };
    let (mut client, name) = match src {
        Source::Mic => {
            let e = DeviceEnumerator::new()?;
            let dev = match mic {
                None => e.get_default_device(Direction::Capture)?,
                Some(want) => {
                    let mut found = None;
                    for d in &e.get_device_collection(Direction::Capture)? {
                        let d = d?;
                        if d.get_friendlyname()?.contains(want.as_str()) {
                            found = Some(d);
                        }
                    }
                    found.ok_or("no such microphone")?
                }
            };
            (dev.get_iaudioclient()?, dev.get_friendlyname()?)
        }
        Source::EndpointLoopback => {
            let dev = DeviceEnumerator::new()?.get_default_device(Direction::Render)?;
            (dev.get_iaudioclient()?, format!("loopback of {}", dev.get_friendlyname()?))
        }
        Source::ProcessLoopback => (
            AudioClient::new_application_loopback_client(std::process::id(), false)?,
            "process loopback, excluding this program".to_string(),
        ),
    };
    client.initialize_client(&fmt16k(), Direction::Capture, mode, None)?;
    let h = client.set_get_eventhandle()?;
    let cc = client.get_audiocaptureclient()?;
    client.start_stream()?;
    Ok((client, cc, h, name))
}

fn default_id(dir: Direction) -> Option<String> {
    DeviceEnumerator::new().ok()?.get_default_device(dir).ok()?.get_id().ok()
}

/// One stream: open, read every packet with its timestamp, reopen when the device goes
/// away or (for a device stream) when the Windows default changes.
fn capture(src: Source, track: usize, mic: Option<String>, tx: Sender<Msg>, stop: Arc<AtomicBool>) {
    let _ = initialize_mta();
    let follow = match src {
        Source::Mic if mic.is_none() => Some(Direction::Capture),
        Source::Mic => None,
        Source::EndpointLoopback => Some(Direction::Render),
        Source::ProcessLoopback => None,
    };
    let mut buf = vec![0u8; 4 * RATE as usize];
    while !stop.load(Ordering::Relaxed) {
        let (client, cc, h, name) = match open(src, &mic) {
            Ok(x) => x,
            Err(e) => {
                let _ = tx.send(Msg::Note(format!("{src:?}: cannot open: {e}")));
                thread::sleep(Duration::from_millis(1000));
                continue;
            }
        };
        let _ = tx.send(Msg::Note(format!("{src:?}: recording from {name}")));
        let opened_default = follow.and_then(default_id);
        let mut last_check = Instant::now();
        let mut waits_without_data = 0u32;
        let reason = 'run: loop {
            if stop.load(Ordering::Relaxed) {
                break 'run None;
            }
            if h.wait_for_event(200).is_err() {
                waits_without_data += 1;
            }
            loop {
                match cc.get_next_packet_size() {
                    Ok(Some(n)) if n > 0 => {}
                    Ok(_) => break,
                    Err(e) => break 'run Some(format!("packet size: {e}")),
                }
                match cc.read_from_device(&mut buf) {
                    Ok((frames, info)) => {
                        if frames == 0 {
                            break;
                        }
                        waits_without_data = 0;
                        let data = buf[..frames as usize * 4]
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        let _ = tx.send(Msg::Packet(Packet {
                            track,
                            ts: info.timestamp,
                            arrival: qpc(),
                            discontinuity: info.flags.data_discontinuity,
                            silent: info.flags.silent,
                            ts_error: info.flags.timestamp_error,
                            data,
                        }));
                    }
                    Err(e) => break 'run Some(format!("read: {e}")),
                }
            }
            if last_check.elapsed() > Duration::from_secs(1) {
                last_check = Instant::now();
                if let Some(dir) = follow {
                    if default_id(dir) != opened_default {
                        break 'run Some("the Windows default device changed".into());
                    }
                }
                if waits_without_data == 10 {
                    let _ = tx.send(Msg::Note(format!("{src:?}: no packets for 2 s")));
                }
            }
        };
        let _ = client.stop_stream();
        match reason {
            None => break,
            Some(r) => {
                let _ = tx.send(Msg::Note(format!("{src:?}: reopening ({r})")));
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Play `samples` (16 kHz mono) on the default output device; with `None`, play silence
/// until `stop` (the keep-alive stream for endpoint loopback).
fn play(samples: Option<Vec<f32>>, gain: f32, stop: Arc<AtomicBool>) -> Res<()> {
    let _ = initialize_mta();
    let dev = DeviceEnumerator::new()?.get_default_device(Direction::Render)?;
    let mut client = dev.get_iaudioclient()?;
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 1_000_000 };
    client.initialize_client(&fmt16k(), Direction::Render, mode, None)?;
    let h = client.set_get_eventhandle()?;
    let rc = client.get_audiorenderclient()?;
    let mut pos = 0usize;
    let mut started = false;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let space = client.get_available_space_in_frames()? as usize;
        let bytes: Vec<u8> = match &samples {
            Some(s) => {
                let n = space.min(s.len() - pos);
                let b = s[pos..pos + n].iter().flat_map(|x| (x * gain).to_le_bytes()).collect();
                pos += n;
                b
            }
            None => vec![0u8; space * 4],
        };
        let silent = samples.is_none().then(|| BufferFlags { silent: true, ..BufferFlags::none() });
        rc.write_to_device(&bytes, silent)?;
        if !started {
            client.start_stream()?;
            started = true;
        }
        if let Some(s) = &samples {
            if pos >= s.len() {
                thread::sleep(Duration::from_millis(300));
                break;
            }
        }
        let _ = h.wait_for_event(1000);
    }
    client.stop_stream()?;
    Ok(())
}

fn db(rms: f64) -> f64 {
    20.0 * rms.max(1e-6).log10()
}

struct Arrival {
    max_ms: f64,
    sum_ms: f64,
    n: f64,
}

fn record(args: &Args) -> Res<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<Msg>();
    let t0 = qpc();
    let cpu0 = cpu_seconds();
    let wall0 = Instant::now();
    let mut sources = Vec::new();
    if args.me {
        sources.push((Source::Mic, 0));
    }
    match args.them.as_str() {
        "process" => sources.push((Source::ProcessLoopback, 1)),
        "endpoint" => sources.push((Source::EndpointLoopback, 1)),
        _ => {}
    }
    let mut threads = Vec::new();
    for (src, track) in sources {
        let (tx, stop, mic) = (tx.clone(), stop.clone(), args.mic.clone());
        threads.push(thread::spawn(move || capture(src, track, mic, tx, stop)));
    }
    if args.keepalive {
        let stop = stop.clone();
        thread::spawn(move || {
            if let Err(e) = play(None, 0.0, stop) {
                eprintln!("keep-alive: {e}");
            }
        });
    }
    if let Some(p) = &args.self_play {
        let (_, s, _) = wav::read(p)?;
        let stop = stop.clone();
        let gain = 10f32.powf(args.gain_db / 20.0);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            if let Err(e) = play(Some(s), gain, stop) {
                eprintln!("self-play: {e}");
            }
        });
    }
    drop(tx);

    let mut writer = match (&args.out, args.no_audio) {
        (Some(p), false) => Some(wav::Writer::create(p)?),
        _ => None,
    };
    let tol = (RATE / 100) as i64; // 10 ms
    let mut tracks = [Track::new(tol), Track::new(tol)];
    let mut fits = [RateFit::default(), RateFit::default()];
    let mut best_fit = [RateFit::default(), RateFit::default()];
    let mut delivered = [0u64; 2];
    let mut flags = [[0u64; 3]; 2];
    let mut arrival = [Arrival { max_ms: 0.0, sum_ms: 0.0, n: 0.0 }, Arrival { max_ms: 0.0, sum_ms: 0.0, n: 0.0 }];
    let mut levels: [(f64, u64); 2] = [(0.0, 0); 2];
    let mut second = Instant::now();
    let margin = RATE as u64 * 4 / 10; // what is older than 400 ms is final
    let end = args.seconds.map(|s| Duration::from_secs_f64(s));
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Msg::Packet(p)) => {
                let k = p.track;
                let at = index_of(p.ts, t0);
                let diff = at - tracks[k].end() as i64;
                if diff.abs() > tol && tracks[k].stats.packets > 0 {
                    eprintln!(
                        "[{:6.2}] {}: {} of {:.1} ms at {:.2} s (packet of {} frames, flags d{} s{} e{})",
                        wall0.elapsed().as_secs_f64(),
                        ["me", "them"][k],
                        if diff > 0 { "hole" } else { "overlap" },
                        diff.abs() as f64 / 16.0,
                        at as f64 / 16000.0,
                        p.data.len(),
                        p.discontinuity as u8,
                        p.silent as u8,
                        p.ts_error as u8
                    );
                    // the rate is measured over the longest run without a correction
                    if fits[k].len() > best_fit[k].len() {
                        best_fit[k] = fits[k].clone();
                    }
                    fits[k] = RateFit::default();
                }
                fits[k].add(delivered[k], p.ts);
                delivered[k] += p.data.len() as u64;
                flags[k][0] += p.discontinuity as u64;
                flags[k][1] += p.silent as u64;
                flags[k][2] += p.ts_error as u64;
                let late = (p.arrival as f64 - p.ts as f64) / 1e4;
                arrival[k].max_ms = arrival[k].max_ms.max(late);
                arrival[k].sum_ms += late;
                arrival[k].n += 1.0;
                tracks[k].place(at, &p.data);
            }
            Ok(Msg::Note(n)) => eprintln!("[{:6.2}] {n}", wall0.elapsed().as_secs_f64()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let horizon = (index_of(qpc(), t0).max(0) as u64).saturating_sub(margin);
        if horizon >= tracks[0].end().min(tracks[1].end()) + RATE as u64 / 10 || second.elapsed() >= Duration::from_secs(1) {
            let l = tracks[0].take(horizon);
            let r = tracks[1].take(horizon);
            for (k, s) in [&l, &r].into_iter().enumerate() {
                levels[k].0 += s.iter().map(|x| (*x as f64).powi(2)).sum::<f64>();
                levels[k].1 += s.len() as u64;
            }
            if let Some(w) = writer.as_mut() {
                w.append(&l, &r)?;
            }
        }
        if second.elapsed() >= Duration::from_secs(1) {
            second = Instant::now();
            if let Some(w) = writer.as_mut() {
                w.checkpoint()?;
            }
            let lv = |k: usize| db((levels[k].0 / levels[k].1.max(1) as f64).sqrt());
            eprintln!(
                "[{:6.2}] me {:6.1} dBFS   them {:6.1} dBFS",
                wall0.elapsed().as_secs_f64(),
                lv(0),
                lv(1)
            );
            levels = [(0.0, 0); 2];
        }
        if end.is_some_and(|e| wall0.elapsed() >= e) && !stop.load(Ordering::Relaxed) {
            stop.store(true, Ordering::Relaxed);
        }
    }
    for t in threads {
        let _ = t.join();
    }
    let horizon = tracks[0].end().max(tracks[1].end());
    let (l, r) = (tracks[0].take(horizon), tracks[1].take(horizon));
    if let Some(w) = writer.as_mut() {
        w.append(&l, &r)?;
        w.checkpoint()?;
    }
    for k in 0..2 {
        if fits[k].len() > best_fit[k].len() {
            best_fit[k] = fits[k].clone();
        }
    }
    let fits = best_fit;
    let wall = wall0.elapsed().as_secs_f64();
    let cpu = cpu_seconds() - cpu0;
    println!("seconds {:.1}, processor {:.2} % of one core ({:.3} s)", wall, cpu / wall * 100.0, cpu);
    for (k, name) in ["me", "them"].iter().enumerate() {
        let s = &tracks[k].stats;
        let a = &arrival[k];
        println!(
            "{name}: {} packets, {:.2} s delivered, rate {} ppm against the clock (over {:.1} s); \
             holes {} ({:.0} ms), overlaps {} ({:.0} ms cut), silence written for lack of data {:.0} ms, \
             largest jitter absorbed {:.1} ms; flags: discontinuity {}, silent {}, timestamp error {}; \
             packets reach the program {:.1} ms after capture on average, {:.1} ms at most",
            s.packets,
            delivered[k] as f64 / RATE as f64,
            fits[k].ppm().map(|p| format!("{p:+.1}")).unwrap_or("n/a".into()),
            fits[k].seconds(),
            s.gaps,
            s.gap_frames as f64 / 16.0,
            s.overlaps,
            s.dropped_frames as f64 / 16.0,
            s.filled_frames as f64 / 16.0,
            s.max_jitter as f64 / 16.0,
            flags[k][0],
            flags[k][1],
            flags[k][2],
            a.sum_ms / a.n.max(1.0),
            a.max_ms,
        );
    }
    if let (Some(a), Some(b)) = (fits[0].ppm(), fits[1].ppm()) {
        println!(
            "drift between the tracks: {:+.1} ppm, {:.0} ms an hour if they were simply joined",
            a - b,
            (a - b).abs() * 3600.0 / 1000.0
        );
    }
    Ok(())
}

/// Levels of both tracks, and the delay and strength of "them" inside "me" (the speakers
/// heard by the microphone), over windows at the start and the end.
fn analyse(path: &PathBuf) -> Res<()> {
    let (ch, me, them) = wav::read(path)?;
    if ch != 2 {
        return Err("needs a two-channel file".into());
    }
    let rms = |s: &[f32]| (s.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / s.len().max(1) as f64).sqrt();
    println!("length {:.2} s; me {:.1} dBFS, them {:.1} dBFS", me.len() as f64 / 16000.0, db(rms(&me)), db(rms(&them)));
    let win = 2 * RATE as usize;
    let starts: Vec<usize> = (0..me.len().saturating_sub(win)).step_by(RATE as usize).collect();
    for s in starts {
        let label = format!("{:5.1} s", s as f64 / RATE as f64);
        let lo = -(RATE as i64) / 10; // "me" up to 100 ms before "them" ..
        let hi = RATE as i64 * 6 / 10; // .. to 600 ms after
        let mut best = (0i64, 0f64, 0f64);
        if them[s..s + win].iter().all(|x| *x == 0.0) {
            println!("{label}: \"them\" is silent");
            continue;
        }
        for lag in lo..=hi {
            let (mut xy, mut xx, mut yy) = (0f64, 0f64, 0f64);
            for i in (s..s + win).step_by(2) {
                let j = i as i64 + lag;
                if j < 0 || j as usize >= me.len() {
                    continue;
                }
                let x = them[i] as f64;
                let y = me[j as usize] as f64;
                xy += x * y;
                xx += x * x;
                yy += y * y;
            }
            let c = xy / (xx * yy).sqrt().max(1e-12);
            if c > best.1 {
                best = (lag, c, xy / xx.max(1e-12));
            }
        }
        println!(
            "{label}: \"them\" is heard in \"me\" {:.1} ms later, correlation {:.2}, at {:.1} dB of its loopback level",
            best.0 as f64 / 16.0,
            best.1,
            db(best.2.abs())
        );
    }
    Ok(())
}

fn devices() -> Res<()> {
    let _ = initialize_mta();
    let e = DeviceEnumerator::new()?;
    for dir in [Direction::Capture, Direction::Render] {
        let def = e.get_default_device(dir)?.get_id()?;
        for d in &e.get_device_collection(dir)? {
            let d = d?;
            let mark = if d.get_id()? == def { "*" } else { " " };
            println!("{dir:?} {mark} {}", d.get_friendlyname()?);
        }
    }
    Ok(())
}

#[derive(Default)]
struct Args {
    cmd: String,
    file: Option<PathBuf>,
    out: Option<PathBuf>,
    seconds: Option<f64>,
    them: String,
    me: bool,
    keepalive: bool,
    self_play: Option<PathBuf>,
    no_audio: bool,
    gain_db: f32,
    mic: Option<String>,
    delay_ms: u64,
    reference: Option<PathBuf>,
    rest: Vec<PathBuf>,
}

/// Echo cancellation of a "me" track (raw f32, 16 kHz mono) with the "them" track as the
/// sound the speakers played: `aec me.f32 them.f32 out.f32`.
fn aec(files: &[PathBuf]) -> Res<()> {
    use sonora::config::EchoCanceller;
    use sonora::{AudioProcessing, Config, StreamConfig};
    let read = |p: &PathBuf| -> Res<Vec<f32>> {
        Ok(std::fs::read(p)?.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
    };
    let (me, them) = (read(&files[0])?, read(&files[1])?);
    let sc = StreamConfig::new(RATE as u32, 1);
    let mut apm = AudioProcessing::builder()
        .config(Config { echo_canceller: Some(EchoCanceller::default()), ..Default::default() })
        .capture_config(sc)
        .render_config(sc)
        .build();
    let n = 160;
    let mut out = vec![0f32; me.len()];
    let mut r_out = vec![0f32; n];
    let t = Instant::now();
    let cpu0 = cpu_seconds();
    for k in 0..me.len() / n {
        let r = &them[(k * n).min(them.len())..((k + 1) * n).min(them.len())];
        let mut rf = r.to_vec();
        rf.resize(n, 0.0);
        apm.process_render_f32(&[&rf], &mut [&mut r_out])?;
        let (a, b) = (k * n, (k + 1) * n);
        let mut o = vec![0f32; n];
        apm.process_capture_f32(&[&me[a..b]], &mut [&mut o])?;
        out[a..b].copy_from_slice(&o);
    }
    let secs = me.len() as f64 / RATE as f64;
    eprintln!("aec: {:.1} s of sound in {:.3} s, {:.2} % of one core", secs, t.elapsed().as_secs_f64(), (cpu_seconds() - cpu0) / secs * 100.0);
    std::fs::write(&files[2], out.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;
    Ok(())
}

/// Where the played file sits in the "them" track, window by window: if the offset stays,
/// the track keeps the file's own timing; a jump is time the track gained or lost.
fn locate(path: &PathBuf, reference: &PathBuf) -> Res<()> {
    let (_, _, them) = wav::read(path)?;
    let (_, src, _) = wav::read(reference)?;
    let win = RATE as usize / 2;
    let mut s = 0;
    let mut prev: Option<i64> = None;
    while s + win <= them.len() {
        let w = &them[s..s + win];
        let ww: f64 = w.iter().map(|x| (*x as f64).powi(2)).sum();
        if ww > 1e-9 {
            let mut best = (0usize, 0f64);
            let all = 0..src.len().saturating_sub(win);
            let range = match prev {
                None => all,
                Some(p) => {
                    let c = s as i64 - p;
                    (c - 4800).max(0) as usize..((c + 4800).max(0) as usize).min(all.end)
                }
            };
            for off in range {
                let r = &src[off..off + win];
                let (mut xy, mut rr) = (0f64, 0f64);
                for i in (0..win).step_by(4) {
                    xy += w[i] as f64 * r[i] as f64;
                    rr += r[i] as f64 * r[i] as f64;
                }
                let c = xy / (ww / 4.0 * rr).sqrt().max(1e-12);
                if c > best.1 {
                    best = (off, c);
                }
            }
            if best.1 > 0.5 {
                prev = Some(s as i64 - best.0 as i64);
                println!(
                    "{:6.2} s of the track is {:6.2} s of the file: offset {:8.2} ms (correlation {:.2})",
                    s as f64 / 16000.0,
                    best.0 as f64 / 16000.0,
                    (s as f64 - best.0 as f64) / 16.0,
                    best.1
                );
            }
        }
        s += win;
    }
    Ok(())
}

fn parse() -> Args {
    let mut a = Args { them: "process".into(), me: true, gain_db: -20.0, ..Default::default() };
    let mut it = std::env::args().skip(1);
    a.cmd = it.next().unwrap_or_default();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--out" => a.out = it.next().map(PathBuf::from),
            "--seconds" => a.seconds = it.next().and_then(|s| s.parse().ok()),
            "--them" => a.them = it.next().unwrap_or_default(),
            "--me" => a.me = it.next().as_deref() != Some("none"),
            "--keepalive" => a.keepalive = true,
            "--mic" => a.mic = it.next(),
            "--delay-ms" => a.delay_ms = it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            "--self-play" => a.self_play = it.next().map(PathBuf::from),
            "--no-audio" => a.no_audio = true,
            "--reference" => a.reference = it.next().map(PathBuf::from),
            "--gain-db" => a.gain_db = it.next().and_then(|s| s.parse().ok()).unwrap_or(-20.0),
            _ => {
                a.rest.push(PathBuf::from(&x));
                a.file = Some(PathBuf::from(x));
            }
        }
    }
    a
}

fn main() -> Res<()> {
    let a = parse();
    match a.cmd.as_str() {
        "devices" => devices(),
        "record" => record(&a),
        "play" => {
            let (_, s, _) = wav::read(a.file.as_ref().ok_or("which file?")?)?;
            thread::sleep(Duration::from_millis(a.delay_ms));
            play(Some(s), 10f32.powf(a.gain_db / 20.0), Arc::new(AtomicBool::new(false)))
        }
        "check" => {
            let p = a.file.ok_or("which file?")?;
            let c = wav::check(&p)?;
            println!(
                "header says {:.2} s, the file holds {:.2} s",
                c.header_frames as f64 / 16000.0,
                c.file_frames as f64 / 16000.0
            );
            Ok(())
        }
        "repair" => {
            let n = wav::repair(&a.file.ok_or("which file?")?)?;
            println!("repaired: {:.2} s", n as f64 / 16000.0);
            Ok(())
        }
        "analyse" => analyse(a.file.as_ref().ok_or("which file?")?),
        "aec" => aec(&a.rest),
        "locate" => locate(a.file.as_ref().ok_or("which file?")?, a.reference.as_ref().ok_or("--reference")?),
        _ => Err("record | play | check | repair | analyse | locate | devices".into()),
    }
}
