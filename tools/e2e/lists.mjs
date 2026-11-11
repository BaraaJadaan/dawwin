// Long recordings and big folders, measured in the real app: a made-up transcript of 5,000 lines
// (how fast it opens and how fast search answers a key), 300 transcripts in History, a folder of a
// thousand recordings, a huge folder dropped by mistake and stopped, a folder run a second time;
// the lists keep the focus and take the keyboard; the interface zooms and keeps its size.
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { check, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);

const home = makeHome("lists-home");
const words = ["طيب", "خلونا", "نبدأ", "اليوم", "عندنا", "الـ", "dashboard", "تسجيل", "الدخول،", "والتقارير.", "مين", "ممكن", "ياخذ", "المهمة؟", "سامي", "the", "API", "من", "فريق", "الأرقام", "ما", "بتطابق", "invoices"];
const line = (k) => Array.from({ length: 9 + (k % 13) }, (_, j) => words[(k * 7 + j * 3) % words.length]).join(" ");
const transcript = (id, title, n, created) => ({
  id, title, source_path: "C:/made-up/" + id + ".m4a", created_ms: created, duration_s: n * 6, language: "ar", elapsed_s: 1,
  device: "CPU", model: "made up", partial: false,
  segments: Array.from({ length: n }, (_, k) => ({ start: k * 6, end: k * 6 + 5.5, text: line(k), edited: false })),
});
for (let k = 0; k < 300; k++) writeFileSync(join(home, "history", `old-${k}.json`), JSON.stringify(transcript(`old-${k}`, `اجتماع ${k}`, 40, Date.now() - 1e8 + k)));
writeFileSync(join(home, "history", "long.json"), JSON.stringify(transcript("long", "اجتماع طويل جدًا", 5000, Date.now())));

const a = await open(home, PORT + 2);

// ---- History of 300 transcripts
const t1 = await a.ev(`(async()=>{const t=performance.now(); await window.__TAURI__.core.invoke('list_history'); return performance.now()-t})()`);
const t2 = await a.ev(`(async()=>{const t=performance.now(); await window.__TAURI__.core.invoke('list_history'); return performance.now()-t})()`);
check("History lists 301 transcripts, and again without reading them", t2 < 100 && (await a.invoke("list_history")).length === 301, { first_ms: Math.round(t1), again_ms: Math.round(t2) });
await a.ev(`loadHistory()`);
check("all of them in the side panel", (await a.ev(`document.querySelectorAll('#history li').length`)) === 301);

// ---- a transcript of 5,000 lines
const opened = await a.ev(`(async()=>{const t=performance.now(); await openTranscript('long'); await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))); return {ms: performance.now()-t, drawn: S.rows.length}})()`);
check("a 5,000-line transcript is on screen within a second", opened.ms < 1000 && opened.drawn > 0, opened);
let all = 0;
for (let i = 0; i < 100 && all < 5000; i++) { await sleep(100); all = await a.ev(`S.rows.length`); }
check("and the rest of its lines are drawn while it shows", all === 5000, { drawn: all });
const jump = await a.ev(`(async()=>{const b=document.getElementById('segments'); let worst=0; for (const f of [0.25,0.5,0.75,1]) { const t=performance.now(); b.scrollTop=b.scrollHeight*f; await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))); worst=Math.max(worst, performance.now()-t); } b.scrollTop=0; return worst})()`);
check("a jump to its middle or end is drawn within a fifth of a second", jump < 200, { worst_ms: Math.round(jump) });
const smooth = await a.ev(`(async()=>{const b=document.getElementById('segments'); b.scrollTop=b.scrollHeight/2; await new Promise(r=>requestAnimationFrame(r)); let last=performance.now(), worst=0; for (let k=0;k<90;k++) { b.scrollTop+=300; await new Promise(r=>requestAnimationFrame(r)); const now=performance.now(); worst=Math.max(worst, now-last); last=now; } b.scrollTop=0; return worst})()`);
check("scrolling through it stays smooth (no frame over 50 ms)", smooth < 50, { worst_frame_ms: Math.round(smooth) });

// each key, as typed: the time from the key to the hits painted
const typing = await a.ev(`(async()=>{
  const box=document.getElementById('search'); const out=[];
  for (const q of ['ا','ال','الم','e','th','the','dashboard','invoices','ممكن ياخذ']) {
    box.value=q; const t=performance.now(); box.dispatchEvent(new Event('input'));
    await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
    out.push({q, ms: Math.round(performance.now()-t), hits: S.hits.length});
  }
  return out})()`);
const work = await a.ev(`(()=>{const box=document.getElementById('search'); const out=[]; for (const q of ['ا','ال','الم','th','the']) { box.value=q; const t=performance.now(); runSearch(false); out.push(Math.round(performance.now()-t)); } return out})()`);
check("search answers each key in under 50 ms on 5,000 lines", Math.max(...work) < 50, { search_ms: work, key_to_screen: typing });
const steps = await a.ev(`(()=>{const t=performance.now(); for (let k=0;k<20;k++) stepHit(1); return Math.round((performance.now()-t)/20)})()`);
check("going to the next hit is as quick", steps < 50, { ms_each: steps });
await a.ev(`document.getElementById('search').value=''; runSearch(false)`);

// ---- keyboard: a row of History opens with Enter; Escape closes the export menu
await a.ev(`showView('home')`);
await a.ev(`document.querySelector('#history li[data-key="old-5"]').focus()`);
await a.call("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
await a.call("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
await sleep(500);
check("a transcript in History opens from the keyboard", (await a.ev(`S.view === 'transcript' && S.current.id`)) === "old-5");
await a.ev(`document.getElementById('btn-export').click()`);
await a.call("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
await sleep(200);
check("Escape closes the export menu, and the focus goes back to its button", await a.ev(`document.getElementById('export-menu').hidden && document.activeElement.id === 'btn-export'`));
check("the line of a transcript shows where the focus is", await a.ev(`(()=>{const e=S.rows[0].querySelector('.text'); e.focus(); return getComputedStyle(e).boxShadow !== 'none'})()`));
await a.ev(`document.activeElement.blur()`);
check("\"a folder\" on the start page can be reached with Tab", await a.ev(`document.getElementById('home-folder').tabIndex === 0`));

// ---- a folder of a thousand recordings
const thousand = join(OUT, "thousand");
rmSync(thousand, { recursive: true, force: true });
mkdirSync(thousand, { recursive: true });
ffmpeg(["-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-t", "0.3", join(thousand, "0000.wav")]);
for (let k = 1; k < 1000; k++) copyFileSync(join(thousand, "0000.wav"), join(thousand, String(k).padStart(4, "0") + ".wav"));
await a.ev(`window.__lag = 0; window.__probe = setInterval(() => { const t = performance.now(); setTimeout(() => { window.__lag = Math.max(window.__lag, performance.now() - t); }, 0); }, 20); window.__events = 0; window.__TAURI__.event.listen('state', () => window.__events++); 1`);
const added = await a.ev(`(async()=>{const t=performance.now(); await addFolder(${JSON.stringify(slash(thousand))}); let n=0; while ((n=document.querySelectorAll('#queue li').length) < 1000 && performance.now()-t < 10000) await new Promise(r=>setTimeout(r,20)); return {ms: Math.round(performance.now()-t), rows: n}})()`);
const lag = await a.ev(`window.__lag`);
check("a folder of 1,000 recordings is in the list within two seconds", added.rows >= 1000 && added.ms < 2000, added);
check("and the window answers all the while", lag < 250, { worst_wait_ms: Math.round(lag) });
await a.ev(`window.__events = 0; 1`);
await sleep(2000);
const perSecond = (await a.ev(`window.__events`)) / 2;
check("the window hears of the queue at most about ten times a second", perSecond <= 11, { per_second: perSecond });
// the focus on a row stays while the jobs change
await a.ev(`document.querySelector('#queue li:nth-child(3)').focus(); window.__row = document.activeElement; 1`);
await sleep(1500);
check("a row of the list keeps the focus while jobs run", await a.ev(`document.activeElement === window.__row && window.__row.isConnected`));
await a.invoke("cancel_all_jobs");
await a.idle(120);
await a.ev(`clearInterval(window.__probe); 1`);

// ---- a whole drive dropped by mistake: told at once, shown while looked through, and stopped
const kindMs = await a.ev(`(async()=>{const t=performance.now(); const k=await window.__TAURI__.core.invoke('path_kind',{path:'C:/Windows'}); return [k, Math.round(performance.now()-t)]})()`);
check("a huge folder is told a folder at once", kindMs[0] === "folder" && kindMs[1] < 300, kindMs);
await a.ev(`window.__scan = addFolder('C:/Windows'); 1`);
let scan = null;
for (let i = 0; i < 40 && !scan; i++) { await sleep(100); scan = (await a.state()).scans[0]; }
check("looking through it shows in the list", !!scan && (await a.ev(`!!document.querySelector('#queue li[data-key^="scan-"]')`)), scan);
await sleep(600);
const later = (await a.state()).scans[0];
await a.ev(`document.querySelector('#queue li[data-key^="scan-"] .x').click()`);
await a.ev(`window.__scan`);
const st = await a.state();
check("with how many files it looked at, and Stop ends it with nothing added", !!later && later.looked > 0 && st.scans.length === 0 && st.queue.filter((j) => j.status === "queued").length === 0, { looked: later?.looked, found: later?.found });

// ---- a folder run a second time: only what is new, never a file Dawwin wrote
const worked = join(OUT, "worked");
rmSync(worked, { recursive: true, force: true });
mkdirSync(worked, { recursive: true });
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-en.wav"))) execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
const talk = join(worked, "talk.mp4");
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=320x240:rate=15", "-ss", "0", "-t", "20", "-i", join(voices, "meeting-en.wav"), "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", talk]);
await a.ev(`S.spoken='en'; 1`);
const runSteps = { remove_silence: false, save_audio: true, transcribe: true, translate: null, video: true, burn: false };
const first = await a.invoke("enqueue_folder", { path: slash(worked), language: "en", steps: runSteps });
await a.idle(300);
const wrote = readdirSync(worked).sort();
check("the first run transcribes the video and saves its sound and subtitles next to it", first.added.length === 1 && wrote.includes("talk.m4a") && wrote.some((f) => f.startsWith("talk (") && f.endsWith(".mp4")), wrote);
copyFileSync(talk, join(worked, "talk (2).mp4")); // the user's own copy
const second = await a.invoke("enqueue_folder", { path: slash(worked), language: "en", steps: runSteps });
const second_paths = (await a.state()).queue.filter((j) => second.added.includes(j.id)).map((j) => j.path.split(/[\\/]/).pop());
check("a second run adds only the new recording, none of Dawwin's own files", second.added.length === 1 && second_paths[0] === "talk (2).mp4" && second.already === 1, { added: second_paths, already: second.already });
await a.invoke("cancel_all_jobs");
await a.idle(120);
const t = (await a.invoke("list_history")).find((h) => h.source_path.endsWith("talk.mp4"));
const full = await a.invoke("get_transcript", { id: t.id });
check("the transcript remembers the files written for it", full.outputs?.length >= 3, full.outputs);
a.close();

// ---- the size of the interface: Ctrl and plus, kept for the next start
const b = await open(home, PORT + 3, { REAL_SIZE: "1" });
const dpr0 = await b.ev(`window.devicePixelRatio`);
for (const code of ["Equal", "Equal"]) {
  await b.call("Input.dispatchKeyEvent", { type: "keyDown", key: "=", code, modifiers: 2, windowsVirtualKeyCode: 187 });
  await b.call("Input.dispatchKeyEvent", { type: "keyUp", key: "=", code, modifiers: 2, windowsVirtualKeyCode: 187 });
  await sleep(400);
}
const dpr1 = await b.ev(`window.devicePixelRatio`);
const saved = JSON.parse(readFileSync(join(home, "settings.json"), "utf8")).zoom;
check("Ctrl and plus enlarge the whole interface, and the size is saved", Math.abs(dpr1 / dpr0 - 1.2) < 0.02 && saved === 1.2, { before: dpr0, after: dpr1, saved });
b.close();
await sleep(1500);
const c = await open(home, PORT + 3, { REAL_SIZE: "1", KEEP_WEBVIEW: "1" });
await sleep(500);
const dpr2 = await c.ev(`window.devicePixelRatio`);
check("the next start keeps the size", Math.abs(dpr2 - dpr1) < 0.02, { now: dpr2 });
await c.call("Input.dispatchKeyEvent", { type: "keyDown", key: "0", code: "Digit0", modifiers: 2, windowsVirtualKeyCode: 48 });
await c.call("Input.dispatchKeyEvent", { type: "keyUp", key: "0", code: "Digit0", modifiers: 2, windowsVirtualKeyCode: 48 });
await sleep(400);
check("and Ctrl and zero bring it back", Math.abs((await c.ev(`window.devicePixelRatio`)) - dpr0) < 0.02 && !("zoom" in JSON.parse(readFileSync(join(home, "settings.json"), "utf8"))));
c.close();
finish();
