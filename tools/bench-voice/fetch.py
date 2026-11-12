"""Download the voice models the app offers, file by file as src-tauri/src/reading/voices.rs pins
them (repository, revision, path, size, SHA-256), into the folders the app keeps them in:

    fetch.py <root>          → <root>/voices/<model id>/<name>

A file whose size or SHA-256 differs from the pin stops it: the app would refuse it too.
"""
import hashlib
import os
import re
import sys
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = open(os.path.join(HERE, "..", "..", "src-tauri", "src", "reading", "voices.rs"), encoding="utf-8").read()
CONSTS = dict(re.findall(r'const (\w+): &str = "([^"]*)";', SRC))


def value(v):
    v = v.strip()
    return v.strip('"') if v.startswith('"') else CONSTS[v]


def files_of(block):
    out = []
    for m in re.finditer(r"ModelFile \{\s*repository: ([^,]+),\s*revision: ([^,]+),\s*path: \"([^\"]+)\",\s*name: \"([^\"]+)\",\s*size: ([\d_]+),\s*sha256: \"([0-9a-f]+)\"", block):
        out.append((value(m[1]), value(m[2]), m[3], m[4], int(m[5].replace("_", "")), m[6]))
    for m in re.finditer(r'phonon_voice\("[^"]+", "([^"]+)", "([^"]+)", "([0-9a-f]+)"\)', block):
        out.append((CONSTS["PHONON_VOICES"], CONSTS["PHONON_VOICES_REV"], m[1], m[2], 256_136, m[3]))
    return out


def models():
    for block in re.split(r"\npub const ", SRC)[1:]:
        if "VoiceModel {" not in block.split("\n", 1)[0]:
            continue
        mid = re.search(r'id: "([^"]+)"', block)[1]
        if mid != "stand-in":
            yield mid, files_of(block)


def fetch(root):
    for mid, files in models():
        for repo, rev, path, name, size, sha in files:
            dest = os.path.join(root, "voices", mid, *name.split("/"))
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            if not (os.path.exists(dest) and os.path.getsize(dest) == size):
                url = f"https://huggingface.co/{repo}/resolve/{rev}/{path}"
                print(f"{mid}: {path} ({size} bytes)", flush=True)
                urllib.request.urlretrieve(url, dest + ".part")
                os.replace(dest + ".part", dest)
            h = hashlib.sha256()
            with open(dest, "rb") as f:
                for chunk in iter(lambda: f.read(1 << 20), b""):
                    h.update(chunk)
            if os.path.getsize(dest) != size or h.hexdigest() != sha:
                sys.exit(f"{dest}: not the pinned file (size {os.path.getsize(dest)}, sha256 {h.hexdigest()})")
        print(f"{mid}: {len(files)} files, all as pinned", flush=True)


if __name__ == "__main__":
    fetch(sys.argv[1])
