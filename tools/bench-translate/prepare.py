"""Pick the test sentences from Google's WMT24++ (Apache 2.0).

The Arabic side is Saudi dialect, close to how people speak in a meeting. Sentences from the news,
speech (YouTube captions) and social domains, one line each, the same for every model.
"""
import json
import os
import random
import sys
import urllib.request

ALL_FILES = {"ar": "en-ar_SA", "da": "en-da_DK", "tr": "en-tr_TR", "fr": "en-fr_FR"}
# BENCH_DIRECTIONS="ar-en,en-ar" and BENCH_N=150 for a closer look at a few directions
DIRECTIONS = [tuple(d.split("-")) for d in os.environ.get("BENCH_DIRECTIONS", "ar-en,en-ar,da-en,en-da,tr-en,en-tr,en-fr").split(",")]
PER_DIRECTION = int(os.environ.get("BENCH_N", "40"))
FILES = {lang: name for lang, name in ALL_FILES.items() if any(lang in d for d in DIRECTIONS)}


def load(name):
    url = f"https://huggingface.co/datasets/google/wmt24pp/resolve/main/{name}.jsonl"
    text = urllib.request.urlopen(url, timeout=120).read().decode("utf-8")
    rows = [json.loads(line) for line in text.splitlines() if line.strip()]
    return {r["segment_id"]: r for r in rows}


def usable(r):
    return (
        not r["is_bad_source"]
        and r["domain"] in ("news", "speech", "social")
        and 30 <= len(r["source"]) <= 350
        and "\n" not in r["source"]
        and "\n" not in r["target"]
        and r["target"].strip()
    )


def main(out):
    data = {lang: load(name) for lang, name in FILES.items()}
    ids = sorted(set.intersection(*[{i for i, r in d.items() if usable(r)} for d in data.values()]))
    random.Random(2026).shuffle(ids)
    # sentences English → X and X → English are different ones
    into, out_of = ids[:PER_DIRECTION], ids[PER_DIRECTION : 2 * PER_DIRECTION]
    tests = []
    for src, tgt in DIRECTIONS:
        lang = tgt if src == "en" else src
        for i in into if src == "en" else out_of:
            r = data[lang][i]
            source, reference = (r["source"], r["target"]) if src == "en" else (r["target"], r["source"])
            tests.append({"id": f"{src}-{tgt}-{i}", "src": src, "tgt": tgt, "domain": r["domain"], "source": source, "reference": reference})
    with open(out, "w", encoding="utf-8") as f:
        for t in tests:
            f.write(json.dumps(t, ensure_ascii=False) + "\n")
    print(f"{len(tests)} sentences, {len(ids)} usable")


if __name__ == "__main__":
    main(sys.argv[1])
