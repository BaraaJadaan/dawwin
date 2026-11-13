// The pictures of "How to use" (ui/guide/<lang>/<part>.webp) and the README's (docs/screenshots),
// from the real app, in English and Arabic. Made-up data only. Writes into the repository: look at
// what it made before committing.
//   node tools/e2e/pictures.mjs [part …]
// Parts, all by default: drop steps sound live listen speakers translate minutes export settings read
// screen home readme setup. "live" transcribes talks spoken by the voices of Windows
// (tools/timing/speak.ps1) with DAWWIN_E2E_MODEL. "screen" also makes screen.png and player.png;
// "readme" the transcript, its translation, the export menu, a search, Play from here, the Arabic
// interface and the queue at work; "setup" the first-run page. "settings" (the subtitle look with its
// preview) needs DAWWIN_E2E_TOOLS; it, "export" and "readme" need a full ffmpeg for their videos.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { APP, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL); // so that the app shows itself ready, as a user's does
const want = process.argv.slice(2);
const wanted = (p) => !want.length || want.includes(p);
const home = makeHome("pictures-home");
// a few made-up transcripts in History
const made = [
  ["اجتماع متطلبات التطبيق", "طيب، خلونا نبدأ. اليوم عندنا ثلاث نقاط: الشاشة الرئيسية، تسجيل الدخول، والتقارير.", 1834],
  ["Weekly product review", "Okay, let's get started: three things on the list today.", 2710],
  ["محاضرة تاريخ العلوم", "السلام عليكم. نكمل اليوم الحديث عن بيت الحكمة في بغداد.", 3402],
  ["Interview with Sam", "Thanks for making the time. Let's start with how the project began.", 1290],
];
made.forEach(([title, text, seconds], k) => writeFileSync(join(home, "history", `made-${k}.json`), JSON.stringify({
  id: `made-${k}`, title, source_path: `C:/made-up/${k}.m4a`, created_ms: Date.now() - k * 86400000, duration_s: seconds,
  language: /[a-z]/i.test(title) ? "en" : "ar", elapsed_s: 30, device: "GPU", model: "made up", partial: false,
  segments: [{ start: 0.5, end: 6, text, edited: false }],
})));
// a meeting with its speakers named where they change, and a video of test pictures for the look
writeFileSync(join(home, "history", "made-who.json"), JSON.stringify({
  id: "made-who", title: "اجتماع الفريق الأسبوعي", source_path: "", created_ms: Date.now() - 3600000, duration_s: 34, language: "ar",
  elapsed_s: 4, device: "GPU · NVIDIA GeForce RTX 4060 Laptop GPU", model: "Cohere Transcribe Arabic (07-2026) · Q8", partial: false,
  segments: [
    [0, 6.1, 1, "طيب، خلونا نبدأ. اليوم عندنا ثلاث نقاط: الشاشة الرئيسية، تسجيل الدخول، والتقارير."],
    [6.1, 11.4, 1, "بالنسبة للشاشة الرئيسية، العميل طلب إن الـ dashboard يطلع أول شي بعد الدخول."],
    [11.4, 15.2, 2, "تمام، وأنا جهزت نسخة أولى من الـ design، أرسلها اليوم؟"],
    [15.2, 19.8, 2, "وكمان أضفت زر export للـ Excel في صفحة التقارير."],
    [19.8, 24.6, 1, "ممتاز. تسجيل الدخول لازم يدعم الـ OTP على الجوال قبل نهاية الشهر."],
    [24.6, 29.0, 2, "مين ممكن ياخذ هالمهمة؟ سامي، تقدر تخلصها هالأسبوع؟"],
    [29.0, 33.5, 1, "أقدر، بس أحتاج الـ API من فريق الـ backend يوم الأحد."],
  ].map(([start, end, speaker, text]) => ({ start, end, speaker, text, edited: false })),
  speakers: [{ id: 1, name: "سامي" }, { id: 2, name: "" }],
}));
// two of the minutes bench's made-up meetings (tools/bench-minutes/meetings), with their speakers and
// minutes as the hand-made answers give them
const MEET = join(REPO, "tools", "bench-minutes", "meetings");
// ten minutes of silence (8 kHz, 16-bit, mono) as their recording, so the player shows as it does for a user
const SILENCE = join(OUT, "pictures-silence.wav");
{
  const bytes = 600 * 8000 * 2;
  const head = Buffer.alloc(44);
  head.write("RIFF", 0); head.writeUInt32LE(36 + bytes, 4); head.write("WAVEfmt ", 8); head.writeUInt32LE(16, 16);
  head.writeUInt16LE(1, 20); head.writeUInt16LE(1, 22); head.writeUInt32LE(8000, 24); head.writeUInt32LE(16000, 28);
  head.writeUInt16LE(2, 32); head.writeUInt16LE(16, 34); head.write("data", 36); head.writeUInt32LE(bytes, 40);
  writeFileSync(SILENCE, Buffer.concat([head, Buffer.alloc(bytes)]));
}
function withMinutes(id, title, summary, language) {
  const rows = readFileSync(join(MEET, `${id}.txt`), "utf8").trim().split("\n").map((l) => l.replace(/\r$/, "").split("\t"));
  const at = (t) => t.split(":").reduce((a, x) => a * 60 + Number(x), 0);
  const names = [...new Set(rows.map((r) => r[1]).filter(Boolean))];
  const segments = rows.map(([t, who, text], i) => ({ start: at(t), end: i + 1 < rows.length ? at(rows[i + 1][0]) - 0.3 : at(t) + 4, text, edited: false, ...(who ? { speaker: names.indexOf(who) + 1 } : {}) }));
  const gold = JSON.parse(readFileSync(join(MEET, `${id}.json`), "utf8"));
  const lines = (ls) => ls.map((n) => n - 1);
  const time = (ls) => segments[ls[0] - 1].start;
  const cap = (x) => x.charAt(0).toUpperCase() + x.slice(1);
  const pick = (alts) => (language === "ar" ? alts.find((x) => /[؀-ۿ]/.test(x)) : cap(alts.find((x) => !/[؀-ۿ]/.test(x)) || "")) || "";
  const minutes = {
    language, model: "Jais-3 7B · Q4", written_ms: Date.now(), from: "", kind: "meeting", summary, summary_edited: false,
    decisions: gold.decisions.map((d) => ({ text: d.text, lines: lines(d.lines), time: time(d.lines), edited: false })),
    actions: gold.actions.map((x) => ({ who: pick(x.who), what: x.what, due: x.due.length ? pick(x.due) : "", lines: lines(x.lines), time: time(x.lines), edited: false })),
    topics: gold.topics.map((x) => ({ title: x.title, line: x.line - 1, time: segments[x.line - 1].start, edited: false })),
    open_questions: gold.open_questions.map((q) => ({ text: q.text, lines: lines(q.lines), time: time(q.lines), edited: false })),
  };
  writeFileSync(join(home, "history", `${id}.json`), JSON.stringify({
    id, title, source_path: slash(SILENCE), created_ms: Date.now() - 5400000, duration_s: segments[segments.length - 1].end, language,
    elapsed_s: 30, device: "GPU", model: "made up", partial: false, segments, minutes, speakers: names.map((name, i) => ({ id: i + 1, name })),
  }));
}
withMinutes("en-sprint", "Sprint planning, release 3.2",
  "The team set the scope and date of release 3.2. The calendar sync fix ships behind a feature flag, dark mode moves to 3.3, the code freeze is Friday at noon and 3.2 goes out on Monday, November 30th. Store review time and QA cover during Kenji's leave are still open.", "en");
withMinutes("ar-levant-product", "اجتماع الإطلاق",
  "اتفق الفريق على تأجيل الـ dark mode للسبرنت الجاي، وعلى إطلاق الأندرويد والـ iOS سوا يوم الأربعا 15 نيسان. أول نسخة من الـ dashboard فيها تلات metrics بس. بقي سؤالان: هل يشوف الـ clients الـ dashboard، ومين on call بالويكند.", "ar");
// the weekly meeting of made-who again, as a video of test pictures with its English translation, its
// words placed in time, for Follow words, the translation, the export menu and the README
const TALK = [
  ["طيب، خلونا نبدأ. اليوم عندنا ثلاث نقاط: الشاشة الرئيسية، تسجيل الدخول، والتقارير.", "Okay, let's start. We have three points today: the home screen, login, and the reports."],
  ["بالنسبة للشاشة الرئيسية، العميل طلب إن الـ dashboard يطلع أول شي بعد الدخول.", "About the home screen, the client asked for the dashboard to come up first after login."],
  ["تمام، وأنا جهزت نسخة أولى من الـ design، أرسلها اليوم؟", "Fine, and I have a first version of the design ready. Shall I send it today?"],
  ["وكمان أضفت زر export للـ Excel في صفحة التقارير.", "I also added a button to export to Excel on the reports page."],
  ["ممتاز. تسجيل الدخول لازم يدعم الـ OTP على الجوال قبل نهاية الشهر.", "Great. Login has to support OTP on the phone before the end of the month."],
  ["مين ممكن ياخذ هالمهمة؟ سامي، تقدر تخلصها هالأسبوع؟", "Who can take this task? Sami, can you finish it this week?"],
  ["أقدر، بس أحتاج الـ API من فريق الـ backend يوم الأحد.", "I can, but I need the API from the backend team on Sunday."],
];
if (wanted("listen") || wanted("translate") || wanted("export") || wanted("readme")) {
  const video = join(OUT, "pictures-meeting.mp4");
  if (!existsSync(video)) ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=854x480:rate=25:duration=36", "-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-t", "36", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "aac", video]);
  const who = [1, 1, 2, 2, 1, 2, 1];
  let at = 0.4;
  const segments = TALK.map(([text], i) => {
    const words = [];
    const len = text.split(/\s+/).length;
    let t = at;
    for (const m of text.matchAll(/\S+/g)) { words.push([m.index, Number(t.toFixed(2))]); t += 4.6 / len; }
    const seg = { start: at, end: at + 4.8, text, edited: false, speaker: who[i], words };
    at += 5.1;
    return seg;
  });
  writeFileSync(join(home, "history", "made-talk.json"), JSON.stringify({
    id: "made-talk", title: "اجتماع تطبيق الجوال", source_path: slash(video), created_ms: Date.now() - 1800000, duration_s: 36, language: "ar",
    elapsed_s: 6, device: "GPU · NVIDIA GeForce RTX 4060 Laptop GPU", model: "Cohere Transcribe Arabic (07-2026) · Q8", partial: false, timed: true,
    segments, speakers: [{ id: 1, name: "سامي" }, { id: 2, name: "" }],
    translations: [{ language: "en", model: "MiLMMT-46 12B · Q4", lines: TALK.map(([from, text]) => ({ text, from, edited: false })) }],
  }));
}
// talks spoken by the voices of Windows, to be transcribed while the picture is taken
const voices = join(OUT, "voices");
if ((wanted("live") || wanted("readme")) && !existsSync(join(voices, "meeting-ar.wav"))) {
  execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
}
const talks = join(OUT, "pictures-talks");
mkdirSync(talks, { recursive: true });
const talk = (from, name) => { const to = join(talks, name); copyFileSync(join(voices, from), to); return slash(to); };
if (wanted("settings") && TOOLS) {
  const video = join(OUT, "pictures-look.mp4");
  ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=854x480:rate=25:duration=8", "-f", "lavfi", "-i", "sine=duration=8", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "aac", video]);
  writeFileSync(join(home, "history", "made-video.json"), JSON.stringify({
    id: "made-video", title: "عرض الشاشة الجديدة", source_path: slash(video), created_ms: Date.now() - 7200000, duration_s: 8, language: "ar",
    elapsed_s: 2, device: "GPU", model: "made up", partial: false,
    segments: [{ start: 0.5, end: 4, text: "هذه هي الشاشة الرئيسية الجديدة، والـ dashboard أولًا.", edited: false }],
  }));
}
if (wanted("read")) {
  // VoxCPM 2 shown as downloaded (a file of its size, of nothing), so that the page names it; the
  // stand-in voice reads, should anything be played
  const dir = join(home, "models", "voices", "voxcpm2");
  mkdirSync(dir, { recursive: true });
  execFileSync("fsutil", ["file", "createnew", join(dir, "voxcpm2-q8_0.gguf"), "2955007488"], { stdio: "ignore" });
  writeFileSync(join(home, "reading.json"), JSON.stringify({ voices: { ar: "voxcpm2/ar-f/ar", en: "voxcpm2/en-f/en" } }));
}
// the test sound source (debug builds): made-up microphones, never this PC's own names, beside Record;
// the stand-in voice for Read aloud
const a = await open(home, PORT + 12, { DAWWIN_TEST_RECORD: "silent", DAWWIN_TEST_VOICE: join(dirname(APP), "examples", "fake_voice.exe") });
await a.call("Emulation.setDeviceMetricsOverride", { width: 1200, height: 820, deviceScaleFactor: 1, mobile: false });
await sleep(800);

/** The union of some elements' boxes, with room around. */
const box = (sels, pad = 14) => a.ev(`(()=>{const rs=${JSON.stringify(sels)}.map(s=>document.querySelector(s).getBoundingClientRect());const x=Math.max(0,Math.min(...rs.map(r=>r.left))-${pad}),y=Math.max(0,Math.min(...rs.map(r=>r.top))-${pad});return {x,y,width:Math.max(...rs.map(r=>r.right))+${pad}-x,height:Math.max(...rs.map(r=>r.bottom))+${pad}-y}})()`);
async function picture(file, area, format = "webp") {
  await a.ev(`document.getElementById('toast').hidden=true`);
  const r = await a.call("Page.captureScreenshot", { format, quality: format === "webp" ? 85 : undefined, clip: { ...area, scale: 1 } });
  writeFileSync(file, Buffer.from(r.result.data, "base64"));
  console.log("made", file);
}
const steps = (s) => a.ev(`(()=>{S.steps={...DEFAULT_STEPS,...${JSON.stringify(s)}};paintSteps();showView('home');})()`);
/** The line being played and its word, as Follow words draws them (no sound needed). */
const playing = (i, word) => a.ev(`(()=>{$('follow').checked=true;S.follow=true;document.querySelectorAll('.seg.playing').forEach(r=>r.classList.remove('playing'));const row=rowOf(${i});row.classList.add('playing');const el=row.querySelector('.text');const at=S.current.segments[${i}].text.indexOf(${JSON.stringify(word)});CSS.highlights.set('now-word',new Highlight(textRange(el,at,at+${word.length})));})()`);
/** A word selected and right-clicked: the menu with Play from here. */
const rightClick = (i, word) => a.ev(`(()=>{const el=rowOf(${i}).querySelector('.text');const at=S.current.segments[${i}].text.indexOf(${JSON.stringify(word)});const r=textRange(el,at,at+${word.length});const sel=getSelection();sel.removeAllRanges();sel.addRange(r);const b=r.getBoundingClientRect();el.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:b.left+b.width/2,clientY:b.bottom+6}));})()`);
const unclick = () => a.ev(`(()=>{hideCtx();getSelection().removeAllRanges();CSS.highlights.delete('now-word');document.querySelectorAll('.seg.playing').forEach(r=>r.classList.remove('playing'));})()`);
const openTalk = async (lang) => { await a.ev(`(async()=>{await loadHistory();Minutes.tab='lines';await openTranscript('made-talk'${lang ? `, ${JSON.stringify(lang)}` : ""});})()`); await sleep(700); };
const exportMenu = async (scope) => {
  await a.ev(`(()=>{$('btn-export').click();const b=document.querySelector('#export-scope [data-scope=${scope}]');if(b&&!b.closest('[hidden]'))b.click();})()`);
  await sleep(400);
};
const closeMenus = () => a.ev(`document.querySelectorAll('.menu').forEach(m=>m.hidden=true)`);
const windowSize = async (width, height) => { await a.call("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false }); await sleep(600); };
/** Talks put in the queue to be transcribed, the first one open; back as soon as it is well under way
 *  (on a GPU a talk of two minutes takes a few seconds), the others still waiting. */
async function transcribing(files) {
  const ids = [];
  await a.ev(`(()=>{$('follow').checked=false;S.follow=false;})()`);
  // on the processor, as Settings allows: there it takes long enough to be seen
  await a.ev(`window.__TAURI__.core.invoke('set_run_on', { value: 'cpu' }).then(onState)`);
  for (const [path, language] of files) ids.push(...(await a.invoke("enqueue", { paths: [path], language, steps: await a.ev(`(S.steps={...DEFAULT_STEPS,transcribe:true},stepsForJob())`) })));
  const first = JSON.stringify(ids[0]);
  for (let i = 0; i < 100 && !(await a.ev(`!!jobById(${first})`)); i++) await sleep(100);
  await a.ev(`openTranscript(${first})`);
  for (let i = 0; i < 2400; i++) {
    const p = await a.ev(`S.current?.id === ${first} && S.current.segments.length >= 3 ? (jobById(${first}) || {}).progress || 0 : 0`);
    if (p >= 0.4) break;
    await sleep(100);
  }
  return ids;
}
/** Stop them, and leave nothing of them in History for the pictures after. */
const cancelAll = async (ids) => {
  for (const id of ids) await a.invoke("cancel_job", { id });
  await a.idle(60);
  await a.ev(`window.__TAURI__.core.invoke('set_run_on', { value: 'auto' }).then(onState)`);
  for (const id of ids) {
    await a.invoke("dismiss_job", { id }).catch(() => {});
    await a.invoke("delete_transcript", { id }).catch(() => {});
  }
  await a.ev(`loadHistory()`);
};

for (const lang of ["en", "ar"]) {
  await a.ev(`setUiLang(${JSON.stringify(lang)})`);
  const dir = join(REPO, "ui", "guide", lang);
  mkdirSync(dir, { recursive: true });
  if (wanted("drop")) {
    // the start page: drop files or a folder, or record
    await steps({ transcribe: true });
    await sleep(400);
    await picture(join(dir, "drop.webp"), await box(["#dropzone", "#record-bar"]));
  }
  if (wanted("live")) {
    // a meeting of two of Windows' voices, its lines appearing while it is transcribed
    const ids = await transcribing([[talk("meeting-ar.wav", lang === "ar" ? "اجتماع الفريق.wav" : "Team meeting.wav"), "ar"]]);
    await picture(join(dir, "live.webp"), { ...(await box(["#main"], 0)), height: 470 });
    await cancelAll(ids);
  }
  if (wanted("listen")) {
    // Follow words on a line being played, and a word selected and right-clicked
    await openTalk();
    await playing(0, "الشاشة");
    await rightClick(1, "العميل");
    await sleep(300);
    const main = await box(["#main"], 0);
    const menuEnd = await a.ev(`$('ctx-menu').getBoundingClientRect().bottom`);
    await picture(join(dir, "listen.webp"), { ...main, height: Math.max(470, Math.ceil(menuEnd + 14 - main.y)) });
    await unclick();
  }
  if (wanted("translate")) {
    // the English translation under each line
    await openTalk("en");
    await picture(join(dir, "translate.webp"), { ...(await box(["#main"], 0)), height: 470 });
  }
  if (wanted("export")) {
    // the export menu with both languages chosen, the video's own entries at its end
    await openTalk("en");
    await exportMenu("both");
    // the menu and, on the side the page reads from, the lines it exports
    const m = await a.ev(`(()=>{const r=$('export-menu').getBoundingClientRect(),t=$('btn-export').getBoundingClientRect(),w=$('main').getBoundingClientRect();return {x:r.left,y:t.top,right:r.right,bottom:r.bottom,left0:w.left,right0:w.right}})()`);
    const x = Math.max(m.left0, lang === "ar" ? m.x - 320 : m.x - 14);
    const right = Math.min(m.right0, lang === "ar" ? m.right + 14 : m.right + 320);
    const y = Math.max(0, m.y - 14);
    await picture(join(dir, "export.webp"), { x, y, width: right - x, height: Math.min(820, m.bottom + 14) - y });
    await closeMenus();
  }
  if (wanted("steps")) {
    // the common jobs, and the four steps ticked by hand, the speakers and the language found too
    await a.ev(`setSpoken('auto')`);
    await steps({ remove_silence: true, speakers: true, minutes: true, translate: true, target: "en", video: true, burn: true });
    await sleep(400);
    await picture(join(dir, "steps.webp"), await box(["#jobs", "#steps"]));
  }
  if (wanted("sound")) {
    await steps({ transcribe: false, save_audio: true });
    await sleep(400);
    await picture(join(dir, "sound.webp"), await box(["#dropzone", "#jobs", "#steps"]));
  }
  if (wanted("speakers")) {
    // the names where the speaker changes, and a name's menu
    await a.ev(`(async()=>{await loadHistory();await openTranscript('made-who');})()`);
    await sleep(600);
    await a.ev(`document.querySelector('.seg.playing')?.classList.remove('playing'); document.querySelectorAll('.seg .who')[0].click()`);
    await sleep(400);
    await picture(join(dir, "speakers.webp"), { ...(await box(["#main"], 0)), height: 430 });
    if (lang === "en") await picture(join(REPO, "docs", "screenshots", "speakers.png"), { x: 0, y: 0, width: 1200, height: 820 }, "png");
    await a.ev(`closePop()`);
  }
  if (wanted("minutes")) {
    // a meeting's minutes: the summary, the decisions with their times, who does what by when
    await a.ev(`(async()=>{await loadHistory();Minutes.tab='minutes';await openTranscript(${JSON.stringify(lang === "ar" ? "ar-levant-product" : "en-sprint")});})()`);
    await sleep(800);
    await picture(join(dir, "minutes.webp"), { ...(await box(["#main"], 0)), height: 560 });
    if (lang === "en") {
      await a.call("Emulation.setDeviceMetricsOverride", { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false });
      await sleep(600);
      await picture(join(REPO, "docs", "screenshots", "minutes.png"), { x: 0, y: 0, width: 1400, height: 900 }, "png");
      await a.call("Emulation.setDeviceMetricsOverride", { width: 1200, height: 820, deviceScaleFactor: 1, mobile: false });
      await sleep(400);
    }
    await a.ev(`showMinutesTab('lines')`);
  }
  if (wanted("settings") && TOOLS) {
    // the subtitle look, with a second of a video burned in it
    await a.ev(`(async()=>{await openTranscript('made-video');showView('settings');})()`);
    await sleep(1500);
    await a.ev(`(async()=>{document.getElementById('look-size').closest('section').scrollIntoView();await previewLook();})()`);
    for (let i = 0; i < 40 && !(await a.ev(`document.getElementById('look-video').readyState >= 2`)); i++) await sleep(250);
    await sleep(800);
    await a.ev(`document.getElementById('look-size').closest('section').scrollIntoView({ block: 'start' })`);
    await sleep(300);
    const r = await a.ev(`(()=>{const b=document.getElementById('look-size').closest('section').getBoundingClientRect();const y=Math.max(0,b.top-14);return {x:b.left-14,y,width:b.width+28,height:Math.min(b.height+28,820-y)}})()`);
    await picture(join(dir, "settings.webp"), r);
  }
  if (wanted("read")) {
    // a made-up article, read from its second paragraph
    const text = lang === "ar"
      ? `# العمل من المنزل

صار العمل من المنزل جزءًا من حياة كثير من الناس. ويقول بعضهم إنه يمنحهم وقتًا أطول مع العائلة، بينما يشعر آخرون بالعزلة.

وتنصح الدراسات بتخصيص مكان ثابت للعمل، وبأخذ استراحة قصيرة كل ساعة. وكما قال أحد المديرين: the best office is the one you can leave at five. ويبقى الأهم أن تجد التوازن الذي يناسبك.

وفي استطلاع شمل 1200 موظف، قال 64% إنهم يفضّلون يومين على الأقل في المنزل كل أسبوع.`
      : `# Working from home

Working from home has become part of life for many people. Some say it gives them more time with their family, while others feel cut off.

Studies suggest keeping one fixed place to work, and taking a short break every hour. As one manager put it: the best office is the one you can leave at five. What matters most is finding the balance that suits you.

In a survey of 1,200 employees, 64% said they would like at least two days a week at home.`;
    await a.ev(`openReading({ text: ${JSON.stringify(text)} }).then(() => { READ.i = 3; markSentence(false); paintReadPlayer(); })`);
    await sleep(800);
    await picture(join(dir, "read.webp"), await box(["#read-doc"], 0));
    if (lang === "en") await picture(join(REPO, "docs", "screenshots", "read.png"), { x: 0, y: 0, width: 1200, height: 820 }, "png");
    await a.ev(`closeReading(); showView('home')`);
  }
  if (wanted("screen")) {
    // Read anything on the screen in Settings, the shortcut on as a user sees it (drawn so, without
    // taking the shortcut on this PC), with a word in Say it like this
    await a.ev(`showView('settings')`);
    await sleep(500);
    const say = lang === "ar" ? { lang: "ar", written: "AI", said: "إيه آي" } : { lang: "en", written: "Dawwin", said: "Dow-win" };
    await a.ev(`(async()=>{const s=await window.__TAURI__.core.invoke('screen_state');s.settings.on=true;s.settings.say=[${JSON.stringify(say)}];s.shortcut={state:'on',keys:s.settings.shortcut};window.__screen.render(s);document.getElementById('set-screen').scrollIntoView({block:'start'});})()`);
    await sleep(500);
    const area = await box(["#set-screen"]);
    await picture(join(dir, "screen.webp"), area);
    if (lang === "en") {
      await picture(join(REPO, "docs", "screenshots", "screen.png"), area, "png");
      // the small player reading a selection, with the stand-in voice: its own window
      await a.invoke("test_screen_read", { text: "The small player reads what you selected, in the voice you chose. Press the shortcut again to stop." });
      for (let i = 0; i < 40; i++) {
        const t = (await (await fetch(`http://127.0.0.1:${PORT + 12}/json`)).json()).find((x) => x.type === "page" && x.url.includes("player.html"));
        if (t) {
          const ws = new WebSocket(t.webSocketDebuggerUrl);
          await new Promise((r) => (ws.onopen = r));
          await sleep(1200);
          const shot = await new Promise((r) => { ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id === 1) r(d); }; ws.send(JSON.stringify({ id: 1, method: "Page.captureScreenshot", params: { format: "png" } })); });
          writeFileSync(join(REPO, "docs", "screenshots", "player.png"), Buffer.from(shot.result.data, "base64"));
          console.log("made", join(REPO, "docs", "screenshots", "player.png"));
          ws.close();
          break;
        }
        await sleep(250);
      }
      await a.invoke("test_screen_read", { text: "stop" });
    }
    await a.ev(`showView('home')`);
  }
  if (wanted("home") && lang === "en") {
    await steps({ transcribe: true });
    await a.call("Emulation.setDeviceMetricsOverride", { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false });
    await sleep(600);
    await picture(join(REPO, "docs", "screenshots", "home.png"), { x: 0, y: 0, width: 1400, height: 900 }, "png");
    await a.call("Emulation.setDeviceMetricsOverride", { width: 1200, height: 820, deviceScaleFactor: 1, mobile: false });
    await sleep(400);
  }
}
if (wanted("readme")) {
  // the README's pictures of a transcript, at the size of the README's others
  await a.ev(`setUiLang('en')`);
  await windowSize(1500, 1000);
  const shot = (name) => picture(join(REPO, "docs", "screenshots", name), { x: 0, y: 0, width: 1500, height: 1000 }, "png");
  await openTalk();
  await playing(1, "dashboard");
  await shot("transcript.png");
  await unclick();
  await openTalk();
  await rightClick(4, "OTP");
  await sleep(300);
  await shot("play-from-here.png");
  await unclick();
  await openTalk("en");
  await shot("translation.png");
  await exportMenu("both");
  await shot("export-video.png");
  await closeMenus();
  await openTalk();
  await a.ev(`(()=>{const s=$('search');s.value='الـ';s.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await sleep(500);
  await exportMenu("original");
  await shot("search-export.png");
  await closeMenus();
  await a.ev(`(()=>{const s=$('search');s.value='';s.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await a.ev(`setUiLang('ar')`);
  await openTalk("en");
  await shot("arabic.png");
  await a.ev(`setUiLang('en')`);
  // three talks in the queue, the first being written
  const ids = await transcribing([
    [talk("meeting-ar.wav", "اجتماع الفريق.wav"), "ar"],
    [talk("meeting-en.wav", "Weekly meeting.wav"), "en"],
    [talk("lecture-en.wav", "Lecture on the history of science.wav"), "en"],
  ]);
  await shot("transcribing.png");
  await cancelAll(ids);
  await windowSize(1200, 820);
}
await a.ev(`setUiLang('en')`);
a.close();
if (wanted("setup")) {
  // the first-run page: no model yet, the language chosen, the models for it offered
  const b = await open(makeHome("pictures-setup", { model_file: null }), PORT + 13);
  await b.call("Emulation.setDeviceMetricsOverride", { width: 1500, height: 1000, deviceScaleFactor: 1, mobile: false });
  // the folder as a user's own default reads, not this test's (or this PC's account)
  await b.ev(`(()=>{showView('setup');const m=$('setup-spoken');m.value='da';m.onchange();$('download-dir').textContent=${JSON.stringify(String.raw`C:\Users\Sam\AppData\Local\Dawwin\models`)};})()`);
  await sleep(800);
  const r = await b.call("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: 1500, height: 1000, scale: 1 } });
  writeFileSync(join(REPO, "docs", "screenshots", "setup.png"), Buffer.from(r.result.data, "base64"));
  console.log("made", join(REPO, "docs", "screenshots", "setup.png"));
  b.close();
}
finish();
