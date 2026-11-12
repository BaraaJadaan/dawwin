# Minutes bench, 1.6.0

How the minutes models Dawwin 1.6.0 offers were chosen. The test and the scoring are in
[README.md](README.md); the models are the files at the revisions in
[the workflow](../../.github/workflows/minutes-bench.yml).

- **Run 43131826608** (12 November 2026): 21 models on GitHub's 4-core CPU runners, the context cache
  in 16 bits. ALLaM 7B failed without a log and is not in the table.
- **Run 43139830537**: Qwen3.5 4B and 9B again, after the schema came to ask for a summary of at least
  20 characters (Qwen3.5 4B had left it empty on the Arabic meetings). Their rows are from this run.
- **Run 43178418953**: the three chosen, with the context cache in 8 bits, as the app keeps it on a GPU.

Left out before the run: Liquid's LFM2.5 (its licence caps the users' revenue), Cohere's Tiny Aya
(non-commercial), Jais-2 (gated), Meta's Llama (gated, its own licence) and SmolLM3 (no Arabic). The
runners have 16 GB of memory, so nothing larger than about 14B was measured.

## All the models (16-bit cache)

Minutes of 8 made-up recordings (6 meetings, the rest a lecture and an interview), Q4_K_M unless named, greedy, on a 4-core CPU runner. Score 0–100 (see score.py). Decisions and actions: share of the answer's items found, and items given that match none (invented or misplaced).

| Model | GB | Score | Arabic | English | Other language | In parts | Decisions found | invented | Actions found | invented | Owner right | Deadline right | Summary facts | Topics | Kind | Language right | Broken | Prompt tok/s | Output tok/s | s per run |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| jais3-7b | 5.59 | **88.7** | 89.8 | 86.9 | 91.6 | 75.5 | 83 | 1 | 87 | 0 | 96 | 86 | 85 | 75 | 100 | 100 | 0 | 20.9 | 4.1 | 267.2 |
| ministral3-8b | 5.20 | **85.8** | 87.5 | 82.9 | 70.3 | 77.1 | 88 | 5 | 93 | 3 | 100 | 97 | 83 | 66 | 100 | 92 | 0 | 17.1 | 3.0 | 445.1 |
| gemma4-12b | 7.12 | **83.9** | 82.5 | 86.2 | 88.8 | 82.1 | 88 | 3 | 93 | 1 | 96 | 90 | 85 | 81 | 100 | 92 | 0 | 13.0 | 3.1 | 478.6 |
| granite42-8b | 5.35 | **82.6** | 82.2 | 83.3 | 66.7 | 54.3 | 83 | 16 | 87 | 2 | 88 | 85 | 87 | 66 | 100 | 92 | 0 | 10.4 | 2.9 | 721.2 |
| qwen35-4b | 2.74 | **80.0** | 81.2 | 77.9 | 79.1 | 71.2 | 88 | 6 | 97 | 3 | 93 | 79 | 45 | 77 | 100 | 100 | 0 | 25.4 | 8.0 | 214.1 |
| qwen35-9b | 5.68 | **79.2** | 76.9 | 83.2 | 82.5 | 71.9 | 92 | 11 | 90 | 4 | 100 | 78 | 78 | 75 | 100 | 92 | 0 | 23.6 | 5.3 | 286.2 |
| ministral3-3b | 2.15 | **78.7** | 81.2 | 74.5 | 72.6 | 71.5 | 71 | 6 | 100 | 7 | 90 | 73 | 74 | 71 | 100 | 100 | 0 | 26.7 | 7.2 | 234.3 |
| ministral3-14b | 8.24 | **76.0** | 76.0 | 76.0 | 64.1 | 73.3 | 92 | 12 | 97 | 0 | 97 | 90 | 74 | 65 | 100 | 85 | 0 | 7.0 | 2.6 | 743.6 |
| jais3-3.7b | 3.16 | **74.1** | 69.9 | 80.9 | 72.0 | 54.0 | 84 | 14 | 90 | 1 | 93 | 85 | 80 | 83 | 100 | 69 | 0 | 34.9 | 5.9 | 197.9 |
| gemma4-e2b | 3.11 | **72.2** | 71.1 | 74.0 | 70.4 | 70.3 | 76 | 6 | 83 | 0 | 92 | 83 | 63 | 79 | 100 | 85 | 0 | 144.5 | 10.0 | 97.3 |
| gemma4-e4b | 4.98 | **71.1** | 71.1 | 71.2 | 69.9 | 69.7 | 88 | 8 | 77 | 2 | 90 | 88 | 71 | 67 | 100 | 85 | 0 | 35.8 | 6.9 | 197.5 |
| jais3-0.9b | 0.67 | **65.0** | 61.5 | 70.8 | 56.9 | 55.2 | 56 | 12 | 63 | 3 | 87 | 59 | 54 | 80 | 100 | 92 | 0 | 125.3 | 24.8 | 46.7 |
| qwen35-2b-q8 | 2.01 | **61.0** | 51.3 | 77.2 | 60.2 | 47.2 | 76 | 12 | 73 | 22 | 59 | 62 | 65 | 60 | 100 | 77 | 0 | 48.2 | 15.1 | 129.4 |
| fanar1-9b | 5.38 | **59.1** | 52.1 | 70.8 | 59.5 | 54.6 | 79 | 11 | 77 | 1 | 96 | 87 | 45 | 73 | 100 | 62 | 0 | 15.4 | 3.1 | 405.3 |
| granite42-3b | 2.24 | **50.7** | 39.6 | 69.3 | 50.2 | 38.0 | 76 | 19 | 67 | 9 | 75 | 67 | 50 | 53 | 88 | 69 | 0 | 24.5 | 4.8 | 346.6 |
| qwen35-2b | 1.28 | **48.3** | 36.6 | 67.9 | 62.4 | 60.8 | 85 | 15 | 80 | 6 | 43 | 53 | 57 | 54 | 88 | 77 | 1 | 65.9 | 17.9 | 106.3 |
| phi4-mini | 2.49 | **45.1** | 29.2 | 71.5 | 50.9 | 60.0 | 82 | 20 | 63 | 28 | 72 | 61 | 37 | 46 | 88 | 62 | 0 | 25.8 | 7.0 | 203.9 |
| qwen35-0.8b-q8 | 0.81 | **31.5** | 29.6 | 34.7 | 32.1 | 32.9 | 65 | 47 | 40 | 76 | 60 | 12 | 32 | 54 | 75 | 77 | 0 | 165.6 | 30.4 | 76.4 |
| qwen35-0.8b | 0.53 | **26.7** | 25.0 | 29.5 | 26.8 | 0.0 | 53 | 16 | 44 | 22 | 40 | 33 | 27 | 53 | 62 | 46 | 4 | 244.2 | 40.0 | 52.7 |
| milmmt46-4b | 2.49 | **16.2** | 14.1 | 19.8 | 33.0 | 12.2 | 35 | 71 | 13 | 31 | 25 | 25 | 6 | 29 | 75 | 54 | 1 | 28.0 | 8.1 | 303.9 |

Per recording (score):

| Model | ar-egypt-campaign.ar | ar-gulf-events.ar | ar-gulf-events.en | ar-lecture-water.ar | ar-levant-product.ar | ar-levant-product.ar.parts180 | ar-levant-product.en | en-committee.en | en-interview.en | en-sprint.ar | en-sprint.en | en-sprint.en.parts180 | mix-startup.ar |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| jais3-7b | 87.4 | 89.2 | 90.9 | 91.8 | 91.7 | 73.0 | 88.3 | 84.5 | 89.0 | 95.7 | 87.2 | 78.1 | 89.0 |
| ministral3-8b | 90.2 | 93.3 | 92.7 | 79.0 | 87.6 | 79.2 | 80.6 | 82.8 | 76.7 | 37.7 | 89.3 | 75.0 | 87.3 |
| gemma4-12b | 93.6 | 86.6 | 88.0 | 95.0 | 92.0 | 85.4 | 85.6 | 85.0 | 80.7 | 92.9 | 92.9 | 78.7 | 45.3 |
| granite42-8b | 80.6 | 84.4 | 87.2 | 94.0 | 76.5 | 37.0 | 72.6 | 82.1 | 80.7 | 40.2 | 87.2 | 71.6 | 75.3 |
| qwen35-4b | 78.4 | 81.0 | 77.1 | 83.4 | 80.8 | 73.3 | 86.0 | 80.2 | 75.7 | 74.1 | 77.8 | 69.1 | 82.4 |
| qwen35-9b | 86.4 | 88.6 | 88.7 | 90.0 | 41.0 | 73.3 | 81.3 | 81.8 | 83.3 | 77.4 | 84.6 | 70.4 | 78.3 |
| ministral3-3b | 80.4 | 82.6 | 68.8 | 86.7 | 88.3 | 70.3 | 75.3 | 77.1 | 61.7 | 73.8 | 84.7 | 72.7 | 67.9 |
| ministral3-14b | 81.0 | 87.8 | 72.4 | 79.0 | 88.3 | 71.8 | 78.9 | 75.2 | 61.7 | 41.0 | 91.3 | 74.9 | 43.8 |
| jais3-3.7b | 40.7 | 81.7 | 84.1 | 91.4 | 92.0 | 36.0 | 85.6 | 73.6 | 79.7 | 46.4 | 89.4 | 72.1 | 44.0 |
| gemma4-e2b | 82.8 | 80.2 | 85.7 | 71.4 | 80.0 | 71.7 | 83.2 | 82.0 | 58.1 | 42.2 | 81.8 | 68.9 | 41.1 |
| gemma4-e4b | 73.4 | 81.5 | 86.7 | 73.0 | 84.8 | 65.0 | 77.3 | 81.2 | 52.6 | 45.6 | 79.6 | 74.3 | 42.7 |
| jais3-0.9b | 65.4 | 72.5 | 64.1 | 67.0 | 71.2 | 52.2 | 66.4 | 74.8 | 56.0 | 40.3 | 81.4 | 58.2 | 31.7 |
| qwen35-2b-q8 | 28.1 | 35.5 | 75.6 | 66.0 | 60.0 | 29.2 | 66.1 | 75.6 | 75.4 | 38.8 | 80.6 | 65.2 | 66.8 |
| fanar1-9b | 38.1 | 73.6 | 62.4 | 67.8 | 41.2 | 36.2 | 79.5 | 75.9 | 65.8 | 36.6 | 70.7 | 73.0 | 39.8 |
| granite42-3b | 29.6 | 32.9 | 62.8 | 55.0 | 57.3 | 24.2 | 67.3 | 78.1 | 48.9 | 20.6 | 80.8 | 51.8 | 23.1 |
| qwen35-2b | 28.9 | 32.4 | 60.5 | 63.0 | 0.0 | 58.2 | 73.3 | 71.7 | 58.8 | 53.5 | 73.3 | 63.4 | 58.6 |
| phi4-mini | 22.3 | 24.2 | 56.7 | 46.0 | 21.4 | 54.2 | 62.6 | 70.9 | 71.2 | 33.3 | 72.3 | 65.8 | 32.3 |
| qwen35-0.8b-q8 | 18.9 | 21.1 | 30.5 | 33.0 | 49.1 | 26.1 | 29.0 | 37.3 | 24.2 | 36.7 | 42.5 | 39.6 | 26.1 |
| qwen35-0.8b | 14.3 | 25.8 | 49.9 | 31.1 | 33.5 | 0.0 | 0.0 | 0.0 | 24.2 | 30.5 | 64.2 | 0.0 | 20.4 |
| milmmt46-4b | 9.5 | 9.8 | 52.9 | 0.0 | 38.6 | 12.5 | 33.7 | 25.0 | 14.3 | 12.5 | 20.1 | 11.9 | 12.5 |

## The three chosen, with the cache in 8 bits

| Model | GB | Score | Arabic | English | Other language | In parts | Decisions found | invented | Actions found | invented | Owner right | Deadline right | Summary facts | Topics | Kind | Language right | Broken | Prompt tok/s | Output tok/s | s per run |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| jais3-7b | 5.59 | **87.6** | 89.5 | 84.6 | 89.6 | 80.1 | 83 | 1 | 87 | 1 | 100 | 90 | 78 | 77 | 100 | 100 | 0 | 14.1 | 3.7 | 337.0 |
| ministral3-3b | 2.15 | **77.9** | 80.6 | 73.4 | 72.4 | 71.1 | 75 | 10 | 100 | 5 | 90 | 67 | 78 | 76 | 100 | 100 | 0 | 16.4 | 5.7 | 336.2 |
| jais3-0.9b | 0.67 | **70.7** | 71.0 | 70.3 | 66.8 | 52.9 | 75 | 12 | 83 | 0 | 84 | 75 | 57 | 77 | 100 | 92 | 0 | 35.6 | 11.7 | 129.4 |

The scores move by about a point either way (Jais-3 0.9B by more, up), and the order stays. On these
CPUs the 8-bit cache was slower (337 s a recording for Jais-3 7B, against 267 s), so the app keeps it
in 8 bits only on a GPU, where memory is short.

## The choice

The maintainer's rule fills four places:

1. **The best, period**: Jais-3 7B (88.7), best in Arabic, in the other language and in parts, every
   time in the language asked for, and next to nothing invented.
2. **The best that fits a PC with an 8 GB GPU** (an RTX 4060 laptop, 16 GB of memory): Jais-3 7B again.
   With 16k of context its cache takes 2.4 GB in 16 bits (8.5 GB in all, too much), 1.3 GB in 8 bits
   (7.3 GB). Being the same model, this place is not filled twice.
3. **The best for its size**: Ministral 3 3B (78.7 at 2.1 GB). Qwen3.5 4B scores a little higher
   (80.0 at 2.7 GB) and finds the most tasks, but its summaries of the Arabic meetings are wrong
   ("no one attended"; empty before the schema asked for one).
4. **The smallest that is not bad**: Jais-3 0.9B (65.0 at 0.7 GB; 70.7 with the 8-bit cache). The
   smaller Qwen3.5 0.8B (26.7) is bad, and Qwen3.5 2B (48.3) writes English minutes when Arabic is asked.

Most of the low Arabic scores are minutes in English when Arabic was asked for (read by hand: the
scorer's language check is right about them).
