// The pictures of "How to use" (ui/guide/<lang>/<part>.webp) and the README's start page
// (docs/screenshots/home.png) and speakers (docs/screenshots/speakers.png), from the real app, in
// English and Arabic. Made-up data only. Writes into the repository: look at what it made before
// committing.
//   node tools/e2e/pictures.mjs [part …]     (parts: steps sound home speakers settings; all by default)
// "settings" (the subtitle look with its preview) needs DAWWIN_E2E_TOOLS, and a full ffmpeg for its video.
import { mkdirSync, writeFileSync } from "node:fs";
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
if (wanted("settings") && TOOLS) {
  const video = join(OUT, "pictures-look.mp4");
  ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=854x480:rate=25:duration=8", "-f", "lavfi", "-i", "sine=duration=8", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "aac", video]);
  writeFileSync(join(home, "history", "made-video.json"), JSON.stringify({
    id: "made-video", title: "عرض الشاشة الجديدة", source_path: slash(video), created_ms: Date.now() - 7200000, duration_s: 8, language: "ar",
    elapsed_s: 2, device: "GPU", model: "made up", partial: false,
    segments: [{ start: 0.5, end: 4, text: "هذه هي الشاشة الرئيسية الجديدة، والـ dashboard أولًا.", edited: false }],
  }));
}
const a = await open(home, PORT + 12);
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
    await steps({ remove_silence: true, speakers: true, translate: true, target: "en", video: true, burn: true });
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
