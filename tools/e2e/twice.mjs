// One Dawwin per data folder, in the real app: while the first one transcribes on the GPU, Dawwin
// started again on the same folder quits at once, with no window of its own, and the first one's
// job carries on with its audio in the cache and its engine; Dawwin on another data folder opens
// beside it; once the first one is closed, Dawwin opens on that folder again, with its history.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { APP, check, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);

// ---- a recording long enough to be in the middle of: the Arabic meeting four times, in a quiet room
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-ar.wav"))) {
  execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
}
const work = join(OUT, "twice-files");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const long = join(work, "long meeting.m4a");
ffmpeg(["-stream_loop", "3", "-i", join(voices, "meeting-ar.wav"), "-f", "lavfi", "-i", "anoisesrc=c=pink:a=0.003:r=16000:seed=7", "-filter_complex", "[0:a][1:a]amix=inputs=2:duration=first:normalize=0", "-c:a", "aac", long]);

const home = makeHome("twice-home");
const a = await open(home, PORT + 27);
const audioInCache = () => readdirSync(join(home, "cache")).filter((f) => f.endsWith(".f32")).sort().join();
const childrenOf = (pid) => execFileSync("pwsh", ["-NoProfile", "-c", `Get-CimInstance Win32_Process -Filter 'ParentProcessId=${pid}' | ForEach-Object { "$($_.ProcessId) $($_.Name)" }`], { encoding: "utf8" })
  .split(/\r?\n/).filter((l) => /dawwin-engine/i.test(l)).map((l) => +l.split(" ")[0]);
const jobOf = async (id) => (await a.state()).queue.find((j) => j.id === id);

// ---- the first one transcribes
const [id] = await a.invoke("enqueue", { paths: [slash(long)], language: "ar", steps: { remove_silence: false, save_audio: false, transcribe: true, translate: null, video: false, burn: false } });
let j;
for (let i = 0; i < 480; i++) {
  j = await jobOf(id);
  if (!j || (j.status === "transcribing" && j.progress > 0.05)) break;
  await sleep(250);
}
check("the first one is transcribing", j?.status === "transcribing", j && [j.status, j.progress, j.error]);
const cacheBefore = audioInCache();
const engineBefore = childrenOf(a.proc.pid);
const progressBefore = j?.progress ?? 0;

// ---- started again on the same data folder
const webview2 = join(OUT, `webview-${PORT + 28}`);
rmSync(webview2, { recursive: true, force: true });
const t0 = Date.now();
const second = spawn(APP, [], {
  stdio: "ignore",
  env: { ...process.env, DAWWIN_HOME: home, WEBVIEW2_USER_DATA_FOLDER: webview2, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT + 28} --mute-audio` },
});
process.on("exit", () => second.kill());
let code = null, answered = false;
second.on("exit", (c) => { code = c; });
for (let i = 0; i < 100 && code === null; i++) {
  try { await fetch(`http://127.0.0.1:${PORT + 28}/json`); answered = true; } catch { /* nothing there */ }
  await sleep(100);
}
const took = (Date.now() - t0) / 1000;
check("Dawwin started again on the same data folder quits at once", code === 0 && took < 10, { code, seconds: took });
check("with no window of its own", !answered && !existsSync(webview2));
j = await jobOf(id);
check("the first one's job carries on", j?.status === "transcribing" && !j.error, j && [j.status, j.progress, j.error]);
check("its audio in the cache is still there", cacheBefore !== "" && audioInCache() === cacheBefore, { before: cacheBefore, now: audioInCache() });
check("and so is its engine, the only one", engineBefore.length === 1 && childrenOf(a.proc.pid).join() === engineBefore.join(), { before: engineBefore, now: childrenOf(a.proc.pid) });
await sleep(2000);
j = await jobOf(id);
check("it goes on from where it was", !j || j.progress > progressBefore, j && [j.status, j.progress, progressBefore]);

// ---- Dawwin on another data folder opens beside it
let other = null;
try { other = await open(makeHome("twice-other-home"), PORT + 29); } catch { /* checked below */ }
check("Dawwin on another data folder opens beside it", !!other && !!(await other.state()).setup);
other?.close();

// ---- the job ends whole
check("the job ends", await a.idle(600));
const h = (await a.invoke("list_history")).find((x) => slash(x.source_path) === slash(long));
const t = h ? await a.invoke("get_transcript", { id: h.id }) : null;
const end = t?.segments.at(-1)?.end ?? 0;
check("its transcript is whole, to the end", t && !t.partial && t.duration_s > 400 && end > t.duration_s - 15, t && { partial: t.partial, lines: t.segments.length, seconds: t.duration_s, lastEnds: end });
a.close();
await sleep(1500);

// ---- once it is closed, Dawwin opens on that folder again
let again = null;
try { again = await open(home, PORT + 28); } catch { /* checked below */ }
check("once the first one is closed, Dawwin opens on that folder again", !!again);
if (again) {
  check("with its history", (await again.invoke("list_history")).some((x) => slash(x.source_path) === slash(long)));
  again.close();
}
finish();
