# Tafreegh (تفريغ)

Speech-to-text for Arabic recordings that runs entirely on your own Windows PC. It handles
dialects (Syrian, Gulf, Egyptian, …) and Arabic mixed with English terms. It uses
[Cohere Transcribe Arabic](https://huggingface.co/CohereLabs/cohere-transcribe-arabic-07-2026),
an open 2B-parameter model. Your audio and transcripts never leave the computer.

- **Drop recordings on the window** (or choose them): m4a, mp3, wav, ogg/opus WhatsApp voice notes,
  flac, mp4/mov video. Several files queue up; each shows its progress and time left and can be cancelled.
- **Read it as it is written**: the transcript appears line by line, right to left, with a
  timestamp for every chunk (up to 30 s).
- **Check and fix**: click a timestamp to play the recording from there; click a line to correct it.
  Lines where the model got stuck repeating a word are cut and marked for checking.
- **Hear a word**: Ctrl+click a word, or select text, right-click it and choose **▶ Play from here**,
  to play from about where it is said. The model times whole lines, so the word's place is estimated from where the speech is in
  the line; playback starts 1.5 s early so you hear it within the next few seconds.
- **Search** with Ctrl+F. أ/إ/آ/ا, ة/ه and ى/ي count as the same letter, and diacritics are ignored.
- **Export** to Word (.docx, right to left), .txt, .md, .srt and .vtt subtitles, or copy everything.
- **History** of past transcripts, renamed or deleted from the transcript view.
- Runs on an **NVIDIA GPU (CUDA)**, or on the **CPU** for computers without one (see *Run on* below).

## Install

1. Download `Tafreegh_x.y.z_x64-setup.exe` from [Releases](../../releases) and run it.
   Windows SmartScreen may warn that the app is unrecognised, because it is not code-signed.
   Choose **More info → Run anyway**.
2. On first start, Tafreegh opens **Engine and model**, which needs two one-time steps:
   - **Set up engine.** This installs Python 3.12 with PyTorch and Transformers in
     `%LOCALAPPDATA%\Tafreegh\pyenv`. Tafreegh downloads [uv](https://docs.astral.sh/uv/) by itself
     to do this, so nothing needs installing first. With an NVIDIA GPU (driver 572 or newer), it
     installs the CUDA build: about 3 GB to download, 5 GB on disk. Without an NVIDIA GPU, it
     installs the CPU build, which is smaller but transcribes much more slowly. If the connection
     drops, press the button again to continue.
   - **Model** (about 4 GB). Pick one way:
     - **Download it.** The model is free but gated. Sign in to Hugging Face, open the
       [model page](https://huggingface.co/CohereLabs/cohere-transcribe-arabic-07-2026) and accept
       its terms. Then create a [read token](https://huggingface.co/settings/tokens) and paste it into
       Tafreegh. Choose where to save the model, then press **Download model**. After a dropped
       connection, **Pause**, or even closing the app, the download continues from where it stopped
       (Tafreegh asks only for the missing bytes), and each file is checked against its published
       SHA-256. If you already signed in with `hf auth login`, no token is needed.
     - **Import the files you already have.** Use this for a copy from another computer, files
       downloaded by hand from the model page's *Files* tab (`config.json`, `model.safetensors`,
       `tokenizer.json`, `tokenizer.model`, `tokenizer_config.json` and the other `.json` files), or a
       Hugging Face cache folder (`models--CohereLabs--cohere-transcribe-arabic-07-2026`). If the
       model is already in your Hugging Face cache (`HF_HOME`), Tafreegh finds it on its own.
3. Drop a recording on the window.

**Speed:** an RTX 4060 Laptop GPU transcribes 24 minutes of audio in under a minute, plus 15–50 s to
load the model when the first file starts. The model stays in memory (about 4 GB) and is freed after
10 idle minutes.

**Run on** (on the *Engine and model* page):

| Setting | What it does | 2 min of audio on a Ryzen 7 laptop (8 cores) |
| --- | --- | --- |
| GPU when available (default) | CUDA on an NVIDIA GPU; switches to the CPU when the GPU has too little memory, or there is no NVIDIA GPU | 5 s on an RTX 4060 |
| CPU, faster | an 8-bit version of the model; about 3× faster than real time, slightly less exact (90% of words identical to the GPU result in a test) | 35 s |
| CPU, exact | full precision; as exact as the GPU but about 7× slower than real time | about 14 min |

**Formats:** wav, mp3, m4a, mp4, flac and ogg/Vorbis work out of the box. WhatsApp voice notes
(opus) and other formats need ffmpeg (`winget install Gyan.FFmpeg`); Tafreegh uses it when it is
installed.

**Requirements:** Windows 10 or 11, 64-bit, about 10 GB of free disk space and 8 GB of RAM (about
5 GB of it free while transcribing). An NVIDIA GPU with 6 GB of VRAM or more makes it fast, but the
CPU works too.

## Where things live

| What | Where |
| --- | --- |
| Model (downloaded) | `%LOCALAPPDATA%\Tafreegh\models\cohere-transcribe-arabic-07-2026`, or the folder you chose |
| Python env and uv | `%LOCALAPPDATA%\Tafreegh\pyenv`, `%LOCALAPPDATA%\Tafreegh\bin` |
| Engine log | `%LOCALAPPDATA%\Tafreegh\logs\worker.log` |
| Transcripts | `%APPDATA%\Tafreegh\history\*.json` |
| Settings, saved Hugging Face token | `%APPDATA%\Tafreegh\settings.json`, `%APPDATA%\Tafreegh\hf-token` |

Uninstalling the app leaves these folders in place; delete them to remove everything.

## How it works

The app is Rust ([Tauri 2](https://tauri.app)) with a plain HTML/CSS/JS interface in `ui/`.
Inference runs in a Python worker process that the app creates and manages, because the model has no
Rust implementation yet.

- `src-tauri/src/audio.rs` decodes any file to 16 kHz mono, using ffmpeg when it is installed and
  [symphonia](https://github.com/pdeljanov/Symphonia) otherwise. It cuts the audio into chunks of at
  most 30 s, at the quietest moment of each chunk's last 8 s (20 ms RMS frames smoothed over 200 ms),
  so words are not split.
- `src-tauri/src/engine.rs` defines the `Engine` interface. Its one implementation, `PythonWorker`,
  runs `worker/worker.py` and talks JSON lines over stdin/stdout. The worker runs
  `CohereAsrForConditionalGeneration` in bf16 on CUDA, 4 chunks per batch. On the CPU it converts the
  linear layers to int8 (dynamic quantization) one at a time, which is about 12× faster than bf16 on
  CPUs without native bf16. Its model download uses HTTP Range requests on a `.part` file, because
  huggingface_hub 1.x starts an interrupted file over. A native engine (ONNX through
  `ort`, or a `candle` port) can replace it by implementing the same trait.
- `src-tauri/src/queue.rs` runs the queue on one engine thread. The model sometimes loops on a word
  ("ست ست ست …"). Six repeats in a row, or a short phrase repeated, sends the chunk back through in two
  halves. A half that still loops gets the repetition cut to two and a "check this line" mark.
- `src-tauri/src/setup.rs` finds the model, keeps the settings, fetches uv and builds the Python env.
- `audio::voice_profile` records, for each line, when its speech happens (skipping pauses); the UI uses
  it to place a selected word in the audio.
- `src-tauri/src/transcript.rs` holds the exports. Subtitle cues split each chunk into lines of about
  14 words, with times inside a chunk estimated from character counts.

## Building from source

You need [Rust](https://rustup.rs) (stable, MSVC), the Visual Studio C++ Build Tools with a Windows
SDK, and Node.js (for the Tauri CLI that makes the installer).

```bash
npm install
npm run build
```

If linking fails with `LNK1104: cannot open file 'msvcrt.lib'`, Rust picked a Visual Studio install
without the C++ x64 libraries (for example VS 2026 Build Tools without the "MSVC x64/x86 build tools"
component). Add that component in the Visual Studio Installer, or build from a developer prompt of a
complete install (`vcvars64.bat` of VS 2022).

The installer lands in `src-tauri/target/release/bundle/nsis/`, and the bare app in
`src-tauri/target/release/tafreegh.exe`. For a quick build without the installer, run
`cargo build --release` in `src-tauri`.

Run the tests with `cargo test` in `src-tauri`. They cover chunking, loop detection, export formats
and model-folder checks. The icons are drawn by `tools/make_icons.py` (Pillow).

To release: push a tag such as `v0.1.0`. `.github/workflows/release.yml` then builds the installer
and attaches it to a draft GitHub release.

## License

The app is MIT licensed (see `LICENSE`). The model is not part of this repository. It is published
by Cohere under Apache-2.0 on Hugging Face, behind an access agreement each user accepts themselves.
