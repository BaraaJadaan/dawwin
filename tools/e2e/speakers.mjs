// Who said what, the language heard, words and names, and the subtitle look, through the real app.
// A two-voice recording is made from Windows' voices (David and Zira taking turns) and transcribed
// with "Detect the language" and "Tell speakers apart"; then rename, merge, a line given to someone
// new, every export, names in subtitles (right to left behind a name), search within one speaker,
// words and names, the look's preview and a burn with sharp text, Find the speakers on a transcript
// made without them; and the clicks a person makes on a made-up meeting. Dawwin's own ffmpeg only.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, FFMPEG, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, sleep, slash, TOOLS } from "./lib.mjs";

const SPEAKERS = process.env.DAWWIN_E2E_SPEAKERS || "";
const LANGUAGE = process.env.DAWWIN_E2E_LANGUAGE || "";
need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);
need("DAWWIN_E2E_SPEAKERS (the speaker model, diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf)", SPEAKERS);
need("DAWWIN_E2E_LANGUAGE (the language model, lang-id-voxlingua107-ecapa-Q8_0.gguf)", LANGUAGE);
const ff = join(TOOLS, "ffmpeg-video", "ffmpeg.exe");
const probe = (file) => { try { execFileSync(ff, ["-hide_banner", "-i", file], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }); return ""; } catch (e) { return String(e.stderr); } };

// ---------------------------------------------------------------- a made-up two-voice recording
const media = join(OUT, "speakers-media");
rmSync(media, { recursive: true, force: true });
mkdirSync(media, { recursive: true });
const said = [
  ["Microsoft David Desktop", "Good morning everyone. Let's start with the first point on the list, the login page, which is late."],
  ["Microsoft Zira Desktop", "Thank you. The client wants the dashboard to come up first, right after signing in, and the reports a click away."],
  ["Microsoft David Desktop", "That is fine with me. Who can take the login page this week, and send the new screens to the team?"],
  ["Microsoft Zira Desktop", "I can do it. Then we send the new screens on Thursday, and we meet again next week."],
];
const ps = said.map(([voice, text], k) => `$s.SelectVoice('${voice}'); $s.SetOutputToWaveFile('${join(media, `part${k}.wav`)}', (New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo 16000, ([System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen), ([System.Speech.AudioFormat.AudioChannel]::Mono))); $s.Speak('${text.replace(/'/g, "''")}');`).join(" ");
execFileSync("powershell", ["-NoProfile", "-Command", `Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; ${ps} $s.SetOutputToNull()`]);
ffmpeg(["-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-t", "0.9", join(media, "gap.wav")]);
const parts = said.flatMap((_, k) => ["-i", join(media, `part${k}.wav`), "-i", join(media, "gap.wav")]);
ffmpeg([...parts, "-filter_complex", `${said.map((_, k) => `[${2 * k}][${2 * k + 1}]`).join("")}concat=n=${2 * said.length}:v=0:a=1`, "-c:a", "aac", "-b:a", "96k", join(media, "two-voices.m4a")]);
const VIDEO = join(media, "two-voices-480p.mp4");
ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=854x480:rate=25", "-i", join(media, "two-voices.m4a"), "-shortest", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "copy", VIDEO]);
check("a two-voice recording is made", existsSync(VIDEO), probe(VIDEO).match(/Duration: [^,]*/)?.[0]);

const home = makeHome("speakers-home", { model_files: { speakers: SPEAKERS, language: LANGUAGE } });
const out = join(OUT, "speakers-out");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const a = await open(home, PORT + 30);
const { ev, invoke } = a;

/** A job to its end: the page takes finished jobs off the list at once, so gone means done. */
async function waitJob(id, seconds) {
  for (let i = 0; i < seconds; i++) {
    const j = (await invoke("get_state")).queue.find((x) => x.id === id);
    if (!j && i > 0) return { status: "done" };
    if (j && !ACTIVE_JOB.includes(j.status)) return j;
    await sleep(1000);
  }
  return null;
}
const ACTIVE_JOB = ["queued", "decoding", "detecting", "loading", "transcribing", "timing", "speakers", "translating", "saving_audio", "saving_video"];

// ---------------------------------------------------------------- the steps
check("the language menu starts with Detect the language", await ev(`document.getElementById('spoken').options[0].value === 'auto'`));
check("but setting up a model does not offer it", await ev(`document.getElementById('setup-spoken').options[0].value !== 'auto'`));
await ev(`setSpoken('auto'); setStep('speakers', true)`);
check("Tell speakers apart sits in step 2, ticked", await ev(`document.querySelector('.step[data-step=text] #step-speakers').checked`));
check("it says on hover that it tells up to four apart", (await ev(`document.getElementById('step-speakers').closest('label').title`)).includes("four"));
check("the drop message says it", (await ev(`dropText()`)).includes("tell the speakers apart"), await ev(`dropText()`));

// ---------------------------------------------------------------- transcribe with both
const [id] = await invoke("enqueue", { paths: [slash(VIDEO)], language: "auto", steps: await ev(`stepsForJob()`) });
await ev(`openTranscript(${JSON.stringify(id)})`);
const job = await waitJob(id, 900);
check("transcribed with the language detected and the speakers told apart", job?.status === "done", job);
let tx = await invoke("get_transcript", { id });
check("the language heard is English, and kept as heard", tx.language === "en" && tx.detected === "en", { language: tx.language, detected: tx.detected });
const lines = tx.segments.filter((s) => s.text.trim());
console.log(lines.map((s) => `   ${s.start.toFixed(1)}-${s.end.toFixed(1)} ${s.speaker} ${s.text.slice(0, 50)}`).join("\n"));
check("two speakers", new Set(lines.map((s) => s.speaker)).size === 2);
check("David and Zira take turns", lines.every((s, k) => k === 0 || s.speaker !== lines[k - 1].speaker || s.text.length < 20), lines.map((s) => s.speaker));
await ev(`openTranscript(${JSON.stringify(id)})`);
await sleep(800);
const labels = await ev(`[...document.querySelectorAll('.seg .who')].map((b) => b.textContent)`);
check("a name where the speaker changes", labels.length >= 2 && labels[0] === "Speaker 1", labels);
check("the meta line says what was heard", (await ev(`document.getElementById('t-meta').textContent`)).includes("heard as English"));

// ---------------------------------------------------------------- rename, exports, names in subtitles
await invoke("rename_speaker", { id, speaker: 2, name: "Zira" });
const exp = async (ext) => { const p = join(out, `t.${ext}`); await invoke("export_to_path", { id, path: slash(p), stamps: false }); return readFileSync(p, ext === "docx" ? null : "utf8"); };
const txt = await exp("txt");
check("text: the name before each turn", txt.includes("Zira: ") && txt.includes("Speaker 1: "), txt.slice(0, 160));
check("Markdown: the name in bold", (await exp("md")).includes("**Zira:**"));
check("Word: a document", (await exp("docx")).subarray(0, 2).toString() === "PK");
check("CSV: a speaker column", (await exp("csv")).split("\r\n")[0] === "﻿start,end,speaker,text");
check("TSV: a speaker column", (await exp("tsv")).split("\n")[0] === "start\tend\tspeaker\ttext");
check("JSON: a speaker on each line", JSON.parse(await exp("json")).segments.every((s) => s.speaker));
check("WebVTT: the voice tag, on by default", (await exp("vtt")).includes("<v Zira>"));
check("SRT: no names unless asked", !(await exp("srt")).includes("Zira:"));
await invoke("set_subtitle_names", { srt: true, vtt: true });
check("SRT: the name where the speaker changes, when asked", (await exp("srt")).includes("Zira: "));

// ---------------------------------------------------------------- search within one speaker
await ev(`document.getElementById('search').value = 'the'; WHO.filter = 0; runSearch(false)`);
const all = await ev(`S.hits.length`);
await ev(`WHO.filter = 2; runSearch(false)`);
const zira = await ev(`S.hits.length`);
check("search within one speaker's lines", all > 0 && zira > 0 && zira < all, { all, zira });
await ev(`WHO.filter = 0; document.getElementById('search').value = ''; runSearch(false)`);

// ---------------------------------------------------------------- words and names
await invoke("set_words", { words: [{ write: "Dashboard", heard: ["dashboard"] }] });
tx = await invoke("apply_words", { id });
check("Apply my words", tx.segments.some((s) => s.text.includes("Dashboard")) && !tx.segments.some((s) => /\bdashboard\b/.test(s.text)));
tx = await invoke("always_write", { id, heard: "login", write: "Login" });
check("Always write it like this… adds the word and applies it", (await invoke("get_extras")).words.some((w) => w.write === "Login") && !tx.segments.some((s) => /\blogin\b/.test(s.text)));

// ---------------------------------------------------------------- merge, a line to someone new
tx = await invoke("merge_speakers", { id, from: 2, into: 1 });
check("merged: one speaker", new Set(tx.segments.map((s) => s.speaker).filter(Boolean)).size === 1);
const last = tx.segments.length - 1;
tx = await invoke("give_line", { id, index: last, speaker: null, start: tx.segments[last].start });
check("a line given to someone new", tx.segments[last].speaker === 2);
check("an edit of a line that moved is refused", await invoke("give_line", { id, index: last, speaker: 1, start: tx.segments[last].start + 1 }).then(() => false, () => true));

// ---------------------------------------------------------------- the subtitle look, sharp text
const fonts = await invoke("subtitle_fonts");
check("only fonts seen drawing Arabic right", fonts.includes("Segoe UI") && !fonts.includes("Arial"), fonts);
const look = { size: "large", font: "Tahoma", color: "yellow", back: "box", place: "top", sharp: true };
await invoke("set_look", { look });
const preview = await invoke("preview_look", { id, look });
check("the preview is one second of the user's own video, sharp", existsSync(preview) && /Duration: 00:00:01/.test(probe(preview)) && /\d+x1080/.test(probe(preview)));
const burned = join(out, "burned.mp4");
const bdone = await waitJob(await invoke("export_burned_to_path", { id, path: slash(burned) }), 300);
check("burned with sharp text: 480p comes out at 1080p", bdone?.status === "done" && /19\d\dx1080/.test(probe(burned)), bdone);

// ---------------------------------------------------------------- Find the speakers on a transcript without them
const [id2] = await invoke("enqueue", { paths: [slash(join(media, "two-voices.m4a"))], language: "en", steps: { transcribe: true } });
await waitJob(id2, 900);
let t2 = await invoke("get_transcript", { id: id2 });
check("made without speakers, it has none", !t2.segments.some((s) => s.speaker) && !t2.detected);
const fdone = await waitJob(await invoke("find_speakers", { id: id2 }), 300);
t2 = await invoke("get_transcript", { id: id2 });
check("Find the speakers gives it two", fdone?.status === "done" && new Set(t2.segments.map((s) => s.speaker).filter(Boolean)).size === 2, fdone);

// ---------------------------------------------------------------- the clicks, on a made-up meeting
writeFileSync(join(home, "history", "meeting-who.json"), JSON.stringify({
  id: "meeting-who", title: "اجتماع متطلبات التطبيق", source_path: "", created_ms: Date.now(), duration_s: 34, language: "ar",
  elapsed_s: 4, device: "", model: "made up", partial: false,
  segments: [
    [0, 6.1, 1, "طيب، خلونا نبدأ. اليوم عندنا ثلاث نقاط: الشاشة الرئيسية، تسجيل الدخول، والتقارير."],
    [6.1, 11.4, 1, "بالنسبة للشاشة الرئيسية، العميل طلب إن الـ dashboard يطلع أول شي بعد الدخول."],
    [11.4, 15.2, 2, "تمام، وأنا جهزت نسخة أولى من الـ design، أرسلها اليوم؟"],
    [15.2, 19.8, 2, "وكمان أضفت زر export للـ Excel في صفحة التقارير."],
    [19.8, 24.6, 1, "ممتاز. تسجيل الدخول لازم يدعم الـ OTP على الجوال قبل نهاية الشهر."],
  ].map(([start, end, speaker, text]) => ({ start, end, speaker, text, edited: false })),
  speakers: [{ id: 1, name: "سامي" }, { id: 2, name: "" }],
}));
await ev(`loadHistory()`);
await ev(`openTranscript('meeting-who')`);
await sleep(700);
await ev(`document.querySelectorAll('.seg .who')[1].click()`);
await sleep(200);
await ev(`[...document.querySelectorAll('.who-pop button')].find(b => b.textContent.startsWith('Rename')).click()`);
await sleep(200);
await ev(`(() => { const i = document.querySelector('.who-pop input'); i.value = 'Huda'; i.form.requestSubmit(); })()`);
await sleep(600);
check("renamed from the name's menu", (await ev(`[...document.querySelectorAll('.seg .who')].map(b => b.textContent)`)).filter((x) => x === "Huda").length === 1);
await ev(`document.querySelectorAll('#segments .seg')[1].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }))`);
await sleep(300);
const items = await ev(`[...document.querySelectorAll('#ctx-menu button')].filter(b => !b.hidden && b.offsetParent).map(b => b.textContent)`);
check("right-click on a line offers who said it", items.includes("Said by Huda") && items.includes("Said by someone new"), items);
await ev(`[...document.querySelectorAll('#ctx-who button')].find(b => b.textContent === 'Said by Huda').click()`);
await sleep(600);
check("the line is Huda's now", await ev(`S.current.segments[1].speaker === 2`));
await ev(`(() => { const el = document.querySelectorAll('#segments .seg .text')[1]; const node = el.firstChild; const i = node.textContent.indexOf('dashboard'); const r = document.createRange(); r.setStart(node, i); r.setEnd(node, i + 9); const sel = document.getSelection(); sel.removeAllRanges(); sel.addRange(r); const b = r.getBoundingClientRect(); el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: b.left + 5, clientY: b.bottom + 4 })); })()`);
await sleep(300);
check("a selection offers Always write it like this…", await ev(`!document.getElementById('ctx-always').hidden`));
await ev(`document.getElementById('ctx-always').click()`);
await sleep(300);
await ev(`(() => { const i = document.querySelector('.who-pop input'); i.value = 'Dashboard'; i.form.requestSubmit(); })()`);
await sleep(800);
check("the line says Dashboard now", (await ev(`S.current.segments[1].text`)).includes("Dashboard"));
await ev(`document.querySelectorAll('.seg .who')[0].click()`);
await sleep(200);
await ev(`[...document.querySelectorAll('.who-pop button')].find(b => b.textContent.startsWith('Same person')).click()`);
await sleep(600);
check("merged from the name's menu", await ev(`new Set(S.current.segments.map(s => s.speaker)).size === 1`));
check("the new controls have their tooltips", await ev(`['step-speakers','btn-more','look-font'].every(id => { const el = document.getElementById(id); return (el.closest('label') || el).title.length > 10 || el.title.length > 10; })`));

a.close();
finish();
