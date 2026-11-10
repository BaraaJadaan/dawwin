"""Pack files into a .tar.xz the way Dawwin's downloads are made (src-tauri/src/tools.rs reads them):
the files at the top level, sorted, with no owner and no time, and compressed as small as xz goes.

    python tools/pack.py OUT.tar.xz FILE_OR_FOLDER... [--skip NAME]...

A folder adds the files in it, not its subfolders. Prints the size and SHA-256 the app pins.
"""
import hashlib
import lzma
import os
import sys
import tarfile

args = sys.argv[1:]
skip = {args[i + 1] for i, a in enumerate(args) if a == "--skip"}
args = [a for i, a in enumerate(args) if a != "--skip" and (i == 0 or args[i - 1] != "--skip")]
out, sources = args[0], args[1:]

files = {}
for source in sources:
    found = [os.path.join(source, n) for n in os.listdir(source)] if os.path.isdir(source) else [source]
    for f in found:
        name = os.path.basename(f)
        if os.path.isfile(f) and name not in skip:
            if name in files:
                sys.exit(f"{name} is there twice: {files[name]} and {f}")
            files[name] = f
if not files:
    sys.exit("nothing to pack")

os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
with tarfile.open(out, "w:xz", preset=9 | lzma.PRESET_EXTREME, format=tarfile.GNU_FORMAT) as tar:
    for name in sorted(files):
        info = tar.gettarinfo(files[name], arcname=name)
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mtime = 0
        info.mode = 0o755 if name.endswith((".exe", ".dll")) else 0o644
        with open(files[name], "rb") as fh:
            tar.addfile(info, fh)

data = open(out, "rb").read()
print(f"{os.path.basename(out)}: {len(files)} files, {sum(os.path.getsize(f) for f in files.values())} bytes unpacked")
print(f"size {len(data)}")
print(f"sha256 {hashlib.sha256(data).hexdigest()}")
