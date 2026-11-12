// Recording a meeting, through the real app, with two made-up tracks played at clock speed through
// the test sound source (`DAWWIN_TEST_RECORD`, debug builds): "them" is two of Windows' voices
// taking turns, with 24 s of digital silence in the middle, and "me" hears them through the speakers
// (56 ms late, 20 dB down) over a quiet room, and speaks once. Checked: live text, Me and Them as
// speakers and the others told apart, the language detected, the echo not written twice, the clock
// across the silence, the M4A, the save folder, the timing pass and Find the speakers on a meeting,
// falling behind, a forced close and the recovery, the engine missing, Cancel after Stop, a silent
// microphone and another chosen, the disk, one source alone, a translation after it, OneDrive.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { APP, check, FAKE_TRANSLATOR, finish, makeHome, MODEL, need, open, OUT, PORT, sleep, TOOLS } from "./lib.mjs";

const SPEAKERS = process.env.DAWWIN_E2E_SPEAKERS || "";
const LANGUAGE = process.env.DAWWIN_E2E_LANGUAGE || "";
need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);
need("DAWWIN_E2E_SPEAKERS (the speaker model)", SPEAKERS);
need("DAWWIN_E2E_LANGUAGE (the language model)", LANGUAGE);
const ONLY = process.argv[2] || "all";
const ff = join(TOOLS, "ffmpeg-audio", "ffmpeg.exe");
const duration = (file) => {
  let info = "";
  try { execFileSync(ff, ["-hide_banner", "-i", file], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }); } catch (e) { info = String(e.stderr); }
  const m = /Duration: (\d+):(\d+):([\d.]+)/.exec(info);
  return m ? +m[1] * 3600 + +m[2] * 60 + +m[3] : NaN;
};

// ---------------------------------------------------------------- the made-up tracks
const SR = 16000;
const media = join(OUT, "record-media");
rmSync(media, { recursive: true, force: true });
mkdirSync(media, { recursive: true });
const said = [
  ["Microsoft David Desktop", "Good morning everyone, and thank you for joining the call about the new monthly reports."],
  ["Microsoft Zira Desktop", "Thank you. The reports are ready now, and the numbers finally match the invoices."],
  ["Microsoft David Desktop", "Then we can send them to the client on Thursday, and meet again next week to review."],
  ["Microsoft Zira Desktop", "Agreed. I will write the summary of this call and share it with the whole team today."],
  ["Microsoft David Desktop", "I have one question about the budget for the second quarter, before we finish."],
];
const ps = said.map(([voice, text], k) => `$s.SelectVoice('${voice}'); $s.SetOutputToWaveFile('${join(media, `said${k}.wav`)}', (New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo 16000, ([System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen), ([System.Speech.AudioFormat.AudioChannel]::Mono))); $s.Speak('${text.replace(/'/g, "''")}');`).join(" ");
execFileSync("powershell", ["-NoProfile", "-Command", `Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; ${ps} $s.SetOutputToNull()`]);
const pcm = (file) => {
  const b = readFileSync(file);
  let at = 12;
  while (at + 8 <= b.length && b.toString("latin1", at, at + 4) !== "data") at += 8 + b.readUInt32LE(at + 4);
  const n = Math.min(b.readUInt32LE(at + 4), b.length - at - 8) / 2;
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) out[i] = b.readInt16LE(at + 8 + 2 * i) / 32768;
  return out;
};
const voice = said.map((_, k) => pcm(join(media, `said${k}.wav`)));
const LEN = 66;
const them = new Float32Array(LEN * SR);
let at = Math.round(0.5 * SR);
them.set(voice[0], at); at += voice[0].length + Math.round(0.6 * SR);
them.set(voice[1], at);
const AFTER = 44; // 24 s of digital silence before it
at = AFTER * SR;
them.set(voice[2], at); at += voice[2].length + Math.round(0.6 * SR);
them.set(voice[3], at);
const OWN = 25; // "me" speaks in the silence
let seed = 1;
const noise = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff - 0.5) * 2;
const me = new Float32Array(LEN * SR);
const delay = Math.round(0.056 * SR);
for (let i = 0; i < me.length; i++) me[i] = (i >= delay ? them[i - delay] * 0.1 : 0) + noise() * 0.002 + (i >= OWN * SR && i - OWN * SR < voice[4].length ? voice[4][i - OWN * SR] : 0);
const wav = (path, f) => {
  const data = Buffer.alloc(f.length * 2);
  for (let i = 0; i < f.length; i++) data.writeInt16LE(Math.max(-32767, Math.min(32767, Math.round(f[i] * 32767))), i * 2);
  const h = Buffer.alloc(44);
  h.write("RIFF", 0); h.writeUInt32LE(36 + data.length, 4); h.write("WAVE", 8); h.write("fmt ", 12); h.writeUInt32LE(16, 16);
  h.writeUInt16LE(1, 20); h.writeUInt16LE(1, 22); h.writeUInt32LE(SR, 24); h.writeUInt32LE(SR * 2, 28); h.writeUInt16LE(2, 32); h.writeUInt16LE(16, 34);
  h.write("data", 36); h.writeUInt32LE(data.length, 40);
  writeFileSync(path, Buffer.concat([h, data]));
};
const ME = join(media, "me.wav"), THEM = join(media, "them.wav");
wav(ME, me);
wav(THEM, them);
const TEST = `me=${ME}|them=${THEM}`;
const OWN_END = OWN + voice[4].length / SR;

// ---------------------------------------------------------------- helpers
let port = PORT + 60;
const home = (name, extra = {}, settings = {}) => {
  const h = makeHome("record-" + name, { model_files: { speakers: SPEAKERS, language: LANGUAGE }, ...settings });
  writeFileSync(join(h, "recording.json"), JSON.stringify({ sources: "both", mic: null, notice_shown: true, ...extra }));
  return h;
};
const job = async (a, id) => (await a.invoke("get_state")).queue.find((j) => j.id === id);
/** A job to its end: the page takes finished jobs off the list at once, so gone means done. */
async function waitDone(a, id, seconds = 240) {
  let j;
  for (let i = 0; i < seconds * 2; i++) {
    await sleep(500);
    j = await job(a, id);
    if (!j || ["done", "failed", "cancelled"].includes(j.status)) break;
  }
  return j || { status: "done", gone: true };
}
const tryInvoke = async (a, cmd, args) => { try { return await a.invoke(cmd, args); } catch (e) { return { error: String(e.message || e) }; } };
const words = (t) => t.toLowerCase().replace(/[^\p{L}\s]/gu, "").split(/\s+/).filter(Boolean);
const STEPS = { remove_silence: false, save_audio: false, transcribe: true, speakers: false, translate: null, video: false, burn: false };

// ---- 1. a meeting: the language heard, Me and the others told apart, live text, the clock, the file
if (ONLY === "all" || ONLY === "meeting") {
  const saveDir = join(OUT, "record-saved");
  rmSync(saveDir, { recursive: true, force: true });
  mkdirSync(saveDir, { recursive: true });
  const h = home("meeting", {}, { save_dir: saveDir });
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const st = await a.invoke("get_state");
  check("Dawwin's own ffmpeg is the one in use", (st.setup.ffmpeg || "").includes(join(h, "tools", "ffmpeg-")), st.setup.ffmpeg);
  const setup = await a.invoke("record_setup");
  check("recordings go to the folder chosen in Settings", setup.folder === saveDir && setup.synced === false, setup.folder);
  const id = await a.invoke("start_recording", { language: "auto", steps: { ...STEPS, speakers: true } });
  check("Record starts, with Detect the language", typeof id === "string", id);
  await sleep(4000);
  const v = await a.invoke("recording_state");
  check("a level for Me and one for Them while it records", v?.status?.me && v?.status?.them, v?.status);
  await sleep(31000);
  const live = await a.invoke("get_transcript", { id });
  check("text while it records, in the language heard", live.segments.length > 0 && live.language === "en", { n: live.segments.length, language: live.language });
  check("live lines are Me or Them from the first", live.segments.every((s) => s.speaker === (s.track === "me" ? 1 : 2)) && live.speakers.map((x) => x.name).join() === "Me,Them", live.speakers);
  await sleep(LEN * 1000 - 35000 + 1500);
  const t0 = Date.now();
  await a.invoke("stop_recording");
  const j = await waitDone(a, id);
  check("after Stop it is done within moments (the others told apart too)", j.status === "done" && Date.now() - t0 < 60000, { status: j.status, s: (Date.now() - t0) / 1000, err: j.error });
  const tx = await a.invoke("get_transcript", { id });
  const segs = tx.segments;
  console.log(segs.map((s) => `   ${s.track} ${s.speaker} ${s.start.toFixed(1)}-${s.end.toFixed(1)} ${s.text.slice(0, 60)}`).join("\n"));
  check("the language is English, as heard", tx.language === "en" && tx.detected === "en", { language: tx.language, detected: tx.detected });
  const mine = segs.filter((s) => s.track === "me"), theirs = segs.filter((s) => s.track === "them");
  check("Me is speaker 1, named Me", mine.length > 0 && mine.every((s) => s.speaker === 1) && tx.speakers.find((x) => x.id === 1)?.name === "Me", tx.speakers);
  check("the others are told apart by voice", new Set(theirs.map((s) => s.speaker)).size === 2 && theirs.every((s) => s.speaker > 1), theirs.map((s) => s.speaker));
  const after = theirs.find((s) => s.start > 30);
  check("the time after 24 s of silence stays true to the clock", after && Math.abs(after.start - AFTER) < 1.5, after?.start);
  check("Me only where Me speaks: the echo is not written twice", mine.every((s) => s.start >= OWN - 1 && s.end <= OWN_END + 2), mine.map((s) => [s.start, s.end]));
  const theirWords = new Set(theirs.flatMap((s) => words(s.text)));
  const shared = mine.flatMap((s) => words(s.text)).filter((w) => theirWords.has(w) && w.length > 3);
  check("Me's words are its own", shared.length <= 2, shared);
  check("the lines are in time order", segs.every((s, i) => i === 0 || segs[i - 1].start <= s.start));
  const kept = tx.source_path;
  check("kept as an M4A in the folder chosen", kept.endsWith(".m4a") && dirname(kept) === saveDir && existsSync(kept), kept);
  check("the M4A is as long as the recording", Math.abs(duration(kept) - tx.duration_s) < 1, { m4a: duration(kept), recorded: tx.duration_s });
  const perHour = statSync(kept).size / 1e6 / (tx.duration_s / 3600);
  check("about 45 MB an hour", perHour > 25 && perHour < 60, perHour.toFixed(1));
  check("the WAV and the journal are gone", readdirSync(saveDir).every((f) => !f.endsWith(".wav") && !f.endsWith(".json")), readdirSync(saveDir));
  check("the M4A is listed as the transcript's own file", tx.outputs_listed && tx.outputs.includes(kept));
  const txt = await a.invoke("transcript_text", { id, stamps: true });
  check("the text says who: Me, and the others", txt.includes("Me: ") && /Speaker \d: /.test(txt), txt.slice(0, 160));

  // ---- the timing pass on a meeting: each line heard on its own track
  const fid = await a.invoke("fix_timing", { id });
  const fj = await waitDone(a, fid, 300);
  const timed = await a.invoke("get_transcript", { id });
  const wordsOf = (s) => (s.words || []).map((w) => w[1]);
  const myTimes = timed.segments.filter((s) => s.track === "me").flatMap(wordsOf);
  // a line with pauses in it is heard again in stretches (one without keeps its estimated times)
  const heardAgain = (track) => timed.segments.some((s) => s.track === track && (s.words || []).length > 0);
  check("Fix the timing works on a meeting, both tracks", fj.status === "done" && timed.timed && heardAgain("me") && heardAgain("them"), { status: fj.status, timed: timed.timed });
  check("Me's words are placed where Me speaks", myTimes.length > 0 && myTimes.every((x) => x >= OWN - 0.5 && x <= OWN_END + 0.5), [Math.min(...myTimes), Math.max(...myTimes)]);
  await a.ev(`openTranscript(${JSON.stringify(id)})`);
  await sleep(1200);
  const labels = await a.ev(`[...document.querySelectorAll('.seg .who')].map((b) => b.textContent)`);
  check("the page names each line: Me, Speaker 2, Speaker 3", labels.includes("Me") && labels.some((l) => /Speaker [23]/.test(l)), labels);
  await a.shot(join(OUT, "record-meeting.png"));
  a.close();
}

// ---- 2. Find the speakers on a meeting recorded without them: Me stays, the others are told apart
if (ONLY === "all" || ONLY === "find") {
  const h = home("find");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(LEN * 1000 + 1500);
  await a.invoke("stop_recording");
  await waitDone(a, id);
  let tx = await a.invoke("get_transcript", { id });
  check("without the step: Me and Them", tx.speakers.map((x) => x.name).join() === "Me,Them" && tx.segments.every((s) => s.speaker === (s.track === "me" ? 1 : 2)), tx.speakers);
  await a.invoke("rename_speaker", { id, speaker: 1, name: "Baraa" });
  const sid = await a.invoke("find_speakers", { id });
  const sj = await waitDone(a, sid, 300);
  tx = await a.invoke("get_transcript", { id });
  const theirs = tx.segments.filter((s) => s.track === "them");
  check("Find the speakers splits the others and leaves Me as named", sj.status === "done" && new Set(theirs.map((s) => s.speaker)).size === 2 && tx.segments.filter((s) => s.track === "me").every((s) => s.speaker === 1) && tx.speakers.find((x) => x.id === 1)?.name === "Baraa", tx.speakers);
  a.close();
}

// ---- 3. a PC too slow to keep up: the recording stays whole, the text comes after it
if (ONLY === "all" || ONLY === "behind") {
  const h = home("behind");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_ENGINE_DELAY_MS: "14000", DAWWIN_TEST_RECORD_BEHIND_S: "8" });
  const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(56000);
  const v = await a.invoke("recording_state");
  check("live text stops when the engine falls behind", v?.behind === true, v?.behind);
  await a.invoke("stop_recording");
  const j = await waitDone(a, id, 400);
  const tx = await a.invoke("get_transcript", { id });
  check("after Stop the rest is transcribed from the file", j.status === "done" && tx.segments.some((s) => s.track === "them" && s.start > 40) && tx.segments.some((s) => s.track === "me"), tx.segments.map((s) => [s.track, Math.round(s.start)]));
  check("the recording is whole", Math.abs(duration(tx.source_path) - 56) < 2, duration(tx.source_path));
  a.close();
}

// ---- 4. closed by force: the recording up to the last second, offered at the next start
if (ONLY === "all" || ONLY === "crash") {
  const h = home("crash");
  let a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(30000);
  execFileSync("taskkill", ["/F", "/T", "/PID", String(a.proc.pid)], { stdio: "ignore" });
  await sleep(1500);
  const folder = join(h, "recordings");
  const wavs = readdirSync(folder).filter((f) => f.endsWith(".wav"));
  check("a WAV and its journal are there after the crash", wavs.length === 1 && readdirSync(folder).some((f) => f.endsWith(".dawwin-recording.json")), readdirSync(folder));
  check("the WAV is readable up to the last second or so", duration(join(folder, wavs[0])) > 27, duration(join(folder, wavs[0])));
  a = await open(h, port++, {});
  const found = await a.invoke("interrupted_recordings");
  check("the next start offers it back", found.length === 1 && found[0].seconds > 27, found);
  const rid = await a.invoke("recover_recording", { journal: found[0].journal, keep: true });
  const j = await waitDone(a, rid, 300);
  const tx = await a.invoke("get_transcript", { id: rid });
  check("Transcribe it writes its text, Me and Them", rid === id && j.status === "done" && tx.segments.some((s) => s.track === "them") && tx.segments.some((s) => s.track === "me"), { status: j.status, n: tx.segments.length });
  check("nothing is offered any more", (await a.invoke("interrupted_recordings")).length === 0);
  a.close();
  a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(6000);
  execFileSync("taskkill", ["/F", "/T", "/PID", String(a.proc.pid)], { stdio: "ignore" });
  await sleep(1500);
  a = await open(h, port++, {});
  const again = await a.invoke("interrupted_recordings");
  await a.invoke("recover_recording", { journal: again[0].journal, keep: false });
  check("Delete it removes the file and the journal", readdirSync(folder).every((f) => !f.endsWith(".wav") && !f.endsWith(".json")), readdirSync(folder));
  a.close();
}

// ---- 5. the speech engine cannot start: the recording goes on, and is offered again
if (ONLY === "all" || ONLY === "engine") {
  const engineDir = join(dirname(APP), "engine");
  const away = engineDir + "-away";
  renameSync(engineDir, away);
  try {
    const h = home("engine");
    let a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
    const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
    await sleep(12000);
    const v = await a.invoke("recording_state");
    check("without the engine the recording goes on", v?.status && !v.status.ended && v.status.seconds > 10, v?.status?.seconds);
    await a.invoke("stop_recording");
    const j = await waitDone(a, id, 60);
    check("after Stop the job says the engine is missing", j.status === "failed" && /engine/i.test(j.error || ""), { status: j.status, error: j.error });
    const offered = await a.invoke("interrupted_recordings");
    check("the recording is kept whole and offered again", offered.length === 1 && Math.abs(offered[0].seconds - 12.5) < 2, offered);
    a.close();
    renameSync(away, engineDir);
    a = await open(h, port++, {});
    const rid = await a.invoke("recover_recording", { journal: offered[0].journal, keep: true });
    const j2 = await waitDone(a, rid, 120);
    const tx = await a.invoke("get_transcript", { id: rid });
    check("with the engine back, its text is written", j2.status === "done" && tx.segments.length > 0, { status: j2.status, n: tx.segments.length });
    a.close();
  } finally {
    if (existsSync(away)) renameSync(away, engineDir);
  }
}

// ---- 6. Cancel after Stop: the text stops, the recording stays to be offered again
if (ONLY === "all" || ONLY === "cancel") {
  const h = home("cancel");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_ENGINE_DELAY_MS: "6000", DAWWIN_TEST_RECORD_BEHIND_S: "5" });
  const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(40000);
  await a.invoke("stop_recording");
  let j;
  for (let i = 0; i < 60; i++) { await sleep(500); j = await job(a, id); if (j?.status === "transcribing") break; }
  check("after Stop it is transcribed from the file", j?.status === "transcribing", j?.status);
  await a.invoke("cancel_job", { id });
  j = await waitDone(a, id, 60);
  check("Cancel stops the text", j.status === "cancelled", j.status);
  const folder = join(h, "recordings");
  check("the WAV and its journal are kept, and offered again", readdirSync(folder).some((f) => f.endsWith(".wav")) && (await a.invoke("interrupted_recordings")).length === 1, readdirSync(folder));
  const found = (await a.invoke("interrupted_recordings"))[0];
  await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(2000);
  const rid = await a.invoke("recover_recording", { journal: found.journal, keep: true });
  await a.invoke("cancel_job", { id: rid });
  await a.invoke("stop_recording");
  await sleep(15000);
  const rj = await job(a, rid);
  check("a recovered recording cancelled while it waited does not run, and is offered again", (!rj || rj.status === "cancelled") && (await a.invoke("interrupted_recordings")).some((x) => x.journal === found.journal), rj?.status);
  a.close();
}

// ---- 7. a microphone that sends nothing, another chosen in its menu; a disk running low
if (ONLY === "all" || ONLY === "mic") {
  const h = home("mic");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST + "|silent", DAWWIN_TEST_DISK_FREE: "1500000000" });
  const setup = await a.invoke("record_setup");
  check("the microphones are listed", setup.microphones?.length === 2, setup.microphones);
  const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
  await sleep(6000);
  let v = await a.invoke("recording_state");
  check("a microphone that sends only silence is said to", v?.status?.mic_silent === true, v?.status);
  check("a disk under 2 GB is warned of", v?.status?.disk_low === true);
  await a.ev(`showView("home")`);
  await sleep(800);
  await a.shot(join(OUT, "record-silent.png"));
  const menu = await a.ev(`(async () => { const m = document.querySelector('#record-bar select[data-rec="mic-now"]'); if (!m) return "no menu"; m.focus(); await new Promise((r) => setTimeout(r, 1500)); return m.isConnected && document.activeElement === m ? [...m.options].map((o) => o.textContent) : "drawn again under the pointer"; })()`);
  check("the menu of the other microphones stays open while it records", Array.isArray(menu) && menu.length === 3, menu);
  await a.ev(`(() => { const m = document.querySelector('#record-bar select[data-rec="mic-now"]'); m.value = "test-other"; m.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  await sleep(5000);
  v = await a.invoke("recording_state");
  check("another microphone, chosen while it records, is heard at once", v?.status?.mic_silent === false && v?.status?.mic === "test-other" && v.status.me.db > -70, v?.status);
  await a.invoke("stop_recording");
  await waitDone(a, id);
  a.close();
  const low = home("disk");
  const b = await open(low, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_DISK_FREE: "400000000" });
  await b.ev(`window.__TAURI__.event.listen("recording", (e) => { window.__last = e.payload.status; }), true`);
  const id2 = await b.invoke("start_recording", { language: "en", steps: STEPS });
  const j = await waitDone(b, id2, 60);
  const last = await b.ev(`window.__last`);
  await b.ev(`showView("home")`);
  await sleep(500);
  const bar = await b.ev(`document.getElementById("record-bar").textContent`);
  check("under 500 MB the recording stops cleanly by itself, and says why", last?.disk_stopped === true && last.ended && j.status === "done" && bar.includes("disk"), { last, status: j.status });
  b.close();
}

// ---- 8. the microphone alone, and the PC's sound alone
if (ONLY === "all" || ONLY === "one") {
  for (const sources of ["mic", "pc"]) {
    const h = home("one-" + sources, { sources });
    const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
    const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
    await sleep(40000);
    await a.invoke("stop_recording");
    const j = await waitDone(a, id);
    const tx = await a.invoke("get_transcript", { id });
    const want = sources === "mic" ? "me" : "them";
    check(`${sources} only: every line is ${want}, one speaker`, j.status === "done" && tx.segments.length > 0 && tx.segments.every((s) => s.track === want && s.speaker === 1) && tx.speakers.length === 1, tx.speakers);
    a.close();
  }
}

// ---- 9. the steps ticked on the start page follow: a translation (the stand-in translator)
if (ONLY === "all" || ONLY === "translate") {
  const h = home("translate");
  const model = join(h, "models", "MiLMMT-46-1B-v1.0.Q4_K_M.gguf");
  execFileSync("fsutil", ["file", "createnew", model, "806057408"], { stdio: "ignore" });
  const { openSync, writeSync, closeSync } = await import("node:fs");
  const fd = openSync(model, "r+"); writeSync(fd, Buffer.from([0x47, 0x47, 0x55, 0x46, 3, 0, 0, 0]), 0, 8, 0); closeSync(fd);
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_TRANSLATOR_DELAY_MS: "60" });
  const id = await a.invoke("start_recording", { language: "en", steps: { ...STEPS, translate: "ar" } });
  await sleep(30000);
  await a.invoke("stop_recording");
  await waitDone(a, id);
  let tx;
  for (let i = 0; i < 120; i++) { await sleep(500); tx = await a.invoke("get_transcript", { id }); if (tx.segments.length && tx.translations?.[0]?.lines?.filter((l) => l.text).length === tx.segments.length) break; }
  check("a translation ticked on the start page follows the recording", tx.translations?.[0]?.language === "ar" && tx.translations[0].lines.filter((l) => l.text).length === tx.segments.length, tx.translations?.[0]?.lines?.length);
  a.close();
  rmSync(model, { force: true });
}

// ---- 10. a folder OneDrive uploads: said where Record is
if (ONLY === "all" || ONLY === "onedrive") {
  const synced = join(OUT, "record-synced", "OneDrive - Contoso");
  mkdirSync(synced, { recursive: true });
  const h = home("onedrive", {}, { save_dir: synced });
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const setup = await a.invoke("record_setup");
  check("a save folder inside OneDrive is known", setup.synced === true && setup.folder === synced, setup);
  await a.ev(`(async () => { REC.setup = await window.__TAURI__.core.invoke("record_setup"); showView("home"); renderRecordBar(); })()`);
  await sleep(500);
  const bar = await a.ev(`document.getElementById("record-bar").textContent`);
  check("the start page says recordings would be uploaded", bar.includes("OneDrive"), bar.replace(/\s+/g, " ").slice(0, 200));
  await a.shot(join(OUT, "record-onedrive.png"));
  a.close();
  const b = await open(home("not-onedrive"), port++, { DAWWIN_TEST_RECORD: TEST });
  check("a folder outside OneDrive is not flagged", (await b.invoke("record_setup")).synced === false);
  b.close();
}

// ---- the pictures of How to use (ui/guide/<lang>/record.webp) and the README (docs/screenshots/
// record.png): a meeting being recorded, its levels and its lines. Writes into the repository; not
// part of "all":  node tools/e2e/record.mjs pictures
if (ONLY === "pictures") {
  const { REPO } = await import("./lib.mjs");
  const h = home("pictures", {}, { model_files: { speakers: SPEAKERS, language: LANGUAGE } });
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const box = (sel, maxH) => a.ev(`(()=>{const r=document.querySelector(${JSON.stringify(sel)}).getBoundingClientRect();return {x:r.left,y:r.top,width:r.width,height:Math.min(r.height,${maxH})}})()`);
  for (const lang of ["en", "ar"]) {
    await a.call("Emulation.setDeviceMetricsOverride", { width: 1200, height: 820, deviceScaleFactor: 1, mobile: false });
    await a.ev(`setUiLang(${JSON.stringify(lang)})`);
    await a.ev(`showView('home')`);
    await sleep(600);
    const id = await a.invoke("start_recording", { language: "en", steps: STEPS });
    await a.ev(`openTranscript(${JSON.stringify(id)})`);
    for (let i = 0; i < 400; i++) {
      await sleep(250);
      if ((await a.ev(`[...document.querySelectorAll('#segments .seg .who')].map((b) => b.textContent)`)).length >= 3) break;
    }
    await a.ev(`document.getElementById('toast').hidden = true`);
    const r = await a.call("Page.captureScreenshot", { format: "webp", quality: 85, clip: { ...(await box("#main", 470)), scale: 1 } });
    writeFileSync(join(REPO, "ui", "guide", lang, "record.webp"), Buffer.from(r.result.data, "base64"));
    if (lang === "en") {
      await a.call("Emulation.setDeviceMetricsOverride", { width: 1500, height: 1000, deviceScaleFactor: 1, mobile: false });
      await sleep(800);
      await a.shot(join(REPO, "docs", "screenshots", "record.png"));
    }
    await a.invoke("stop_recording");
    for (let i = 0; i < 240; i++) { await sleep(500); if (!(await a.ev(`S.snap.queue.some((j) => j.kind === 'record' && ACTIVE.has(j.status))`))) break; }
  }
  await a.ev(`setUiLang('en')`);
  check("the pictures are made", true);
  a.close();
}

finish();
