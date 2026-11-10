// Puts the speech engine (dawwin-engine.exe and its DLLs) in src-tauri/engine-bin, where the app's
// build picks it up. The translator (llama.cpp's server) is not in the app: it downloads with the
// first translation model (src-tauri/src/tools.rs). --translator-only puts it in
// src-tauri/engine-bin/translate, for the Tools workflow to pack and the Engine workflow to test.
//
//   node tools/stage-engine.mjs               build the engine here: with GPU support when the Vulkan
//                                             SDK is installed (VULKAN_SDK is set), else CPU only
//   node tools/stage-engine.mjs --from-ci     take the newest engine build from GitHub Actions instead
//   node tools/stage-engine.mjs --from-ci ID  take the engine build of that workflow run
//   --translator-only                         the translator, and nothing else
//   --translator-zip FILE                     the llama.cpp zip from this file instead of downloading it
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const bin = join(root, "src-tauri", "engine-bin");
const translate = join(bin, "translate");
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: "inherit", shell: process.platform === "win32" });
const arg = (name) => { const i = process.argv.indexOf(name); return i >= 0 ? process.argv[i + 1] ?? "" : null; };

// llama.cpp, pinned: the release, and the checksum GitHub gives for its file
const LLAMA = {
  tag: "b12237",
  file: "llama-b12237-bin-win-vulkan-x64.zip",
  sha256: "287206f220004c6ece23d574161c623f5c56b987f450511667ac9aa99aabb1a2",
};
// what the server needs of it: no other tools, and no RPC backend
const wanted = (name) =>
  name === "llama-server.exe" || name === "LICENSE-LLVM-OpenMP"
  || /^(llama-server-impl|llama-common|llama|mtmd|ggml|ggml-base|ggml-vulkan|libomp)\.dll$/.test(name)
  || /^ggml-cpu-[a-z0-9]+\.dll$/.test(name);

/** Empty a folder, except for its README. */
function clear(dir) {
  mkdirSync(dir, { recursive: true });
  for (const name of readdirSync(dir)) {
    if (name !== "README.md" && name !== "translate") rmSync(join(dir, name), { recursive: true, force: true });
  }
}

function stageEngine() {
  clear(bin);
  const fromCi = process.argv.indexOf("--from-ci");
  if (fromCi >= 0) {
    const id = process.argv[fromCi + 1];
    const args = ["run", "download", "-n", "dawwin-engine-windows-x64", "-D", bin];
    if (id && !id.startsWith("--")) args.splice(2, 0, id);
    run("gh", args);
  } else {
    const args = ["build", "--release"];
    if (process.env.VULKAN_SDK) args.push("--features", "vulkan");
    else console.warn("VULKAN_SDK is not set: building for the CPU only.");
    run("cargo", args, join(root, "engine"));
    const release = join(root, "engine", "target", "release");
    for (const name of readdirSync(release)) {
      if (name === "dawwin-engine.exe" || name.endsWith(".dll")) copyFileSync(join(release, name), join(bin, name));
    }
  }
  const files = readdirSync(bin).filter((n) => n !== "README.md" && n !== "translate");
  if (!existsSync(join(bin, "dawwin-engine.exe"))) throw new Error("no dawwin-engine.exe in " + bin);
  console.log(`engine-bin holds ${files.length} files. GPU support: ${files.includes("ggml-vulkan.dll") ? "yes" : "no"}`);
}

async function stageTranslator() {
  let zip = arg("--translator-zip");
  if (!zip) {
    const cache = join(root, "src-tauri", "engine-cache");
    mkdirSync(cache, { recursive: true });
    zip = join(cache, LLAMA.file);
    if (!existsSync(zip)) {
      const url = `https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA.tag}/${LLAMA.file}`;
      console.log(`downloading ${url}`);
      const r = await fetch(url);
      if (!r.ok) throw new Error(`${url}: ${r.status}`);
      writeFileSync(zip, Buffer.from(await r.arrayBuffer()));
    }
  }
  const sha = createHash("sha256").update(readFileSync(zip)).digest("hex");
  if (sha !== LLAMA.sha256) throw new Error(`${zip} is not llama.cpp ${LLAMA.tag}: its SHA-256 is ${sha}`);

  clear(translate);
  const unpacked = join(root, "src-tauri", "target", "llama-unpacked");
  rmSync(unpacked, { recursive: true, force: true });
  mkdirSync(unpacked, { recursive: true });
  // Windows' own tar reads zip files
  const tar = join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe");
  execFileSync(tar, ["-xf", zip, "-C", unpacked], { stdio: "inherit" });
  let kept = 0;
  for (const name of readdirSync(unpacked)) {
    if (wanted(name)) { copyFileSync(join(unpacked, name), join(translate, name)); kept++; }
  }
  copyFileSync(join(root, "tools", "llama.cpp-LICENSE.txt"), join(translate, "LICENSE-llama.cpp.txt"));
  rmSync(unpacked, { recursive: true, force: true });
  for (const need of ["llama-server.exe", "llama-server-impl.dll", "llama.dll", "ggml-vulkan.dll", "ggml-cpu-x64.dll"]) {
    if (!existsSync(join(translate, need))) throw new Error(`no ${need} in ${LLAMA.file}`);
  }
  console.log(`engine-bin/translate holds llama.cpp ${LLAMA.tag}: ${kept} files.`);
}

if (process.argv.includes("--translator-only")) await stageTranslator();
else stageEngine();
