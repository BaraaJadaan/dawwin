# Tests of the real app

These scripts start a debug build of Dawwin, each in a data folder of its own (`DAWWIN_HOME`), open
its WebView's debugging port, and drive it through its own commands and its page, as a person would.
They make their own sample recordings (from the voices of Windows and test pictures), so they need
no private data. Only the processes they start are ever stopped.

## What they need

- A debug build: `cargo build --locked --examples --bins` in `src-tauri`, from the VS 2022 prompt.
- Node.js 22 or newer.
- Environment:

| Variable | What |
| --- | --- |
| `DAWWIN_E2E_MODEL` | A speech model file (`.gguf`). The Arabic one does English too. |
| `DAWWIN_E2E_TOOLS` | A folder with Dawwin's own ffmpeg builds unpacked into `ffmpeg-audio` and `ffmpeg-video` (the `dawwin-ffmpeg-*.tar.xz` of a release). The app is tested with those, as its users have them. |
| `DAWWIN_E2E_FFMPEG` | A full ffmpeg, only to make the sample videos (default: `ffmpeg` on the PATH). |
| `DAWWIN_E2E_OUT` | Where the data folders and samples go (default `tools/e2e/out`, ignored by git). |
| `DAWWIN_E2E_PORT` | The first debugging port (default 9400). |
| `DAWWIN_APP` | Another build of the app. |

A script without what it needs says so and is skipped.

## The scripts

| Script | What it checks |
| --- | --- |
| `timing.mjs` | Subtitles that follow the speaker: words placed in time while a video with subtitles is made, when subtitles are exported, from "Fix the timing" and for Follow words; edited lines; a recording that moved. |
| `subtitles.mjs` | Subtitles in Chinese, Japanese, Korean, Thai and Khmer cut to each language's line length; a subtitle track and burned-in subtitles with Dawwin's own ffmpeg; both languages, the translation in whole sentences; exports a player or a spreadsheet would misread. |
| `lists.mjs` | Long recordings and big folders: a made-up 5,000-line transcript (opening, scrolling, search a key at a time), 300 transcripts in History, a folder of 1,000 recordings, a huge folder stopped, a folder run twice, keyboard access, zoom. |

```bash
node tools/e2e/timing.mjs
node tools/e2e/lists.mjs
node tools/e2e/subtitles.mjs
```
