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
# "cpu" runs on the processor even when a CUDA GPU is there (the app's "Run on" setting)
FORCE_CPU = os.environ.get("TAFREEGH_DEVICE", "").lower() == "cpu"
# What a model folder needs; the repo's audio examples and README are left out
PATTERNS = ["*.json", "*.safetensors", "tokenizer.model", "*.py"]
_lock = threading.Lock()

# Messages the user sees, in the app's interface language (TAFREEGH_LANG)
LANG = os.environ.get("TAFREEGH_LANG", "en")
_AR = {
    "Hugging Face did not accept the token. Check it, or make a new read token.":
        "لم يقبل Hugging Face الرمز. تحقّق منه أو أنشئ رمز قراءة جديدًا.",
    "This Hugging Face account has no access to the model yet. Open the model page, accept its terms with the same account, then press Download again.":
        "لا يملك حساب Hugging Face هذا صلاحية على النموذج بعد. افتح صفحة النموذج، ووافق على شروطه بالحساب نفسه، ثم اضغط «تنزيل النموذج» مجددًا.",
    "Not enough free space in {dest}: the model needs {need} GB more, {free} GB is free. Choose another folder.":
        "لا توجد مساحة كافية في {dest}: يحتاج النموذج {need} غيغابايت إضافية، والمتاح {free} غيغابايت. اختر مجلدًا آخر.",
    "{name} came out the wrong size; it will download again.":
        "حجم {name} غير صحيح؛ سيُنزَّل مجددًا.",
    "{name} is damaged (checksum mismatch); press Download again.":
        "{name} تالف (المجموع الاختباري لا يطابق)؛ اضغط «تنزيل النموذج» مجددًا.",
    "The GPU does not have enough free memory ({err}); running on the CPU.":
        "ذاكرة معالج الرسوميات لا تكفي ({err})؛ يعمل التفريغ على المعالج.",
}


def tr(en, **kw):
    return (_AR.get(en, en) if LANG == "ar" else en).format(**kw)


def quantize_for_cpu(model):
    """Linear layers to int8 (dynamic quantization), the rest to fp32. On CPUs without native
    bf16 this is several times faster than bf16, in about half the memory of fp32. Layers are
    converted one at a time so memory never holds a full fp32 copy."""
    import torch
    from torch import nn
    from torch.ao.quantization import quantize_dynamic

    for parent in list(model.modules()):
        for name, child in list(parent.named_children()):
            if isinstance(child, nn.Linear):
                q = quantize_dynamic(nn.Sequential(child.float()), {nn.Linear}, dtype=torch.qint8)[0]
                setattr(parent, name, q)
    return model.float()  # what is left: convolutions, norms, embeddings


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
            "force_cpu": FORCE_CPU,
            "cpu_threads": torch.get_num_threads(),
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
        cuda = torch.cuda.is_available() and not FORCE_CPU
        # bf16 on CPU too: fp32 would need ~8 GB of RAM
        self.dtype = torch.bfloat16
        self.processor = AutoProcessor.from_pretrained(MODEL)
        fallback = None
        for device in (["cuda:0", "cpu"] if cuda else ["cpu"]):
            self.device = device
            send({"event": "loading", "device": device})
            try:
                self.model = CohereAsrForConditionalGeneration.from_pretrained(
                    MODEL, device_map=device, dtype=self.dtype
                ).eval()
                if device == "cpu" and os.environ.get("TAFREEGH_CPU_MODE", "int8") == "int8":
                    send({"event": "loading", "device": device, "step": "optimizing for the CPU"})
                    self.model = quantize_for_cpu(self.model)
                    self.dtype = "int8"
                break
            except torch.cuda.OutOfMemoryError as e:  # a small GPU: run on the CPU instead
                fallback = tr("The GPU does not have enough free memory ({err}); running on the CPU.", err=e)
                log(fallback)
                self.model = None
                torch.cuda.empty_cache()
        st = self._status()
        if fallback:
            st["fallback"] = fallback
        st["load_seconds"] = round(time.time() - t0, 1)
        log(f"model loaded on {self.device} in {st['load_seconds']} s")
        return st

    def _status(self):
        import torch

        st = {"device": self.device, "dtype": str(self.dtype).replace("torch.", "")}
        if self.device == "cpu":
            st["cpu_threads"] = torch.get_num_threads()
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
    #
    # huggingface_hub (1.x) restarts an interrupted file from zero, which never finishes a
    # 4 GB file on a connection that drops. This downloader keeps a `<file>.part` and asks for
    # the rest with an HTTP Range request after every drop, Pause, or restart of the app.
    def download(self, req):
        import fnmatch
        import hashlib
        import shutil

        import httpx
        from huggingface_hub import HfApi, get_hf_file_metadata, hf_hub_url
        from huggingface_hub.errors import GatedRepoError, RepositoryNotFoundError
        from huggingface_hub.utils import build_hf_headers

        dest = req["local_dir"]
        patterns = req.get("patterns") or PATTERNS
        os.makedirs(dest, exist_ok=True)

        def fatal(e):
            if isinstance(e, (GatedRepoError, RepositoryNotFoundError)):
                return True
            code = getattr(getattr(e, "response", None), "status_code", None)
            return code in (401, 403, 404)

        def explain(e):
            code = getattr(getattr(e, "response", None), "status_code", None)
            if code == 401 and not isinstance(e, GatedRepoError):
                return tr("Hugging Face did not accept the token. Check it, or make a new read token.")
            return tr(
                "This Hugging Face account has no access to the model yet. Open the model page, "
                "accept its terms with the same account, then press Download again."
            )

        attempts, max_attempts = 0, int(req.get("max_attempts", 1000))

        def retry(e):
            nonlocal attempts
            if fatal(e):
                raise RuntimeError(explain(e)) from e
            attempts += 1
            if attempts >= max_attempts:
                raise e
            wait = min(60, 2 * attempts)
            log(f"download interrupted ({type(e).__name__}: {e}); retry {attempts} in {wait}s")
            send({"event": "download_retry", "attempt": attempts, "wait": wait, "error": str(e)[:200]})
            time.sleep(wait)

        while True:  # the file list, with sizes and checksums
            try:
                api = HfApi()
                api.auth_check(MODEL_ID)
                info = api.model_info(MODEL_ID, files_metadata=True)
                break
            except Exception as e:
                retry(e)
        wanted = [s for s in info.siblings if any(fnmatch.fnmatch(s.rfilename, p) for p in patterns)]
        total = sum(s.size or 0 for s in wanted)

        def size(p):
            try:
                return os.path.getsize(p)
            except OSError:
                return 0

        def have():
            n = 0
            for s in wanted:
                final = os.path.join(dest, s.rfilename)
                n += size(final) if os.path.exists(final) else size(final + ".part")
            return min(n, total)

        missing = total - have()
        free = shutil.disk_usage(dest).free
        if free < missing + 200 * 2**20:
            raise RuntimeError(tr(
                "Not enough free space in {dest}: the model needs {need} GB more, {free} GB is free. Choose another folder.",
                dest=dest, need=f"{missing / 1e9:.1f}", free=f"{free / 1e9:.1f}",
            ))

        stop = threading.Event()

        def report():
            while not stop.wait(1.0):
                send({"event": "download", "done": have(), "total": total})

        threading.Thread(target=report, daemon=True).start()
        timeout = httpx.Timeout(30.0, read=60.0)
        try:
            for s in wanted:
                final = os.path.join(dest, s.rfilename)
                if os.path.exists(final) and size(final) == s.size:
                    continue
                part = final + ".part"
                os.makedirs(os.path.dirname(final), exist_ok=True)
                while size(part) < s.size:
                    try:
                        # a fresh signed CDN address each time: they expire
                        meta = get_hf_file_metadata(hf_hub_url(MODEL_ID, s.rfilename, revision=info.sha))
                        start = size(part)
                        headers = {"Range": f"bytes={start}-"}
                        if httpx.URL(meta.location).host.endswith("huggingface.co"):
                            headers.update(build_hf_headers())  # small files come from the Hub itself
                        with httpx.stream(
                            "GET", meta.location, headers=headers,
                            follow_redirects=True, timeout=timeout,
                        ) as r:
                            if r.status_code == 416:
                                break
                            r.raise_for_status()
                            if r.status_code != 206 and start:
                                start = 0  # the server ignored the range: start over
                            with open(part, "r+b" if start else "wb") as f:
                                f.seek(start)
                                f.truncate()
                                for chunk in r.iter_bytes(256 * 1024):
                                    f.write(chunk)
                        attempts = 0
                    except Exception as e:  # network drop: wait, then continue where it stopped
                        retry(e)
                if size(part) != s.size:
                    os.remove(part)
                    raise RuntimeError(tr("{name} came out the wrong size; it will download again.", name=s.rfilename))
                if s.lfs and getattr(s.lfs, "sha256", None):
                    h = hashlib.sha256()
                    with open(part, "rb") as f:
                        for block in iter(lambda: f.read(8 * 2**20), b""):
                            h.update(block)
                    if h.hexdigest() != s.lfs.sha256:
                        os.remove(part)
                        raise RuntimeError(tr("{name} is damaged (checksum mismatch); press Download again.", name=s.rfilename))
                os.replace(part, final)
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
