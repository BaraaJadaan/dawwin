// Subtitles that follow the speaker, in the real app: talks made with the voices of Windows
// (tools/timing/speak.ps1), so the moment every word is said is known. A video transcribed with
// "add the subtitles" gets its words placed in time as it goes; a transcript made without them is
// placed in time when its subtitles are exported, from "Fix the timing", and for Follow words.
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { check, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);

// ---- the talks, and what was said when
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-ar.truth.json"))) {
  execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
}
const truth = (name) => JSON.parse(readFileSync(join(voices, `${name}.truth.json`), "utf8").replace(/^﻿/, "")).words;
const work = join(OUT, "timing-recordings");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const room = (wav, out, extra = []) => ffmpeg(["-i", wav, "-f", "lavfi", "-i", "anoisesrc=c=pink:a=0.003:r=16000:seed=7", "-filter_complex", "[0:a][1:a]amix=inputs=2:duration=first:normalize=0", ...extra, out]);
const lecture = join(work, "lecture.mp4");
room(join(voices, "lecture-en.wav"), join(work, "lecture.wav"));
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=640x360:rate=24", "-i", join(work, "lecture.wav"), "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", lecture]);
const meetingAr = join(work, "اجتماع الفريق.m4a");
room(join(voices, "meeting-ar.wav"), meetingAr, ["-c:a", "aac"]);
const meetingEn = join(work, "weekly meeting.m4a");
room(join(voices, "meeting-en.wav"), meetingEn, ["-c:a", "aac"]);

// ---- words compared as the app compares them
const fold = (w) => w.toLowerCase().normalize("NFC")
  .replace(/[ً-ٰٟـ]/g, "").replace(/[أإآٱ]/g, "ا").replace(/[ىئ]/g, "ي").replace(/ة/g, "ه").replace(/ؤ/g, "و")
  .replace(/[^\p{L}\p{N}]/gu, "");
/** For each of `ours`, the index of the same word in `theirs`, in order (longest common run). */
function match(ours, theirs) {
  const n = ours.length, m = theirs.length;
  const L = Array.from({ length: n + 1 }, () => new Int32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) L[i][j] = ours[i] && ours[i] === theirs[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
  const out = new Array(n).fill(-1);
  for (let i = 0, j = 0; i < n && j < m;) {
    if (ours[i] && ours[i] === theirs[j]) { out[i++] = j++; } else if (L[i + 1][j] >= L[i][j + 1]) i++; else j++;
  }
  return out;
}
const srtTime = (s) => { const [h, m, rest] = s.split(":"); return +h * 3600 + +m * 60 + +rest.replace(",", "."); };
function readSrt(file) {
  return readFileSync(file, "utf8").replace(/^﻿/, "").split(/\r?\n\r?\n/).map((b) => b.trim().split(/\r?\n/)).filter((b) => b.length >= 3)
    .map((b) => { const [a, z] = b[1].split(" --> "); return { start: srtTime(a), end: srtTime(z), text: b.slice(2).join(" ") }; });
}
/** How the subtitles of `file` keep up with the words of `name`'s truth. */
function scoreSrt(file, name) {
  const cues = readSrt(file);
  const said = truth(name);
  const words = cues.flatMap((c, k) => c.text.split(/\s+/).filter(Boolean).map((w) => ({ w: fold(w), k })));
  const m = match(words.map((x) => x.w), said.map((x) => fold(x.text)));
  let goneEarly = 0, counted = 0;
  const lastOf = new Map();
  const early = [];
  words.forEach((x, i) => { if (m[i] >= 0) lastOf.set(x.k, said[m[i]].start); });
  for (const [k, t] of lastOf) { counted++; if (cues[k].end < t) { goneEarly++; early.push({ cue: cues[k], lastWordSaid: t }); } }
  return { cues, counted, goneEarly, early, said, words, m };
}
/** How many words of a transcript start within half a second of when they are said. */
function scoreWords(tx, name) {
  const said = truth(name);
  const ours = [];
  for (const s of tx.segments) for (const [a, t] of s.words || []) ours.push({ w: fold(s.text.slice(a).split(/\s/)[0]), t });
  const m = match(ours.map((x) => x.w), said.map((x) => fold(x.text)));
  let n = 0, within = 0;
  ours.forEach((x, i) => { if (m[i] >= 0) { n++; if (Math.abs(x.t - said[m[i]].start) <= 0.5) within++; } });
  return { n, within, share: n ? within / n : 0 };
}

const home = makeHome("timing-home");
const a = await open(home, PORT + 1);
const st = (await a.state()).setup;
check("Dawwin's own ffmpeg is the one in use", (st.ffmpeg || "").includes(slash(home).split("/").pop()), st.ffmpeg);
const steps = (o) => ({ remove_silence: false, save_audio: false, transcribe: true, translate: null, video: false, burn: false, ...o });
const transcriptOf = async (path) => {
  const h = (await a.invoke("list_history")).find((x) => slash(x.source_path) === slash(path));
  return h ? a.invoke("get_transcript", { id: h.id }) : null;
};

// ---- 1. a video with "add the subtitles": timed as it is transcribed
await a.invoke("enqueue", { paths: [slash(lecture)], language: "en", steps: steps({ video: true }) });
check("the video job ends", await a.idle());
const lec = await transcriptOf(lecture);
check("its words are placed in time", lec?.timed === true && lec.segments.some((s) => s.heard?.length), { timed: lec?.timed });
const wl = scoreWords(lec, "lecture-en");
check("nine words in ten start within half a second of when they are said", wl.share >= 0.9, wl);
const withSubs = readdirSync(work).find((f) => f.startsWith("lecture (") && f.endsWith(".mp4"));
check("the video with subtitles is saved next to it", !!withSubs, readdirSync(work));
const lecSrt = join(work, "lecture.srt");
await a.invoke("export_to_path", { id: lec.id, path: slash(lecSrt), stamps: false });
const ls = scoreSrt(lecSrt, "lecture-en");
check("no subtitle of the lecture is gone before its last word is said", ls.goneEarly === 0, { gone: ls.goneEarly, of: ls.counted, early: ls.early });
const subject = ls.cues.find((c) => /subject/.test(c.text));
const subjectSaid = ls.said.find((w) => w.text === "subject")?.start;
check("\"subject to\" is on the screen when it is said", subject && subject.start <= subjectSaid && subject.end >= subjectSaid + 0.3, { cue: subject, said: subjectSaid });
if (withSubs) {
  const inside = join(work, "inside.srt");
  ffmpeg(["-i", join(work, withSubs), "-map", "0:s:0", inside]);
  check("the subtitles in the video are the timed ones", readSrt(inside).length === ls.cues.length, { inVideo: readSrt(inside).length, exported: ls.cues.length });
}

// ---- 2. a transcript made without subtitles: placed in time when its subtitles are exported
await a.invoke("enqueue", { paths: [slash(meetingAr)], language: "ar", steps: steps() });
check("the Arabic meeting is transcribed", await a.idle());
let ar = await transcriptOf(meetingAr);
check("transcribing alone does not place the words", ar && !ar.timed && ar.segments.every((s) => !s.heard?.length), { timed: ar?.timed });
await a.ev(`openTranscript(${JSON.stringify(ar.id)})`);
await sleep(500);
check("\"Fix the timing\" is offered on it", (await a.ev(`!document.getElementById('btn-fix-timing').hidden`)) === true);
const arSrt = join(work, "meeting-ar.srt");
const t0 = Date.now();
await a.ev(`window.__export = window.__TAURI__.core.invoke('export_to_path', ${JSON.stringify({ id: ar.id, path: slash(arSrt), stamps: false })}).then(() => 'saved', (e) => String(e)); 1`);
let seen = "";
for (let i = 0; i < 40 && !seen; i++) {
  seen = (await a.state()).queue.find((j) => j.kind === "timing" && j.transcript === ar.id)?.status || "";
  await sleep(100);
}
const saved = await a.ev(`window.__export`);
check("the export waits for a timing job, then saves", seen !== "" && saved === "saved" && existsSync(arSrt), { seen, saved, seconds: (Date.now() - t0) / 1000 });
ar = await a.invoke("get_transcript", { id: ar.id });
check("and the transcript is placed in time now", ar.timed === true);
const as = scoreSrt(arSrt, "meeting-ar");
check("the Arabic subtitles keep up with the speaker (at most one gone early)", as.goneEarly <= 1, { gone: as.goneEarly, of: as.counted });
const wa = scoreWords(ar, "meeting-ar");
check("most Arabic words start within half a second", wa.share >= 0.85, wa);
await sleep(600);
check("\"Fix the timing\" is not offered any more", (await a.ev(`document.getElementById('btn-fix-timing').hidden`)) === true);

// ---- 3. an edited line keeps its times
const k = ar.segments.findIndex((s) => s.heard?.length >= 2);
const before = ar.segments[k];
const edited = before.text.replace(/^(\S+)/, "$1 يعني");
await a.invoke("update_segment", { id: ar.id, index: k, text: edited });
const after = (await a.invoke("get_transcript", { id: ar.id })).segments[k];
check("an edited line is placed in time again, without the audio", after.words?.length === edited.split(/\s+/).filter(Boolean).length, { words: after.words?.length });

// ---- 4. Follow words asks for the timing
await a.invoke("enqueue", { paths: [slash(meetingEn)], language: "en", steps: steps() });
check("the English meeting is transcribed", await a.idle());
const en = await transcriptOf(meetingEn);
await a.ev(`document.getElementById('follow').checked && document.getElementById('follow').click()`);
await a.ev(`openTranscript(${JSON.stringify(en.id)})`);
await sleep(400);
await a.ev(`document.getElementById('follow').click()`);
await sleep(800);
check("turning Follow words on starts placing the words", (await a.state()).queue.some((j) => j.kind === "timing" && j.transcript === en.id));
await a.idle();
await sleep(800);
const en2 = await a.invoke("get_transcript", { id: en.id });
check("Follow words then has the time of each word", en2.timed && (await a.ev(`lineWords(1).length > 0 && S.current.segments[1].words.length > 0`)), { timed: en2.timed });
const we = scoreWords(en2, "meeting-en");
check("and nine in ten are within half a second", we.share >= 0.9, we);

// ---- 5. a recording that moved: said why
const ghost = join(work, "moved.m4a");
await a.invoke("enqueue", { paths: [slash(meetingEn)], language: "en", steps: steps() });
await a.idle();
const again = (await a.invoke("list_history")).filter((x) => slash(x.source_path) === slash(meetingEn)).sort((x, y) => y.created_ms - x.created_ms)[0];
renameSync(meetingEn, ghost);
await a.ev(`openTranscript(${JSON.stringify(again.id)})`);
await sleep(400);
await a.ev(`document.getElementById('btn-fix-timing').click()`);
await sleep(800);
const toast = await a.ev(`document.getElementById('toast').hidden ? '' : document.getElementById('toast').innerText`);
check("\"Fix the timing\" on a recording that moved says why", /not at/.test(toast), toast);
await a.shot(join(OUT, "timing-transcript.png"));

a.close();
finish();
