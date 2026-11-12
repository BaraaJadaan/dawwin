// The pictures of "How to use" (ui/guide/<lang>/<part>.webp) and the README's start page
// (docs/screenshots/home.png), from the real app, in English and Arabic. Made-up data only.
// Writes into the repository: look at what it made before committing.
//   node tools/e2e/pictures.mjs [part …]     (parts: steps sound home; all of them by default)
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { finish, makeHome, MODEL, need, open, PORT, REPO, sleep } from "./lib.mjs";

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
    // the common jobs, and the four steps ticked by hand
    await steps({ remove_silence: true, translate: true, target: "en", video: true, burn: true });
    await sleep(400);
    await picture(join(dir, "steps.webp"), await box(["#jobs", "#steps"]));
  }
  if (wanted("sound")) {
    await steps({ transcribe: false, save_audio: true });
    await sleep(400);
    await picture(join(dir, "sound.webp"), await box(["#dropzone", "#jobs", "#steps"]));
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
