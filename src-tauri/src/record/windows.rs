//! Windows capture through WASAPI. "Me": shared-mode capture of the chosen microphone, or of the
//! Windows default, followed when it changes. "Them": process loopback in exclude mode on Dawwin's
//! own process tree (everything the PC plays but Dawwin, whatever the device); where that cannot
//! be opened, loopback of the default output device with a silent stream played to it so that
//! packets keep coming, opened again when the device goes or the default changes. Every stream
//! asks Windows for 48 kHz float, the PC's sound in stereo: that is kept for listening, and the
//! speech engine's 16 kHz mono is made from it here (`resample`). Measured in `tools/record-trial`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use wasapi::{AudioCaptureClient, AudioClient, BufferFlags, DeviceEnumerator, Direction, Handle, SampleType, StreamMode, WasapiError, WaveFormat};

use super::resample::{self, Decimator, HIFI};
use super::source::{Event, Factory};
use super::{Track, SR};

const E_ACCESSDENIED: u32 = 0x8007_0005;

/// What a capture stream asks for: 48 kHz float, two channels for the PC's sound, one for the microphone.
fn format(track: Track) -> WaveFormat {
    WaveFormat::new(32, 32, SampleType::Float, HIFI, channels(track), None)
}

fn channels(track: Track) -> usize {
    if track == Track::Them { 2 } else { 1 }
}

/// What the silent stream plays: anything will do, Windows converts it.
fn silence_format() -> WaveFormat {
    WaveFormat::new(32, 32, SampleType::Float, SR, 1, None)
}

/// Why a stream cannot open, for the user: Windows' privacy settings, no device, or something else.
fn problem(e: &WasapiError) -> &'static str {
    match e {
        WasapiError::Windows(w) if w.code().0 as u32 == E_ACCESSDENIED => "mic-blocked",
        WasapiError::Windows(w) if matches!(w.code().0 as u32, 0x8007_0490 | 0x8889_0004) => "no-device",
        _ => "failed",
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Microphone {
    pub id: String,
    pub name: String,
    pub default: bool,
}

/// The microphones Windows has, the default marked.
pub fn microphones() -> Vec<Microphone> {
    let _ = wasapi::initialize_mta();
    let Ok(e) = DeviceEnumerator::new() else { return Vec::new() };
    let default = e.get_default_device(Direction::Capture).ok().and_then(|d| d.get_id().ok());
    let Ok(all) = e.get_device_collection(Direction::Capture) else { return Vec::new() };
    (&all)
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let id = d.get_id().ok()?;
            Some(Microphone { default: default.as_deref() == Some(&id), name: d.get_friendlyname().unwrap_or_else(|_| id.clone()), id })
        })
        .collect()
}

fn default_id(dir: Direction) -> Option<String> {
    DeviceEnumerator::new().ok()?.get_default_device(dir).ok()?.get_id().ok()
}

struct Open {
    client: AudioClient,
    capture: AudioCaptureClient,
    event: Handle,
    name: String,
    mode: Option<String>,
    /// The default device it follows: it opens again when that changes
    follows: Option<(Direction, Option<String>)>,
    /// The silent stream that keeps an output device's loopback going
    keepalive: Option<Arc<AtomicBool>>,
}

fn open(track: Track, device: &Option<String>) -> Result<Open, WasapiError> {
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 2_000_000 };
    let start = |mut client: AudioClient, name: String, mode_name: Option<String>, follows| -> Result<Open, WasapiError> {
        client.initialize_client(&format(track), Direction::Capture, mode, None)?;
        let event = client.set_get_eventhandle()?;
        let capture = client.get_audiocaptureclient()?;
        client.start_stream()?;
        Ok(Open { client, capture, event, name, mode: mode_name, follows, keepalive: None })
    };
    let e = DeviceEnumerator::new()?;
    match track {
        Track::Me => {
            // the chosen microphone while it is there; the Windows default otherwise
            let chosen = device.as_ref().and_then(|id| e.get_device(id).ok());
            let follows = chosen.is_none().then(|| (Direction::Capture, default_id(Direction::Capture)));
            let dev = match chosen {
                Some(d) => d,
                None => e.get_default_device(Direction::Capture)?,
            };
            let name = dev.get_friendlyname().unwrap_or_default();
            start(dev.get_iaudioclient()?, name, None, follows)
        }
        Track::Them => {
            if let Ok(client) = AudioClient::new_application_loopback_client(std::process::id(), false) {
                if let Ok(o) = start(client, String::new(), Some("process".into()), None) {
                    return Ok(o);
                }
            }
            let dev = e.get_default_device(Direction::Render)?;
            let name = dev.get_friendlyname().unwrap_or_default();
            let mut o = start(dev.get_iaudioclient()?, name, Some("device".into()), Some((Direction::Render, default_id(Direction::Render))))?;
            let stop = Arc::new(AtomicBool::new(false));
            let s = stop.clone();
            std::thread::spawn(move || {
                let _ = play_silence(s);
            });
            o.keepalive = Some(stop);
            Ok(o)
        }
    }
}

/// Silence played to the default output device, so that its loopback sends packets while
/// nothing else plays (what OBS does).
fn play_silence(stop: Arc<AtomicBool>) -> Result<(), WasapiError> {
    let _ = wasapi::initialize_mta();
    let dev = DeviceEnumerator::new()?.get_default_device(Direction::Render)?;
    let mut client = dev.get_iaudioclient()?;
    client.initialize_client(&silence_format(), Direction::Render, StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 1_000_000 }, None)?;
    let event = client.set_get_eventhandle()?;
    let render = client.get_audiorenderclient()?;
    let mut started = false;
    while !stop.load(Ordering::Relaxed) {
        let space = client.get_available_space_in_frames()? as usize;
        render.write_to_device(&vec![0u8; space * 4], Some(BufferFlags { silent: true, ..BufferFlags::none() }))?;
        if !started {
            client.start_stream()?;
            started = true;
        }
        let _ = event.wait_for_event(500);
    }
    let _ = client.stop_stream();
    Ok(())
}

pub struct Wasapi;

impl Factory for Wasapi {
    fn spawn(&self, track: Track, device: Option<String>, tx: Sender<Event>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        std::thread::spawn(move || capture(track, device, tx, stop))
    }
    fn hifi(&self) -> bool {
        true
    }
}

/// One stream: open, read every packet with its timestamp, open again when the device goes away
/// or the default it follows changes. The recorder never waits for it: a gap is filled there.
fn capture(track: Track, device: Option<String>, tx: Sender<Event>, stop: Arc<AtomicBool>) {
    let _ = wasapi::initialize_mta();
    let ch = channels(track);
    // a second of sound, more than any packet holds
    let mut buf = vec![0u8; 4 * ch * HIFI];
    let mut told: Option<String> = None;
    while !stop.load(Ordering::Relaxed) {
        let o = match open(track, &device) {
            Ok(o) => o,
            Err(e) => {
                let code = problem(&e).to_string();
                if told.as_deref() != Some(&code) {
                    let _ = tx.send(Event::Problem { track, code: code.clone(), detail: e.to_string() });
                    told = Some(code);
                }
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            }
        };
        told = None;
        let _ = tx.send(Event::Opened { track, name: o.name.clone(), mode: o.mode.clone() });
        let mut down = Decimator::new();
        let mut checked = Instant::now();
        let again = 'run: loop {
            if stop.load(Ordering::Relaxed) {
                break 'run false;
            }
            let _ = o.event.wait_for_event(200);
            loop {
                match o.capture.get_next_packet_size() {
                    Ok(Some(n)) if n > 0 => {}
                    Ok(_) => break,
                    Err(_) => break 'run true,
                }
                match o.capture.read_from_device(&mut buf) {
                    Ok((0, _)) => break,
                    Ok((frames, info)) => {
                        let n = frames as usize * ch;
                        // a packet Windows marks silent holds nothing to keep
                        let data: Vec<f32> = if info.flags.silent {
                            vec![0.0; n]
                        } else {
                            buf[..n * 4].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
                        };
                        let (ts, low) = down.push(info.timestamp, &resample::mono(&data, ch));
                        let sent = tx.send(Event::Packet { track, ts, data: low }).is_ok()
                            && tx.send(Event::Hifi { track, ts: info.timestamp, channels: ch as u16, data }).is_ok();
                        if !sent {
                            break 'run false;
                        }
                    }
                    Err(_) => break 'run true,
                }
            }
            if checked.elapsed() > Duration::from_secs(1) {
                checked = Instant::now();
                if let Some((dir, id)) = &o.follows {
                    if default_id(*dir) != *id {
                        break 'run true;
                    }
                }
                // a chosen microphone that went away: carry on with the default
                if device.is_some() && o.follows.is_none() && track == Track::Me {
                    if let Ok(e) = DeviceEnumerator::new() {
                        if device.as_ref().and_then(|id| e.get_device(id).ok()).and_then(|d| d.get_state().ok()) != Some(wasapi::DeviceState::Active) {
                            break 'run true;
                        }
                    }
                }
            }
        };
        let _ = o.client.stop_stream();
        if let Some(k) = &o.keepalive {
            k.store(true, Ordering::Relaxed);
        }
        if !again {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real capture of what the PC plays, for two seconds: the streams open at 48 kHz, the PC's in
    /// stereo, and the engine's 16 kHz mono comes from them. Only how much sound came is counted;
    /// none is kept. `cargo test real_capture -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_capture_gives_48k_stereo_and_16k_mono() {
        let (tx, rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let t = Wasapi.spawn(Track::Them, None, tx, stop.clone());
        std::thread::sleep(Duration::from_secs(2));
        stop.store(true, Ordering::SeqCst);
        let _ = t.join();
        let (mut low, mut high, mut ch, mut opened, mut problem) = (0usize, 0usize, 0u16, None, None);
        for ev in rx.try_iter() {
            match ev {
                Event::Packet { data, .. } => low += data.len(),
                Event::Hifi { data, channels, .. } => {
                    ch = channels;
                    high += data.len() / channels.max(1) as usize;
                }
                Event::Opened { mode, .. } => opened = mode,
                Event::Problem { code, .. } => problem = Some(code),
            }
        }
        println!("opened {opened:?}, problem {problem:?}, 16 kHz samples {low}, 48 kHz frames {high}, channels {ch}");
        assert!(problem.is_none() && opened.is_some(), "the PC's sound opens");
        assert_eq!(ch, 2, "in stereo");
        assert!(high > 60_000 && low > 20_000, "about two seconds of each");
        assert!((high as f64 / low as f64 - 3.0).abs() < 0.1, "three 48 kHz frames for each 16 kHz sample");
    }
}
