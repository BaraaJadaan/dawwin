# Dawwin · دَوِّن

*Dawwin* means "write it down" in Arabic.

**Turn recordings and videos into text, translate it, and subtitle them, on your own Windows PC.**
Arabic first, with its dialects and the English words people mix into it, and about 100 other
languages. Translation into 46 languages. Nothing leaves your computer: no account, no upload, no
subscription, no limit on hours.

**[Download the latest release](https://github.com/BaraaJadaan/dawwin/releases/latest)** · free and open source (MIT)

<div dir="rtl">

**دَوِّن** يفرّغ التسجيلات والفيديوهات إلى نص على حاسوبك، ويترجمها، ويضيف إليها نصًّا على الشاشة.
العربية أولًا، بلهجاتها والكلمات الإنجليزية التي تتخللها، ونحو 100 لغة أخرى، والترجمة إلى 46 لغة.
لاجتماعات العمل، والمحاضرات والدروس والخطب، والمقابلات والأبحاث، والفيديوهات والأفلام، والرسائل
الصوتية. لا شيء يغادر جهازك: لا حساب ولا رفع ولا اشتراك.

</div>

![A finished transcript in Dawwin. The word being said is highlighted while the recording plays.](docs/screenshots/transcript.png)

## What people use it for

- **Business meetings.** Record on Zoom, Teams or Google Meet, drop the video on the window, and get
  the meeting as text you can search, correct and send as a Word document. Requirement meetings in
  Arabic full of English terms ("الـ dashboard", "الـ API") come out as they were said.
- **Colleagues who speak another language.** Translate the transcript and share it on its own, or as a
  bilingual document with each line and its translation under it.
- **Videos, courses and YouTube.** Subtitles timed the way professional ones are. Translate them to
  reach viewers in 46 languages, and add them to the video as tracks the viewer turns on and off.
- **Films and series.** Subtitles for a film that has none, or none in your language. Dawwin writes
  them into the film's MKV, next to the tracks it already has.
- **Interviews and research.** Exact quotes with their time, every word searchable, and an export to
  CSV or JSON for analysis. Journalists, researchers and students keep their sources private.
- **Lectures, lessons and sermons.** University lectures, Arabic lessons, khutbahs and Quran
  recitations become text you can search and quote, with the recording a click away from any word.
- **Podcasts.** Transcripts and show notes for listeners and for search engines.
- **Voice notes.** WhatsApp voice notes to text.
- **Accessibility.** Captions for people who are deaf or hard of hearing.
- **Learning a language.** Read along with **Follow words**, with the translation under each line.
- **Confidential recordings.** Legal, medical, HR and other private calls are transcribed on your
  own machine, never on someone else's server.

## Features

**Transcribe**

- Drop recordings and videos on the window: m4a, mp3, wav, WhatsApp voice notes, flac, mp4 and mov.
  With ffmpeg installed, also mkv, webm, avi and any other format. Several files queue up.
- Transcribe a whole folder, subfolders included. The transcripts are saved next to the recordings, and
  recordings that are already done are skipped, so a stopped run picks up where it left off.
- Choose the spoken language, and Dawwin uses the model that suits it. If that model is not on the PC
  yet, it offers to download it.
- The transcript appears line by line while it is written, in the direction of its language.
- **Follow words**: the word being said is highlighted as the recording plays. Ctrl+click a word, or
  select text and right-click → **Play from here**, to hear it.
- Runs on the GPU of any brand (NVIDIA, AMD, Intel), or on the CPU when there is none.

**Translate** (new in 1.3)

- Translate a finished transcript into any of 46 languages, among them Arabic, English, French,
  Spanish, German, Turkish, Persian, Urdu, Hindi, Chinese and Danish. Each translated line appears
  under its line as it comes.
- Correct a translation by hand. A line you edit afterwards is marked as out of date, and translating
  again only redoes the lines that changed.
- Export or copy the transcript, the translation, or both: a bilingual Word document, two-line
  subtitles, or a spreadsheet with a translation column.

**Subtitles** (new in 1.3)

- Cut and timed by the rules Netflix and the BBC give their subtitlers: at most two lines of 42
  characters, on screen between 5/6 of a second and 7 seconds and slow enough to read, on when the
  speaker starts and off when they stop, and broken where a sentence ends or the speaker pauses, not
  in the middle of a phrase.
- SRT and WebVTT files of the transcript, of the translation, or of both.
- **Video with subtitles**: the video itself, with a subtitle track for each language. The picture and
  sound are copied untouched, so it takes seconds, even for a long film. MP4 and MOV videos give an
  MP4, everything else an MKV. This needs ffmpeg.

**Edit, search and export**

- Correct lines in place, and search with Arabic in mind: أ/إ/ا, ة/ه and ى/ي match.
- Export to Word, text, Markdown, subtitles (SRT, VTT) or data (CSV, TSV, JSON).
- English or Arabic interface.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/translation.png" alt="An Arabic meeting transcript with the English translation under each line."><br>A meeting in Arabic, with its English translation under each line.</td>
    <td width="50%"><img src="docs/screenshots/export-video.png" alt="The export menu with a choice of original, translation or both, and Video with subtitles at the bottom."><br>Export the original, the translation or both, or add them to the video.</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/home.png" alt="The start page with a drop area for recordings and a choice of spoken language."><br>Drop recordings on the window, or choose a whole folder.</td>
    <td><img src="docs/screenshots/transcribing.png" alt="A recording being transcribed, with its text appearing and two more recordings waiting in the queue."><br>Read the text while it is being written. The rest of the queue waits its turn.</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/search-export.png" alt="A search with every match highlighted, and the export menu open."><br>Search the transcript and export it in eight formats.</td>
    <td><img src="docs/screenshots/play-from-here.png" alt="A selected word with a right-click menu offering Play from here."><br>Select a word and right-click to hear it being said.</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/arabic.png" alt="The same transcript with the interface in Arabic, laid out right to left."><br>The whole interface also comes in Arabic, right to left.</td>
    <td><img src="docs/screenshots/setup.png" alt="The first-run page with three steps, a language menu set to Danish, and a choice of two models with a short description of each."><br>Choose a language and a model. The download survives pausing, closing the app and a lost connection.</td>
  </tr>
</table>

## Getting started

1. Download `Dawwin_x.y.z_x64-setup.exe` from
   [Releases](https://github.com/BaraaJadaan/dawwin/releases) and run it. Windows may say the app is
   unrecognised: choose **More info → Run anyway**. Or use the portable `.zip`: unpack it and run `dawwin.exe`.
2. Choose the language of your recordings, then a model. No account is needed, and you only need the
   models for the languages you use.

   | Model | For | Download | |
   | --- | --- | --- | --- |
   | **Arabic, full quality** (Cohere Q8) | Arabic with its dialects, English | 2.4 GB | The most accurate. About 3 GB of GPU memory. |
   | **Arabic, smaller** (Cohere Q4) | Arabic with its dialects, English | 1.6 GB | Almost as accurate. For weaker PCs and slow connections. About 2 GB of GPU memory. |
   | **European languages** (Canary 1B v2) | 25 languages: Danish, French, Spanish, German, Russian, … | 0.7 GB | Accurate and fast, also without a GPU. |
   | **All languages** (Whisper large-v3 turbo) | About 100 languages: Turkish, Persian, Hindi, … | 0.5 GB | Accurate, but much slower without a GPU. |

   The download can be paused. If the connection drops or Dawwin closes, it carries on from where it
   stopped. Already have the file? Press **I have the file**: any Cohere Transcribe, Canary or Whisper
   GGUF file works.
3. Drop a recording or a video on the window.
4. To translate, pick a language in the menu above the transcript. The first time, Dawwin asks for a
   translation model, which you download in **Settings → Translation**:

   | Model | Download | |
   | --- | --- | --- |
   | **Translation, best quality** (MiLMMT-46 12B) | 7.3 GB | The best of the ten open translation models we tested. Fast with a GPU of 8 GB, slow without one. |
   | **Translation, smaller** (MiLMMT-46 4B) | 2.5 GB | As good as the best one from and into Arabic, a little less in other languages. Faster without a GPU. |

With the Arabic model on an RTX 4060 laptop GPU, 24 minutes of audio take about 40 seconds. On an 8-core
laptop CPU, it runs about 3 to 4 times faster than real time. On a small cloud CPU without a GPU, Canary
ran about 7 times faster than real time, and Whisper about as fast as real time. The smaller
translation model translated a short line in about 2 seconds on a small cloud CPU without a GPU.

**Requirements:** Windows 10 or 11 (64-bit), 8 GB of RAM (16 GB for the best translation model without a
GPU), and room for the models. For WhatsApp voice notes, videos other than MP4 and MOV, and adding
subtitles to a video, install ffmpeg: `winget install Gyan.FFmpeg`.

## How the translation models were chosen

We gave ten open translation models that run on a PC the same sentences from Google's
[WMT24++](https://huggingface.co/datasets/google/wmt24pp) test set, through the same engine, and scored
their translations with COMET-22 (higher is better). The Arabic side of the test is Saudi dialect, close
to how people talk in a meeting.

| Model | Size | Arabic ⇄ English (300 sentences) | 7 directions (280 sentences) |
| --- | --- | --- | --- |
| Tencent Hy-MT2 7B | 4.6 GB | **79.9** | 82.3 (weak in Danish) |
| **Xiaomi MiLMMT-46 12B** | 7.3 GB | 79.2 | **84.3** |
| **Xiaomi MiLMMT-46 4B** | 2.5 GB | 79.0 | 83.7 |
| Google TranslateGemma 12B | 7.3 GB | | 83.3 |
| Tencent Hy-MT2 1.8B | 1.1 GB | 77.6 | 77.5 |
| Xiaomi MiLMMT-46 1B | 0.8 GB | 76.8 | 81.8 |
| PFN PLaMo 3 Translate 8B | 5.3 GB | | 77.1 (no Danish or Turkish) |

The seven directions are Arabic, Danish and Turkish to and from English, and English to French. All the
numbers, and the test itself, are in [tools/bench-translate](tools/bench-translate).

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

Contributions are welcome: bug reports, fixes, translations of the interface and new features. Please
contact me before starting on something bigger, so we can agree on the approach: open an
[issue](https://github.com/BaraaJadaan/dawwin/issues) or reach me on GitHub
([@BaraaJadaan](https://github.com/BaraaJadaan)). Then send a pull request.

If Dawwin is useful to you, a star on GitHub helps other people find it.

## Privacy

Dawwin goes online only when you ask it to: to download a model (from
[Hugging Face](https://huggingface.co)), and to check for updates (from GitHub; also at startup if you
turn that on in Settings). Recordings, transcripts and translations stay on your computer. The
translator listens only to Dawwin on the same computer, with a key it is given at start.

## Uninstall

Uninstall Dawwin from Windows Settings → Apps. The models and your transcripts are kept in case you
reinstall. To remove them too, delete the folders `%LOCALAPPDATA%\Dawwin` and `%APPDATA%\Dawwin` (and
the model folder, if you chose another place for it).

## License

[MIT](LICENSE). Four things are not part of this repository and have their own terms:

- **The speech models**: Cohere publishes its model under Apache-2.0, NVIDIA publishes Canary under CC
  BY 4.0, and OpenAI publishes Whisper under the MIT license. Dawwin downloads GGUF conversions of
  [Cohere](https://huggingface.co/handy-computer/cohere-transcribe-arabic-07-2026-gguf),
  [Canary](https://huggingface.co/handy-computer/canary-1b-v2-gguf) and
  [Whisper](https://huggingface.co/handy-computer/whisper-large-v3-turbo-gguf) made by the
  [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) project.
- **The translation models**: Xiaomi's [MiLMMT-46](https://huggingface.co/collections/xiaomi-research/milmmt-46)
  is built on Google's Gemma 3 and published under the [Gemma Terms of Use](https://ai.google.dev/gemma/terms).
  Dawwin downloads GGUF conversions of the [12B](https://huggingface.co/mradermacher/MiLMMT-46-12B-v1.0-GGUF)
  and the [4B](https://huggingface.co/mradermacher/MiLMMT-46-4B-v1.0-GGUF) made by mradermacher.
- **The engines**: [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) and
  [llama.cpp](https://github.com/ggml-org/llama.cpp), both under the MIT license.
- **The font**: official builds embed [Thmanyah Sans](https://font.thmanyah.com), © Thmanyah Publishing
  and Distribution. Its license does not allow sharing the font files, so builds from this repository
  use the system font instead.
