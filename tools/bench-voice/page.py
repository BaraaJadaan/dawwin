"""Make the listening page from the Voice bench's output.

    page.py <artifacts dir> <listen dir> <key dir>

Each passage's sentences are joined the way Dawwin reads them (a breath between sentences, longer at
a paragraph's end and after a heading), brought to the same loudness, and saved small (Opus). The
candidates get letters in a random order: <listen>/index.html names only the letters, and
<key>/key.html says which model each letter is, with its size, license, speed and memory.
"""
import glob
import html
import json
import os
import random
import string
import subprocess
import sys
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
TEXTS = json.load(open(os.path.join(HERE, "texts.json"), encoding="utf-8"))
CANDIDATES = json.load(open(os.path.join(HERE, "candidates.json"), encoding="utf-8"))["candidates"]
PASSAGES = {p["id"]: p for p in TEXTS["passages"]}

BETWEEN_SENTENCES = 0.30
PARAGRAPH_END = 0.65
AFTER_HEADING = 0.90
BETWEEN_PIECES = 0.06


def read(path):
    with wave.open(path) as w:
        return w.getparams(), w.readframes(w.getnframes())


def silence(params, seconds):
    return b"\0" * (int(params.framerate * seconds) * params.sampwidth * params.nchannels)


def join(parts, out):
    """parts: [(wav path, pause after)]; one WAV at the first part's format."""
    params, frames = None, []
    for path, pause in parts:
        p, data = read(path)
        if params is None:
            params = p
        elif (p.framerate, p.sampwidth, p.nchannels) != (params.framerate, params.sampwidth, params.nchannels):
            raise ValueError(f"{path}: another format")
        frames.append(data)
        frames.append(silence(params, pause))
    with wave.open(out, "wb") as w:
        w.setparams(params)
        w.writeframes(b"".join(frames))


def pauses(passage, n):
    """The pause after each of the n sentences of a passage (heading first, if any)."""
    out = []
    if passage.get("heading"):
        out.append(AFTER_HEADING)
    for para in passage["paragraphs"]:
        out += [BETWEEN_SENTENCES] * (len(para) - 1) + [PARAGRAPH_END]
    return (out + [BETWEEN_SENTENCES] * n)[:n]


def encode(wav, ogg):
    subprocess.run(["ffmpeg", "-nostdin", "-loglevel", "error", "-y", "-i", wav, "-af", "loudnorm=I=-20:TP=-2:LRA=11",
                    "-ac", "1", "-c:a", "libopus", "-b:a", "48k", ogg], check=True)
    r = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", ogg],
                       capture_output=True, text=True)
    return round(float(r.stdout.strip() or 0), 1)


def main(art, listen, keydir):
    os.makedirs(os.path.join(listen, "samples"), exist_ok=True)
    os.makedirs(keydir, exist_ok=True)
    rows, files = [], {}
    for path in glob.glob(os.path.join(art, "**", "results.jsonl"), recursive=True):
        base = os.path.dirname(path)
        for line in open(path, encoding="utf-8"):
            r = json.loads(line)
            r["dir"] = base
            rows.append(r)
        f = os.path.join(base, "files.json")
        if os.path.exists(f):
            files[os.path.basename(base)] = json.load(open(f))
    runtime = "\n".join(open(p, encoding="utf-8", errors="replace").read() for p in glob.glob(os.path.join(art, "**", "runtime.txt"), recursive=True))

    present = sorted({r["candidate"] for r in rows})
    order = present[:]
    random.Random(os.environ.get("GITHUB_RUN_ID", "")).shuffle(order)
    letters = {c: string.ascii_uppercase[i] for i, c in enumerate(order)}
    gallery_numbers = {c: {v: i + 1 for i, v in enumerate(CANDIDATES[c].get("gallery", []))} for c in present}

    samples, problems = [], []
    for r in rows:
        c = r["candidate"]
        sub = r["dir"]
        if r["kind"] == "gallery":
            sub = os.path.join(sub, f"gallery-{r['lang']}-{r['voice'].lstrip('@')}")
        else:
            sub = os.path.join(sub, r["sex"], r["passage"])
        wavs = [os.path.join(sub, s["id"] + ".wav") for s in r["sentences"]]
        missing = [s["id"] for s in r["sentences"] if not s["audio_s"]]
        if missing:
            problems.append(f"{letters[c]} ({c}) {r['passage']} {r['voice']}: no sound for {len(missing)} of {len(wavs)} requests (exit {r['exit']})")
            if len(missing) == len(wavs):
                continue
        name = f"{letters[c]}-{r['passage'].replace('~', '_')}-{r['voice'].lstrip('@')}"
        joined = os.path.join(art, name + ".wav")
        try:
            if r["kind"] == "gallery":
                parts = [(wavs[0], 0.0)]
            elif r["kind"] == "pieces":
                parts = []
                ids = [s["id"] for s in r["sentences"]]
                for k, (wav, rid) in enumerate(zip(wavs, ids)):
                    last_piece = k + 1 == len(ids) or ids[k + 1].split("-")[0] != rid.split("-")[0]
                    parts.append((wav, BETWEEN_SENTENCES if last_piece else BETWEEN_PIECES))
            else:
                parts = list(zip(wavs, pauses(PASSAGES[r["passage"].split("~")[0]], len(wavs))))
            parts = [(w, p) for w, p in parts if os.path.exists(w)]
            join(parts, joined)
            seconds = encode(joined, os.path.join(listen, "samples", name + ".ogg"))
        except Exception as e:  # one broken sample must not cost the page
            problems.append(f"{letters[c]} ({c}) {r['passage']} {r['voice']}: {e}")
            continue
        variant = ""
        if "~" in r["passage"]:
            tail = r["passage"].split("~", 1)[1]
            variant = "English part in the English voice" if tail == "split" else next(
                (e["label"] for e in CANDIDATES[c].get("extra", []) if e["lang"] == tail), tail)
        samples.append({
            "letter": letters[c],
            "passage": r["passage"].split("~")[0] if r["kind"] != "gallery" else f"gallery-{r['lang']}",
            "voice": r["voice"] if r["kind"] != "gallery" else str(gallery_numbers[c].get(r["voice"], r["voice"])),
            "variant": variant,
            "src": f"samples/{name}.ogg",
            "seconds": seconds,
        })

    passages = [{"id": p["id"], "lang": p["lang"], "title": p["title"],
                 "text": ([p["heading"]] if p.get("heading") else []) + [s for para in p["paragraphs"] for s in para]}
                for p in TEXTS["passages"]]
    passages += [{"id": f"gallery-{lang}", "lang": lang, "title": f"Every built-in voice ({'Arabic' if lang == 'ar' else 'English'})",
                  "text": [TEXTS["gallery"][lang]]} for lang in ("ar", "en")]
    data = {"passages": passages, "samples": samples, "letters": sorted(letters.values())}
    page = open(os.path.join(HERE, "listen.html"), encoding="utf-8").read()
    page = page.replace("/*DATA*/null", json.dumps(data, ensure_ascii=False))
    open(os.path.join(listen, "index.html"), "w", encoding="utf-8").write(page)

    # the numbers, by candidate
    stats = {}
    for c in present:
        main_rows = [r for r in rows if r["candidate"] == c and r["kind"] in ("main", "pieces", "extra")]
        timed = [s for r in main_rows for s in r["sentences"] if s["audio_s"] and s["wall_s"]]
        audio = sum(s["audio_s"] for s in timed)
        wall = sum(s["wall_s"] for s in timed)
        firsts = sorted(r["sentences"][0]["wall_s"] for r in main_rows if r["sentences"] and r["sentences"][0]["wall_s"])
        loads = sorted(r["process_s"] - sum(s["wall_s"] or 0 for s in r["sentences"]) for r in main_rows
                       if r["exit"] == 0 and all(s["wall_s"] for s in r["sentences"]))
        stats[c] = {
            "letter": letters[c],
            "rtf": round(wall / audio, 2) if audio else None,
            "first_sentence_s": firsts[len(firsts) // 2] if firsts else None,
            "load_s": round(loads[len(loads) // 2], 1) if loads else None,
            "peak_ram_mb": max([r["peak_ws_mb"] for r in rows if r["candidate"] == c] or [0]),
            "speech_s": round(audio, 1),
            "download_mb": round(sum(files.get(c, {}).values()) / 1e6) if files.get(c) else None,
            "passages_failed": sorted({r["passage"] for r in main_rows if r["exit"] != 0}),
        }
    json.dump({"letters": letters, "stats": stats, "gallery": gallery_numbers, "problems": problems},
              open(os.path.join(keydir, "results.json"), "w", encoding="utf-8"), ensure_ascii=False, indent=1)

    def cell(v, unit=""):
        return "—" if v is None else f"{v}{unit}"

    lines = ["| Letter | Model | Size | License | Speed (time per second of speech) | First sentence | Loading | Peak memory | Failed |",
             "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
    for c in sorted(present, key=lambda k: letters[k]):
        cand, s = CANDIDATES[c], stats[c]
        lines.append(f"| {s['letter']} | {cand['name']} ({cand['maker']}, {cand['params']}) | {cell(s['download_mb'], ' MB')} | {cand['license']} | "
                     f"{cell(s['rtf'], ' s')} | {cell(s['first_sentence_s'], ' s')} | {cell(s['load_s'], ' s')} | {cell(s['peak_ram_mb'], ' MB')} | {', '.join(s['passages_failed']) or 'none'} |")
    gal = []
    for c in sorted(present, key=lambda k: letters[k]):
        if gallery_numbers[c]:
            gal.append(f"- {letters[c]}: " + ", ".join(f"{n} = {v.lstrip('@')}" for v, n in gallery_numbers[c].items()))
    md = ["# Voice bench: the key", "",
          "Measured on a GitHub Actions Windows runner, on its processor (4 threads), with audio.cpp's official Windows build. "
          "Speed is the time taken per second of speech, summed over every sentence (under 1 is faster than real time). "
          "First sentence: the middle value of the time to make a passage's first sentence once the model is loaded. "
          "Loading: the middle value of a process's time outside its sentences. A GPU changes all of these.", ""]
    md += lines + ["", "Built-in voices on the page, by number:", ""] + gal
    md += ["", "Voices that need a recording to speak like read from voices made by VoxCPM 2 from a description (no real person).", ""]
    if problems:
        md += ["Problems:", ""] + [f"- {p}" for p in problems] + [""]
    md += ["Runtime:", "", "```", runtime.strip(), "```", ""]
    open(os.path.join(keydir, "results.md"), "w", encoding="utf-8").write("\n".join(md))
    body = "\n".join(md)
    open(os.path.join(keydir, "key.html"), "w", encoding="utf-8").write(
        "<!doctype html><meta charset=utf-8><title>Voice bench key</title><style>body{font:15px/1.5 system-ui;margin:24px;max-width:1100px}"
        "table{border-collapse:collapse}td,th{border:1px solid #8884;padding:4px 8px;text-align:left}pre{white-space:pre-wrap}</style>"
        f"<pre>{html.escape(body)}</pre>")
    print("\n".join(md))


if __name__ == "__main__":
    main(*sys.argv[1:4])
