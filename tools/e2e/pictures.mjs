// The pictures of "How to use" (ui/guide/<lang>/<part>.webp) and the README's start page
// (docs/screenshots/home.png), speakers (docs/screenshots/speakers.png) and minutes
// (docs/screenshots/minutes.png), from the real app, in English and Arabic. Made-up data only. Writes
// into the repository: look at what it made before committing.
//   node tools/e2e/pictures.mjs [part …]     (parts: steps sound home speakers minutes settings; all by default)
// "settings" (the subtitle look with its preview) needs DAWWIN_E2E_TOOLS, and a full ffmpeg for its video.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

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
if (wanted("settings") && TOOLS) {
  const video = join(OUT, "pictures-look.mp4");
  ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=854x480:rate=25:duration=8", "-f", "lavfi", "-i", "sine=duration=8", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "aac", video]);
  writeFileSync(join(home, "history", "made-video.json"), JSON.stringify({
    id: "made-video", title: "عرض الشاشة الجديدة", source_path: slash(video), created_ms: Date.now() - 7200000, duration_s: 8, language: "ar",
    elapsed_s: 2, device: "GPU", model: "made up", partial: false,
    segments: [{ start: 0.5, end: 4, text: "هذه هي الشاشة الرئيسية الجديدة، والـ dashboard أولًا.", edited: false }],
  }));
}
// the test sound source (debug builds): made-up microphones, never this PC's own names, beside Record
const a = await open(home, PORT + 12, { DAWWIN_TEST_RECORD: "silent" });
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

for (const lang of ["en", "ar"]) {
  await a.ev(`setUiLang(${JSON.stringify(lang)})`);
  const dir = join(REPO, "ui", "guide", lang);
  mkdirSync(dir, { recursive: true });
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
  if (wanted("home") && lang === "en") {
    await steps({ transcribe: true });
    await a.call("Emulation.setDeviceMetricsOverride", { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false });
    await sleep(600);
    await picture(join(REPO, "docs", "screenshots", "home.png"), { x: 0, y: 0, width: 1400, height: 900 }, "png");
    await a.call("Emulation.setDeviceMetricsOverride", { width: 1200, height: 820, deviceScaleFactor: 1, mobile: false });
    await sleep(400);
  }
}
await a.ev(`setUiLang('en')`);
a.close();
finish();
