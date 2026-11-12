# How Dawwin works

Notes for contributors. The README covers installing and using the app.

## Layout

Three programs. The app is Rust ([Tauri 2](https://tauri.app)) with a plain HTML/CSS/JS interface in
`ui/` (no bundler). The speech engine is a small Rust program around
[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp), and the translator is the official
build of [llama.cpp](https://github.com/ggml-org/llama.cpp)'s server. Each runs in a process of its
own, so a crash in a GPU driver costs a retry on the CPU instead of the whole app. The translator and
ffmpeg are not in the installer: Dawwin downloads them when they are wanted (see **Tools** below).

| Path | What it does |
| --- | --- |
| `engine/src/main.rs` | `dawwin-engine.exe`: loads one model, talks JSON lines over stdin/stdout. Picks a dedicated GPU first, and falls back on the CPU. `--cpu` keeps the GPU driver out of the process. |
| `src-tauri/src/engine.rs` | The `Engine` interface, and `EngineProcess`, which runs the engine and notices when it dies. |
| `src-tauri/src/queue.rs` | The queue, on one engine thread: decode, chunk, transcribe, fix loops, save. |
| `src-tauri/src/audio.rs` | Decodes any file to 16 kHz mono (ffmpeg when there is one, [symphonia](https://github.com/pdeljanov/Symphonia) otherwise): with ffmpeg, its first sound track on the file's own timeline (`aresample=async=1:first_pts=0`), so sound that starts after the picture keeps its place. Cuts it into chunks of at most 30 s, at the quietest moment of each chunk's last 8 s (20 ms RMS frames smoothed over 200 ms), so words are not split. |
| `src-tauri/src/models.rs` | The models, their pinned revisions and checksums, which model each language goes to, and the download. |
| `src-tauri/src/setup.rs` | Where things live, the settings, and which model file a language uses. |
| `src-tauri/src/transcript.rs` | The transcript data, its translations, and the exports (txt, md, srt, vtt, docx, csv, tsv, json), of the transcript, a translation or both. |
| `src-tauri/src/translate.rs` | `TranslatorProcess`: runs `llama-server.exe` on 127.0.0.1 with a random key, on one device, and asks it for a line at a time. |
| `src-tauri/src/captions.rs` | Subtitles cut and timed the way professional ones are. |
| `src-tauri/src/video.rs` | Subtitles into the video itself: ffmpeg copies its streams and adds a track per language, or burns them into the picture. |
| `src-tauri/src/tools.rs` | The tools Dawwin downloads when they are wanted (the translator, ffmpeg in two builds): pinned files on the release, downloaded, checked and unpacked. |
| `src-tauri/src/record/` | Recording a meeting: the microphone and what the PC plays (WASAPI) as two tracks on one clock, a WAV that survives a crash, echo cancellation, and pieces for the engine as the sound arrives. |
| `src-tauri/src/queue/meeting.rs` | The recording as a job: live text, "me" and "them", the rest after Stop, recovery, the M4A. |
| `src-tauri/src/folder.rs` | Transcribing a folder: finds the recordings and writes the files next to them. Also the one list of the extensions Dawwin takes, which the interface asks for. |
| `src-tauri/src/instance.rs` | One Dawwin per data folder: started again, it brings the open window forward and quits. |
| `src-tauri/src/i18n.rs`, `ui/i18n.js` | Messages in English and Arabic. |

## Details worth knowing

- **GPUs.** Vulkan covers every brand and needs nothing but the graphics driver. The engine uses shared
  backends, so a machine without Vulkan simply has no GPU device, and the CPU code is chosen at run
  time from `ggml-cpu-*.dll`. If the GPU fails (the engine process dies, or its reply marks the error
  as the device's), the app stops using it until it restarts, and runs the same recording again on the
  CPU. Any other error (a broken model file, a missing audio file) fails that job and leaves the GPU in use.
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
  (downloaded with the first translation model, see **Tools**). The
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
  `-c copy`: MP4 and MOV whose streams an MP4 holds give an MP4 with `mov_text` tracks, a MOV it cannot
  hold (ProRes, PCM sound) stays a MOV, anything else an MKV that keeps all its streams. Only the tracks
  added are written as text; the video's own subtitles (PGS, VobSub, styled ASS) are copied as they are.
  Each track has its ISO 639-2 language and a name, and the first one added is on by default.
  **Burned in**, the subtitles go through libass in Segoe UI (Windows' Arial draws ث before لا as a box),
  with an outline and a soft shadow, and `Encoding=-1`, so libass finds each line's direction (without
  it an Arabic line is laid out from the left: its full stop at the start, the words around an English
  term swapped). The picture is encoded again into an MP4: at a quarter above its own bitrate (for a
  picture that states none, as in MKV and WebM, the file's bitrate less its sound's), capped by its
  size, with no floor but 300 kb/s, which keeps the file about the size of the original, and with the
  first H.264 encoder that starts (NVIDIA, AMD, Intel, then
  Windows' Media Foundation, which uses any chip or software, then x264 of a full ffmpeg). Sound an MP4
  can hold is copied, the rest becomes AAC. About 5 seconds a minute of 1080p on an RTX 4060 laptop.
- **Steps.** A job carries the steps chosen on the start page (`queue::Steps`): silences out, sound
  saved, transcribe, translate, video. Translating is a job of its own that the transcription starts
  when it ends, and the video comes after the translation, or right after the transcription without
  one. **Silences**: `audio::speech_spans` keeps everything but the pauses over 0.7 s (silent means near
  the recording's own quietest level, or 35 dB under its loud parts); the model hears only the kept
  parts, and `audio::original_time` takes every time back to the original recording. The sound is
  saved through ffmpeg (decoded at 48 kHz, cut, and handed to a second ffmpeg as a WAV stream through
  `pipe:0`, encoded as AAC), or as WAV without it. Dawwin's own builds have no raw-sample reader and no
  `fd:` protocol, so a bare `-` as input fails there. A child process whose pipe is still open is killed
  and let go before the app waits for it.
- **Tools.** The translator (llama.cpp's official Windows build, the files its server needs) and two
  builds of ffmpeg are not in the installer. `tools.rs` pins each as a `.tar.xz` on a release, with its
  size and SHA-256. It downloads like a model (resumable, checked), then unpacks (xz and tar, plain files
  at the top level only, dated when they are unpacked) into a folder of its own under
  `%LOCALAPPDATA%\Dawwin\tools`. A tool counts as installed only when every file it needs is there
  (`Tool::files`), and files 1.3.0 unpacked with the date 1970 get today's at start. The first
  translation model brings the translator along. ffmpeg is built by `tools/ffmpeg/build.sh`, LGPL only,
  cross-compiled with mingw: **audio** reads every common format, saves the sound as M4A and adds subtitle
  tracks; **video** adds libass (with FreeType, HarfBuzz and FriBidi, so Arabic is shaped and runs right
  to left), the video decoders and the encoders of NVIDIA, AMD and Windows. The app uses Dawwin's own
  ffmpeg, the larger first, else one on the PATH; burning in takes the video one, or an installed ffmpeg
  that has libass. The **Tools** workflow builds and packs them (`tools/pack.py`), tries them on Windows
  with the app's own unpacking on files of the kinds people bring, runs the app's own sound and video
  tests on them, and, run by hand with a tag, puts them on that release.
- **Recording a meeting.** "Them" is process loopback in exclude mode on Dawwin's own process tree
  (everything the PC plays but Dawwin and its WebView2, on whatever device), with loopback of the
  default output device plus a silent stream as the fallback; "me" is the chosen or default microphone,
  opened again when it goes or the default changes. Every packet is placed on one timeline by its
  capture timestamp, holes become silence, and what is older than 400 ms is written: a 16 kHz
  two-channel WAV (left me, right them) whose header is rewritten every second, with a journal beside
  it. The microphone goes through WebRTC's echo cancellation (the `sonora` crate) with "them" as the
  reference: without it the model writes the others' words out again from the speakers, even 20 dB
  down. Each track is cut as it arrives exactly where `audio::split_points` would cut it (25 s), pieces
  without speech never reach the model (it invents a line on near-silence), and a "me" line that only
  says what "them" said at the same time is dropped. More than two minutes behind, live text stops and
  the rest is transcribed from the file after Stop. Measurements: `tools/record-trial`. Debug builds
  take made-up tracks instead of the devices (`DAWWIN_TEST_RECORD="me=<file>|them=<file>"`).
- **`DAWWIN_HOME`** keeps all the app's data in one folder.
- **One Dawwin per data folder.** A named mutex keyed on the data folder: a second start sets an event
  that brings the first window forward, and quits before it touches the cache, a download or the GPU. Leftover audio in the
  cache is deleted only by the Dawwin that holds the folder. A test run with another `DAWWIN_HOME` runs
  beside the installed one.

## Where the app keeps things

| What | Where |
| --- | --- |
| Models | `%LOCALAPPDATA%\Dawwin\models`, or the folder chosen |
| The translator and ffmpeg | `%LOCALAPPDATA%\Dawwin\tools` |
| Engine and translator logs | `%LOCALAPPDATA%\Dawwin\logs\engine.log`, `translate.log` |
| Transcripts | `%APPDATA%\Dawwin\history\*.json` |
| Settings | `%APPDATA%\Dawwin\settings.json`, and `recording.json` for recording |
| Recordings of meetings | `Documents\Dawwin` |

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

`npm run engine` builds the engine with GPU support when `VULKAN_SDK` is set (install the
[Vulkan SDK](https://vulkan.lunarg.com)), and for the CPU otherwise. `npm run engine -- --from-ci`
takes the newest engine GitHub Actions built instead (needs the `gh` command).

`cargo test` in `src-tauri` runs the unit tests. The sound and video tests also run on a full ffmpeg on
the PATH when there is one, and on Dawwin's own builds when `DAWWIN_TEST_TOOLS_DIR` names a folder laid
out like the app's tools folder (`ffmpeg-audio\ffmpeg.exe`, `ffmpeg-video\ffmpeg.exe`): a feature that
worked only with a full ffmpeg has shipped broken before. If linking fails with
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
   with a pause in the middle and loads them, and translates with each translation model through the
   translator Dawwin downloads.
4. If a tool changes (llama.cpp, ffmpeg): create the release as a draft
   (`gh release create vX.Y.Z --draft --target <commit>`), run the **Tools** workflow by hand with the
   tag, and put the sizes and SHA-256 it prints, with the new file names and `RELEASE`, into `tools.rs`.
   Otherwise the app keeps downloading them from the release that has them.
5. `npm run engine -- --from-ci`, then `npm run build`. The last line it prints must say yes to both GPU
   support and the font.
6. Upload `dist/*` to the release and publish it (`gh release edit vX.Y.Z --draft=false --latest`), or
   `gh release create vX.Y.Z dist/* --title "Dawwin X.Y.Z" --notes-file notes.md` when no tool changed.
