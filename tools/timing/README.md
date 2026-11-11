# Timing bench

How well each way of timing the words of a line places them, against recordings in which the
moment every word starts is known.

1. `pwsh tools/timing/speak.ps1` makes the recordings in `tools/timing/out` (ignored by git) from
   `talks.json`, with the voices of Windows, which report when each word starts. The talks stop in
   the middle of sentences for 0.3 to 2.5 s and change speed from phrase to phrase. The Arabic one
   has punctuation and English words in it.
2. From the VS 2022 prompt, in `src-tauri`:

   ```bash
   DAWWIN_TIMING_DIR=../tools/timing/out DAWWIN_TIMING_MODEL=<model.gguf> cargo test --locked --lib timing::tests::bench -- --ignored --nocapture
   ```

   It transcribes each recording as the queue does (with a quiet room's noise added), runs the
   timing pass, and scores every method. `DAWWIN_TIMING_ENGINE` points at another engine,
   `DAWWIN_TIMING_CPU` keeps it on the CPU, `DAWWIN_TIMING_ONLY` picks recordings by name.

## Results, 2026-11-11

Cohere Transcribe Arabic Q8 on an RTX 4060 laptop; 316 s of speech, 640 words, 3 recordings.

| Words timed by | Within 0.5 s | Median error | 90th percentile | Over 0.5 s early | Subtitles gone before their last word starts | On over 0.5 s late |
|---|---|---|---|---|---|---|
| Subtitles until 1.3.2 (share of the text over the speech, pauses by share of the speech) | 47 % | 0.60 s | 3.21 s | 37 % | 23 of 63 | 12 |
| Follow words until 1.3.2 (share of the text over the speech profile) | 42 % | 0.65 s | 2.96 s | 40 % | | |
| Timing pass | **98 %** | **0.08 s** | **0.25 s** | 1 % | **2 of 61** | 1 |
| Guess, a subtitle held 0.5 s into the speech after a pause | | | | | 17 of 63 | 17 |
| Guess, held 0.8 s (kept) | | | | | 14 of 63 | 23 |
| Guess, held 1.2 s | | | | | 10 of 63 | 27 |

The timing pass took 1.5 to 1.7 times as long as the transcription (8.0 s after 5.5 s for the
108 s lecture). The maintainer's example comes back in the lecture: until 1.3.2 the subtitle
"meaning it's not pegged / on or subject to what" was gone before "subject to" was said.

In transcribe.cpp 0.4.1 no model Dawwin uses gives word times: Cohere and Canary give none, Whisper
only times whole phrases. So there is no model truth to measure against; the voices of Windows are.
Their speech is evener than a person's; a test on real speech is still owed.
