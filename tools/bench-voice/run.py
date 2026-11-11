"""Speak the bench texts with one candidate through audio.cpp's own CLI, the way Dawwin reads:
one request per sentence, the model loaded once per passage.

    run.py <candidate> <audiocpp_cli> <models dir> <out dir> [--sexes m,f] [--refs DIR]
           [--backend cpu] [--threads 4] [--budget-min 300]
    run.py --make-refs <audiocpp_cli> <models dir> <refs dir>

Writes <out>/<candidate>/<voice>/<passage>/<nn>.wav and one line per passage in
<out>/<candidate>/results.jsonl: the time of each sentence, its length, the passage's process time,
and the peak memory (working set) of the process. Passages are done in the order of texts.json
and stop being started once the budget is spent.

--make-refs makes the ready-made voices (texts.json "refs") with VoxCPM 2's voice design.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import threading
import time
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
TEXTS = json.load(open(os.path.join(HERE, "texts.json"), encoding="utf-8"))
CANDIDATES = json.load(open(os.path.join(HERE, "candidates.json"), encoding="utf-8"))["candidates"]


def fetch(key, cand, models):
    """The model's files, and the voice files it names, from Hugging Face."""
    from huggingface_hub import hf_hub_download, list_repo_files

    dest = os.path.join(models, key)
    paths = []
    for f in cand["files"]:
        for attempt in range(5):
            try:
                paths.append(hf_hub_download(cand["repo"], f, local_dir=dest))
                break
            except Exception as e:  # a flaky connection: try again
                print(f"download {f}: {e}", flush=True)
                time.sleep(10 * (attempt + 1))
        else:
            sys.exit(f"could not download {f}")
    ids = {v for per in cand.get("voices", {}).values() for v in per.values() if v.startswith("@")}
    ids |= {v for v in cand.get("gallery", []) if v.startswith("@")}
    voice_files = {}
    if ids:
        listing = list_repo_files(cand["voice_repo"])
        for vid in sorted(ids):
            found = [f for f in listing if vid[1:] in f and f.endswith(".safetensors")]
            if not found:
                sys.exit(f"no voice file for {vid} in {cand['voice_repo']}")
            voice_files[vid] = hf_hub_download(cand["voice_repo"], found[0], local_dir=os.path.join(dest, "voices"))
    sizes = {os.path.basename(p): os.path.getsize(p) for p in paths}
    print(json.dumps(sizes), flush=True)
    return os.path.dirname(paths[0]), paths[0], voice_files, sizes


def wav_seconds(path):
    try:
        with wave.open(path) as w:
            return w.getnframes() / w.getframerate()
    except (wave.Error, EOFError, FileNotFoundError):
        return 0.0


def watch(proc):
    """Peak working set and peak commit of a running process, in MB."""
    import psutil

    peak_ws = peak_commit = 0
    try:
        pp = psutil.Process(proc.pid)
        while proc.poll() is None:
            try:
                mi = pp.memory_info()
                peak_ws = max(peak_ws, getattr(mi, "peak_wset", mi.rss))
                peak_commit = max(peak_commit, getattr(mi, "peak_pagefile", getattr(mi, "vms", 0)))
            except psutil.Error:
                break
            time.sleep(0.1)
    except psutil.Error:
        pass
    proc.wait()
    return round(peak_ws / 2**20), round(peak_commit / 2**20)


METRIC = re.compile(r"metrics\[([^\]]+)\]\.(\w+)\s*[=:]?\s*(-?[0-9.]+)")


def run_sequence(args, cand, model_arg, requests, outdir):
    os.makedirs(outdir, exist_ok=True)
    seq = os.path.join(outdir, "requests.json")
    with open(seq, "w", encoding="utf-8") as f:
        json.dump({"requests": requests}, f, ensure_ascii=False, indent=1)
    cmd = [args.cli, "--task", "tts", "--family", cand["family"], "--model", model_arg, "--backend", args.backend,
           "--threads", str(args.threads), "--request-sequence", seq, "--out-dir", outdir, "--metrics"]
    for k, v in cand.get("session_options", {}).items():
        cmd += ["--session-option", f"{k}={v}"]
    log_path = os.path.join(outdir, "log.txt")
    started = time.perf_counter()
    with open(log_path, "w", encoding="utf-8", errors="replace") as log:
        proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, cwd=os.path.dirname(args.cli))
        peak_ws, peak_commit = watch(proc)
    wall = time.perf_counter() - started
    text = open(log_path, encoding="utf-8", errors="replace").read()
    metrics = {}
    for rid, name, value in METRIC.findall(text):
        metrics.setdefault(rid, {})[name] = float(value)
    sentences = []
    for r in requests:
        audio = wav_seconds(os.path.join(outdir, r["id"] + ".wav"))
        m = metrics.get(r["id"], {})
        sentences.append({"id": r["id"], "audio_s": round(audio, 3), "wall_s": round(m["wall_ms"] / 1000, 3) if "wall_ms" in m else None})
    return {
        "exit": proc.returncode,
        "process_s": round(wall, 2),
        "peak_ws_mb": peak_ws,
        "peak_commit_mb": peak_commit,
        "sentences": sentences,
        "log_tail": "\n".join(text.splitlines()[-25:]) if proc.returncode else "",
    }


def request(cand, lang, sex, voice, text, rid, refs, voice_files, code=None):
    r = {"id": rid, "text": text}
    code = cand["lang"].get(lang, "") if code is None else code
    if code:
        r["language"] = code
    options = dict(cand.get("options", {}))
    if voice == "ref":
        r["voice_ref"] = os.path.abspath(os.path.join(refs, f"{lang}-{sex}.wav"))
        r["reference_text"] = TEXTS["refs"]["text"][lang]
    elif voice.startswith("@"):
        options["voice_embedding_file"] = os.path.abspath(voice_files[voice])
    elif voice and voice != "default":
        r["voice_id"] = voice
    if options:
        r["options"] = options
    return r


def sentences_of(passage):
    out = []
    if passage.get("heading"):
        out.append(passage["heading"])
    for para in passage["paragraphs"]:
        out.extend(para)
    return out


def jobs_for(cand, sexes):
    """(voice label, lang, sex, voice, passage, requests-maker) in the order they run."""
    jobs = []
    only = cand.get("only")
    for passage in TEXTS["passages"]:
        if only and passage["id"] not in only:
            continue
        lang = passage["lang"]
        for sex in sexes:
            voice = cand.get("voices", {}).get(lang, {}).get(sex)
            if voice is None:
                continue
            jobs.append(("main", lang, sex, voice, passage, None))
            for extra in cand.get("extra", []):
                if extra["passage"] == passage["id"]:
                    jobs.append(("extra", lang, sex, voice, passage, extra))
            if passage.get("pieces"):
                other = {p[0] for s in passage["pieces"] for p in s} - {lang}
                if all(cand.get("voices", {}).get(o, {}).get(sex) for o in other):
                    jobs.append(("pieces", lang, sex, voice, passage, None))
    if "gallery" in cand and "m" in sexes:
        for lang in ("ar", "en"):
            if lang in cand.get("lang", {}):
                for voice in cand["gallery"]:
                    jobs.append(("gallery", lang, None, voice, None, None))
    return jobs


def main_bench(args):
    cand = CANDIDATES[args.candidate]
    model_dir, model_file, voice_files, sizes = fetch(args.candidate, cand, args.models)
    out = os.path.join(args.out, args.candidate)
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "files.json"), "w") as f:
        json.dump(sizes, f)
    results = open(os.path.join(out, "results.jsonl"), "a", encoding="utf-8")
    deadline = time.time() + args.budget_min * 60
    model_arg = model_dir
    for kind, lang, sex, voice, passage, extra in jobs_for(cand, args.sexes.split(",")):
        if time.time() > deadline:
            print("budget spent: stopping", flush=True)
            break
        if kind == "gallery":
            name = f"gallery-{lang}-{voice.lstrip('@')}"
            reqs = [request(cand, lang, None, voice, TEXTS["gallery"][lang], "00", args.refs, voice_files)]
            pid, vlabel = f"gallery-{lang}", voice
        else:
            pid = passage["id"] + ({"extra": "~" + (extra or {}).get("lang", ""), "pieces": "~split"}.get(kind, ""))
            vlabel = sex
            name = f"{sex}/{pid}"
            reqs = []
            if kind == "pieces":
                for i, pieces in enumerate(passage["pieces"]):
                    for j, (plang, ptext) in enumerate(pieces):
                        pvoice = cand["voices"][plang][sex]
                        reqs.append(request(cand, plang, sex, pvoice, ptext, f"{i:02d}-{j}", args.refs, voice_files))
            else:
                code = extra["lang"] if extra else None
                for i, s in enumerate(sentences_of(passage)):
                    reqs.append(request(cand, lang, sex, voice, s, f"{i:02d}", args.refs, voice_files, code))
        outdir = os.path.join(out, name.replace("/", os.sep))
        print(f"== {args.candidate} {name}: {len(reqs)} requests", flush=True)
        r = run_sequence(args, cand, model_arg, reqs, outdir)
        if r["exit"] != 0 and model_arg == model_dir and not any(s["audio_s"] for s in r["sentences"]):
            # the CLI may want the GGUF itself rather than its folder
            print("retrying with the model file", flush=True)
            model_arg = model_file
            r = run_sequence(args, cand, model_arg, reqs, outdir)
        r.update({"candidate": args.candidate, "kind": kind, "lang": lang, "sex": sex, "voice": vlabel, "passage": pid,
                  "texts": [q["text"] for q in reqs], "langs": [q.get("language", "") for q in reqs]})
        results.write(json.dumps(r, ensure_ascii=False) + "\n")
        results.flush()
        spoken = sum(s["audio_s"] for s in r["sentences"])
        print(f"   exit {r['exit']}, {r['process_s']} s for {spoken:.1f} s of speech, peak {r['peak_ws_mb']} MB", flush=True)
        if r["exit"] != 0:
            print(r["log_tail"], flush=True)


def main_refs(args):
    key = "voxcpm2-q8"
    cand = CANDIDATES[key]
    model_dir, model_file, _, _ = fetch(key, cand, args.models)
    os.makedirs(args.out, exist_ok=True)
    reqs = []
    for vid, description in TEXTS["refs"]["design"].items():
        lang = vid.split("-")[0]
        # VoxCPM 2 takes a voice described in brackets before the text
        reqs.append({"id": vid, "text": f"({description}){TEXTS['refs']['text'][lang]}", "seed": 1234})
    for model_arg in (model_dir, model_file):
        r = run_sequence(args, cand, model_arg, reqs, args.out)
        if r["exit"] == 0:
            break
    print(json.dumps(r, indent=1), flush=True)
    if r["exit"] != 0:
        sys.exit("could not make the ready-made voices")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--make-refs", action="store_true")
    ap.add_argument("candidate", nargs="?")
    ap.add_argument("cli")
    ap.add_argument("models")
    ap.add_argument("out")
    ap.add_argument("--sexes", default="m,f")
    ap.add_argument("--refs", default="refs")
    ap.add_argument("--backend", default="cpu")
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--budget-min", type=float, default=300)
    a = ap.parse_args()
    if a.make_refs:
        main_refs(a)
    else:
        main_bench(a)
