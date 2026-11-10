"""Translate the test sentences with one model through llama-server, the way Dawwin would.

    run.py <key> <family> <model.gguf> <llama-server> <tests.jsonl> <out dir>

Each family gets the prompt its makers give on the model card. Greedy decoding for all.
"""
import concurrent.futures
import json
import os
import subprocess
import sys
import time
import urllib.request

import jinja2
from jinja2.sandbox import ImmutableSandboxedEnvironment

NAMES = {"ar": "Arabic", "en": "English", "da": "Danish", "tr": "Turkish", "fr": "French"}
PORT = 8090
SLOTS = 4


def gguf_strings(path):
    from gguf import GGUFReader

    reader = GGUFReader(path)

    def text(name):
        f = reader.fields.get(name)
        return None if f is None else bytes(f.parts[f.data[0]]).decode("utf-8")

    template = text("tokenizer.chat_template")
    bos = ""
    f = reader.fields.get("tokenizer.ggml.bos_token_id")
    tokens = reader.fields.get("tokenizer.ggml.tokens")
    if f is not None and tokens is not None:
        bos_id = int(f.parts[f.data[0]][0])
        bos = bytes(tokens.parts[tokens.data[bos_id]]).decode("utf-8")
    return template, bos


def renderer(template, bos):
    def raise_exception(message):
        raise jinja2.exceptions.TemplateError(message)

    env = ImmutableSandboxedEnvironment(trim_blocks=True, lstrip_blocks=True, extensions=["jinja2.ext.loopcontrols"])
    env.globals["raise_exception"] = raise_exception
    env.globals["strftime_now"] = lambda fmt: time.strftime(fmt)
    compiled = env.from_string(template)

    def render(messages):
        text = compiled.render(messages=messages, add_generation_prompt=True, bos_token=bos, eos_token="")
        # llama-server adds the BOS itself
        return text[len(bos):] if bos and text.startswith(bos) else text

    return render


def prompt_for(family, render, t):
    src, tgt = NAMES[t["src"]], NAMES[t["tgt"]]
    if family == "hymt":
        message = (
            f"Translate the following text into {tgt}. Note that you should only output the translated result "
            f"without any additional explanation:\n\n{t['source']}"
        )
        return render([{"role": "user", "content": message}]), {"repeat_penalty": 1.05}
    if family == "tgemma":
        content = [{"type": "text", "source_lang_code": t["src"], "target_lang_code": t["tgt"], "text": t["source"]}]
        return render([{"role": "user", "content": content}]), {}
    if family == "plamo":
        p = (
            "<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>input<|plamo:msg|>"
            f"{t['source']}<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>{t['tgt']}<|plamo:msg|>"
        )
        return p, {"stop": ["<|plamo:tag|>", "<|plamo:bos|>"]}
    if family == "milmmt":
        return f"Translate this from {src} to {tgt}:\n{src}: {t['source']}\n{tgt}:", {"stop": ["\n"]}
    raise SystemExit(f"unknown family {family}")


def post(path, body):
    req = urllib.request.Request(
        f"http://127.0.0.1:{PORT}{path}", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=900) as r:
        return json.loads(r.read())


def wait_ready(server):
    for _ in range(600):
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


def main(key, family, model, server_bin, tests_path, out_dir):
    tests = [json.loads(line) for line in open(tests_path, encoding="utf-8")]
    render = None
    if family in ("hymt", "tgemma"):
        template, bos = gguf_strings(model)
        render = renderer(template, bos)
    env = dict(os.environ, LD_LIBRARY_PATH=os.path.dirname(server_bin))
    args = [server_bin, "-m", model, "-c", str(2048 * SLOTS), "-np", str(SLOTS), "-t", str(os.cpu_count()),
            "--port", str(PORT), "--host", "127.0.0.1", "--no-webui"]
    log = open(os.path.join(out_dir, f"{key}.server.log"), "w")
    server = subprocess.Popen(args, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        wait_ready(server)
        first = prompt_for(family, render, tests[0])[0]
        print("prompt example:\n" + first)

        def one(t):
            p, extra = prompt_for(family, render, t)
            body = {"prompt": p, "n_predict": 512, "temperature": 0, "top_k": 1, "cache_prompt": False}
            body.update(extra)
            started = time.time()
            r = post("/completion", body)
            return {
                "id": t["id"],
                "hyp": r["content"].strip(),
                "seconds": round(time.time() - started, 2),
                "tokens": r.get("tokens_predicted"),
                "truncated": r.get("stop_type") == "limit",
            }

        started = time.time()
        with concurrent.futures.ThreadPoolExecutor(SLOTS) as pool:
            results = list(pool.map(one, tests))
        wall = time.time() - started
    finally:
        server.terminate()
        server.wait(60)
    with open(os.path.join(out_dir, f"{key}.jsonl"), "w", encoding="utf-8") as f:
        for r in results:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    meta = {
        "key": key,
        "family": family,
        "bytes": os.path.getsize(model),
        "wall_seconds": round(wall, 1),
        "tokens": sum(r["tokens"] or 0 for r in results),
        "truncated": sum(r["truncated"] for r in results),
        "cpus": os.cpu_count(),
    }
    json.dump(meta, open(os.path.join(out_dir, f"{key}.meta.json"), "w"))
    print(json.dumps(meta))
    for r, t in list(zip(results, tests))[::40]:
        print(f"{t['id']}: {t['source'][:80]} => {r['hyp'][:120]}")


if __name__ == "__main__":
    main(*sys.argv[1:7])
