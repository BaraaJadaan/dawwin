# Dawwin · دَوِّن

*Dawwin* means "write it down" in Arabic.

Turn Arabic recordings into text on your own Windows PC. Dawwin runs the open
[Cohere Transcribe Arabic](https://huggingface.co/CohereLabs/cohere-transcribe-arabic-07-2026) model
locally. It handles dialects (Syrian, Gulf, Egyptian, …) and Arabic mixed with English terms.
Your recordings and transcripts never leave the computer.

**[Download the latest release](https://github.com/BaraaJadaan/dawwin/releases/latest)**

## Features

- Drop recordings on the window: m4a, mp3, wav, WhatsApp voice notes, flac, mp4 video. Several files queue up.
- Transcribe a whole folder, subfolders included. The transcripts are saved next to the recordings, and
  recordings that are already done are skipped, so a stopped run picks up where it left off.
- The transcript appears line by line, right to left, with timestamps.
- **Follow words**: the word being said is highlighted as the recording plays. Ctrl+click a word, or
  select text and right-click → **Play from here**, to hear it.
- Correct lines in place, search (أ/إ/ا, ة/ه and ى/ي match), and export to Word, text, Markdown,
  subtitles (SRT, VTT) or data (CSV, TSV, JSON).
- Runs on the GPU of any brand (NVIDIA, AMD, Intel), or on the CPU when there is none.
- English or Arabic interface.

## Getting started

1. Download `Dawwin_x.y.z_x64-setup.exe` from
   [Releases](https://github.com/BaraaJadaan/dawwin/releases) and run it. Windows may say the app is
   unrecognised: choose **More info → Run anyway**. Or use the portable `.zip`: unpack it and run `dawwin.exe`.
2. Choose a model. No account is needed.

   | | Download | GPU memory | |
   | --- | --- | --- | --- |
   | **Full quality** (Q8) | 2.4 GB | about 3 GB | Almost the same as the original model |
   | **Smaller** (Q4) | 1.6 GB | about 2 GB | For weaker PCs and slow connections |

   The download can be paused. If the connection drops or Dawwin closes, it carries on from where it
   stopped. Already have the file? Press **I have the file**.
3. Drop a recording on the window.

**Requirements:** Windows 10 or 11 (64-bit), 8 GB of RAM, and room for the model. For WhatsApp voice
notes (opus), install ffmpeg: `winget install Gyan.FFmpeg`.

## Building from source

You need [Rust](https://rustup.rs), the Visual Studio C++ Build Tools, CMake and Node.js. For GPU
support, also the [Vulkan SDK](https://vulkan.lunarg.com); without it the engine is built for the CPU.

```bash
npm install
npm run engine
npm run build
```

The installer and a portable zip of the current version land in `dist/`, with their checksums.

How the app is put together, and how to release it: [docs/how-it-works.md](docs/how-it-works.md).

## Contributing

Contributions are welcome: bug reports, fixes, translations and new features. Please contact me
before starting on something bigger, so we can agree on the approach: open an
[issue](https://github.com/BaraaJadaan/dawwin/issues) or reach me on GitHub
([@BaraaJadaan](https://github.com/BaraaJadaan)). Then send a pull request.

## Privacy

Dawwin goes online only when you ask it to: to download a model (from
[Hugging Face](https://huggingface.co)), and to check for updates (from GitHub; also at startup if you
turn that on in Settings).

## Uninstall

Uninstall Dawwin from Windows Settings → Apps. The models and your transcripts are kept in case you
reinstall. To remove them too, delete the folders `%LOCALAPPDATA%\Dawwin` and `%APPDATA%\Dawwin` (and
the model folder, if you chose another place for it).

## License

[MIT](LICENSE). Three things are not part of this repository and have their own terms:

- **The model**: Cohere publishes it under Apache-2.0. Dawwin downloads the
  [GGUF conversion](https://huggingface.co/handy-computer/cohere-transcribe-arabic-07-2026-gguf) made by
  the [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) project.
- **The engine**: [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp), under the MIT license.
- **The font**: official builds embed [Thmanyah Sans](https://font.thmanyah.com), © Thmanyah Publishing
  and Distribution. Its license does not allow sharing the font files, so builds from this repository
  use the system font instead.
