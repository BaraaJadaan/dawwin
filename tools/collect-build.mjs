// After `tauri build`: put the installer and a portable zip of this version in dist/, with SHA-256
// checksums, and remove the files of older versions. `npm run build` runs it.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const { version } = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const release = join(root, "src-tauri", "target", "release");
const engine = join(root, "src-tauri", "engine-bin");
const setup = `Dawwin_${version}_x64-setup.exe`;
const portable = `Dawwin_${version}_x64-portable.zip`;

for (const f of [join(release, "bundle", "nsis", setup), join(release, "dawwin.exe"), join(engine, "dawwin-engine.exe")]) {
  if (!existsSync(f)) throw new Error(`missing build output: ${f}`);
}

const dist = join(root, "dist");
mkdirSync(dist, { recursive: true });
for (const name of readdirSync(dist)) rmSync(join(dist, name), { recursive: true, force: true });
copyFileSync(join(release, "bundle", "nsis", setup), join(dist, setup));

// the portable version is a folder: the app, and the engine next to it
const staging = join(root, "src-tauri", "target", "portable");
rmSync(staging, { recursive: true, force: true });
const folder = join(staging, "Dawwin");
mkdirSync(join(folder, "engine"), { recursive: true });
copyFileSync(join(release, "dawwin.exe"), join(folder, "dawwin.exe"));
cpSync(engine, join(folder, "engine"), { recursive: true });
// Windows' own tar: it makes zip files, and another tar on the PATH may take "D:" for a host name
const tar = join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe");
execFileSync(tar, ["-a", "-cf", join(dist, portable), "-C", staging, "Dawwin"], { stdio: "inherit" });
rmSync(staging, { recursive: true, force: true });

const sums = [setup, portable].map((n) => `${createHash("sha256").update(readFileSync(join(dist, n))).digest("hex")}  ${n}`);
writeFileSync(join(dist, "SHA256SUMS.txt"), sums.join("\n") + "\n");

const gpu = existsSync(join(engine, "ggml-vulkan.dll"));
const font = existsSync(join(root, "ui", "fonts", "thmanyahsans-Regular.otf"));
console.log(`dist/ now holds Dawwin ${version}:\n  ${setup}\n  ${portable}\n  SHA256SUMS.txt`);
console.log(`GPU support: ${gpu ? "yes" : "NO, CPU only"}. Thmanyah font: ${font ? "yes" : "NO, system font"}.`);
