// After `tauri build`: put the installer and a portable exe of this version in dist/, with
// SHA-256 checksums, and remove the files of older versions. `npm run build` runs it.
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const { version } = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const release = join(root, "src-tauri", "target", "release");
const files = [
  [join(release, "bundle", "nsis", `Tafreegh_${version}_x64-setup.exe`), `Tafreegh_${version}_x64-setup.exe`],
  [join(release, "tafreegh.exe"), `Tafreegh_${version}_x64-portable.exe`],
];

const dist = join(root, "dist");
mkdirSync(dist, { recursive: true });
for (const name of readdirSync(dist)) rmSync(join(dist, name), { recursive: true, force: true });

const sums = [];
for (const [from, name] of files) {
  if (!existsSync(from)) throw new Error(`missing build output: ${from}`);
  copyFileSync(from, join(dist, name));
  sums.push(`${createHash("sha256").update(readFileSync(from)).digest("hex")}  ${name}`);
}
writeFileSync(join(dist, "SHA256SUMS.txt"), sums.join("\n") + "\n");
console.log(`dist/ now holds Tafreegh ${version}:\n  ${files.map(([, n]) => n).join("\n  ")}\n  SHA256SUMS.txt`);
