# Minutes bench

Which local models Dawwin offers for writing minutes. Every candidate writes the minutes of the same
made-up recordings, the way the app asks for them, and the answers are scored against minutes written
by hand. It runs on GitHub Actions (the **Minutes bench** workflow), on a CPU runner, so that no model
is downloaded on a contributor's connection.

## The recordings

`meetings/` holds eight transcripts made up for the test (no real meeting), as speech recognition
would give them, with fillers, restarts and people talking over each other, and for each one the hand-made
answer: its kind, the decisions, the action items (who, what, by when), the open questions, the topics,
and the key facts a short summary should mention, each tied to the lines it was said on.

| Recording | Language | Speakers | Lines |
| --- | --- | --- | --- |
| `ar-levant-product` | Levantine Arabic with English terms, a product team | named | 90 |
| `ar-gulf-events` | Gulf Arabic, planning a conference | numbered ("Speaker 2"), names only in what is said | 78 |
| `ar-egypt-campaign` | Egyptian Arabic, a marketing campaign | none | 88 |
| `mix-startup` | Jordanian Arabic and English, switching all the time | named | 89 |
| `ar-lecture-water` | Modern Standard Arabic, a lecture | none | 46 |
| `en-sprint` | English, a software team | named | 91 |
| `en-committee` | English, a school committee | none | 80 |
| `en-interview` | English, a podcast interview | named | 50 |

The meetings hold traps: a proposal that is turned down, a task that moves to someone else, a day
that is corrected ("Thursday, no, Wednesday"), dates that are not deadlines, a task with no deadline,
and things "we'll think about" that are not decisions. The answers list them under `traps`.

## What is run

`run.py` starts llama.cpp's server (the build Dawwin downloads) with the model and asks it, through
`/v1/chat/completions`, with the prompts and the JSON schema in `src-tauri/prompts` that the app uses
(a test in the app checks it asks the same, from `same_as_app.json`). Greedy decoding, thinking off,
room for 16,384 tokens with their cache in 8 bits, as the app keeps it on a GPU (`BENCH_CACHE=f16` keeps
it in 16 bits, as the app does on the processor and the first run of all the models did). Each model writes the minutes of every recording in its own language, three
in the other language (two Arabic meetings in English, the English sprint in Arabic), and two meetings
in parts of three minutes, notes first and then the minutes from the notes, the way a long recording
goes in the app.

## How it is scored

`score.py`. An item the model gives is one of the answer's when one of the lines it cites is within
two lines of one of the answer's lines (the app links every item to its lines, so an item citing the
wrong lines is wrong there too); of several such, the one with the same owner and the most words in
common is taken. Each answer item counts once.

- **Decisions found / actions found**: the share of the answer's items found. **Invented**: items given
  that match none of the answer's.
- **Owner right**: of the actions found, the share whose owner is the right person (any of the
  person's spellings, Arabic or Latin). **Deadline right**: the deadline as said, or none when there is none.
- **Summary facts**: the share of the key facts the summary mentions.
- **Topics**: topics that start within three lines of the answer's.
- **Kind**: meeting, lecture or interview.
- **Language right**: the minutes are in the language asked for (Arabic minutes may keep English terms).
- **Score**, 0 to 100, per recording: for a meeting, 25 % decisions, 25 % actions (both F1), 10 % owners,
  10 % deadlines, 15 % summary facts, 10 % topics, 5 % kind; for a lecture or an interview, 40 % summary
  facts, 30 % topics, 15 % kind, 15 % for giving no decisions or actions. Minutes in the wrong language
  count half.

Text is compared after folding Arabic letter forms (أ/إ/ا, ة/ه, ى/ي), diacritics, digits and case.
The scores measure finding the right things and citing them; they do not judge how well the summary
is written, which was read by hand for the finalists.

## Running it

From the Actions tab: **Minutes bench**, with model keys to run some of them. The table is in the
`results` artifact. The runs that chose the models Dawwin offers (since 1.8.0) are in [results-1.8.0.md](results-1.8.0.md). Locally, against Dawwin's stand-in server (no model needed):

```bash
cargo build --example fake_translator --manifest-path src-tauri/Cargo.toml
echo any > model.gguf
mkdir out-fake
python tools/bench-minutes/run.py fake model.gguf src-tauri/target/debug/examples/fake_translator.exe out-fake
python tools/bench-minutes/score.py . results.md
```
