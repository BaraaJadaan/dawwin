"""Dawwin's own voice program (voice/, dawwin-voice.exe) with audio.cpp's official library, on a
real voice: does it load, speak Arabic and English, how fast, and does it sound the same as
audio.cpp's own CLI with the same voice and text.

    program.py <folder with dawwin-voice.exe and the library> <audiocpp_cli.exe> <models dir> <out dir> [label]

Exits non-zero when the program gives no sound on the processor build.
"""
import json
import os
import subprocess
import sys
import time
import wave

from huggingface_hub import hf_hub_download

folder, cli, models, out = sys.argv[1:5]
label = sys.argv[5] if len(sys.argv) > 5 else os.path.basename(folder)
os.makedirs(out, exist_ok=True)
model = hf_hub_download("audio-cpp/audio.cpp-gguf", "Supertonic-3-GGUF/supertonic-3-f16.gguf", local_dir=os.path.join(models, "supertonic3-f16"))
model = os.path.abspath(model)
SENTENCES = [
    ("ar", "F1", "في صباح يوم هادئ، جلست سلمى قرب النافذة تقرأ رسالة وصلتها من صديقتها القديمة."),
    ("en", "M1", "The old lighthouse stood at the edge of the cliff, its white paint peeling after decades of salty wind."),
    ("ar", "F1", "ارتفعت الأسعار بنسبة خمسة عشر بالمئة."),
]


def seconds(path):
    with wave.open(path) as w:
        return w.getnframes() / w.getframerate(), w.getframerate()


def samples(path):
    with wave.open(path) as w:
        data = w.readframes(w.getnframes())
    return [int.from_bytes(data[i:i + 2], "little", signed=True) for i in range(0, len(data), 2)]


exe = os.path.join(folder, "dawwin-voice.exe")
report = {"label": label}
p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", cwd=folder)


def ask(req):
    p.stdin.write(json.dumps(req, ensure_ascii=False) + "\n")
    p.stdin.flush()
    line = p.stdout.readline()
    if not line:
        return {"ok": False, "error": "no answer; it exited " + str(p.poll()) + ": " + p.stderr.read()[-2000:]}
    return json.loads(line)


t = time.perf_counter()
r = ask({"cmd": "load", "family": "supertonic", "model": model, "backend": "vulkan", "device": 0, "threads": 4, "options": {}})
report["load"] = {"answer": r, "s": round(time.perf_counter() - t, 2)}
print(label, "load:", r, flush=True)
ok = r.get("ok")
report["sentences"] = []
for i, (lang, voice, text) in enumerate(SENTENCES if ok else []):
    dest = os.path.abspath(os.path.join(out, f"{label}-{i}.wav"))
    t = time.perf_counter()
    r = ask({"cmd": "speak", "id": i, "text": text, "language": lang, "voice_id": voice, "out": dest})
    wall = time.perf_counter() - t
    length = seconds(dest)[0] if r.get("ok") else 0
    report["sentences"].append({"lang": lang, "answer": r, "wall_s": round(wall, 2), "audio_s": round(length, 2)})
    print(label, lang, r, f"{wall:.2f} s for {length:.2f} s", flush=True)
    ok = ok and r.get("ok") and length > 0.5
p.stdin.close()
p.wait(timeout=60)

# the same first sentence through audio.cpp's own CLI
if report["sentences"] and report["sentences"][0]["answer"].get("ok"):
    lang, voice, text = SENTENCES[0]
    ref = os.path.abspath(os.path.join(out, f"{label}-cli-0.wav"))
    subprocess.run([cli, "--task", "tts", "--family", "supertonic", "--model", model, "--backend", "cpu", "--threads", "4",
                    "--language", lang, "--voice-id", voice, "--text", text, "--out", ref], cwd=os.path.dirname(cli), check=False)
    if os.path.exists(ref):
        a, b = samples(os.path.join(out, f"{label}-0.wav")), samples(ref)
        n = min(len(a), len(b))
        diff = max((abs(x - y) for x, y in zip(a[:n], b[:n])), default=0)
        report["same_as_cli"] = {"program_samples": len(a), "cli_samples": len(b), "largest_difference": diff}
        print(label, "against the CLI:", report["same_as_cli"], flush=True)
json.dump(report, open(os.path.join(out, f"{label}.json"), "w", encoding="utf-8"), ensure_ascii=False, indent=1)
sys.exit(0 if ok else 1)
