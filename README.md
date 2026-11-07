# Tafreegh · تفريغ

Turn Arabic recordings into text on your own Windows PC. Tafreegh runs the open
[Cohere Transcribe Arabic](https://huggingface.co/CohereLabs/cohere-transcribe-arabic-07-2026) model
locally. It handles dialects (Syrian, Gulf, Egyptian, …) and Arabic mixed with English terms.
Your recordings and transcripts never leave the computer.

**[Download the latest release](https://github.com/BaraaJadaan/tafreegh/releases/latest)**

## Features

- Drop recordings on the window: m4a, mp3, wav, WhatsApp voice notes, flac, mp4 video. Several files queue up.
- The transcript appears line by line, right to left, with timestamps.
- **Follow words**: the word being said is highlighted as the recording plays. Ctrl+click a word, or
  select text and right-click → **Play from here**, to hear it.
- Correct lines in place, search (أ/إ/ا, ة/ه and ى/ي match), and export to Word, text, Markdown or
  subtitles (SRT, VTT).
- Runs on an NVIDIA GPU, or on the CPU when there is none.
- English or Arabic interface.

## Getting started

1. Download `Tafreegh_x.y.z_x64-setup.exe` from [Releases](https://github.com/BaraaJadaan/tafreegh/releases)
   and run it (or use the portable `.exe`). Windows may say the app is unrecognised: choose
   **More info → Run anyway** (see [Code signing](#code-signing-policy)).
2. On first start, press **Set up engine**. Tafreegh installs Python and PyTorch into its own folder
   (about 3 GB to download with an NVIDIA GPU, less without one). If the connection drops, press it again.
3. Get the model (about 4 GB, free but gated). Sign in to Hugging Face, accept the terms on the
   [model page](https://huggingface.co/CohereLabs/cohere-transcribe-arabic-07-2026), create a
   [read token](https://huggingface.co/settings/tokens), paste it into Tafreegh and press
   **Download model**. The download resumes after interruptions. Already have the files?
   Press **Import folder** instead.
4. Drop a recording on the window.

On an RTX 4060 laptop GPU, 24 minutes of audio take under a minute. On an 8-core laptop CPU, about
3× faster than real time.

**Requirements:** Windows 10 or 11 (64-bit), about 10 GB of free disk space, 8 GB of RAM. An NVIDIA
GPU with 6 GB of memory or more is recommended. For WhatsApp voice notes (opus), install ffmpeg:
`winget install Gyan.FFmpeg`.

## Building from source

You need [Rust](https://rustup.rs), the Visual Studio C++ Build Tools and Node.js.

```bash
npm install
npm run build
```

How the app is put together, and how to release it: [docs/how-it-works.md](docs/how-it-works.md).

## Contributing

Contributions are welcome: bug reports, fixes, translations and new features. Please contact me
before starting on something bigger, so we can agree on the approach: open an
[issue](https://github.com/BaraaJadaan/tafreegh/issues) or reach me on GitHub
([@BaraaJadaan](https://github.com/BaraaJadaan)). Then send a pull request.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/). *(Pending: until SignPath Foundation accepts the project,
releases are unsigned and Windows SmartScreen shows a warning.)*

Releases are built from this repository by GitHub Actions, and every signed release is approved by
hand.

- Committers and reviewers: [@BaraaJadaan](https://github.com/BaraaJadaan)
- Approvers: [@BaraaJadaan](https://github.com/BaraaJadaan)

**Privacy policy:** this program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or operating it. It goes online
only when you press **Set up engine** (downloads [uv](https://github.com/astral-sh/uv), Python
packages from [PyPI](https://pypi.org) and [PyTorch](https://download.pytorch.org)) or
**Download model** (downloads the model from [Hugging Face](https://huggingface.co) with your token),
or **Check for updates** (asks GitHub for the latest version; also at startup if you turn that on in
Settings). Those services' own privacy policies apply.

## Uninstall

Uninstall Tafreegh from Windows Settings → Apps. The engine, the model and your transcripts are kept
in case you reinstall; to remove them too, delete the folders `%LOCALAPPDATA%\Tafreegh` and
`%APPDATA%\Tafreegh` (and the model folder, if you chose another place for it).

## License

[MIT](LICENSE). The model is not part of this repository: Cohere publishes it under Apache-2.0 on
Hugging Face, behind an access agreement each user accepts.
