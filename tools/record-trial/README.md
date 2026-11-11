# Recording trial (Dawwin 1.5.0, issue 4)

A program of its own, not the app: records the microphone ("me") and what the PC plays ("them",
process loopback excluding its own process tree) as two tracks on one clock (each packet placed by
its QPC timestamp, holes filled with silence), into a 16 kHz 16-bit two-channel WAV whose header is
rewritten and flushed every second. `cargo build --release` from the VS 2022 prompt.

Commands: `devices`, `record --out F.wav [--seconds N] [--them process|endpoint|none] [--mic NAME]
[--keepalive] [--self-play F.wav] [--no-audio]`, `play F.wav [--gain-db G] [--delay-ms D]`,
`check`, `repair`, `analyse` (echo of "them" in "me"), `locate F.wav --reference R.wav`,
`aec me.f32 them.f32 out.f32` (sonora AEC3; built, not yet measured).

## Results, 2026-11-12, Windows 11 build 26300, Realtek laptop speakers and microphone array

| What | Measured |
| --- | --- |
| Drift against the clock, 2 min | microphone -0.4 ppm (short runs -1 to -4), Iriun virtual microphone +9.6, process loopback 0.0 |
| Drift between the tracks | 0.4 to 10 ppm: 1 to 35 ms an hour if joined by count; placed by timestamp, jitter absorbed under 2 ms (microphone) |
| Processor | 0.4 % of one core over 2 min (both streams); under 1 % in every run with the WAV written |
| Process loopback through silence | a packet every 10 ms, no holes in 2 min, no SILENT flags |
| Another program starts or stops a sound | a hole of 12-15 ms; 130-250 ms when the first sound starts after silence. Sound already playing shifts by up to 17 ms in the track, and up to ~30 ms of it can be missing at that moment; nothing accumulates |
| Own playback (self-play) | excluded completely: "them" is digital zero while the microphone hears it |
| Killed by force (`kill -9` at 8 s) | header and file agree (6.63 s), ffprobe reads it; lost: the 0.4 s margin plus up to 1 s since the last checkpoint |
| Speakers heard by the microphone | 55-59 ms later, -18 to -25 dB under the loopback level |
| Doubled words (made-up tracks from a Surah, real Cohere Q8 model) | an echo at -20, -12 or -6 dB is transcribed in full: 21 of 21 words doubled. Own speech over the echo gives one mixed line (13 of 21 words echo): dropping whole lines is not enough, echo cancellation is needed |
| Endpoint loopback (fallback), nothing played | packets kept coming on this PC; opening took 0.4 s |

Not tested (needs a person or another machine): headset plugged in or out, Bluetooth, Zoom, Teams,
Meet, Windows 10, a microphone blocked in Windows' privacy settings, sonora's effect on the echo.
