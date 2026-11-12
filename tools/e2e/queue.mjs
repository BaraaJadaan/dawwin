// The queue when something goes wrong, in the real app with a stand-in engine
// (src-tauri/examples/fake_engine.rs): a model load that hangs on the GPU (as with a stuck
// graphics driver) ends after its time limit and the job is done on the CPU; stopping a job while
// the engine hangs ends it within seconds and the next job runs; a bug inside a job fails that job
// only. After finding the GPU, no engine process stays behind. With "Remove silences", a long
// recording takes about as much memory as without.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { APP, check, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, sleep, slash } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL); // only so that the app has a model to name: the stand-in reads none
const FAKE = join(dirname(APP), "examples", "fake_engine.exe");
need("the stand-in engine (cargo build --example fake_engine)", existsSync(FAKE) && FAKE);

const work = join(OUT, "queue-recordings");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const tone = (name, seconds) => {
  const f = join(work, name);
  ffmpeg(["-f", "lavfi", "-i", `sine=frequency=220:duration=${seconds}`, "-ar", "16000", "-ac", "1", f]);
  return slash(f);
};
const steps = { remove_silence: false, save_audio: false, transcribe: true, translate: null, video: false, burn: false };
const engines = () => {
  const out = execFileSync("tasklist", ["/FI", "IMAGENAME eq fake_engine.exe", "/FO", "CSV", "/NH"], { encoding: "utf8" });
  return out.split("\n").filter((l) => l.includes("fake_engine.exe")).length;
};
const jobOf = async (a, id) => (await a.state()).queue.find((j) => j.id === id);
// the interface moves a finished job out of the list into History: gone after being seen is done
const waitFor = async (a, id, done, seconds) => {
  let seen = null;
  for (let i = 0; i < seconds * 4; i++) {
    const j = await jobOf(a, id);
    if (!j && seen) return { ...seen, status: "done" };
    if (j && done(j)) return j;
    seen = j || seen;
    await sleep(250);
  }
  return (await jobOf(a, id)) || { ...seen, status: seen ? "done" : "missing" };
};

// ---- a load that hangs on the GPU, a stop while it hangs, the next job
{
  const home = makeHome("queue-hang");
  const before = engines();
  const a = await open(home, PORT + 6, { DAWWIN_TEST_ENGINE: FAKE, FAKE_ENGINE_HANG_LOAD: "1", DAWWIN_TEST_LOAD_LIMIT_S: "6" });
  await sleep(2500);
  const st = await a.state();
  check("the GPU is found, and no engine process stays behind after", st.engine.gpu === "Fake GPU" && engines() === before, { state: st.engine.state, processes: engines() - before });
  const [stuck] = await a.invoke("enqueue", { paths: [tone("stuck.wav", 4)], language: "ar", steps });
  const [next] = await a.invoke("enqueue", { paths: [tone("next.wav", 4)], language: "ar", steps });
  await waitFor(a, stuck, (j) => j.status === "loading", 20);
  const t0 = Date.now();
  await a.invoke("cancel_job", { id: stuck });
  const stopped = await waitFor(a, stuck, (j) => !["queued", "decoding", "loading", "transcribing"].includes(j.status), 15);
  check("stopping a job whose model hangs ends it within seconds, as stopped", stopped.status === "cancelled" && Date.now() - t0 < 6000, { status: stopped.status, ms: Date.now() - t0 });
  const done = await waitFor(a, next, (j) => ["done", "failed"].includes(j.status), 60);
  const engine = (await a.state()).engine;
  check("the next job runs: the load times out on the GPU, and it is done on the CPU", done.status === "done" && engine.note === "gpu-failed" && engine.device === "cpu", { status: done.status, error: done.error, engine });
  a.close();
}

// ---- a bug in a job fails that job only
{
  const home = makeHome("queue-panic");
  const a = await open(home, PORT + 7, { DAWWIN_TEST_ENGINE: FAKE, DAWWIN_TEST_PANIC: "goes-wrong" });
  const [bad] = await a.invoke("enqueue", { paths: [tone("goes-wrong.wav", 3)], language: "ar", steps });
  const [good] = await a.invoke("enqueue", { paths: [tone("fine.wav", 3)], language: "ar", steps });
  const b = await waitFor(a, bad, (j) => ["done", "failed", "cancelled"].includes(j.status), 30);
  const g = await waitFor(a, good, (j) => ["done", "failed", "cancelled"].includes(j.status), 30);
  check("a job that goes wrong inside Dawwin fails, with the reason", b.status === "failed" && /Something went wrong/.test(b.error || ""), { status: b.status, error: b.error });
  check("and the next job still runs", g.status === "done", { status: g.status, error: g.error });
  a.close();
}

// ---- subtitles exported while another job runs: made at once, the person told, timed later
{
  const home = makeHome("queue-busy");
  const said = tone("said.wav", 6);
  const { writeFileSync } = await import("node:fs");
  writeFileSync(join(home, "history", "earlier.json"), JSON.stringify({
    id: "earlier", title: "earlier", source_path: said, created_ms: Date.now(), duration_s: 6, language: "ar", elapsed_s: 1, device: "CPU", model: "made up", partial: false,
    segments: [{ start: 0.2, end: 3, text: "طيب، خلونا نبدأ.", edited: false, voice: [0.2, 3], pauses: [[0.5, 1.4, 1.9]] }, { start: 3, end: 5.8, text: "الـ dashboard أولًا.", edited: false, voice: [3, 5.8] }],
  }));
  const a = await open(home, PORT + 14, { DAWWIN_TEST_ENGINE: FAKE, FAKE_ENGINE_SLOW_MS: "400" });
  await a.ev(`window.__told = []; window.__TAURI__.event.listen('timing-later', () => window.__told.push(1)); 1`);
  const [long] = await a.invoke("enqueue", { paths: [tone("long.wav", 600)], language: "ar", steps });
  await waitFor(a, long, (j) => j.status === "transcribing", 60);
  const t0 = Date.now();
  const srt = join(work, "earlier.srt");
  await a.invoke("export_to_path", { id: "earlier", path: slash(srt), stamps: false });
  const ms = Date.now() - t0;
  const queued = (await a.state()).queue.some((j) => j.kind === "timing" && j.transcript === "earlier");
  check("subtitles exported while a long job runs are saved at once, the words placed in time after it", ms < 3000 && existsSync(srt) && queued, { ms, queued });
  await sleep(300);
  check("and the person is told so", (await a.ev(`window.__told.length`)) === 1 && /estimated/.test(await a.ev(`t('toast.timingLater')`)));
  await a.invoke("cancel_all_jobs");
  await a.idle(60);
  a.close();
}

// ---- a long recording without its silences takes about as much memory as with them
{
  const long = join(work, "an hour.wav");
  // an hour: speech-like tone half the time, silence between
  ffmpeg(["-f", "lavfi", "-i", "sine=frequency=200:duration=3600", "-af", "volume='if(lt(mod(t,8),4),1,0)':eval=frame", "-ar", "16000", "-ac", "1", long]);
  const peak = async (removeSilence) => {
    const home = makeHome(`queue-memory-${removeSilence}`);
    const a = await open(home, PORT + (removeSilence ? 9 : 8), { DAWWIN_TEST_ENGINE: FAKE });
    const [id] = await a.invoke("enqueue", { paths: [slash(long)], language: "ar", steps: { ...steps, remove_silence: removeSilence } });
    const j = await waitFor(a, id, (x) => ["done", "failed", "cancelled"].includes(x.status), 600);
    const bytes = Number(execFileSync("powershell", ["-NoProfile", "-Command", `(Get-Process -Id ${a.proc.pid}).PeakWorkingSet64`], { encoding: "utf8" }).trim());
    a.close();
    await sleep(1500);
    return { status: j?.status, mb: Math.round(bytes / 1e6) };
  };
  const without = await peak(false);
  const withRemoval = await peak(true);
  check("an hour with \"Remove silences\" takes about the memory it takes without", without.status === "done" && withRemoval.status === "done" && withRemoval.mb < without.mb * 1.15, { without, with: withRemoval });
}
finish();
