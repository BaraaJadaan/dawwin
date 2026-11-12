# Dawwin · دَوِّن

*Dawwin* means "write it down" in Arabic.

**Turn recordings and videos into text, translate it, subtitle them, and write the minutes of a meeting, on your own Windows PC.**
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
  Arabic full of English terms ("الـ dashboard", "الـ API") come out as they were said. Then one press
  writes the minutes: a short summary, the decisions, and who does what by when, written on your own
  computer instead of a cloud chatbot.
- **Colleagues who speak another language.** Translate the transcript and share it on its own, or as a
  bilingual document with each line and its translation under it.
- **Videos, courses and YouTube.** Subtitles timed the way professional ones are. Translate them to
  reach viewers in 46 languages, and add them to the video as tracks the viewer turns on and off, or
  burn them into the picture for WhatsApp, Instagram and players that show no tracks.
- **Films and series.** Subtitles for a film that has none, or none in your language. Dawwin writes
  them into the film's MKV, next to the tracks it already has.
- **Interviews and research.** Exact quotes with their time, every word searchable, and an export to
  CSV or JSON for analysis. Journalists, researchers and students keep their sources private.
- **Lectures, lessons and sermons.** University lectures, Arabic lessons, khutbahs and Quran
  recitations become text you can search and quote, with the recording a click away from any word.
- **Podcasts.** Transcripts and show notes for listeners and for search engines, and the long silences
  cut out of the sound.
- **Voice notes.** WhatsApp voice notes to text.
- **Accessibility.** Captions for people who are deaf or hard of hearing.
- **Learning a language.** Read along with **Follow words**, with the translation under each line.
- **Confidential recordings.** Legal, medical, HR and other private calls are transcribed on your
  own machine, never on someone else's server.

## Features

**One drop, several steps** (new in 1.3)

- Choose on the start page what happens to what you drop, in order: take the long silences out and
  save the sound, transcribe, translate, and add the subtitles to the video, as tracks or burned in.
  Turn on only what you need: Dawwin runs the steps one after the other, and each one can also be done later on its own.

![The start page: a drop area, and under it four steps: Sound, Text (with Write the minutes), Translation and Video.](docs/screenshots/home.png)

**Transcribe**

- Drop recordings and videos on the window: m4a, mp3, wav, flac, mp4, mov and mkv. With ffmpeg, also
  WhatsApp voice notes, webm, avi, wmv, flv, mpg, a camera's mts, m2ts and ts, aiff, caf and ac3:
  Dawwin offers it when a file needs it (a download of 1.3 MB), or uses one already on the PC. Several
  files queue up. The message while you drag a file over the window says what the ticked steps will do.
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
  MP4 (a MOV an MP4 cannot hold, such as ProRes from a camera, stays a MOV), everything else an MKV.
  The video's own subtitles stay exactly as they were, picture-based (PGS, VobSub) and styled (ASS) ones too.
- **Video with subtitles burned in**: the subtitles drawn on the picture, for WhatsApp, Instagram, TVs
  and players that show no tracks. Arabic is joined and runs right to left, its full stops and the
  English words in it in their place, and the picture is encoded again on the graphics chip (NVIDIA,
  AMD or Intel, or Windows' own encoder): about 5 seconds for a minute of 1080p video on an RTX 4060
  laptop. The new file is about the size of the original. The list on the side shows how far it is.
- Both need ffmpeg, which Dawwin downloads the first time you ask: 1.3 MB for subtitle tracks, 3.2 MB
  for burning in. Both are in **Settings**, to download or delete by hand.

**Sound** (new in 1.3)

- **Remove silences**: every pause longer than 0.7 seconds is shortened, and the silence before and
  after the speech goes. Transcribing what is left is faster, and the transcript still has the times
  of the original recording, so playback, Follow words and subtitles stay in step.
- Save the sound as a file of its own (M4A): from a video, or without its silences.

**Minutes** (new in 1.6)

- **Write the minutes** on a transcript's Minutes tab: a summary of a few sentences, the decisions, the
  action items (who, what, by when, with the speakers' names when the transcript has them), the topics
  in order with their start times, and the questions left open. A lecture or an interview gets a summary
  and its topics, without empty decisions and actions.
- Every decision and action shows its time: press it to see the lines it came from in the transcript.
- In the language of the meeting, with its English terms as they were said, or in another language:
  the minutes of an Arabic meeting in English, without translating the transcript first.
- Correct any line by hand; your text is kept. **Write again** after correcting the transcript, with a
  warning that it replaces what you edited.
- A meeting of three hours is written in parts, with the progress shown; stop it and it carries on where
  it stopped, and the minutes cover the whole meeting.
- Export the minutes alone, or followed by the transcript, to Word, Markdown or text. A step on the start
  page writes them after the transcript, and a folder run saves them next to each recording.
- Written on your computer by a model you download once in Settings, chosen by measuring (see below).
  The panel offers the models with their sizes and says which fits your computer.

![A meeting's minutes in Dawwin: the summary, four decisions with their times, and action items with who, what and by when.](docs/screenshots/minutes.png)

**Edit, search and export**

- Correct lines in place, and search with Arabic in mind: أ/إ/ا, ة/ه and ى/ي match.
- Export to Word, text, Markdown, subtitles (SRT, VTT) or data (CSV, TSV, JSON).
- English or Arabic interface, and a **How to use** page with pictures of every part of the app.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/translation.png" alt="An Arabic meeting transcript with the English translation under each line."><br>A meeting in Arabic, with its English translation under each line.</td>
    <td width="50%"><img src="docs/screenshots/export-video.png" alt="The export menu with a choice of original, translation or both, and Video with subtitles at the bottom."><br>Export the original, the translation or both, or add them to the video.</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/transcribing.png" alt="A recording being transcribed, with its text appearing and two more recordings waiting in the queue."><br>Read the text while it is being written. The rest of the queue waits its turn.</td>
    <td><img src="docs/screenshots/search-export.png" alt="A search with every match highlighted, and the export menu open."><br>Search the transcript and export it in eight formats.</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/play-from-here.png" alt="A selected word with a right-click menu offering Play from here."><br>Select a word and right-click to hear it being said.</td>
    <td><img src="docs/screenshots/arabic.png" alt="The same transcript with the interface in Arabic, laid out right to left."><br>The whole interface also comes in Arabic, right to left.</td>
  </tr>
  <tr>
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
   translation model, which you download in **Settings → Translation**. The translator that runs it
   (llama.cpp, 14 MB) comes along with the first one:

   | Model | Download | |
   | --- | --- | --- |
   | **Translation, best quality** (MiLMMT-46 12B) | 7.3 GB | The best of the ten open translation models we tested. Fast with a GPU of 8 GB, slow without one. |
   | **Translation, smaller** (MiLMMT-46 4B) | 2.5 GB | As good as the best one from and into Arabic, a little less in other languages. Faster without a GPU. |
   | **Translation, smallest** (MiLMMT-46 1B) | 0.8 GB | For slow connections and older computers: fast on any of them, a little less accurate. |
5. To write the minutes of a meeting, open the **Minutes** tab of its transcript. The first time, download
   a minutes model there or in **Settings → Minutes**. The translator runs it too:

   | Model | Download | |
   | --- | --- | --- |
   | **Minutes, best** (Jais-3 7B) | 5.6 GB | The best of the 20 open models we tested, in Arabic and in English. Fits a GPU with 8 GB, slow without one. |
   | **Minutes, smaller** (Ministral 3 3B) | 2.1 GB | Good minutes at under half the size. Fits a GPU with 4 GB. |
   | **Minutes, smallest** (Jais-3 0.9B) | 0.7 GB | For older computers: fast on any of them, but it misses more of the decisions and tasks. |

With the Arabic model on an RTX 4060 laptop GPU, 24 minutes of audio take about 40 seconds. On an 8-core
laptop CPU, it runs about 3 to 4 times faster than real time. On a small cloud CPU without a GPU, Canary
ran about 7 times faster than real time, and Whisper about as fast as real time. The smaller
translation model translated a short line in about 2 seconds on a small cloud CPU without a GPU. On the
same kind of CPU, the minutes of a meeting of eight minutes took about 4½ minutes with Jais-3 7B and
under a minute with Jais-3 0.9B.

**Requirements:** Windows 10 or 11 (64-bit), 8 GB of RAM (16 GB for the best translation or minutes model
without a GPU), and room for the models. The installer is small (about 10 MB): the translator and ffmpeg download
only for those who use them.

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

## How the minutes models were chosen

We had 20 open models that run on a PC write the minutes of eight recordings made up for the test, the
way Dawwin asks for them: meetings in Levantine, Gulf and Egyptian Arabic, one in Arabic and English
mixed, a lecture in Modern Standard Arabic, and three in English, each also in the other language and
two in parts as a long meeting goes. Their minutes were scored against minutes written by hand: the
decisions and tasks found (at the lines they were said), who and by when, the key facts in the summary,
the topics, and whether the minutes are in the language asked for. Score out of 100:

| Model | Size | Score | Arabic | English | In the other language | Decisions found | Tasks found |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Jais-3 7B** | 5.6 GB | **88.7** | **89.8** | 86.9 | **91.6** | 83 % | 87 % |
| Mistral Ministral 3 8B | 5.2 GB | 85.8 | 87.5 | 82.9 | 70.3 | 88 % | 93 % |
| Google Gemma 4 12B | 7.1 GB | 83.9 | 82.5 | 86.2 | 88.8 | 88 % | 93 % |
| IBM Granite 4.2 8B | 5.4 GB | 82.6 | 82.2 | 83.3 | 66.7 | 83 % | 87 % |
| Alibaba Qwen3.5 4B | 2.7 GB | 80.0 | 81.2 | 77.9 | 79.1 | 88 % | 97 % |
| Alibaba Qwen3.5 9B | 5.7 GB | 79.2 | 76.9 | 83.2 | 82.5 | 92 % | 90 % |
| **Mistral Ministral 3 3B** | 2.1 GB | 78.7 | 81.2 | 74.5 | 72.6 | 71 % | 100 % |
| Mistral Ministral 3 14B | 8.2 GB | 76.0 | 76.0 | 76.0 | 64.1 | 92 % | 97 % |
| Jais-3 3.7B | 3.2 GB | 74.1 | 69.9 | 80.9 | 72.0 | 84 % | 90 % |
| Google Gemma 4 E2B | 3.1 GB | 72.2 | 71.1 | 74.0 | 70.4 | 76 % | 83 % |
| **Jais-3 0.9B** | 0.7 GB | 65.0 | 61.5 | 70.8 | 56.9 | 56 % | 63 % |
| Alibaba Qwen3.5 2B | 1.3 GB | 48.3 | 36.6 | 67.9 | 62.4 | 85 % | 80 % |
| Alibaba Qwen3.5 0.8B | 0.5 GB | 26.7 | 25.0 | 29.5 | 26.8 | 53 % | 44 % |

Most of the lower scores in Arabic are minutes written in English when Arabic was asked for. Qwen3.5 4B
finds as much as the best, but its summaries of the Arabic meetings say nothing ("no one attended").
Dawwin offers the best (Jais-3 7B, which also fits a GPU with 8 GB, its context kept in 8 bits), the best
for its size (Ministral 3 3B) and the smallest that is still usable (Jais-3 0.9B). With the context in
8 bits, as on a GPU, they scored 87.6, 77.9 and 70.7. The models, the scoring and all the numbers are in
[tools/bench-minutes](tools/bench-minutes/results-1.6.0.md).

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
[issue](https://github.com/BaraaJadaan/dawwin/issues), email me at
[braajad1@gmail.com](mailto:braajad1@gmail.com), or reach me on GitHub
([@BaraaJadaan](https://github.com/BaraaJadaan)). Then send a pull request.

Questions, ideas and problems are welcome by email too: [braajad1@gmail.com](mailto:braajad1@gmail.com).

If Dawwin is useful to you, a star on GitHub helps other people find it.

## Privacy

Dawwin goes online only when you ask it to: to download a model (from
[Hugging Face](https://huggingface.co)) or a tool (the translator and ffmpeg, from this repository's
[releases](https://github.com/BaraaJadaan/dawwin/releases)), and to check for updates (from GitHub; also at startup if you
turn that on in Settings). Recordings, transcripts, translations and minutes stay on your computer. The
translator, which also writes the minutes, listens only to Dawwin on the same computer, with a key it is
given at start.

## Uninstall

Uninstall Dawwin from Windows Settings → Apps. The models and your transcripts are kept in case you
reinstall. To remove them too, delete the folders `%LOCALAPPDATA%\Dawwin` and `%APPDATA%\Dawwin` (and
the model folder, if you chose another place for it).

## License

[MIT](LICENSE). Six things are not part of this repository and have their own terms:

- **The speech models**: Cohere publishes its model under Apache-2.0, NVIDIA publishes Canary under CC
  BY 4.0, and OpenAI publishes Whisper under the MIT license. Dawwin downloads GGUF conversions of
  [Cohere](https://huggingface.co/handy-computer/cohere-transcribe-arabic-07-2026-gguf),
  [Canary](https://huggingface.co/handy-computer/canary-1b-v2-gguf) and
  [Whisper](https://huggingface.co/handy-computer/whisper-large-v3-turbo-gguf) made by the
  [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) project.
- **The translation models**: Xiaomi's [MiLMMT-46](https://huggingface.co/collections/xiaomi-research/milmmt-46)
  is built on Google's Gemma 3 and published under the [Gemma Terms of Use](https://ai.google.dev/gemma/terms).
  Dawwin downloads GGUF conversions of the [12B](https://huggingface.co/mradermacher/MiLMMT-46-12B-v1.0-GGUF),
  the [4B](https://huggingface.co/mradermacher/MiLMMT-46-4B-v1.0-GGUF) and the
  [1B](https://huggingface.co/mradermacher/MiLMMT-46-1B-v1.0-GGUF) made by mradermacher.
- **The minutes models**: [Jais-3](https://huggingface.co/inception42/Jais-3-7B) and Mistral AI's
  [Ministral 3](https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512) are published under
  Apache-2.0. Dawwin downloads the GGUF files of Jais-3 [7B](https://huggingface.co/IFM/Jais-3-7B-GGUF)
  and [0.9B](https://huggingface.co/IFM/Jais-3-0.9B-GGUF) published by IFM, and of
  [Ministral 3 3B](https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512-GGUF) published by Mistral AI.
- **The engines**: [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) and
  [llama.cpp](https://github.com/ggml-org/llama.cpp), both under the MIT license.
- **ffmpeg**: Dawwin's own builds of [FFmpeg](https://ffmpeg.org), under the LGPL 2.1, made by
  [tools/ffmpeg/build.sh](tools/ffmpeg/build.sh) from FFmpeg's source. The one for video also holds
  [libass](https://github.com/libass/libass), [FreeType](https://freetype.org),
  [HarfBuzz](https://github.com/harfbuzz/harfbuzz) and [FriBidi](https://github.com/fribidi/fribidi).
  Each download carries their licenses and says where their source is.
- **The font**: official builds embed [Thmanyah Sans](https://font.thmanyah.com), © Thmanyah Publishing
  and Distribution. Its license does not allow sharing the font files, so builds from this repository
  use the system font instead.
