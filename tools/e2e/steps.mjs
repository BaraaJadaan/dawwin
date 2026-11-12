// The steps on the start page, in the real app: how they go together; a video's long silences
// taken out and its sound saved, with nothing else; then the whole chain on a video (silences out,
// transcribed on the GPU, translated by the stand-in translator, subtitles added to the video),
// its times those of the video and not of the shortened sound; the sound saved later from the
// transcript, never over the recording; "How to use" and the address to write to.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, rmSync, truncateSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, FAKE_TRANSLATOR, FFMPEG, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);
need("the stand-in translator (cargo build --example fake_translator)", existsSync(FAKE_TRANSLATOR));

/** How long a file plays, as the full ffmpeg reads it. */
const duration = (file) => {
  let info = "";
  try { execFileSync(FFMPEG, ["-hide_banner", "-i", file], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }); } catch (e) { info = String(e.stderr); }
  const m = /Duration: (\d+):(\d+):([\d.]+)/.exec(info);
  return m ? +m[1] * 3600 + +m[2] * 60 + +m[3] : NaN;
};

// ---- the video: an Arabic talk of the voices of Windows, its own pauses kept under half a second,
// in four parts with 2.5, 3 and 2 s of a quiet room between them, as in a meeting
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-ar.wav"))) {
  execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
}
const work = join(OUT, "steps-videos");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const said = join(work, "said.wav");
ffmpeg(["-i", join(voices, "meeting-ar.wav"), "-af", "silenceremove=stop_periods=-1:stop_duration=0.2:stop_threshold=-50dB:stop_silence=0.1", said]);
const SAID_S = duration(said);
const gaps = [2.5, 3, 2];
const cut = [0, 0.25, 0.5, 0.75, 1].map((f) => +(f * SAID_S).toFixed(3));
const parts = [0, 1, 2, 3].map((k) => `[p${k}]atrim=${cut[k]}:${cut[k + 1]},asetpts=PTS-STARTPTS[s${k}]`);
const silences = gaps.map((g, k) => `aevalsrc=0:d=${g}:s=16000[g${k}]`);
const speech = join(work, "speech.wav");
ffmpeg([
  "-i", said, "-f", "lavfi", "-i", "anoisesrc=c=pink:a=0.003:r=16000:seed=7",
  "-filter_complex", ["[0:a]asplit=4[p0][p1][p2][p3]", ...parts, ...silences, "[s0][g0][s1][g1][s2][g2][s3]concat=n=7:v=0:a=1[sp]", "[sp][1:a]amix=inputs=2:duration=first:normalize=0[out]"].join(";"),
  "-map", "[out]", speech,
]);
const clip = join(work, "lesson.mp4");
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=640x360:rate=24", "-i", speech, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", clip]);
rmSync(said);
rmSync(speech);
const CLIP_S = duration(clip);
const soundOnly = join(work, "talk.mp4");
ffmpeg(["-i", clip, "-c", "copy", soundOnly]);

// ---- the data folder: Dawwin's own ffmpeg, a stand-in for the smallest translation model (its
// name and size, and a GGUF header), and a made-up meeting with its translation
const home = makeHome("steps-home");
const model = join(home, "models", "MiLMMT-46-1B-v1.0.Q4_K_M.gguf");
writeFileSync(model, Buffer.from("GGUF\x03\0\0\0", "binary"));
execFileSync("fsutil", ["sparse", "setflag", model], { stdio: "ignore" });
truncateSync(model, 806057408);
const lines = [
  ["طيب، خلونا نبدأ. اليوم عندنا ثلاث نقاط: الشاشة الرئيسية، تسجيل الدخول، والتقارير.", "Okay, let's start. We have three points today: the home screen, login, and the reports."],
  ["بالنسبة للشاشة الرئيسية، العميل طلب إن الـ dashboard يطلع أول شي بعد الدخول.", "About the home screen: the client asked for the dashboard to come up first after login."],
  ["وكمان بدهم فلتر حسب التاريخ، وزر export للـ Excel.", "They also want a date filter, and a button to export to Excel."],
  ["تسجيل الدخول لازم يدعم الـ OTP على الجوال قبل نهاية الشهر.", "Login has to support OTP on the phone before the end of the month."],
  ["مين ممكن ياخذ هالمهمة؟ سامي، تقدر تخلصها هالأسبوع؟", "Who can take this task? Sami, can you finish it this week?"],
  ["أكيد، بس بحتاج الـ API من فريق الـ backend يوم الأحد.", "Sure, but I need the API from the backend team on Sunday."],
];
let at = 0.6;
const segments = lines.map(([text]) => { const dur = 4 + (text.length % 5); const s = { start: at, end: at + dur, text, edited: false }; at += dur + 0.4; return s; });
writeFileSync(join(home, "history", "meeting-1.json"), JSON.stringify({
  id: "meeting-1", title: "اجتماع متطلبات التطبيق", source_path: clip, created_ms: Date.now() - 60000, duration_s: at, language: "ar",
  elapsed_s: 4.2, device: "GPU", model: "Cohere Transcribe Arabic (07-2026) · Q8", partial: false, timed: true, segments,
  translations: [{ language: "en", model: "MiLMMT-46 1B · Q4", lines: lines.map(([from, text]) => ({ text, from, edited: false })) }],
}));

const a = await open(home, PORT + 20, { DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_TRANSLATOR_DELAY_MS: "60" });
const click = (id) => a.ev(`document.getElementById(${JSON.stringify(id)}).click()`);
const checked = (id) => a.ev(`document.getElementById(${JSON.stringify(id)}).checked`);
const st = (await a.state()).setup;
check("Dawwin's own ffmpeg is the one in use", (st.ffmpeg || "").includes(slash(home).split("/").pop()), st.ffmpeg);
await a.ev(`loadHistory()`);
await a.ev(`showView('home')`);

// ---- the steps go together
check("as it comes: transcribe only", (await checked("step-transcribe")) && !(await checked("step-silence")) && !(await checked("step-translate")) && !(await checked("step-video")));
await click("step-translate");
await click("step-transcribe");
check("no transcript, no translation", !(await checked("step-translate")) && !(await checked("step-transcribe")));
await click("step-translate");
check("translating turns transcribing on", (await checked("step-transcribe")) && (await checked("step-translate")));
await click("step-transcribe"); // all off again
await click("step-silence");
check("silences out with nothing else: the sound is saved", (await checked("step-save-audio")) && (await a.ev(`document.getElementById('step-save-audio').disabled`)));
check("and that is the job \"Sound without silences\"", (await a.ev(`document.querySelector('#jobs [aria-pressed="true"]')?.dataset.job`)) === "silences");

// ---- a job that only takes the silences out
await a.ev(`addFiles([${JSON.stringify(slash(soundOnly))}])`);
await sleep(500);
check("the job ends", await a.idle(180));
const saved = readdirSync(work).find((f) => f.startsWith("talk (without silences)"));
check("the sound without its silences is saved next to the video", !!saved, readdirSync(work));
if (saved) {
  const d = duration(join(work, saved));
  check("and the 7.5 s of room between its parts are gone, the speech kept", d < CLIP_S - 5.5 && d > CLIP_S - 12, { saved: d, from: CLIP_S });
}
check("no transcript was made", (await a.invoke("list_history")).length === 1);

// ---- the whole chain on a video
await click("step-transcribe");
await click("step-translate");
await click("step-video");
check("the four steps are on", (await a.ev(`[...document.querySelectorAll('.step.on')].length`)) === 4);
await a.ev(`(()=>{const m=document.getElementById('step-target');m.value='en';m.dispatchEvent(new Event('change'));})()`);
await a.ev(`addFiles([${JSON.stringify(slash(clip))}])`);
await sleep(1500);
check("the job ends", await a.idle(600));
await sleep(1500);
const all = await a.invoke("list_history");
const id = all.find((h) => h.id !== "meeting-1")?.id;
const t = id ? await a.invoke("get_transcript", { id }) : { segments: [] };
check("transcribed", t.segments?.length >= 2, t.segments?.length);
check("on the GPU", (t.device || "").startsWith("GPU"), t.device);
check("translated", t.translations?.[0]?.language === "en" && t.translations[0].lines.filter((l) => l.text).length === t.segments.filter((s) => s.text.trim()).length, t.translations?.[0]?.lines.filter((l) => l.text).length);
const last = t.segments[t.segments.length - 1];
check("times of the original video, not of the shortened sound", last && last.end > CLIP_S - 3 && last.end <= CLIP_S + 0.5, { end: last?.end, video: CLIP_S });
const vid = readdirSync(work).find((f) => f.startsWith("lesson (English, Arabic)"));
check("the video with subtitles is saved next to it", !!vid, readdirSync(work));
check("the translation shows under the lines", (await a.ev(`S.shown`)) === "en", await a.ev(`S.shown`));

// ---- the sound, later, from a transcript
const later = join(work, "later.m4a");
await a.invoke("export_audio_to_path", { id, path: slash(later), withoutSilences: false });
check("the video's sound, saved from the transcript", existsSync(later) && Math.abs(duration(later) - CLIP_S) < 1.5, existsSync(later) ? { saved: duration(later), video: CLIP_S } : null);
const refuse = await a.ev(`window.__TAURI__.core.invoke('export_audio_to_path',{id:${JSON.stringify(id)},path:${JSON.stringify(slash(clip))},withoutSilences:true}).then(()=>'saved',e=>String(e))`);
check("never over the recording itself", refuse.startsWith("Choose another name"), refuse);
check("and the recording is still whole", Math.abs(duration(clip) - CLIP_S) < 0.1, duration(clip));

// ---- the guide and the address to write to
await a.ev(`showView('guide')`);
await sleep(300);
check("How to use has eight parts", (await a.ev(`document.querySelectorAll('#guide-items .guide-item').length`)) === 8, await a.ev(`[...document.querySelectorAll('#guide-items h2')].map(h=>h.textContent)`));
const mail = await a.ev(`document.getElementById('guide-mail').textContent.trim()`);
check("and an email address to write to", /^[^\s@]+@[^\s@]+\.\w+$/.test(mail), mail);
await a.ev(`showView('settings')`);
check("Settings has the same one", (await a.ev(`document.getElementById('set-mail').textContent.trim()`)) === mail);

a.close();
finish();
