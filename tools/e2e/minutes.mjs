// Minutes in the real app, with the stand-in for llama.cpp's server (cargo build --example
// fake_translator): the offer of models, writing, the line links, corrections, writing again, another
// language, a long meeting in parts stopped and continued, a GPU crash, typing never drawn over, the
// lines read-only while they are written again, exports, a lecture, the step after a transcript, a
// folder run, and the Arabic interface. Made-up meetings and the voices of Windows only.
import { execFileSync } from "node:child_process";
import { closeSync, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, FAKE_TRANSLATOR, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("the stand-in translator (cargo build --example fake_translator)", existsSync(FAKE_TRANSLATOR));

const home = makeHome("minutes");
const work = join(OUT, "minutes-files");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });

// a made-up Arabic meeting with its speakers named, as 1.4 gives them
const MEETING = [
  ["سارة", "طيب يا جماعة نبدأ، عنا تلات نقاط اليوم بخصوص الـ release."],
  ["عمر", "أول شي، الـ login bug لسا موجود عالـ Android."],
  ["سارة", "تمام، اتفقنا نأجل الـ launch للأربعا 15 نيسان."],
  ["رامي", "أنا رح صلّح الـ login bug قبل الخميس."],
  ["لينا", "وأنا رح جهّز الـ dashboard mockups."],
  ["عمر", "مين رح يحكي مع الـ marketing؟"],
  ["سارة", "منشوف بعدين. خلصنا، شكرًا."],
];
const PEOPLE = ["سارة", "عمر", "رامي", "لينا"];
writeFileSync(join(home, "history", "m-ar-1.json"), JSON.stringify({
  id: "m-ar-1", title: "اجتماع الإطلاق", source_path: "", created_ms: Date.now(), duration_s: 70, language: "ar", elapsed_s: 1, device: "CPU", model: "m", partial: false,
  speakers: PEOPLE.map((name, i) => ({ id: i + 1, name })),
  segments: MEETING.map(([who, text], i) => ({ start: i * 10, end: i * 10 + 9, text, edited: false, speaker: PEOPLE.indexOf(who) + 1 })),
}));
// a lecture: nothing decided, nobody takes anything on
const LECTURE = ["اليوم نتكلم عن تحلية مياه البحر.", "هناك طريقتان: التقطير الحراري والتناضح العكسي.", "التناضح العكسي يستعمل membrane تحت ضغط عالٍ.", "والطاقة هي أكبر كلفة في التحلية."];
writeFileSync(join(home, "history", "m-lecture.json"), JSON.stringify({
  id: "m-lecture", title: "محاضرة التحلية", source_path: "", created_ms: Date.now() - 1000, duration_s: 40, language: "ar", elapsed_s: 1, device: "CPU", model: "m", partial: false,
  segments: LECTURE.map((text, i) => ({ start: i * 10, end: i * 10 + 9, text, edited: false })),
}));
// a long English meeting: 40 minutes, a line every 10 s, so it goes in parts
const long = [];
for (let i = 0; i < 240; i++) {
  const text = i % 37 === 5 ? `We agreed to ship version ${i} on Friday.` : i % 41 === 7 ? `Priya will update the docs by Monday.` : i % 53 === 9 ? `Who owns the budget?` : `Line ${i} of a long meeting about the release.`;
  long.push({ start: i * 10, end: i * 10 + 9, text, edited: false });
}
writeFileSync(join(home, "history", "m-long.json"), JSON.stringify({
  id: "m-long", title: "Long planning meeting", source_path: "", created_ms: Date.now() - 2000, duration_s: 2400, language: "en", elapsed_s: 1, device: "CPU", model: "m", partial: false, segments: long,
}));
// a recorded meeting (1.5): "Me" on the microphone's track, "Them" on the PC's
const CALL = [["me", "Let's agree the launch is on Friday."], ["them", "Agreed. I will send the contract by Wednesday."], ["me", "Great, thanks."]];
writeFileSync(join(home, "history", "m-call.json"), JSON.stringify({
  id: "m-call", title: "Call with the client", source_path: "", created_ms: Date.now() - 3000, duration_s: 30, language: "en", elapsed_s: 1, device: "CPU", model: "m", partial: false,
  speakers: [{ id: 1, name: "Me" }, { id: 2, name: "Them" }],
  segments: CALL.map(([track, text], i) => ({ start: i * 10, end: i * 10 + 9, text, edited: false, track, speaker: track === "me" ? 1 : 2 })),
}));

const app = await open(home, PORT + 70, { DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_MINUTES_DELAY_MS: "70", FAKE_TRANSLATOR_CRASH_AFTER: "0" });
const { call, ev, invoke } = app;
const shot = (file) => app.shot(join(work, file));
await ev(`loadHistory()`);
const minutesJobs = () => ev(`window.__TAURI__.core.invoke('get_state').then(s=>s.queue.filter(j=>j.kind==='minutes').map(j=>({id:j.id,status:j.status,progress:j.progress,error:j.error,device:j.device,target:j.target,transcript:j.transcript})))`);
async function waitMinutes(timeout = 120) {
  for (let i = 0; i < timeout * 4; i++) {
    const js = await minutesJobs();
    if (!js.some((j) => ["queued", "loading", "writing"].includes(j.status))) return js;
    await sleep(250);
  }
  return minutesJobs();
}
const panel = () => ev(`(()=>{const p=document.getElementById('minutes');return {hidden:p.hidden, text:p.innerText, heads:[...p.querySelectorAll('h2')].map(h=>h.textContent), items:[...p.querySelectorAll('.minutes-item')].map(r=>r.innerText.replace(/\\s+/g,' '))};})()`);
const log = () => { try { return readFileSync(join(home, "logs", "translate.log"), "utf8"); } catch { return ""; } };

// ---- Settings: a card of minutes models; the tool says what it is for now
await ev(`showView('settings')`);
await sleep(400);
const rows = await ev(`[...document.querySelectorAll('#set-minutes .setting')].map(r=>r.querySelector('h3').textContent+' | '+r.querySelector('.status').textContent)`);
check("Settings has a Minutes card with the three models", rows.length === 3 && rows[0].startsWith("Jais-3 7B"), rows);
check("none of them is in the speech models' list", (await ev(`[...document.querySelectorAll('#set-models .setting')].map(r=>r.dataset.id).some(id=>id.startsWith('min-'))`)) === false);
check("the translator's row says it runs translation and minutes", (await ev(`document.querySelector('#set-translator-tool h3').textContent`)).includes("Translation and minutes"));
await ev(`document.getElementById('set-minutes').scrollIntoView({block:'center'})`);
await sleep(200);
await shot("min-settings.png");

// ---- no minutes model yet: the panel offers them, with their sizes and whether they fit
await ev(`openTranscript('m-ar-1')`);
await sleep(500);
await ev(`showMinutesTab('minutes')`);
await sleep(400);
let p = await panel();
const unknown = await ev(`S.snap.engine.device ? null : [...document.querySelectorAll('.minutes-model .fit')].map(f=>f.textContent).join('')`);
check("before the engine has looked for a GPU, no model is marked", unknown === null || unknown === "", unknown);
for (let i = 0; i < 150 && !(await ev(`S.snap.engine.device || ''`)); i++) await sleep(200);
await sleep(300);
const offer = await ev(`[...document.querySelectorAll('.minutes-model')].map(r=>r.innerText.replace(/\\s+/g,' '))`);
check("the panel offers the three models with their sizes", offer.length === 3 && offer.every((x) => /GB/.test(x)) && offer[0].includes("5.6 GB"), offer);
const vram = await ev(`S.snap.engine.vram_total_gb`);
check("it says which fit this computer: with 8 GB of graphics memory, the 7B too", offer.some((x) => x.includes("Fits this computer")) && (vram < 7.5 || offer[0].includes("Fits this computer")), { vram, offer });
check("the panel says the minutes are written on this computer", p.text.includes("written on this computer"));
await shot("min-offer.png");

// a stand-in for the 7B model: its name and size, and a GGUF header
const model = join(home, "models", "Jais-3-7B-Q4_K_M.gguf");
execFileSync("fsutil", ["file", "createnew", model, "5592226016"], { stdio: "ignore" });
const fd = openSync(model, "r+"); writeSync(fd, Buffer.from("GGUF\x03\0\0\0", "binary"), 0, 8, 0); closeSync(fd);
await invoke("refresh_setup");
await sleep(600);
p = await panel();
check("with a model, the panel offers to write the minutes", p.text.includes("Write the minutes"), p.text.slice(0, 200));
check("the language menu holds the transcript's language and English", (await ev(`[...document.querySelector('#minutes select').options].map(o=>o.value).join()`)) === "ar,en");
check("the language menu has a tooltip", !!(await ev(`document.querySelector('#minutes select').title`)));

// ---- writing: progress, then the minutes; the stand-in crashes on its first answer "on the GPU"
// the app starts its log anew with each server: what each one said is gathered while it runs
const starts = new Set();
let gathering = true;
(async () => { while (gathering) { for (const m of log().matchAll(/device: .*\ncontext: .*\ncache: .*/g)) starts.add(m[0]); await sleep(10); } })();
await ev(`document.querySelector('#minutes .minutes-bar .btn').click()`);
let sawProgress = false;
for (let i = 0; i < 60; i++) { await sleep(200); const t = await ev(`document.querySelector('#minutes .minutes-progress')?.innerText || ''`); if (t) { sawProgress = true; break; } }
check("the panel shows how far it is, with Stop", sawProgress);
let js = await waitMinutes();
await sleep(800);
check("a crash on the GPU: it carries on on the CPU and finishes", !js.some((j) => j.status === "failed") && log().includes("device: none") && !!(await invoke("get_transcript", { id: "m-ar-1" })).minutes?.written_ms, js);
gathering = false;
const started = [...starts];
check("the server was started with room for the minutes, its cache in 8 bits on the GPU, in 16 on the processor", started.some((x) => /^device: (?!none)\S+\ncontext: 16384\ncache: q8_0$/.test(x)) && started.includes("device: none\ncontext: 16384\ncache: f16"), started);
check("it asked for JSON under the schema, in Arabic, telling Jais-3 not to think", log().includes("schema: true") && log().includes("in Arabic") && log().includes('{"reasoning_effort":"low"}'), log().split("\n").filter((l) => l.startsWith("minutes")));
p = await panel();
check("summary, decisions, actions and open questions", ["Summary", "Decisions", "Action items", "Open questions"].every((h) => p.heads.includes(h)), p.heads);
check("the decision is there, with its time", p.items.some((x) => x.includes("00:20") && x.includes("اتفقنا")), p.items);
check("the actions use the speakers' names", p.items.some((x) => x.includes("رامي") && x.includes("الخميس")) && p.items.some((x) => x.includes("لينا")), p.items);
check("the open question is listed", p.items.some((x) => x.includes("مين رح يحكي")), p.items);
check("the privacy line names the model", p.text.includes("Written on this computer by Jais-3 7B"), p.text.slice(-120));
const saved = (await invoke("get_transcript", { id: "m-ar-1" })).minutes;
check("the minutes are saved in the transcript's file, with the model and the lines", saved.model === "Jais-3 7B · Q4" && saved.decisions[0].lines[0] === 2 && saved.language === "ar", saved.decisions);
await shot("min-written.png");

// ---- a decision's time shows the line it came from
await ev(`[...document.querySelectorAll('#minutes .minutes-item')].find(r=>r.innerText.includes('اتفقنا')).querySelector('.minutes-time').click()`);
await sleep(300);
check("pressing a time opens the lines, the line marked", (await ev(`!document.getElementById('segments').hidden && !!document.querySelector('.seg.from-minutes[data-i="2"]')`)) === true);
check("the time has a tooltip", (await ev(`(showMinutesTab('minutes'), document.querySelector('#minutes .minutes-time').title)`)) === "Show where this was said");

// ---- a correction by hand is kept
await ev(`(async()=>{const el=[...document.querySelectorAll('#minutes .minutes-action .who')].find(e=>e.textContent.includes('رامي'));el.focus();el.textContent='رامي وعمر';el.blur();await new Promise(r=>setTimeout(r,500));})()`);
const edited = (await invoke("get_transcript", { id: "m-ar-1" })).minutes.actions.find((a) => a.who.includes("رامي"));
check("a corrected owner is saved and marked as the user's", edited?.who === "رامي وعمر" && edited.edited, edited);
await ev(`(async()=>{const el=document.querySelector('#minutes .minutes-text.summary');el.focus();el.textContent='ملخّصي أنا.';el.blur();await new Promise(r=>setTimeout(r,500));})()`);
check("a corrected summary is saved", (await invoke("get_transcript", { id: "m-ar-1" })).minutes.summary === "ملخّصي أنا.");

// ---- what is being typed is never drawn over: not by state updates while a download runs, not by
// the save of the line before
await ev(`(()=>{const el=document.querySelectorAll('#minutes .minutes-action .what')[0];el.focus();el.textContent='نص قيد الكتابة';})()`);
await ev(`(()=>{window.__real=S.snap;for(let i=0;i<5;i++) onState({...S.snap, setup:{...S.snap.setup, download:{...S.snap.setup.download, active:true, done:i, total:10}}});})()`);
check("while a download runs, the line being typed in stays, with what was typed", (await ev(`document.activeElement.isConnected && document.activeElement.textContent`)) === "نص قيد الكتابة");
await ev(`onState(window.__real)`);
await ev(`(async()=>{const b=document.querySelectorAll('#minutes .minutes-action .what')[1];b.focus();b.textContent='والثاني';await new Promise(r=>setTimeout(r,700));})()`);
const typing = await ev(`({ on: document.activeElement.isConnected && document.activeElement.textContent, marked: document.querySelectorAll('#minutes .minutes-item.edited').length })`);
check("from one line into the next: the first is saved and marked, the next keeps what is typed", typing.on === "والثاني" && typing.marked >= 1 && (await invoke("get_transcript", { id: "m-ar-1" })).minutes.actions[0].what === "نص قيد الكتابة", typing);
await ev(`(async()=>{document.activeElement.blur();await new Promise(r=>setTimeout(r,600));})()`);
check("and the next is saved when left", (await invoke("get_transcript", { id: "m-ar-1" })).minutes.actions[1].what === "والثاني");

// ---- writing again warns that it replaces the corrections
await ev(`document.querySelector('#minutes .minutes-bar .btn').click()`);
await sleep(200);
check("Write again asks first, as the corrections would go", (await ev(`document.querySelector('#minutes .minutes-bar .btn').textContent`)) === "Replace my corrections?" && (await ev(`document.getElementById('toast').textContent`)).includes("replaces the minutes"));
check("Write again has a tooltip", !!(await ev(`document.querySelector('#minutes .minutes-bar .btn').title`)));
await ev(`document.querySelector('#minutes .minutes-bar .btn').click()`);
let readOnly = null;
for (let i = 0; i < 100 && !readOnly; i++) {
  await sleep(30);
  readOnly = await ev(`(async()=>document.querySelector('#minutes .minutes-progress') ? { fields: [...document.querySelectorAll('#minutes .minutes-text')].map(e=>e.contentEditable), edit: await window.__TAURI__.core.invoke('edit_minutes',{id:'m-ar-1',minutes:S.current.minutes}).then(()=>'saved',e=>String(e)) } : null)()`);
}
check("while it is written again, the lines cannot be edited, and an edit is refused", !!readOnly && readOnly.fields.length > 0 && readOnly.fields.every((f) => f === "false") && readOnly.edit.includes("being written again"), readOnly);
await waitMinutes();
await sleep(600);
const again = (await invoke("get_transcript", { id: "m-ar-1" })).minutes;
check("written again: the corrections are replaced", again.summary !== "ملخّصي أنا." && !again.actions.some((a) => a.edited), again.summary);

// ---- in another language: English minutes of the Arabic meeting
await ev(`(()=>{const s=document.querySelector('#minutes select');s.value='en';s.dispatchEvent(new Event('change'));})()`);
await ev(`document.querySelector('#minutes .minutes-bar .btn').click()`);
await sleep(300);
await waitMinutes();
await sleep(600);
const en = (await invoke("get_transcript", { id: "m-ar-1" })).minutes;
check("English minutes of an Arabic meeting, without translating the transcript", en.language === "en" && en.summary.startsWith("Stand-in minutes") && !(await invoke("get_transcript", { id: "m-ar-1" })).translations?.length, en.summary);
check("asked for in English", log().includes("in English"));

// ---- a corrected transcript: the minutes say they are older than it
await invoke("update_segment", { id: "m-ar-1", index: 3, text: MEETING[3][1] + " إن شاء الله" });
await ev(`reloadCurrent()`);
await sleep(600);
check("after a correction to the transcript, the minutes say they were written before it", (await ev(`!document.querySelector('#minutes .minutes-note').hidden`)) === true);

// ---- exports: the minutes alone, or with the transcript
const out = (f) => join(work, f);
for (const f of ["min-alone.docx", "min-alone.md", "min-alone.txt", "min-both.txt", "min-both.docx"]) {
  await invoke("export_minutes_to_path", { id: "m-ar-1", path: slash(out(f)), withTranscript: f.includes("both") });
}
const alone = readFileSync(out("min-alone.txt"), "utf8"), both = readFileSync(out("min-both.txt"), "utf8");
check("text: the minutes alone", alone.startsWith("Minutes · اجتماع الإطلاق") && alone.includes("Decisions") && !alone.includes("Transcript"), alone.slice(0, 160));
check("text: followed by the transcript", both.includes("Transcript") && both.includes(MEETING[0][1]));
check("Markdown with headings", readFileSync(out("min-alone.md"), "utf8").includes("## Action items"));
check("Word documents", readFileSync(out("min-alone.docx")).subarray(0, 2).toString() === "PK" && readFileSync(out("min-both.docx")).length > readFileSync(out("min-alone.docx")).length);
await ev(`(()=>{document.querySelector('#minutes .menu-wrap .btn').click();})()`);
await sleep(200);
check("the export menu offers the minutes alone or with the transcript, as Word, Markdown or text", (await ev(`[...document.querySelectorAll('#minutes-menu button')].map(b=>b.textContent).join('|')`)) === "Minutes|With the transcript|Word (.docx)|Markdown (.md)|Text (.txt)");
await shot("min-export.png");
await ev(`document.body.click()`);

// ---- a lecture: a summary and topics, no empty decisions or actions
await ev(`openTranscript('m-lecture')`);
await sleep(400);
await invoke("write_minutes", { id: "m-lecture", language: null });
await waitMinutes();
await ev(`reloadCurrent()`);
await sleep(600);
p = await panel();
check("a lecture: no decisions or actions headings", p.heads.includes("Summary") && !p.heads.includes("Decisions") && !p.heads.includes("Action items"), p.heads);

// ---- a long meeting: parts, stopped half way, continued
await ev(`openTranscript('m-long')`);
await sleep(400);
const before = log().split("\n").filter((l) => l.startsWith("minutes request")).length;
await ev(`document.querySelector('#minutes .minutes-bar .btn').click()`);
let draft = null;
for (let i = 0; i < 200; i++) {
  await sleep(250);
  draft = (await invoke("get_transcript", { id: "m-long" })).minutes?.draft;
  if (draft?.notes?.length >= 2) break;
}
await ev(`document.querySelector('#minutes .minutes-progress button').click()`);
js = await waitMinutes();
await sleep(600);
draft = (await invoke("get_transcript", { id: "m-long" })).minutes?.draft;
check("40 minutes go in parts of ten", draft?.parts?.length === 4, draft?.parts);
check("stopped: the notes done are kept", draft?.notes?.length >= 2 && draft.notes.length < 4 && js.find((j) => j.transcript === "m-long")?.status === "cancelled", { notes: draft?.notes?.length, js });
p = await panel();
check("the panel says where it stopped, with Continue", p.text.includes("Stopped after part") && p.text.includes("Continue"), p.text.slice(0, 200));
await shot("min-stopped.png");
const done = draft.notes.length;
await ev(`[...document.querySelectorAll('#minutes .linkbtn')].find(b=>b.textContent==='Continue').click()`);
await sleep(300);
await waitMinutes();
await sleep(600);
const longMin = (await invoke("get_transcript", { id: "m-long" })).minutes;
const asked = log().split("\n").filter((l) => l.startsWith("minutes request"));
check("continued: only the parts not done, then the notes together", asked.length === 4 - done + 1 && asked[asked.length - 1].includes("whole recording"), asked);
check("the minutes cover the whole meeting, not only its start", longMin.decisions.some((d) => d.lines[0] > 200) && longMin.decisions.some((d) => d.lines[0] < 20), longMin.decisions.map((d) => d.lines));
check("the draft is gone once they are written", !longMin.draft);


// ---- a recorded meeting: the model is told who Me and Them are, and the minutes name them
await ev(`openTranscript('m-call')`);
await sleep(400);
await invoke("write_minutes", { id: "m-call", language: null });
await waitMinutes();
const callMin = (await invoke("get_transcript", { id: "m-call" })).minutes;
check("a recorded meeting: the model is told who Me and Them are", /recorded: true/.test(log().split("\n").filter((l) => l.startsWith("minutes request")).pop() || ""), log().split("\n").filter((l) => l.startsWith("minutes request")).pop());
check("and the minutes name them as the transcript does", callMin?.actions?.some((a) => a.who === "Them" && a.what.includes("contract")), callMin?.actions);
// ---- the step after the transcript, and a folder run that writes the minutes next to each recording
const folder = join(work, "folder");
rmSync(folder, { recursive: true, force: true });
mkdirSync(folder, { recursive: true });
// a recording made of the voices of Windows (tools/timing/speak.ps1), as the other tests make theirs
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-ar.wav"))) execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
copyFileSync(join(voices, "meeting-ar.wav"), join(folder, "شورى.wav"));
await ev(`showView('home')`);
await sleep(300);
await ev(`(()=>{const b=document.getElementById('step-minutes');b.checked=true;b.dispatchEvent(new Event('change'));})()`);
await sleep(200);
check("Write the minutes is a step after the transcript, with a tooltip", (await ev(`document.getElementById('step-minutes').checked && !!document.getElementById('step-minutes').parentElement.title && !!document.getElementById('step-minutes').getAttribute('aria-description')`)) === true);
check("it comes after Tell speakers apart, in the text's step", (await ev(`document.getElementById('step-speakers').closest('label').nextElementSibling?.contains(document.getElementById('step-minutes'))`)) === true);
check("the drop message says it", (await ev(`dropText()`)).includes("write the minutes"), await ev(`dropText()`));
await shot("min-step.png");
const scan = await invoke("enqueue_folder", { path: slash(folder), language: "ar", steps: await ev(`stepsForJob()`) });
for (let i = 0; i < 600; i++) {
  await sleep(500);
  const st = await invoke("get_state");
  const busy = st.queue.some((j) => ["queued", "decoding", "loading", "transcribing", "writing"].includes(j.status));
  if (!busy && st.queue.some((j) => j.kind === "minutes" && j.status !== "queued")) break;
}
const files = readdirSync(folder);
const written = files.find((f) => f.startsWith("شورى.minutes."));
check("the folder run writes the minutes next to the recording, in the folder formats that suit them", !!written, files);
if (written) console.log("      ", readFileSync(join(folder, written), "utf8").slice(0, 300).replace(/\n/g, " / "));
// the same folder again, after the minutes file went: the recording is not transcribed again, its minutes are written again
if (written) rmSync(join(folder, written));
const again2 = await invoke("enqueue_folder", { path: slash(folder), language: "ar", steps: await ev(`stepsForJob()`) });
await sleep(500);
await waitMinutes();
await sleep(500);
check("a folder run again: a recording already done gets its missing minutes, without transcribing again", again2.added.length === 0 && readdirSync(folder).some((f) => f.startsWith("شورى.minutes.")), { again2, files: readdirSync(folder) });

// ---- the Arabic interface: right to left, its own words
await ev(`setUiLang('ar')`);
await ev(`openTranscript('m-ar-1')`);
await sleep(300);
await ev(`showMinutesTab('minutes')`);
await sleep(400);
p = await panel();
check("Arabic interface: the tabs and headings in Arabic", (await ev(`[...document.querySelectorAll('#t-tabs button')].map(b=>b.textContent).join()`)) === "النص,المحضر" && p.heads.includes("الملخّص"), p.heads);
check("the panel runs right to left", (await ev(`document.getElementById('minutes').dir`)) === "rtl");
check("tooltips in Arabic", (await ev(`document.querySelector('#minutes select').title`)) === "يمكن أن يُكتب المحضر بلغة غير لغة النص");
await shot("min-ar.png");
await ev(`setUiLang('en')`);

app.close();
finish();
