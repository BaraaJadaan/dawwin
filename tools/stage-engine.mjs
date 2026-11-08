// Puts the speech engine (dawwin-engine.exe and its DLLs) in src-tauri/engine-bin, where the app's
// build picks it up.
//
//   node tools/stage-engine.mjs               build it here: with GPU support when the Vulkan SDK
//                                             is installed (VULKAN_SDK is set), else CPU only
//   node tools/stage-engine.mjs --from-ci     take the newest build from GitHub Actions instead
//   node tools/stage-engine.mjs --from-ci ID  take the build of that workflow run
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const bin = join(root, "src-tauri", "engine-bin");
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: "inherit", shell: process.platform === "win32" });

mkdirSync(bin, { recursive: true });
for (const name of readdirSync(bin)) {
  if (name !== "README.md") rmSync(join(bin, name), { force: true });
}

const fromCi = process.argv.indexOf("--from-ci");
if (fromCi >= 0) {
  const id = process.argv[fromCi + 1];
  const args = ["run", "download", "-n", "dawwin-engine-windows-x64", "-D", bin];
  if (id) args.splice(2, 0, id);
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

const files = readdirSync(bin).filter((n) => n !== "README.md");
if (!existsSync(join(bin, "dawwin-engine.exe"))) throw new Error("no dawwin-engine.exe in " + bin);
console.log(`engine-bin holds ${files.length} files. GPU support: ${files.includes("ggml-vulkan.dll") ? "yes" : "no"}`);
