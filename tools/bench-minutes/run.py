"""Write the minutes of every test recording with one model, through llama-server, as Dawwin does.

    run.py <key> <model.gguf> <llama-server> <out dir>

Tasks: each recording in its own language in one pass; three in the other language; and two in
parts of three minutes, notes first and then one pass over the notes, the way a long recording
goes. Greedy decoding, thinking off, the answer held to the JSON schema by the server.
"""
import glob
import json
import os
import subprocess
import sys
import time
import urllib.request

import minutes

PORT = 8091
HERE = minutes.HERE
CTX = int(os.environ.get("BENCH_CTX", "16384"))

CROSS = [("ar-levant-product", "en"), ("ar-gulf-events", "en"), ("en-sprint", "ar")]
PARTS = [("ar-levant-product", 180), ("en-sprint", 180)]


def post(path, body):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=3600) as r:
        return json.loads(r.read())


def wait_ready(server):
    for _ in range(900):
        if server.poll() is not None:
            raise SystemExit("llama-server exited")
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=5) as r:
                if r.status == 200:
                    return
        except Exception:
            pass
        time.sleep(1)
    raise SystemExit("llama-server did not start")


def ask(messages, schema):
    body = {
        "messages": messages,
        "response_format": {"type": "json_schema", "json_schema": {"name": "minutes", "schema": schema}},
        "max_tokens": 3072,
        "temperature": 0,
        "top_k": 1,
        "cache_prompt": False,
        "chat_template_kwargs": {"enable_thinking": False},
    }
    started = time.time()
    r = post("/v1/chat/completions", body)
    choice = r["choices"][0]
    text = choice["message"].get("content") or ""
    t = r.get("timings", {})
    try:
        answer = json.loads(text)
    except Exception:
        answer = None
    return {
        "answer": answer,
        "raw": text if answer is None else None,
        "finish": choice.get("finish_reason"),
        "seconds": round(time.time() - started, 2),
        "prompt_tokens": t.get("prompt_n"),
        "prompt_per_s": t.get("prompt_per_second"),
        "tokens": t.get("predicted_n"),
        "tokens_per_s": t.get("predicted_per_second"),
    }


def main(key, model, server_bin, out_dir):
    meetings = {}
    for txt in sorted(glob.glob(os.path.join(HERE, "meetings", "*.txt"))):
        mid = os.path.splitext(os.path.basename(txt))[0]
        gold = json.load(open(txt[:-4] + ".json", encoding="utf-8"))
        meetings[mid] = (minutes.load_meeting(txt), gold["language"])
    tasks = [(mid, lang, lang, None) for mid, (_, lang) in meetings.items()]
    tasks += [(mid, meetings[mid][1], target, None) for mid, target in CROSS if mid in meetings]
    tasks += [(mid, meetings[mid][1], meetings[mid][1], secs) for mid, secs in PARTS if mid in meetings]

    env = dict(os.environ, LD_LIBRARY_PATH=os.path.dirname(server_bin))
    args = [server_bin, "-m", model, "-c", str(CTX), "-np", "1", "-t", str(os.cpu_count()), "--jinja",
            "--port", str(PORT), "--host", "127.0.0.1", "--no-webui"]
    log = open(os.path.join(out_dir, f"{key}.server.log"), "w")
    server = subprocess.Popen(args, env=env, stdout=log, stderr=subprocess.STDOUT)
    results = []
    started = time.time()
    try:
        wait_ready(server)
        for mid, source, target, part_s in tasks:
            lines = meetings[mid][0]
            name = f"{mid}.{target}" + (f".parts{part_s}" if part_s else "")
            try:
                if part_s is None:
                    msgs, sch = minutes.messages_whole(lines, source, target)
                    r = ask(msgs, sch)
                    calls = [r]
                else:
                    spans = minutes.cut(lines, part_s)
                    calls, notes = [], []
                    for n, (first, last) in enumerate(spans, 1):
                        msgs, sch = minutes.messages_part(lines, first, last, n, len(spans), source, target)
                        c = ask(msgs, sch)
                        calls.append(c)
                        notes.append(c["answer"] or {})
                    msgs, sch = minutes.messages_combine(notes, spans, lines, source, target)
                    r = ask(msgs, sch)
                    r["notes"] = notes
                    calls.append(r)
                r = dict(r)
                r.update({
                    "task": name, "meeting": mid, "source": source, "target": target, "parts": part_s,
                    "seconds": round(sum(c["seconds"] for c in calls), 2),
                    "prompt_tokens": sum(c["prompt_tokens"] or 0 for c in calls),
                    "tokens": sum(c["tokens"] or 0 for c in calls),
                    "calls": len(calls),
                })
            except Exception as e:  # one failed task must not lose the others
                r = {"task": name, "meeting": mid, "source": source, "target": target, "parts": part_s, "error": str(e)}
            results.append(r)
            print(json.dumps({k: r.get(k) for k in ("task", "seconds", "prompt_tokens", "tokens", "prompt_per_s", "tokens_per_s", "finish", "error")}), flush=True)
            if r.get("answer"):
                print("  summary:", (r["answer"].get("summary") or "")[:300], flush=True)
    finally:
        server.terminate()
        server.wait(60)
    with open(os.path.join(out_dir, f"{key}.json"), "w", encoding="utf-8") as f:
        json.dump({
            "key": key,
            "bytes": os.path.getsize(model),
            "cpus": os.cpu_count(),
            "wall_seconds": round(time.time() - started, 1),
            "results": results,
        }, f, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main(*sys.argv[1:5])
