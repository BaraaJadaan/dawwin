# How Dawwin works

Notes for contributors. The README covers installing and using the app.

## Layout

Three programs. The app is Rust ([Tauri 2](https://tauri.app)) with a plain HTML/CSS/JS interface in
`ui/` (no bundler). The speech engine is a small Rust program around
[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp), and the translator is the official
build of [llama.cpp](https://github.com/ggml-org/llama.cpp)'s server. Each runs in a process of its
own, so a crash in a GPU driver costs a retry on the CPU instead of the whole app.

| Path | What it does |
| --- | --- |
| `engine/src/main.rs` | `dawwin-engine.exe`: loads one model, talks JSON lines over stdin/stdout. Picks a dedicated GPU first, and falls back on the CPU. `--cpu` keeps the GPU driver out of the process. |
| `src-tauri/src/engine.rs` | The `Engine` interface, and `EngineProcess`, which runs the engine and notices when it dies. |
| `src-tauri/src/queue.rs` | The queue, on one engine thread: decode, chunk, transcribe, fix loops, save. |
| `src-tauri/src/audio.rs` | Decodes any file to 16 kHz mono (ffmpeg when installed, [symphonia](https://github.com/pdeljanov/Symphonia) otherwise). Cuts it into chunks of at most 30 s, at the quietest moment of each chunk's last 8 s (20 ms RMS frames smoothed over 200 ms), so words are not split. |
| `src-tauri/src/models.rs` | The models, their pinned revisions and checksums, which model each language goes to, and the download. |
| `src-tauri/src/setup.rs` | Where things live, the settings, and which model file a language uses. |
| `src-tauri/src/transcript.rs` | The transcript data, its translations, and the exports (txt, md, srt, vtt, docx, csv, tsv, json), of the transcript, a translation or both. |
| `src-tauri/src/translate.rs` | `TranslatorProcess`: runs `llama-server.exe` on 127.0.0.1 with a random key, on one device, and asks it for a line at a time. |
| `src-tauri/src/captions.rs` | Subtitles cut and timed the way professional ones are. |
| `src-tauri/src/video.rs` | Subtitles into the video itself: ffmpeg copies its streams and adds a track per language. |
| `src-tauri/src/folder.rs` | Transcribing a folder: finds the recordings and writes the files next to them. |
| `src-tauri/src/i18n.rs`, `ui/i18n.js` | Messages in English and Arabic. |

## Details worth knowing

- **GPUs.** Vulkan covers every brand and needs nothing but the graphics driver. The engine uses shared
  backends, so a machine without Vulkan simply has no GPU device, and the CPU code is chosen at run
  time from `ggml-cpu-*.dll`. If the GPU fails in any way, the app stops using it until it restarts,
  and runs the same recording again on the CPU.
- **Loops.** The model sometimes repeats itself. transcribe.cpp stops it and returns the text so far,
  which would lose the rest of the chunk, so such a chunk goes back through in two halves. A half that
  still does it gets the repetition cut and a "check this line" mark.
- **Cutting.** We tried speech detection (Silero and FireRed) and padding the chunks with silence.
  Scored against the Quran text on a recitation, both were worse than cutting at the quietest moment:
  11.2 % and 8.9 % word errors with speech detection, 6.5 % with padding, 5.2 % without either.
- **One piece at a time.** Batches are 1.7 times faster on a GPU, but the padding of their shorter
  pieces made the Q8 model invent English sentences past the end of the speech on a recitation.
- **Word timings.** The engine gives none, so the word being said is estimated from
  `audio::voice_profile` (where the speech is in the line). Transcripts from older versions keep the
  timings they were saved with.
- **Which model.** `models_for` in `models.rs` lists the models of each spoken language, the best first:
  Cohere for Arabic, Canary for the European languages (fewer mistakes than Whisper, and several times
  faster), Whisper for the rest. A recording goes to the first one on the disk. A model file is told apart
  by the architecture in its GGUF header, and the settings keep one chosen file per kind of model. An
  engine process holds one model, so a recording in another language starts a new one.
- **Model download.** `<file>.part` holds the bytes, and `<file>.part.ok` how many of them are known to
  be on the disk (synced every 8 MB). A restart asks for the rest with an HTTP range request. The file
  takes its real name only after its SHA-256 matches, and a model in use is never rewritten in place,
  because the engine memory-maps it. `HF_ENDPOINT` selects a mirror.
- **Translation.** A finished transcript is translated a line at a time, in the queue, by Xiaomi's
  [MiLMMT-46](https://huggingface.co/collections/xiaomi-research/milmmt-46) through llama.cpp's server
  (`engine\translate`, staged by `npm run engine` from the official release, checksum pinned). The
  speech engine and the translator take turns: both want the GPU's memory. Each translated line keeps
  the text it was translated from, so a line edited later shows as out of date, and translating again
  only does the lines that need it. The server runs in a Windows job object, so it ends with the app
  however the app ends. It runs on the GPU the speech engine found best (a dedicated one before one in
  the processor), never spread over several. Why MiLMMT-46: we measured ten open translation models
  on WMT24++, see `tools/bench-translate` and the **Translation bench** workflow.
- **Subtitles.** `captions.rs` follows the rules Netflix and the BBC give their subtitlers: two lines
  of 42 characters at most, between 5/6 of a second and 7 seconds on screen, 17 characters a second
  (20 for English), two frames between subtitles, gaps under half a second closed, and breaks where a
  sentence or clause ends or the speaker pauses. The model times whole lines, so inside a line the
  words are placed by where the speech and the pauses are (`audio::pauses`, recorded with each line
  since 1.3), and a translation by the same share of its text. **Video with subtitles** runs ffmpeg with
  `-c copy`: MP4 and MOV give an MP4 with `mov_text` tracks, anything else an MKV that keeps all its
  streams. Each track has its ISO 639-2 language and a name, and the first one added is on by default.
- **`DAWWIN_HOME`** keeps all the app's data in one folder.

## Where the app keeps things

| What | Where |
| --- | --- |
| Models | `%LOCALAPPDATA%\Dawwin\models`, or the folder chosen |
| Engine and translator logs | `%LOCALAPPDATA%\Dawwin\logs\engine.log`, `translate.log` |
| Transcripts | `%APPDATA%\Dawwin\history\*.json` |
| Settings | `%APPDATA%\Dawwin\settings.json` |

## Adding a language

Add its strings to `ui/i18n.js`, and to `LANGUAGES` and a table in `src-tauri/src/i18n.rs`.

## Building

You need [Rust](https://rustup.rs) (stable, MSVC), the Visual Studio C++ Build Tools with a Windows
SDK, CMake and Node.js.

```bash
npm install
npm run engine       # builds the engine into src-tauri/engine-bin
npm run build        # the installer and a portable zip land in dist/
```

`npm run engine` also stages the translator: llama.cpp's official Windows build, downloaded once into
`src-tauri/engine-cache` and checked against its SHA-256. It builds the engine with GPU support when `VULKAN_SDK` is set (install the
[Vulkan SDK](https://vulkan.lunarg.com)), and for the CPU otherwise. `npm run engine -- --from-ci`
takes the newest engine GitHub Actions built instead (needs the `gh` command).

`cargo test` in `src-tauri` runs the unit tests. If linking fails with
`LNK1104: cannot open file 'msvcrt.lib'`, Rust picked a Visual Studio install without the C++ x64
libraries: add the "MSVC x64/x86 build tools" component, or build from the `vcvars64.bat` prompt of a
complete install.

## Releasing

Official builds embed the Thmanyah font, whose license allows that in a compiled product but forbids
uploading the font files anywhere. So they never go near GitHub: `ui/fonts/` is git-ignored, and
releases are built on the maintainer's PC, with the engine from GitHub Actions.

1. Bump the version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` and
   `engine/Cargo.toml`.
2. Put `thmanyahsans-Regular.otf`, `-Medium.otf` and `-Bold.otf` in `ui/fonts/`.
3. Push, and let the **Engine** workflow finish: it builds the engine, then downloads every real model
   with a pause in the middle and loads them, and translates with both translation models through the
   translator as it ships.
4. `npm run engine -- --from-ci`, then `npm run build`. The last line it prints must say yes to both GPU
   support and the font.
5. `gh release create vX.Y.Z dist/* --title "Dawwin X.Y.Z" --notes-file notes.md`
