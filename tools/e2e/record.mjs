// Recording a meeting, through the real app, with two made-up tracks played at clock speed through
// the test sound source (`DAWWIN_TEST_RECORD`, debug builds): "them" is two of Windows' voices
// taking turns, with 24 s of digital silence in the middle, and "me" hears them through the speakers
// (56 ms late, 20 dB down) over a quiet room, and speaks once. Checked: it only records, and after
// Stop asks what to do with it; Me and Them as speakers and the others told apart, the language
// detected, the echo not written twice, the clock across the silence, the M4A, the save folder, the
// timing pass and Find the speakers on a meeting, Just save it and transcribing later, a forced
// close and the recovery, the engine missing, Cancel, a silent microphone and another chosen, the
// disk, one source alone, a translation after it, OneDrive, and the buttons clicked as a person does.
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
const STEPS = { remove_silence: false, save_audio: false, transcribe: true, speakers: false, translate: null, video: false, burn: false, minutes: false };
/** The recording `id` once Stop has made it whole: waiting beside its journal for a choice. */
async function waiting(a, id, seconds = 15) {
  for (let i = 0; i < seconds * 4; i++) {
    const w = (await a.invoke("interrupted_recordings")).find((x) => x.id === id);
    if (w) return w;
    await sleep(250);
  }
  return null;
}
/** Record for `ms`, Stop, and the recording waiting for what to do with it. */
async function recordFor(a, ms, language = "en") {
  const id = await a.invoke("start_recording", { language });
  await sleep(ms);
  await a.invoke("stop_recording", { id });
  return { id, w: await waiting(a, id) };
}
/** Record, Stop, then transcribe it with `steps`: the job's id, and the job at its end. */
async function recordAndTranscribe(a, ms, steps = STEPS, language = "en") {
  const { id, w } = await recordFor(a, ms);
  const jid = await a.invoke("finish_recording", { journal: w.journal, language, steps });
  return { id, jid, j: await waitDone(a, jid) };
}
/** A press of the mouse as a person makes it, on the middle of `sel`: false when it is not there. */
const pressOn = (a) => async (sel, hold = 150) => {
  const r = await a.ev(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e || !e.offsetParent || e.disabled) return null; const b = e.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
  if (!r) return false;
  await a.call("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x, y: r.y });
  await a.call("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: 1 });
  await sleep(hold);
  await a.call("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: 1 });
  return true;
};
const chooserOpen = (a) => a.ev(`!!document.getElementById('rec-choose') && !document.getElementById('rec-choose').hidden`);
async function waitChooser(a, seconds = 10) {
  for (let i = 0; i < seconds * 4; i++) { if (await chooserOpen(a)) return true; await sleep(250); }
  return false;
}
/** Tick the chooser's boxes as given (the others as they are), its menus too. */
const setChoice = (a, ticks, menus = {}) => a.ev(`(() => { const b = document.getElementById('rec-choose');
  for (const [k, on] of Object.entries(${JSON.stringify(ticks)})) { const x = b.querySelector('[data-c="' + k + '"]'); if (x.checked !== on) { x.checked = on; x.dispatchEvent(new Event('change', { bubbles: true })); } }
  for (const [k, v] of Object.entries(${JSON.stringify(menus)})) { const m = b.querySelector('[data-c="' + k + '"]'); m.value = v; m.dispatchEvent(new Event('change', { bubbles: true })); }
  return true; })()`);

// ---- 1. a meeting, clicked as a person does: only recorded, then the question; the language
// heard, Me and the others told apart, the clock, the file
if (ONLY === "all" || ONLY === "meeting") {
  const saveDir = join(OUT, "record-saved");
  rmSync(saveDir, { recursive: true, force: true });
  mkdirSync(saveDir, { recursive: true });
  const h = home("meeting", {}, { save_dir: saveDir });
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const press = pressOn(a);
  const st = await a.invoke("get_state");
  check("Dawwin's own ffmpeg is the one in use", (st.setup.ffmpeg || "").includes(join(h, "tools", "ffmpeg-")), st.setup.ffmpeg);
  const setup = await a.invoke("record_setup");
  check("recordings go to the folder chosen in Settings", setup.folder === saveDir && setup.synced === false, setup.folder);
  await a.ev(`showView('home')`);
  await sleep(600);
  check("Record, clicked, starts recording", await press('#record-bar [data-rec="start"]'));
  const started = Date.now();
  let rec = null;
  for (let i = 0; i < 40 && !rec; i++) { await sleep(250); rec = (await a.invoke("get_state")).queue.find((j) => j.kind === "record" && j.status === "recording"); }
  const id = rec?.id;
  check("a recording runs, on the start page", !!rec && (await a.ev(`S.view`)) === "home", rec?.status);
  await sleep(4000);
  const v = await a.invoke("recording_state");
  check("a level for Me and one for Them while it records", v?.status?.me && v?.status?.them, v?.status);
  await sleep(30000);
  const during = await tryInvoke(a, "get_transcript", { id });
  check("nothing is transcribed while it records", !during.segments?.length && (await a.invoke("get_state")).engine.state !== "loaded", { lines: during.segments?.length, engine: (await a.invoke("get_state")).engine.state });
  await sleep(Math.max(0, LEN * 1000 + 1500 - (Date.now() - started)));
  check("Stop, clicked while the levels move", await press('#record-bar [data-rec="stop"]'));
  check("after Stop, Dawwin asks what to do with it", await waitChooser(a));
  const w = await waiting(a, id);
  check("the recording waits whole beside its journal, its job off the queue", w?.stopped === true && Math.abs(w.seconds - LEN) < 3 && !(await job(a, id)), w);
  const bar = await a.ev(`(() => { const b = document.getElementById('record-bar'); return { live: !!b.querySelector('.rec-live'), start: !!b.querySelector('[data-rec="start"]') }; })()`);
  check("the start page has Record again, not the panel of the stopped recording", !bar.live && bar.start, bar);
  const shown = await a.ev(`document.querySelector('#rec-choose .rc-what').textContent`);
  check("the question names the recording and its length", shown.includes("Recording") && shown.includes("1:0"), shown);
  await a.shot(join(OUT, "record-choose.png"));
  await setChoice(a, { transcribe: true, speakers: true, minutes: false, translate: false, silences: false }, { lang: "auto" });
  const t0 = Date.now();
  check("Start, clicked", await press('#rec-choose [data-c="go"]'));
  await sleep(800);
  check("the question goes, and the text being written is shown", !(await chooserOpen(a)) && (await a.ev(`S.view`)) === "transcript" && (await a.ev(`S.current?.id`)) === id);
  const j = await waitDone(a, id);
  check("it is transcribed from the file (the others told apart too)", j.status === "done", { status: j.status, s: (Date.now() - t0) / 1000, err: j.error });
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
  check("the WAV and the journal are gone", readdirSync(saveDir).every((f) => !f.endsWith(".wav") && !f.endsWith(".json")), readdirSync(saveDir));
  check("the M4A is listed as the transcript's own file", tx.outputs_listed && tx.outputs.includes(kept));
  check("it is known as a recording of both sides", tx.sources === "both" && !tx.partial, tx.sources);
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

// ---- 2. Find the speakers on a meeting transcribed without them: Me stays, the others are told apart
if (ONLY === "all" || ONLY === "find") {
  const h = home("find");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const { id } = await recordAndTranscribe(a, LEN * 1000 + 1500);
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

// ---- 3. Just save it: no text, the sound kept as the recording, the tracks beside it; asked again
// after a restart if the question was put off; transcribed later from the history, with a step
if (ONLY === "all" || ONLY === "save") {
  const h = home("save");
  let a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  // past where Me speaks (25 s to 30 s)
  const { id, w } = await recordFor(a, 33000);
  check("stopped, it waits for a choice", w?.stopped === true, w);
  a.close();
  a = await open(h, port++, {});
  const again = await a.invoke("interrupted_recordings");
  check("put off and Dawwin closed: the next start still asks, as recorded (not cut off)", again.length === 1 && again[0].id === id && again[0].stopped === true, again);
  await sleep(1200);
  const card = await a.ev(`(() => { showView('home'); const c = document.querySelector('#record-bar .rec-cutoff'); return c ? { text: c.textContent.replace(/\\s+/g, ' ').trim(), waiting: c.classList.contains('rec-waiting') } : null; })()`);
  check("the start page shows it waiting, with What to do with it", card?.waiting && /Recorded/.test(card.text) && card.text.includes("What to do with it"), card);
  const sid = await a.invoke("finish_recording", { journal: again[0].journal, language: null, steps: null });
  const sj = await waitDone(a, sid, 120);
  let tx = await a.invoke("get_transcript", { id });
  const folder = join(h, "recordings");
  check("Just save it keeps it without text, as a recording", sid === id && sj.status === "done" && tx.segments.length === 0 && tx.sources === "both" && !tx.partial, { status: sj.status, n: tx.segments.length, sources: tx.sources, err: sj.error });
  // the made-up source gives no sound to listen to: the M4A is the two tracks themselves
  check("the sound kept is the recording, an M4A holding the two tracks", tx.source_path.endsWith(".m4a") && existsSync(tx.source_path) && !tx.tracks, { source: tx.source_path, tracks: tx.tracks });
  check("the WAVs and the journal are gone, nothing is asked any more", readdirSync(folder).every((f) => !f.endsWith(".wav") && !f.endsWith(".json")) && (await a.invoke("interrupted_recordings")).length === 0, readdirSync(folder));
  check("it is in the history", (await a.invoke("list_history")).some((x) => x.id === id));
  const sourceBefore = tx.source_path;
  await a.ev(`openTranscript(${JSON.stringify(id)})`);
  await sleep(1000);
  const page = await a.ev(`(() => { const b = document.querySelector('#segments .rec-saved'); return b ? b.textContent : null; })()`);
  check("its page says it has no text yet, with Transcribe it", page?.includes("no text yet") && page.includes("Transcribe it"), page);
  await a.ev(`document.querySelector('#segments .rec-saved button').click()`);
  check("Transcribe it asks what to do, without Just save it", (await waitChooser(a, 3)) && (await a.ev(`document.querySelector('#rec-choose [data-c="save"]').hidden && document.querySelector('#rec-choose [data-c="delete"]').hidden`)));
  await a.ev(`closeChooser()`);
  await sleep(400);
  const tid = await a.invoke("transcribe_saved", { id, language: "en", steps: { ...STEPS, remove_silence: true } });
  const tj = await waitDone(a, tid, 240);
  tx = await a.invoke("get_transcript", { id });
  console.log(tx.segments.map((s) => `   ${s.track} ${s.speaker} ${s.start.toFixed(1)}-${s.end.toFixed(1)} ${s.text.slice(0, 60)}`).join("\n"));
  const meLine = tx.segments.find((s) => s.track === "me");
  check("transcribed later from the tracks it kept (through ffmpeg): Me and Them, each in its place", tj.status === "done" && tx.segments.some((s) => s.track === "them" && s.start < 3) && meLine && Math.abs(meLine.start - OWN) < 1.5 && !tx.partial, { status: tj.status, n: tx.segments.length, me: meLine?.start, err: tj.error });
  const quiet = tx.outputs.find((o) => o.includes("without silences"));
  check("its sound is saved without its silences, beside it, shorter", !!quiet && existsSync(quiet) && duration(quiet) < duration(tx.source_path) - 5, { quiet, d: quiet && duration(quiet), whole: duration(tx.source_path) });
  check("its files stay where they were", tx.source_path === sourceBefore && existsSync(tx.source_path));
  a.close();
}

// ---- 4. closed by force: the recording up to the last second, offered at the next start
if (ONLY === "all" || ONLY === "crash") {
  const h = home("crash");
  let a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  const id = await a.invoke("start_recording", { language: "en" });
  await sleep(30000);
  execFileSync("taskkill", ["/F", "/T", "/PID", String(a.proc.pid)], { stdio: "ignore" });
  await sleep(1500);
  const folder = join(h, "recordings");
  const wavs = readdirSync(folder).filter((f) => f.endsWith(".wav") && !f.endsWith(".listen.wav"));
  check("a WAV and its journal are there after the crash", wavs.length === 1 && readdirSync(folder).some((f) => f.endsWith(".dawwin-recording.json")), readdirSync(folder));
  check("the WAV is readable up to the last second or so", duration(join(folder, wavs[0])) > 27, duration(join(folder, wavs[0])));
  a = await open(h, port++, {});
  const found = await a.invoke("interrupted_recordings");
  check("the next start offers it back, as cut off", found.length === 1 && found[0].seconds > 27 && found[0].stopped === false, found);
  const rid = await a.invoke("finish_recording", { journal: found[0].journal, language: "en", steps: STEPS });
  const j = await waitDone(a, rid, 300);
  const tx = await a.invoke("get_transcript", { id: rid });
  check("transcribed, Me and Them", rid === id && j.status === "done" && tx.segments.some((s) => s.track === "them") && tx.segments.some((s) => s.track === "me"), { status: j.status, n: tx.segments.length });
  check("nothing is offered any more", (await a.invoke("interrupted_recordings")).length === 0);
  a.close();
  a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
  await a.invoke("start_recording", { language: "en" });
  await sleep(6000);
  execFileSync("taskkill", ["/F", "/T", "/PID", String(a.proc.pid)], { stdio: "ignore" });
  await sleep(1500);
  a = await open(h, port++, {});
  const again = await a.invoke("interrupted_recordings");
  await a.invoke("recover_recording", { journal: again[0].journal, keep: false });
  check("Delete it removes the files and the journal", readdirSync(folder).every((f) => !f.endsWith(".wav") && !f.endsWith(".json")), readdirSync(folder));
  a.close();
}

// ---- 5. the speech engine cannot start: recording needs none; the text fails, and is asked again
if (ONLY === "all" || ONLY === "engine") {
  const engineDir = join(dirname(APP), "engine");
  const away = engineDir + "-away";
  renameSync(engineDir, away);
  try {
    const h = home("engine");
    let a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
    const id = await a.invoke("start_recording", { language: "en" });
    await sleep(12000);
    const v = await a.invoke("recording_state");
    check("without the engine the recording goes on", v?.status && !v.status.ended && v.status.seconds > 10, v?.status?.seconds);
    await a.invoke("stop_recording", { id });
    const w = await waiting(a, id);
    const jid = await a.invoke("finish_recording", { journal: w.journal, language: "en", steps: STEPS });
    const j = await waitDone(a, jid, 60);
    check("transcribing it says the engine is missing", j.status === "failed" && /engine/i.test(j.error || ""), { status: j.status, error: j.error });
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

// ---- 6. Cancel while it is transcribed: the text stops, the recording stays to be asked about again
if (ONLY === "all" || ONLY === "cancel") {
  const h = home("cancel");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_ENGINE_DELAY_MS: "6000" });
  const { id, w } = await recordFor(a, 40000);
  await a.invoke("finish_recording", { journal: w.journal, language: "en", steps: STEPS });
  let j;
  for (let i = 0; i < 120; i++) { await sleep(500); j = await job(a, id); if (j?.status === "transcribing" && j.progress > 0) break; }
  check("it is transcribed from the file", j?.status === "transcribing", j?.status);
  await a.invoke("cancel_job", { id });
  j = await waitDone(a, id, 60);
  check("Cancel stops the text", j.status === "cancelled", j.status);
  const folder = join(h, "recordings");
  const found = (await a.invoke("interrupted_recordings"))[0];
  check("the WAV and its journal are kept, and asked about again", readdirSync(folder).some((f) => f.endsWith(".wav")) && found?.id === id && found.stopped, readdirSync(folder));
  const rid = await a.invoke("recover_recording", { journal: found.journal, keep: true });
  await a.invoke("cancel_job", { id: rid });
  await sleep(8000);
  const rj = await job(a, rid);
  check("one cancelled at once does not go on, and is asked about again", (!rj || rj.status === "cancelled") && (await a.invoke("interrupted_recordings")).some((x) => x.journal === found.journal), rj?.status);
  a.close();
}

// ---- 7. a microphone that sends nothing, another chosen in its menu; a disk running low
if (ONLY === "all" || ONLY === "mic") {
  const h = home("mic");
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST + "|silent", DAWWIN_TEST_DISK_FREE: "1500000000" });
  const setup = await a.invoke("record_setup");
  check("the microphones are listed", setup.microphones?.length === 2, setup.microphones);
  const id = await a.invoke("start_recording", { language: "en" });
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
  await a.invoke("stop_recording", { id });
  check("stopped, it waits for a choice", (await waiting(a, id))?.stopped === true);
  a.close();
  const low = home("disk");
  const b = await open(low, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_DISK_FREE: "400000000" });
  await b.ev(`window.__TAURI__.event.listen("recording", (e) => { window.__last = e.payload.status; }), true`);
  await b.ev(`showView("home")`);
  const id2 = await b.invoke("start_recording", { language: "en" });
  const w2 = await waiting(b, id2, 30);
  const last = await b.ev(`window.__last`);
  await sleep(500);
  const bar = await b.ev(`document.getElementById("record-bar").textContent`);
  check("under 500 MB the recording stops cleanly by itself, and says why", last?.disk_stopped === true && last.ended && w2?.stopped && bar.includes("disk"), { last, w2 });
  check("and Dawwin asks what to do with it", await waitChooser(b, 5));
  b.close();
}

// ---- 8. the microphone alone, and the PC's sound alone
if (ONLY === "all" || ONLY === "one") {
  for (const sources of ["mic", "pc"]) {
    const h = home("one-" + sources, { sources });
    const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST });
    const { id, j } = await recordAndTranscribe(a, 40000);
    const tx = await a.invoke("get_transcript", { id });
    const want = sources === "mic" ? "me" : "them";
    check(`${sources} only: every line is ${want}, one speaker`, j.status === "done" && tx.segments.length > 0 && tx.segments.every((s) => s.track === want && s.speaker === 1) && tx.speakers.length === 1 && tx.sources === sources, tx.speakers);
    if (sources === "mic") {
      await a.ev(`showView('home')`);
      const { id: m2, w } = await recordFor(a, 3000);
      await a.ev(`closeChooser(); openChooser(${JSON.stringify(w || {})})`);
      await sleep(400);
      check("the microphone alone: no Tell speakers apart to choose", await a.ev(`document.querySelector('#rec-choose [data-for="speakers"]').hidden`));
      await a.ev(`closeChooser()`);
      await a.invoke("recover_recording", { journal: w.journal, keep: false });
      check("Delete it from the question's side removes it", !(await a.invoke("interrupted_recordings")).some((x) => x.id === m2));
    }
    a.close();
  }
}

// ---- 9. the steps chosen after Stop follow: a translation, then the minutes (the stand-in translator)
if (ONLY === "all" || ONLY === "translate") {
  const h = home("translate");
  const model = join(h, "models", "MiLMMT-46-1B-v1.0.Q4_K_M.gguf");
  execFileSync("fsutil", ["file", "createnew", model, "806057408"], { stdio: "ignore" });
  const { openSync, writeSync, closeSync } = await import("node:fs");
  const fd = openSync(model, "r+"); writeSync(fd, Buffer.from([0x47, 0x47, 0x55, 0x46, 3, 0, 0, 0]), 0, 8, 0); closeSync(fd);
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_TRANSLATOR_DELAY_MS: "60" });
  const { id } = await recordAndTranscribe(a, 30000, { ...STEPS, translate: "ar" });
  let tx;
  for (let i = 0; i < 120; i++) { await sleep(500); tx = await a.invoke("get_transcript", { id }); if (tx.segments.length && tx.translations?.[0]?.lines?.filter((l) => l.text).length === tx.segments.length) break; }
  check("a translation chosen after Stop follows the transcript", tx.translations?.[0]?.language === "ar" && tx.translations[0].lines.filter((l) => l.text).length === tx.segments.length, tx.translations?.[0]?.lines?.length);
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
// record.png): a meeting just recorded, and the question of what to do with it. Writes into the
// repository; not part of "all":  node tools/e2e/record.mjs pictures
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
    const id = await a.invoke("start_recording", { language: "en" });
    await sleep(6500);
    await a.invoke("stop_recording", { id });
    await waitChooser(a);
    await setChoice(a, { transcribe: true, speakers: true, minutes: true, translate: false, silences: false }, { lang: "en" });
    await a.ev(`document.getElementById('toast').hidden = true; document.activeElement?.blur()`);
    await sleep(700);
    const r = await a.call("Page.captureScreenshot", { format: "webp", quality: 85, clip: { ...(await box("#main", 820)), scale: 1 } });
    writeFileSync(join(REPO, "ui", "guide", lang, "record.webp"), Buffer.from(r.result.data, "base64"));
    if (lang === "en") {
      await a.call("Emulation.setDeviceMetricsOverride", { width: 1500, height: 1000, deviceScaleFactor: 1, mobile: false });
      await sleep(800);
      await a.shot(join(REPO, "docs", "screenshots", "record.png"));
    }
    const w = await waiting(a, id);
    await a.ev(`closeChooser()`);
    await a.invoke("recover_recording", { journal: w.journal, keep: false });
    await sleep(500);
  }
  await a.ev(`setUiLang('en')`);
  check("the pictures are made", true);
  a.close();
}

// ---- the buttons, clicked as a person does: pressed, held a moment while the levels move, let go.
// Stop was drawn again ten times a second, so a click on it was lost (1.8.0); a stopped recording's
// panel came back on the start page (1.8.2); and a click right after Stop, meant for it, put the
// question off.
if (ONLY === "all" || ONLY === "clicks") {
  const h = home("clicks", { sources: "pc" });
  // the engine slowed, so that the text is still being written a while after Start
  const a = await open(h, port++, { DAWWIN_TEST_RECORD: TEST, DAWWIN_TEST_ENGINE_DELAY_MS: "6000" });
  const press = pressOn(a);
  const recording = async () => { let rec = null; for (let i = 0; i < 40 && !rec; i++) { await sleep(250); rec = (await a.invoke("get_state")).queue.find((j) => j.kind === "record" && j.status === "recording"); } return rec; };
  await a.ev(`showView('home')`);
  await sleep(500);
  check("Record, clicked, starts recording", await press('#record-bar [data-rec="start"]'));
  const rec = await recording();
  check("a recording runs", !!rec, rec?.status);
  await sleep(6000);
  await a.ev(`window.__clicks = []; document.addEventListener('click', (e) => window.__clicks.push((e.target.closest('[data-rec],[data-c]')?.dataset.rec || e.target.closest('[data-c]')?.dataset.c || e.target.id || e.target.tagName) + (e.target.isConnected ? '' : ' (gone)')), true)`);
  await press('#record-bar [data-rec="stop"]');
  // a second click where Stop was, as the question opens: it was meant for Stop
  await sleep(250);
  await a.call("Input.dispatchMouseEvent", { type: "mousePressed", x: 300, y: 300, button: "left", clickCount: 1 });
  await a.call("Input.dispatchMouseEvent", { type: "mouseReleased", x: 300, y: 300, button: "left", clickCount: 1 });
  check("Stop, clicked while the levels move, stops it and asks what to do", (await waitChooser(a)) && !(await job(a, rec.id)), { clicks: await a.ev(`window.__clicks`) });
  await sleep(300);
  check("a click right after Stop does not put the question off", await chooserOpen(a));
  // put off: the start page shows it waiting, never the panel of the recording that stopped
  check("Decide later, clicked", await press('#rec-choose [data-c="later"]'));
  await sleep(600);
  await press("#btn-new");
  await sleep(800);
  const bar = await a.ev(`(() => { const b = document.getElementById('record-bar'); return { view: S.view, live: !!b.querySelector('.rec-live'), waiting: b.querySelectorAll('.rec-waiting').length }; })()`);
  check("New transcription shows Record and the recording waiting, not its panel", bar.view === "home" && !bar.live && bar.waiting === 1, bar);
  // a second recording, its own panel and time
  check("Record again, clicked", await press('#record-bar [data-rec="start"]'));
  const rec2 = await recording();
  await sleep(3000);
  const time = await a.ev(`document.querySelector('#record-bar .rec-time')?.textContent`);
  check("the second recording has its own panel and time", !!rec2 && rec2.id !== rec.id && /^00:0[2-4]$/.test(time || ""), { time, id: rec2?.id });
  await press('#record-bar [data-rec="stop"]');
  check("its Stop asks about it", await waitChooser(a));
  check("Just save it, clicked", await press('#rec-choose [data-c="save"]'));
  const sj = await waitDone(a, rec2.id, 60);
  const saved = await a.invoke("get_transcript", { id: rec2.id });
  check("it is saved without its text", sj.status === "done" && saved.segments.length === 0 && saved.sources === "pc", { status: sj.status, sources: saved.sources });
  // the first one, from its card: transcribed, and Stop writing asks first
  await a.ev(`showView('home')`);
  await sleep(500);
  check("What to do with it, on its card, clicked", await press('#record-bar .rec-waiting [data-rec="choose"]'));
  check("it asks again", await waitChooser(a, 3));
  await setChoice(a, { transcribe: true, speakers: false, minutes: false, translate: false, silences: false }, { lang: "en" });
  check("Start, clicked", await press('#rec-choose [data-c="go"]'));
  let j = null;
  for (let i = 0; i < 80; i++) { await sleep(250); j = await job(a, rec.id); if (j?.status === "transcribing") break; }
  await sleep(1500);
  if (j && !["done", "failed"].includes(j.status)) {
    await press("#t-cancel", 60);
    await sleep(200);
    const asked = await a.ev(`document.getElementById('t-cancel').textContent`);
    j = await job(a, rec.id);
    check("one click on Stop writing asks first", j && j.status !== "cancelled" && /\?|؟/.test(asked), { asked, status: j?.status });
  }
  const done = await waitDone(a, rec.id);
  const tx = await a.invoke("get_transcript", { id: rec.id });
  check("the recording ends with its text written", done.status === "done" && tx.segments.length > 0, { status: done.status, lines: tx.segments.length });
  // the one saved without text, from its page: Transcribe it
  await a.ev(`openTranscript(${JSON.stringify(rec2.id)})`);
  await sleep(800);
  check("Transcribe it, on the page of the one saved, clicked", await press('#segments .rec-saved button'));
  check("it asks, and Start transcribes it", (await waitChooser(a, 3)) && (await press('#rec-choose [data-c="go"]')));
  const d2 = await waitDone(a, rec2.id);
  const tx2 = await a.invoke("get_transcript", { id: rec2.id });
  check("the one saved is transcribed later, its files as they were", d2.status === "done" && tx2.segments.length > 0 && tx2.source_path === saved.source_path, { status: d2.status, lines: tx2.segments.length });
  a.close();
}

finish();
