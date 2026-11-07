# How Tafreegh works

Notes for contributors. The README covers installing and using the app.

## Layout

The app is Rust ([Tauri 2](https://tauri.app)) with a plain HTML/CSS/JS interface in `ui/` (no
bundler). Inference runs in a Python worker process that the app creates and manages, because the
model has no Rust implementation yet.

| Path | What it does |
| --- | --- |
| `src-tauri/src/audio.rs` | Decodes any file to 16 kHz mono (ffmpeg when installed, [symphonia](https://github.com/pdeljanov/Symphonia) otherwise). Cuts it into chunks of at most 30 s, at the quietest moment of each chunk's last 8 s (20 ms RMS frames smoothed over 200 ms), so words are not split. |
| `src-tauri/src/engine.rs` | The `Engine` interface, and `PythonWorker`, which runs `worker/worker.py` and talks JSON lines over stdin/stdout. A native engine (ONNX through `ort`, or a `candle` port) can replace it by implementing the same trait. |
| `worker/worker.py` | Runs `CohereAsrForConditionalGeneration`: bf16 on CUDA, 4 chunks per batch; on the CPU it converts the linear layers to int8 (dynamic quantization), about 12× faster than bf16 on CPUs without native bf16. Also aligns words and downloads the model. |
| `src-tauri/src/queue.rs` | The queue, on one engine thread: decode, chunk, transcribe, fix loops, align words, save. |
| `src-tauri/src/setup.rs` | Finds the model, keeps the settings, fetches uv and builds the Python env. |
| `src-tauri/src/transcript.rs` | The transcript data and the exports (txt, md, srt, vtt, docx). |
| `src-tauri/src/i18n.rs`, `ui/i18n.js` | Messages in English and Arabic. |

## Details worth knowing

- **Loops.** The model sometimes repeats one word dozens of times ("ست ست ست …"). Six repeats in a
  row, or a short phrase repeated, sends the chunk back through in two halves. A half that still
  loops gets the repetition cut to two and a "check this line" mark.
- **Word timings.** The model only times whole chunks. After a batch is transcribed, the worker
  teacher-forces each line's text through the decoder and reads its cross-attention (all 64 heads
  averaged, normalised per frame, median-filtered), then takes the monotonic path through it with DTW,
  as Whisper does, minus a 0.2 s lag. It reuses the transcription's encoder output, so it adds little
  time. On a Syrian-Arabic recording, against probed word onsets: median error 0.4 s, 81 % of words
  within a second. Lines without word timings fall back to `audio::voice_profile` (where the speech is
  in the line).
- **Model download.** huggingface_hub 1.x starts an interrupted file over, which never finishes 4 GB
  on a connection that drops. The worker keeps a `.part` file, asks for the rest with HTTP Range
  requests, and checks each file's SHA-256.
- **Python env.** Built with [uv](https://docs.astral.sh/uv/) from `worker/requirements.txt`; the
  PyTorch build (CUDA 12.8 or CPU) is picked from the NVIDIA driver version.

## Where the app keeps things

| What | Where |
| --- | --- |
| Model (downloaded) | `%LOCALAPPDATA%\Tafreegh\models\cohere-transcribe-arabic-07-2026`, or the folder chosen |
| Python env and uv | `%LOCALAPPDATA%\Tafreegh\pyenv`, `%LOCALAPPDATA%\Tafreegh\bin` |
| Engine log | `%LOCALAPPDATA%\Tafreegh\logs\worker.log` |
| Transcripts | `%APPDATA%\Tafreegh\history\*.json` |
| Settings, saved Hugging Face token | `%APPDATA%\Tafreegh\settings.json`, `%APPDATA%\Tafreegh\hf-token` |

## Adding a language

Add its strings to `ui/i18n.js`, to `LANGUAGES` and a table in `src-tauri/src/i18n.rs`, and to the
small table in `worker/worker.py`.

## Building

You need [Rust](https://rustup.rs) (stable, MSVC), the Visual Studio C++ Build Tools with a Windows
SDK, and Node.js.

```bash
npm install
npm run build        # installer in src-tauri/target/release/bundle/nsis/
```

`cargo test` in `src-tauri` runs the unit tests. If linking fails with
`LNK1104: cannot open file 'msvcrt.lib'`, Rust picked a Visual Studio install without the C++ x64
libraries: add the "MSVC x64/x86 build tools" component, or build from the `vcvars64.bat` prompt of a
complete install.

## Releasing

Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, then push
a tag such as `v0.2.0`. `.github/workflows/release.yml` builds the installer and a portable exe on
GitHub Actions, signs them when SignPath is set up (see below), and attaches them with checksums to a
draft release.

### Code signing with SignPath

Once SignPath Foundation accepts the project, set in the repository settings:

- secret `SIGNPATH_API_TOKEN`
- variable `SIGNPATH_ORGANIZATION_ID`
- variables `SIGNPATH_PROJECT_SLUG` and `SIGNPATH_SIGNING_POLICY_SLUG` (from the SignPath project)

The release workflow signs the files only when `SIGNPATH_ORGANIZATION_ID` is set. Check the signing
step against SignPath's instructions when enabling it.
