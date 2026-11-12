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
| `DAWWIN_E2E_PDFS` | A folder of made-up PDFs, an e-book and a picture of text, made by `tools/trials/pdf/make_pdfs.py` (and `make_word.ps1` for Word's PDF), for `screen.mjs`. |
| `DAWWIN_E2E_SPEAKERS`, `DAWWIN_E2E_LANGUAGE` | The speaker model (`diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf`) and the language model (`lang-id-voxlingua107-ecapa-Q8_0.gguf`), for `speakers.mjs` and `record.mjs`. |

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
| `read.mjs` | Read aloud with the stand-in voice (`cargo build --example fake_voice`): pasted text and every file it reads (Markdown, HTML, SRT, VTT, Windows-1256 text, Word), play and the first sound, the sentence marked, pause, back and forward, a click on a sentence, speed, the place remembered, an English part inside Arabic going to the English voice and a single English word staying Arabic, numbers in words, a 300-page file, Listen on a transcript and on its translation, a meeting's two speakers in two voices with their names, Listen on the minutes, a voice that dies once and one that dies twice, a voice that needs the graphics card while a transcription has it, the Arabic interface, and the three voices in Settings with a sample to hear. Needs no model. |
| `speakers.mjs` | Who said what: a recording of two of Windows' voices transcribed with Detect the language and Tell speakers apart; rename, merge, a line given to someone new, every export, names in subtitles, search within one speaker, words and names, the subtitle look's preview and sharp text on a 480p video, Find the speakers on a transcript without them, and the clicks on a made-up meeting (the name's menu, the right-click menu, Always write it like this…). |
| `record.mjs` | Recording a meeting, with two made-up tracks played at clock speed through the debug builds' test sound source (`DAWWIN_TEST_RECORD`; no sound device is used and nothing is recorded from the PC): live text, Me and the others told apart, Detect the language, the echo of the speakers not written twice, the clock across 24 s of silence, the M4A in the folder chosen in Settings, Fix the timing and Find the speakers on a meeting, a PC too slow to keep up, a forced close and the recording offered back, the engine missing, Cancel after Stop, a microphone that sends nothing and another chosen in its menu, the disk running low, one source alone, a translation after it, a folder in OneDrive. `node tools/e2e/record.mjs
node tools/e2e/screen.mjs <case>` runs one case. Needs the speaker and language models. |
| `record-dialogs.mjs` | The native dialogs of recording, read and answered through UI Automation (`dialog.ps1`, `closewin.ps1`): the notice before the first recording in English and Arabic, and again when the recordings would go to OneDrive; closing Dawwin while it records. Run `record.mjs` first (it makes the tracks). |
| `screen.mjs` | Read anything on the screen, with the stand-in voice: the shortcut's settings (a taken shortcut said, with free ones offered; a key alone refused), say it like this, the Settings section in English and Arabic with its tooltips, Start with Windows (in a test registry key, never the real Run key), the small player reading a selection (pause, back, forward, the shortcut again to stop, Read aloud in the window stopping meanwhile, the selection forgotten with its sound and no place kept), closing to the notification area and starting again, PDFium downloaded from a local stand-in for the release, PDFs from Chrome and Word, an e-book's chapters and its place, a scan and a picture read by Windows, saving as sound as a job in the list (an M4B with its chapters, through Dawwin's ffmpeg), and closing with the shortcut off quitting. The taking of a selection from other programs is the trial's (`tools/trials/selection`). Needs the release's `dawwin-pdfium-*.tar.xz` and `ffmpeg-audio` in `DAWWIN_E2E_TOOLS`, and `DAWWIN_E2E_PDFS`. |
| `minutes.mjs` | Minutes with the stand-in translator: the models offered with their sizes and whether they fit, writing with its progress, a crash on the GPU carried on on the processor, the 8-bit cache on the GPU only, each model told not to think its own way, the speakers' names as owners, a recorded meeting's Me and Them, the times that show the lines, corrections kept (and never drawn over while typed), the lines read-only while written again, another language, a long meeting in parts stopped and continued, every export, a lecture, the step after a transcript and a folder run, the Arabic interface. |
| `pictures.mjs` | The pictures of How to use and the README (the minutes among them), in English and Arabic, from made-up data (it writes into the repository). |
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
node tools/e2e/speakers.mjs
node tools/e2e/record.mjs
node tools/e2e/record-dialogs.mjs
node tools/e2e/minutes.mjs
```

Debug builds also take, for `record.mjs`: `DAWWIN_TEST_ENGINE_DELAY_MS` (each piece of a recording
takes this much longer), `DAWWIN_TEST_RECORD_BEHIND_S` (live text stops this far behind, 120 s
otherwise), `DAWWIN_TEST_DISK_FREE` (the free space the recorder sees) and `DAWWIN_TEST_ONEDRIVE` (a
folder taken for OneDrive's). The measurements behind the recorder are a program of their own,
`tools/record-trial`.
