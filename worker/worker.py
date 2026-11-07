"""Tafreegh inference worker: Cohere Transcribe Arabic behind a JSON-lines protocol.

The Rust app starts this script with the Python env it manages and talks to it over
stdin/stdout, one JSON object per line. Everything that is not a protocol message
goes to stderr (the app keeps it in its log).

Requests  (stdin):  {"id": 1, "cmd": "probe" | "load" | "transcribe" | "download" | "quit", ...}
Replies   (stdout): {"id": 1, "ok": true, ...}  or  {"id": 1, "ok": false, "error": "..."}
Events    (stdout): {"event": "...", ...}        progress while a request runs

transcribe: {"path": <raw little-endian float32 mono 16 kHz file>,
             "ranges": [[start_sample, end_sample], ...], "language": "ar" | "en"}
            -> {"texts": ["...", ...]}  (one per range, decoded as one batch)
"""
import json
import os
import sys
import threading
import time
import traceback

PROTO = sys.stdout
sys.stdout = sys.stderr  # libraries print; keep the protocol channel clean

SR = 16000
MODEL_ID = "CohereLabs/cohere-transcribe-arabic-07-2026"
# The folder the app found or imported the model in (falls back to the Hugging Face cache)
MODEL = os.environ.get("TAFREEGH_MODEL") or MODEL_ID
# What a model folder needs; the repo's audio examples and README are left out
PATTERNS = ["*.json", "*.safetensors", "tokenizer.model", "*.py"]
_lock = threading.Lock()


def send(obj):
    with _lock:
        PROTO.write(json.dumps(obj, ensure_ascii=True) + "\n")
        PROTO.flush()


def log(*a):
    print(*a, file=sys.stderr, flush=True)


class Engine:
    def __init__(self):
        self.model = None
        self.processor = None
        self.device = None
        self.dtype = None

    # -- probe: what hardware and versions are here, without loading the model
    def probe(self, _req):
        import torch
        import transformers

        info = {
            "python": sys.version.split()[0],
            "torch": torch.__version__,
            "transformers": transformers.__version__,
            "cuda": bool(torch.cuda.is_available()),
        }
        if info["cuda"]:
            p = torch.cuda.get_device_properties(0)
            free, total = torch.cuda.mem_get_info(0)
            info.update(gpu=p.name, vram_total_gb=round(total / 2**30, 2), vram_free_gb=round(free / 2**30, 2))
        return info

    def load(self, _req):
        if self.model is not None:
            return self._status()
        import torch
        from transformers import AutoProcessor, CohereAsrForConditionalGeneration

        t0 = time.time()
        cuda = torch.cuda.is_available()
        self.device = "cuda:0" if cuda else "cpu"
        # bf16 on CPU too: fp32 would need ~8 GB of RAM, which this laptop rarely has free
        self.dtype = torch.bfloat16
        send({"event": "loading", "device": self.device})
        self.processor = AutoProcessor.from_pretrained(MODEL)
        self.model = CohereAsrForConditionalGeneration.from_pretrained(
            MODEL, device_map=self.device, dtype=self.dtype
        ).eval()
        st = self._status()
        st["load_seconds"] = round(time.time() - t0, 1)
        log(f"model loaded on {self.device} in {st['load_seconds']} s")
        return st

    def _status(self):
        import torch

        st = {"device": self.device, "dtype": str(self.dtype).replace("torch.", "")}
        if self.device.startswith("cuda"):
            st["gpu"] = torch.cuda.get_device_name(0)
            st["vram_used_gb"] = round(torch.cuda.memory_allocated(0) / 2**30, 2)
        return st

    def transcribe(self, req):
        import numpy as np
        import torch

        if self.model is None:
            self.load(req)
        audio = np.memmap(req["path"], dtype="<f4", mode="r")
        lang = req.get("language", "ar")
        segs = [np.array(audio[int(a) : int(b)], dtype=np.float32) for a, b in req["ranges"]]
        inputs = self.processor(segs, sampling_rate=SR, return_tensors="pt", language=lang)
        idx = inputs.get("audio_chunk_index")
        inputs.to(self.model.device, dtype=self.model.dtype)
        with torch.inference_mode():
            out = self.model.generate(**inputs, max_new_tokens=int(req.get("max_new_tokens", 512)))
        kw = {"audio_chunk_index": idx, "language": lang} if idx is not None else {}
        texts = self.processor.decode(out, skip_special_tokens=True, **kw)
        if isinstance(texts, str):
            texts = [texts]
        texts = [t.strip() for t in texts]
        if len(texts) != len(segs):
            raise RuntimeError(f"expected {len(segs)} transcripts, got {len(texts)}")
        return {"texts": texts}

    # -- download: fetch the model into a folder, resuming after network drops
    def download(self, req):
        os.environ.setdefault("HF_HUB_DISABLE_XET", "1")  # plain HTTP resumes .incomplete files
        import fnmatch

        from huggingface_hub import HfApi, snapshot_download
        from huggingface_hub.errors import GatedRepoError, RepositoryNotFoundError

        dest = req["local_dir"]
        patterns = req.get("patterns") or PATTERNS
        os.makedirs(dest, exist_ok=True)
        api = HfApi()

        def fatal(e):
            if isinstance(e, (GatedRepoError, RepositoryNotFoundError)):
                return True
            code = getattr(getattr(e, "response", None), "status_code", None)
            return code in (401, 403, 404)

        def explain(e):
            code = getattr(getattr(e, "response", None), "status_code", None)
            if code == 401 and not isinstance(e, GatedRepoError):
                return "Hugging Face did not accept the token. Check it, or make a new read token."
            return (
                "This Hugging Face account has no access to the model yet. Open the model page, "
                "accept its terms with the same account, then press Download again."
            )

        try:
            api.auth_check(MODEL_ID)
            info = api.model_info(MODEL_ID, files_metadata=True)
        except Exception as e:
            if fatal(e):
                raise RuntimeError(explain(e)) from e
            raise
        wanted = [s for s in info.siblings if any(fnmatch.fnmatch(s.rfilename, p) for p in patterns)]
        total = sum(s.size or 0 for s in wanted)

        def have():
            n = 0
            for root, _dirs, files in os.walk(dest):
                for f in files:
                    try:
                        n += os.path.getsize(os.path.join(root, f))
                    except OSError:
                        pass
            return min(n, total)

        stop = threading.Event()

        def report():
            while not stop.wait(1.0):
                send({"event": "download", "done": have(), "total": total})

        threading.Thread(target=report, daemon=True).start()
        attempts, max_attempts = 0, int(req.get("max_attempts", 1000))
        try:
            while True:
                try:
                    snapshot_download(MODEL_ID, local_dir=dest, allow_patterns=patterns)
                    break
                except Exception as e:  # network drop: wait, then resume where it stopped
                    if fatal(e):
                        raise RuntimeError(explain(e)) from e
                    attempts += 1
                    if attempts >= max_attempts:
                        raise
                    wait = min(60, 2 * attempts)
                    log(f"download interrupted ({type(e).__name__}: {e}); retry {attempts} in {wait}s")
                    send({"event": "download_retry", "attempt": attempts, "wait": wait, "error": str(e)[:200]})
                    time.sleep(wait)
        finally:
            stop.set()
        send({"event": "download", "done": total, "total": total})
        return {"bytes": total, "attempts": attempts + 1, "dir": dest}


def main():
    eng = Engine()
    handlers = {"probe": eng.probe, "load": eng.load, "transcribe": eng.transcribe, "download": eng.download}
    send({"event": "ready", "pid": os.getpid()})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            send({"ok": False, "error": f"bad json: {e}"})
            continue
        rid, cmd = req.get("id"), req.get("cmd")
        if cmd == "quit":
            send({"id": rid, "ok": True})
            break
        fn = handlers.get(cmd)
        if fn is None:
            send({"id": rid, "ok": False, "error": f"unknown command {cmd!r}"})
            continue
        try:
            res = fn(req) or {}
            send({"id": rid, "ok": True, **res})
        except Exception as e:  # report and keep serving
            log(traceback.format_exc())
            oom = "out of memory" in str(e).lower()
            if oom:
                try:
                    import torch

                    torch.cuda.empty_cache()
                except Exception:
                    pass
            send({"id": rid, "ok": False, "error": f"{type(e).__name__}: {e}", "oom": oom})


if __name__ == "__main__":
    main()
