// Translation in the real app, with the stand-in for llama.cpp's server
// (src-tauri/examples/fake_translator.rs): the translation models in Settings, the menu, lines
// filling in as they come, a GPU crash half way, saving, an edited line translated again, a
// translation corrected by hand, exports and copying, stopping half way and carrying on, right to
// left, the Arabic interface, and taking turns with the speech engine on the GPU. Killing the app
// in the middle of a translation ends its translator too.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, truncateSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, FAKE_TRANSLATOR, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);
need("the stand-in translator (cargo build --example fake_translator)", existsSync(FAKE_TRANSLATOR));

// ---- a recording: an Arabic meeting of the voices of Windows, in a quiet room
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-ar.wav"))) {
  execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
}
const work = join(OUT, "translate-files");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const meeting = join(work, "اجتماع التخطيط.m4a");
ffmpeg(["-i", join(voices, "meeting-ar.wav"), "-f", "lavfi", "-i", "anoisesrc=c=pink:a=0.003:r=16000:seed=7", "-filter_complex", "[0:a][1:a]amix=inputs=2:duration=first:normalize=0", "-c:a", "aac", meeting]);

// ---- the data folder: a stand-in for the smaller translation model (its name and size, and a
// GGUF header), and two made-up transcripts
const home = makeHome("translate-home");
const model = join(home, "models", "MiLMMT-46-4B-v1.0.Q4_K_M.gguf");
writeFileSync(model, Buffer.from("GGUF\x03\0\0\0", "binary"));
execFileSync("fsutil", ["sparse", "setflag", model], { stdio: "ignore" });
truncateSync(model, 2489893152);
const seg = (start, text) => ({ start, end: start + 10, text, edited: false });
const AR_LINES = [
  "طيب، خلينا نبدأ الاجتماع. عندنا ثلاث نقاط اليوم.",
  "أول شي: الـ home screen لازم تتغير قبل الخميس.",
  "ثاني شي، زر الـ login ما يشتغل على الجوال!",
  "ثالث شي: التقارير الشهرية تطلع ناقصة، والأرقام ما تطابق الـ invoices.",
  "مين بياخذ مهمة الشاشة الرئيسية؟ سامي؟",
  "تمام، نتكلم الأسبوع الجاي.",
];
writeFileSync(join(home, "history", "tr-ar-1.json"), JSON.stringify({
  id: "tr-ar-1", title: "اجتماع المتطلبات", source_path: meeting, created_ms: Date.now(), duration_s: 60, language: "ar",
  elapsed_s: 5, device: "CPU", model: "Cohere Transcribe Arabic (07-2026) · Q8", partial: false, timed: true,
  segments: AR_LINES.map((t, i) => seg(i * 10, t)),
}));
writeFileSync(join(home, "history", "tr-en-1.json"), JSON.stringify({
  id: "tr-en-1", title: "Planning call", source_path: meeting, created_ms: Date.now() - 1000, duration_s: 30, language: "en",
  elapsed_s: 2, device: "CPU", model: "m", partial: false, timed: true,
  segments: [seg(0, "We need the login page ready by Thursday."), seg(10, "The reports are missing the totals."), seg(20, "Let's meet again next week.")],
}));

const a = await open(home, PORT + 21, { DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_TRANSLATOR_DELAY_MS: "700", FAKE_TRANSLATOR_CRASH_AFTER: "3" });
const jobs = () => a.ev(`window.__TAURI__.core.invoke('get_state').then(s=>s.queue.filter(j=>j.kind==='translate').map(j=>({id:j.id,status:j.status,target:j.target,progress:j.progress,error:j.error,device:j.device})))`);
async function waitJobs(timeout = 120) {
  for (let i = 0; i < timeout * 4; i++) {
    const js = await jobs();
    if (!js.some((j) => ["queued", "loading", "translating"].includes(j.status))) return js;
    await sleep(250);
  }
  return jobs();
}
const choose = (lang) => a.ev(`(async()=>{const m=document.getElementById('t-lang');m.value=${JSON.stringify(lang)};m.dispatchEvent(new Event('change'));await new Promise(r=>setTimeout(r,400));})()`);
const transLines = () => a.ev(`[...document.querySelectorAll('#segments .seg .trans')].map(e=>({text:e.textContent,pending:e.classList.contains('pending'),outdated:e.classList.contains('outdated'),dir:e.dir}))`);
await a.ev(`loadHistory()`);

// ---- Settings: the three translation models
await a.ev(`showView('settings')`);
await sleep(300);
const rows = await a.ev(`[...document.querySelectorAll('#set-translators .setting')].map(r=>r.querySelector('h3').textContent+' | '+r.querySelector('.status').textContent)`);
check("Settings lists the three translation models", rows.length === 3, rows);
check("the smaller one is found on the disk and in use, the others are not downloaded", /smaller.*In use/.test(rows[1]) && /best quality.*Not downloaded/.test(rows[0]) && /smallest.*Not downloaded/.test(rows[2]), rows);
check("the translator is there", (await a.state()).setup.translator_found === true);
await a.ev(`document.getElementById('set-translators').scrollIntoView({block:'center'})`);
await sleep(200);
await a.shot(join(OUT, "translate-settings.png"));

// ---- the menu
await a.ev(`openTranscript('tr-ar-1')`);
await sleep(500);
const opts = await a.ev(`[...document.getElementById('t-lang').options].filter(o=>!o.disabled).map(o=>o.value)`);
check("the menu offers no translation, then 45 languages, Arabic left out", opts[0] === "" && opts.length === 46 && !opts.includes("ar") && opts.includes("en") && opts.includes("da"), opts.length);
check("English comes first", opts[1] === "en", opts.slice(0, 3));

// ---- translating: lines fill in as they come; the stand-in crashes after 3 lines, "on the GPU"
await choose("en");
let mid = [];
for (let i = 0; i < 40; i++) { await sleep(200); mid = await transLines(); if (mid.some((l) => l.text)) break; }
check("lines appear under each line while it translates", mid.length === 6 && mid.some((l) => l.text) && mid.some((l) => l.pending), mid.map((l) => l.text ? "x" : "."));
const status = await a.ev(`document.getElementById('t-status').textContent`);
check("the progress line says what it does", status.startsWith("Translating into English"), status);
check("the status says it translates", (await a.ev(`document.querySelector('#chip-status .label').textContent`)).startsWith("Translating"));
await a.shot(join(OUT, "translate-progress.png"));
let js = await waitJobs();
const first = js.find((j) => j.target === "en");
const crashLog = readFileSync(join(home, "logs", "translate.log"), "utf8");
check("a crash half way: it carries on on the CPU and finishes", (first?.status === "done" || !first) && crashLog.includes("device: none"), { js, log: crashLog.slice(0, 200) });
await sleep(800);
const done = await transLines();
check("every line is translated", done.every((l, i) => l.text === "[English] " + AR_LINES[i]), done.map((l) => l.text.slice(0, 30)));
check("English runs left to right under Arabic", done.every((l) => l.dir === "ltr"));
const saved = await a.invoke("get_transcript", { id: "tr-ar-1" });
check("the translation is saved with the transcript", saved.translations?.[0]?.language === "en" && saved.translations[0].lines.filter((l) => l.text).length === 6, saved.translations?.[0]?.model);
check("each line remembers what it was translated from", saved.translations?.[0]?.lines.every((l, i) => l.from === AR_LINES[i]));
await a.ev(`document.getElementById('segments').scrollTop=0`);
await a.shot(join(OUT, "translate-done.png"));

// ---- an edit to a line: its translation is out of date, and only that line is translated again
const EDITED = AR_LINES[1] + " إن شاء الله";
await a.ev(`(async()=>{const el=document.querySelector('#segments .seg[data-i="1"] .text');el.focus();el.textContent=${JSON.stringify(EDITED)};el.blur();await new Promise(r=>setTimeout(r,500));})()`);
const after = await transLines();
check("the edited line's translation is marked as out of date", after[1]?.outdated && !after[0]?.outdated, after.map((l) => l.outdated));
check("its note says so", (await a.ev(`document.querySelector('#segments .seg[data-i="1"] .note')?.textContent || ''`)).includes("Changed after it was translated"));
check("Update translation appears", (await a.ev(`!document.getElementById('btn-retranslate').hidden`)) === true);
await a.ev(`document.getElementById('btn-retranslate').click()`);
await sleep(300);
js = await waitJobs();
await sleep(600);
const upd = await transLines();
check("only that line was translated again", upd[1]?.text === "[English] " + EDITED && !upd[1].outdated && upd[0].text === "[English] " + AR_LINES[0], upd[1]);
check("Update translation is gone", (await a.ev(`document.getElementById('btn-retranslate').hidden`)) === true);

// ---- correcting a translation by hand
const BY_HAND = "OK, let's start the meeting: three points today.";
await a.ev(`(async()=>{const el=document.querySelector('#segments .seg[data-i="0"] .trans');el.focus();el.textContent=${JSON.stringify(BY_HAND)};el.blur();await new Promise(r=>setTimeout(r,500));})()`);
const hand = (await a.invoke("get_transcript", { id: "tr-ar-1" })).translations[0].lines[0];
check("a translation corrected by hand is saved", hand.text === BY_HAND && hand.edited, hand);
await a.invoke("translate", { id: "tr-ar-1", language: "en" });
await waitJobs();
const kept = (await a.invoke("get_transcript", { id: "tr-ar-1" })).translations[0].lines[0];
check("translating again keeps what was corrected by hand", kept.text === BY_HAND, kept.text);

// ---- exports and copying
await a.ev(`document.getElementById('btn-export').click()`);
await sleep(200);
check("the export menu offers the original, the translation or both", (await a.ev(`!document.getElementById('export-scope').hidden`)) === true);
await a.shot(join(OUT, "translate-export.png"));
await a.ev(`document.body.click()`);
const out = (f) => join(work, f);
await a.invoke("export_to_path", { id: "tr-ar-1", path: slash(out("both.txt")), stamps: false, view: "both", language: "en" });
await a.invoke("export_to_path", { id: "tr-ar-1", path: slash(out("english.srt")), stamps: false, view: "translation", language: "en" });
await a.invoke("export_to_path", { id: "tr-ar-1", path: slash(out("both.docx")), stamps: true, view: "both", language: "en" });
const both = readFileSync(out("both.txt"), "utf8");
check("both languages: each translation under its line", both.startsWith(AR_LINES[0] + "\n" + BY_HAND), both.slice(0, 120));
const srt = readFileSync(out("english.srt"), "utf8");
check("subtitles of the translation, the one corrected by hand too", srt.startsWith("1\n00:00:00,000 --> ") && srt.includes("OK, let's start the meeting") && (srt.match(/\[English\]/g) || []).length === 5, srt.slice(0, 200));
check("a Word document of both", readFileSync(out("both.docx")).length > 1500);
const copied = await a.ev(`(async()=>{S.scope='translation';return await window.__TAURI__.core.invoke('transcript_text',{id:'tr-ar-1',stamps:false,...exportView()});})()`);
check("copying takes the translation when that is chosen", copied.startsWith(BY_HAND), copied.slice(0, 60));

// ---- stopping half way, and carrying on
await choose("fr");
await sleep(1900);
const fj = (await jobs()).find((j) => j.target === "fr" && ["loading", "translating"].includes(j.status));
await a.ev(`document.getElementById('t-cancel').click()`);
js = await waitJobs();
await sleep(600);
const partial = (await a.invoke("get_transcript", { id: "tr-ar-1" })).translations.find((t) => t.language === "fr");
const got = partial?.lines.filter((l) => l.text).length ?? 0;
check("stopped half way: what was done is kept", !!fj && got > 0 && got < 6, { got, job: js.find((j) => j.target === "fr")?.status });
check("Update translation offers the rest", (await a.ev(`!document.getElementById('btn-retranslate').hidden`)) === true);
await a.ev(`document.getElementById('btn-retranslate').click()`);
await sleep(300);
await waitJobs();
const fr = (await a.invoke("get_transcript", { id: "tr-ar-1" })).translations.find((t) => t.language === "fr");
check("carrying on translates the rest", fr?.lines.filter((l) => l.text).length === 6);
check("both translations are in the menu, above the rest", (await a.ev(`[...document.getElementById('t-lang').options].slice(2,4).map(o=>o.value).join()`)) === "en,fr");

// ---- into Arabic: right to left under English
await a.ev(`openTranscript('tr-en-1')`);
await sleep(400);
await choose("ar");
await waitJobs();
await sleep(700);
const arl = await transLines();
check("Arabic translations run right to left", arl.length === 3 && arl.every((l) => l.dir === "rtl" && l.text.startsWith("[Arabic]")), arl);

// ---- the Arabic interface
await a.ev(`openTranscript('tr-ar-1', 'en')`);
await a.ev(`setUiLang('ar')`);
await sleep(500);
check("in Arabic, the menu reads in Arabic", (await a.ev(`document.getElementById('t-lang').options[0].textContent`)) === "دون ترجمة");
await a.shot(join(OUT, "translate-arabic-ui.png"));
await a.ev(`setUiLang('en')`);

// ---- taking turns with the speech engine on the GPU
await a.ev(`showView('home')`);
const sid = await a.ev(`(async()=>{ document.getElementById('spoken').value='ar'; await addFiles([${JSON.stringify(slash(meeting))}]); await new Promise(r=>setTimeout(r,300)); const s = await window.__TAURI__.core.invoke('get_state'); return s.queue.find(j=>!j.kind || j.kind==='transcribe')?.id; })()`);
check("the recording is in the list", !!sid);
await a.idle(600);
const rec = await a.invoke("get_transcript", { id: sid });
check("a recording after translating: the speech engine runs again, on the GPU", rec.segments.length > 3 && rec.device.startsWith("GPU"), rec.device);
await a.invoke("translate", { id: sid, language: "en" });
js = await waitJobs(300);
const rec2 = await a.invoke("get_transcript", { id: sid });
check("and translating it after that: the translator runs again", js.every((j) => j.status !== "failed") && rec2.translations?.[0]?.lines.filter((l) => l.text).length === rec2.segments.filter((s) => s.text.trim()).length, js.filter((j) => j.status === "failed"));
check("a new transcription records where the speaker pauses", rec.segments.some((s) => (s.pauses || []).length > 0), rec.segments.map((s) => (s.pauses || []).length));
const recSrt = out("meeting.srt");
await a.invoke("export_to_path", { id: sid, path: slash(recSrt), stamps: false });
const srtCues = readFileSync(recSrt, "utf8").replace(/^﻿/, "").split(/\r?\n\r?\n/).filter((c) => c.trim());
const parse = (ts) => { const [h, m, rest] = ts.split(":"); return +h * 3600 + +m * 60 + +rest.replace(",", "."); };
const timed = srtCues.map((c) => { const l = c.split(/\r?\n/); const [x, y] = l[1].split(" --> ").map(parse); return { a: x, b: y, lines: l.slice(2) }; });
check("subtitles: two lines at most, 42 characters a line", timed.length > 5 && timed.every((c) => c.lines.length <= 2 && c.lines.every((x) => [...x].length <= 42)), timed.filter((c) => c.lines.length > 2 || c.lines.some((x) => [...x].length > 42)).slice(0, 2));
check("subtitles: 5/6 s to 7 s on screen", timed.every((c) => c.b - c.a >= 0.83 && c.b - c.a <= 7.0001), timed.filter((c) => c.b - c.a < 0.83 || c.b - c.a > 7.0001).slice(0, 2));
check("subtitles: two frames apart at least", timed.every((c, k) => k === 0 || c.a - timed[k - 1].b >= 0.08), timed.map((c, k) => k && +(c.a - timed[k - 1].b).toFixed(3)).filter((g) => g !== 0 && g < 0.08));
console.log("      the first subtitles:\n" + srtCues.slice(0, 4).map((c) => "        " + c.replace(/\r?\n/g, " | ")).join("\n"));
check("the translator's output is kept in a log", readFileSync(join(home, "logs", "translate.log"), "utf8").includes("device:"));

// ---- killing the app while it translates ends its translator
await a.ev(`openTranscript('tr-ar-1')`);
await a.invoke("translate", { id: "tr-ar-1", language: "de" });
for (let i = 0; i < 60 && !(await jobs()).some((j) => j.target === "de" && j.status === "translating"); i++) await sleep(250);
const children = () => execFileSync("pwsh", ["-NoProfile", "-c", `Get-CimInstance Win32_Process -Filter 'ParentProcessId=${a.proc.pid}' | ForEach-Object { "$($_.ProcessId) $($_.Name)" }`], { encoding: "utf8" })
  .split(/\r?\n/).filter((l) => /fake_translator/i.test(l)).map((l) => +l.split(" ")[0]);
const mine = children();
check("it translates with a translator of its own", mine.length === 1, mine);
a.close();
await sleep(1500);
const alive = (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } };
check("killing the app ends its translator too", mine.length > 0 && mine.every((pid) => !alive(pid)), mine.filter(alive));
finish();
