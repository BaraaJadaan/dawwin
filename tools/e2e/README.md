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
| `proxy.mjs` | Downloads and the update check through the proxy `HTTPS_PROXY` names (a stand-in that asks for a sign-in; nothing reaches the internet), and why a download cannot go on, in English and Arabic. Needs no model. |
| `queue.mjs` | The queue when things go wrong, with the stand-in engine (`cargo build --example fake_engine`): a model load that hangs ends after its time limit and the job is done on the CPU; a stop during the hang; a job that goes wrong fails alone; no engine process after the GPU is found; an hour with "Remove silences" in about the memory it takes without. |
| `start.mjs` | The common jobs by name on the start page tick their steps; files saved into the folder of one's own chosen in Settings (and a folder run's next to each recording); a video's sound in the export menu; pictures of the start page and Settings in English and Arabic. |
| `lists.mjs` | Long recordings and big folders: a made-up 5,000-line transcript (opening, scrolling, search a key at a time), 300 transcripts in History, a folder of 1,000 recordings, a huge folder stopped, a folder run twice, keyboard access, zoom. |
| `steps.mjs` | The steps on the start page and how they go together; a video's long silences taken out and its sound saved; the whole chain on a video (silences out, transcribed, translated by the stand-in translator `cargo build --example fake_translator`, subtitles added) with the times of the video; the sound saved later from a transcript, never over the recording; "How to use" and the address to write to. |
| `translate.mjs` | Translation with the stand-in translator: the models in Settings, the menu, lines as they come, a crash of the translator on the GPU that carries on on the CPU, an edited line translated again, a translation corrected by hand, exports and copying, stopping and carrying on, right to left, the Arabic interface, taking turns with the speech engine on the GPU, and the translator ending with the app. |
| `video.mjs` | A video's subtitles as tracks with Dawwin's small ffmpeg (English first, the picture and sound copied, an MKV with SRT tracks); without ffmpeg or the translator, each step and export offering its download, and the app refusing without it; burned in with Dawwin's ffmpeg for video from the steps and from the export menu, how far it is, and one stopped half way leaving nothing. |
| `tools.mjs` | The tool downloads from a local stand-in for the release and Hugging Face: the small ffmpeg paused and resumed, ffmpeg for video from the offer burning in makes, burning in with it, deleting it, a damaged download refused, and the translator coming before the first translation model. Needs the release's `dawwin-ffmpeg-*.tar.xz` themselves in `DAWWIN_E2E_TOOLS`; without the translator's pack there, a stand-in of its size is served. Needs no GPU. |
| `twice.mjs` | One Dawwin per data folder: started again on the same folder while it transcribes, it quits at once with no window, and the first one's job, audio and engine carry on; another data folder opens beside it; the folder opens again once the first is closed. |

```bash
node tools/e2e/timing.mjs
node tools/e2e/lists.mjs
node tools/e2e/subtitles.mjs
node tools/e2e/proxy.mjs
node tools/e2e/queue.mjs
node tools/e2e/start.mjs
node tools/e2e/steps.mjs
node tools/e2e/translate.mjs
node tools/e2e/video.mjs
node tools/e2e/tools.mjs
node tools/e2e/twice.mjs
```
