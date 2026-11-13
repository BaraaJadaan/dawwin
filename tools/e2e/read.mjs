// Read aloud with the stand-in voice (`cargo build --example fake_voice`): pasting, every file format,
// play and the first sound, the sentence marked, pause and resume, back and forward, a click to read
// from there, speed, the place remembered, mixed Arabic and English going to the right voices,
// numbers in words, a 300-page file, a transcript's Listen button and its translation, a voice that
// dies once and one that dies twice, a voice that needs the graphics card while transcription has it,
// the Arabic interface, and the voices in Settings with a sample to hear. Needs no model.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { APP, check, finish, makeHome, open, OUT, PORT, sleep } from "./lib.mjs";

const FAKE = join(dirname(APP), "examples", "fake_voice.exe");
if (!existsSync(FAKE)) {
  console.log("skipped: build the stand-in voice first (cargo build --example fake_voice)");
  process.exit(0);
}
const home = makeHome("read-home");
const files = join(home, "files");
mkdirSync(files, { recursive: true });
const voiceLog = join(home, "voice-requests.jsonl");

// a made-up transcript with a translation
const seg = (start, text) => ({ start, end: start + 3, text, edited: false });
writeFileSync(join(home, "history", "rd-1.json"), JSON.stringify({
  id: "rd-1", title: "اجتماع الفريق", source_path: "C:/made-up/meeting.m4a", created_ms: Date.now(), duration_s: 30, language: "ar",
  elapsed_s: 2, device: "CPU", model: "made up", partial: false,
  segments: [seg(0, "طيب خلينا نبدأ الاجتماع،"), seg(3.2, "عندنا ثلاث نقاط اليوم."), seg(10, "تمام، نتكلم الأسبوع الجاي.")],
  translations: [{ language: "en", model: "made up", lines: [
    { text: "Okay, let's start the meeting,", from: "طيب خلينا نبدأ الاجتماع،", edited: false },
    { text: "we have three points today.", from: "عندنا ثلاث نقاط اليوم.", edited: false },
    { text: "Fine, we'll talk next week.", from: "تمام، نتكلم الأسبوع الجاي.", edited: false },
  ] }],
}));

// a meeting with two speakers told apart (1.4), and its minutes (1.8)
const said = (start, speaker, text) => ({ start, end: start + 2.5, speaker, text, edited: false });
writeFileSync(join(home, "history", "rd-2.json"), JSON.stringify({
  id: "rd-2", title: "اجتماع المتحدثين", source_path: "C:/made-up/meeting2.m4a", created_ms: Date.now() - 1000, duration_s: 20, language: "ar",
  elapsed_s: 2, device: "CPU", model: "made up", partial: false,
  segments: [said(0, 1, "صباح الخير، نبدأ بالتقارير."), said(3, 2, "تمام، التقرير جاهز."), said(6, 1, "ممتاز، نرسله اليوم.")],
  speakers: [{ id: 1, name: "سامي" }, { id: 2, name: "" }],
  minutes: {
    language: "ar", model: "made up", written_ms: Date.now(), from: "x", kind: "meeting",
    summary: "اتفق الفريق على إرسال التقرير اليوم.",
    decisions: [{ text: "إرسال التقرير اليوم.", lines: [2], time: 6, edited: false }],
    actions: [{ who: "سامي", what: "يرسل التقرير", due: "اليوم", lines: [2], time: 6, edited: false }],
    topics: [], open_questions: [],
  },
}));

// files of every kind it reads
writeFileSync(join(files, "notes.md"), "# Plan\n\nRead [the guide](https://example.com) first.\n\n```js\nconsole.log('skip me')\n```\n\n- one item\n");
writeFileSync(join(files, "page.html"), "<html><head><title>A page</title><script>skip()</script></head><body><h1>Welcome</h1><p>Hello <a href='https://x.org'>there</a>.</p><img src='a.png'></body></html>");
writeFileSync(join(files, "film.srt"), "1\n00:00:01,000 --> 00:00:02,000\nFirst line.\n\n2\n00:00:02,500 --> 00:00:03,000\n<i>Second line.</i>\n");
writeFileSync(join(files, "talk.vtt"), "WEBVTT\n\n00:01.000 --> 00:02.000\n<v Sara>مرحبا بكم.\n");
// "مرحبا بكم. هذا ملف قديم." in Windows-1256
writeFileSync(join(files, "old-arabic.txt"), Buffer.from([0xE3, 0xD1, 0xCD, 0xC8, 0xC7, 0x20, 0xC8, 0xDF, 0xE3, 0x2E, 0x20, 0xE5, 0xD0, 0xC7, 0x20, 0xE3, 0xE1, 0xDD, 0x20, 0xDE, 0xCF, 0xED, 0xE3, 0x2E]));
// 300 pages: about 600,000 characters
const para = "هذه فقرة طويلة من كتاب كبير، فيها جمل كثيرة تشبه بعضها. وهي هنا لتجربة ملف من ثلاثمئة صفحة. ";
writeFileSync(join(files, "big-book.txt"), Array.from({ length: 3000 }, (_, i) => `${para}${para}\n\nالفقرة رقم ${i + 1}.\n\n`).join(""));

const port = PORT + 40;
const voiceEnv = (more = {}) => ({
  DAWWIN_TEST_VOICE: FAKE, FAKE_VOICE_DELAY_MS: "120", FAKE_VOICE_LOG: voiceLog,
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --mute-audio --autoplay-policy=no-user-gesture-required`, ...more,
});
let a = await open(home, port, voiceEnv());
const ev = (x) => a.ev(x);
const state = () => ev(`(() => { const R = window.__readAloud; return { i: R.i, playing: R.playing, paused: R.audio.paused, waiting: R.waiting, error: R.error, rate: R.audio.playbackRate, n: R.doc ? R.doc.sentences.length : 0, now: document.querySelector('.rs.now')?.dataset.i, title: R.doc?.title, start: R.startedAt && R.pressedAt ? R.startedAt - R.pressedAt : null }; })()`);
async function until(cond, timeout = 15000) {
  const end = Date.now() + timeout;
  let s;
  while (Date.now() < end) { s = await state(); if (cond(s)) return s; await sleep(100); }
  return s;
}
const requests = () => existsSync(voiceLog) ? readFileSync(voiceLog, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)).filter((r) => r.cmd === "speak") : [];
async function ready() {
  for (let i = 0; i < 120; i++) {
    try { if (await ev(`typeof showView === 'function' && typeof openReading === 'function' && !!window.__TAURI__`)) return; } catch { /* loading */ }
    await sleep(500);
  }
  throw new Error("the page did not load");
}
const openFile = (path) => ev(`openReading({ path: ${JSON.stringify(path)} }).then(() => window.__readAloud.doc && window.__readAloud.doc.sentences.map(s => s.text))`);
const paste = (text) => ev(`document.getElementById('read-text').value = ${JSON.stringify(text)}; document.getElementById('read-text-go').click()`);
async function again(more) {
  a.close();
  await sleep(1500);
  a = await open(home, port, voiceEnv(more));
  await ready();
  await ev(`showView('read')`);
}

await ready();
await ev(`showView('read')`);
check("the Read aloud page opens from the sidebar", await ev(`!document.getElementById('view-read').hidden && !!document.getElementById('read-drop')`));

// pasting
const PASTED = "رفعنا النسخة على GitHub أمس. وفي نهاية الاجتماع قال المدير: we will ship it next week, no matter what. ارتفعت الأسعار 15%. Short one.";
await paste(PASTED);
let s = await until((x) => x.n > 0);
check("pasted text is cut into sentences", s.n === 4, s);
check("the voices to choose show the stand-in", await ev(`[...document.querySelectorAll('#read-voices select')].map(x => x.options.length).join(',')`) === "2,2");

// play, first sound
await ev(`document.getElementById('read-play').click()`);
s = await until((x) => x.start !== null && !x.paused);
check("speech starts within two seconds of pressing play", s.start !== null && s.start < 2000, Math.round(s.start));
check("the sentence being read is marked", s.now === "0", s.now);
s = await until((x) => x.i >= 1, 20000);
check("it goes on to the next sentence by itself", s.i >= 1 && s.now === String(s.i), s);

// the right voice for each part
const mixed = requests().filter((r) => r.text.includes("ship") || r.text.includes("المدير"));
check("an English part inside Arabic goes to the English voice", mixed.some((r) => r.language === "en" && r.voice_id.startsWith("en") && r.text.startsWith("we will ship")) && mixed.some((r) => r.language === "ar" && r.text.includes("قال المدير")), mixed.map((r) => [r.language, r.voice_id, r.text]));
const github = requests().filter((r) => r.text.includes("GitHub"));
check("a single English word stays with the Arabic voice", github.length === 1 && github[0].language === "ar", github.map((r) => [r.language, r.text]));
s = await until((x) => x.i >= 2, 20000);
await sleep(600);
check("numbers are read in words in the language around them", requests().some((r) => r.text.includes("خمسة عشر بالمئة")), requests().map((r) => r.text));

// pause and resume
await ev(`document.getElementById('read-play').click()`);
s = await until((x) => x.paused);
const pausedAt = await ev(`window.__readAloud.audio.currentTime`);
check("pause stops the sound", s.paused && !s.playing, s);
await sleep(500);
check("it stays where it was paused", Math.abs((await ev(`window.__readAloud.audio.currentTime`)) - pausedAt) < 0.05);
await ev(`document.getElementById('read-play').click()`);
s = await until((x) => !x.paused);
check("play goes on from there", !s.paused && (s.i === 2 || s.i === 3), s);

// back and forward
const at = (await state()).i;
await ev(`document.getElementById('read-back').click()`);
s = await until((x) => x.i <= at && !x.waiting);
check("back starts the sentence again, or goes to the one before", s.i <= at, { at, s });
await ev(`window.__readAloud.audio.pause(); window.__readAloud.playing = true; readStep(-1)`);
s = await until((x) => !x.waiting && !x.paused);
await ev(`document.getElementById('read-forward').click()`);
const fw = await until((x) => !x.waiting && !x.paused);
check("forward goes to the next sentence", fw.i === s.i + 1, { before: s.i, after: fw.i });

// click a sentence
await ev(`document.querySelector('.rs[data-i="0"]').click()`);
s = await until((x) => x.i === 0 && !x.paused && !x.waiting);
check("clicking a sentence reads from there", s.i === 0 && s.now === "0", s);

// speed
await ev(`document.getElementById('read-speed').click()`);
check("the speed control changes the speed", (await state()).rate === 1.25, (await state()).rate);
for (let k = 0; k < 5; k++) await ev(`document.getElementById('read-speed').click()`);

// the place is remembered
await ev(`readFrom(2)`);
await until((x) => x.i === 2 && !x.waiting);
await ev(`closeReading()`);
await paste(PASTED);
s = await until((x) => x.n > 0);
check("opening it again starts where reading stopped", s.i === 2 && s.now === "2", s);

// every file format
const md = await openFile(join(files, "notes.md"));
check("Markdown: headings and lists read, code and link addresses not", JSON.stringify(md) === JSON.stringify(["Plan", "Read the guide first.", "one item"]), md);
const html = await openFile(join(files, "page.html"));
check("a web page: its text, without scripts or images", JSON.stringify(html) === JSON.stringify(["Welcome", "Hello there."]) && (await state()).title === "A page", html);
const srt = await openFile(join(files, "film.srt"));
check("SRT: without numbers or times", JSON.stringify(srt) === JSON.stringify(["First line.", "Second line."]), srt);
const vtt = await openFile(join(files, "talk.vtt"));
check("VTT: without times or tags", JSON.stringify(vtt) === JSON.stringify(["مرحبا بكم."]), vtt);
const old = await openFile(join(files, "old-arabic.txt"));
check("a text file in Windows-1256 Arabic", JSON.stringify(old) === JSON.stringify(["مرحبا بكم.", "هذا ملف قديم."]), old);
await a.invoke("export_to_path", { id: "rd-1", path: join(files, "meeting.docx"), stamps: false, view: null, language: null });
const docx = await openFile(join(files, "meeting.docx"));
check("a Word document (Dawwin's own export)", Array.isArray(docx) && docx.join(" ").includes("خلينا نبدأ الاجتماع"), docx);
// timed inside the page: what the reader waits for, not this test copying 15,000 sentences back
const { openMs, count } = await ev(`(async () => { const t0 = performance.now(); await openReading({ path: ${JSON.stringify(join(files, "big-book.txt"))} }); return { openMs: Math.round(performance.now() - t0), count: window.__readAloud.doc.sentences.length }; })()`);
await ev(`document.getElementById('read-play').click()`);
s = await until((x) => x.start !== null && !x.paused);
check("in a 300-page file, speech starts within two seconds of pressing play", s.start < 2000 && count > 5000, { startMs: Math.round(s.start), sentences: count });
check("a 300-page file opens within three seconds (a debug build)", openMs < 3000, { openMs });
await ev(`stopReading()`);

// a transcript's Listen button, and its translation
await ev(`openTranscript('rd-1')`);
await sleep(800);
await ev(`document.getElementById('btn-listen').click()`);
s = await until((x) => x.n > 0 && x.title === "اجتماع الفريق");
check("Listen on a transcript reads its lines", s.n >= 2 && await ev(`window.__readAloud.doc.sentences[0].text`) === "طيب خلينا نبدأ الاجتماع، عندنا ثلاث نقاط اليوم.", await ev(`window.__readAloud.doc.sentences.map(s=>s.text)`));
await ev(`openTranscript('rd-1')`);
await sleep(500);
await ev(`showTranslation('en')`);
await sleep(500);
await ev(`document.getElementById('btn-listen').click()`);
await until((x) => x.n > 0 && x.title === "اجتماع الفريق");
await sleep(300);
const tr = await ev(`window.__readAloud.doc.sentences.map(s=>s.text)`);
check("Listen with a translation shown reads the translation", tr[0] === "Okay, let's start the meeting, we have three points today.", tr);
await ev(`stopReading()`);

// each speaker in a voice of their own, with their name where they start
await ev(`openTranscript('rd-2')`);
await sleep(800);
await ev(`document.getElementById('btn-listen').click()`);
s = await until((x) => x.n > 0 && x.title === "اجتماع المتحدثين");
const who = await ev(`[...document.querySelectorAll('#read-body .read-who')].map(e => e.textContent)`);
check("a transcript's speakers are named where each starts to speak", JSON.stringify(who) === JSON.stringify(["سامي", "Speaker 2", "سامي"]), who);
const before = requests().length;
await ev(`readFrom(0)`);
s = await until((x) => x.i === 2 && !x.waiting && !x.paused, 20000);
await sleep(400);
const spoken = requests().slice(before);
const voiceOf = (bit) => spoken.find((r) => r.text.includes(bit))?.voice_id;
check("each speaker reads in a voice of their own, the same one each time", voiceOf("صباح الخير") === voiceOf("ممتاز") && voiceOf("التقرير جاهز") && voiceOf("التقرير جاهز") !== voiceOf("صباح الخير"), spoken.map((r) => [r.voice_id, r.text]));
await ev(`stopReading()`);

// the minutes read aloud from their tab
await ev(`openTranscript('rd-2')`);
await sleep(800);
await ev(`showMinutesTab('minutes')`);
await sleep(500);
const listenMinutes = await ev(`(() => { const b = [...document.querySelectorAll('#minutes .minutes-bar button')].find(b => b.textContent === t('read.listen')); if (!b) return 'none'; b.click(); return 'clicked'; })()`);
s = await until((x) => x.n > 0 && x.title === "اجتماع المتحدثين" && x.n !== 3);
const minutesText = await ev(`window.__readAloud.doc.sentences.map(s => s.text)`);
check("Listen on the minutes reads them, heading by heading", listenMinutes !== "none" && minutesText.includes("اتفق الفريق على إرسال التقرير اليوم.") && minutesText.some((x) => x.includes("يرسل التقرير")), minutesText);
await ev(`showMinutesTab('lines')`);
await ev(`stopReading()`);

// a voice that dies once is started again without a gap
await again({ FAKE_VOICE_CRASH_AFTER: "2" });
await paste("One more. And two. Then three. Now four.");
await until((x) => x.n > 0);
await ev(`readFrom(0)`);
s = await until((x) => x.i === 3 || !!x.error, 30000);
check("a voice that dies once is started again, and reading goes on", s.i === 3 && !s.error, s);
await ev(`stopReading()`);

// a voice that dies twice running: told, not silence, and trying again works
const crashes = join(home, "crashes.txt");
rmSync(crashes, { force: true });
await again({ FAKE_VOICE_CRASH_TIMES: "2", FAKE_VOICE_CRASH_FILE: crashes });
await paste("One. Two. Three. Four. Five. Six.");
await until((x) => x.n > 0);
await ev(`readFrom(0)`);
s = await until((x) => !!x.error, 25000);
check("a voice that dies mid-reading is told, not left silent", !!s.error && !(await ev(`document.getElementById('read-error').hidden`)), s.error);
await ev(`document.querySelector('#read-error button').click()`);
s = await until((x) => !x.error && !x.paused && !x.waiting, 15000);
check("Try again reads on with a new voice process", !s.error && !s.paused, s);
await ev(`stopReading()`);

// a voice that needs the graphics card waits while transcription has it, and says so
await again({ DAWWIN_TEST_GPU_BUSY: "1" });
await paste("One more. And two.");
await until((x) => x.n > 0);
await ev(`readFrom(0)`);
s = await until((x) => !!x.error, 15000);
check("a voice that needs the graphics card says it waits while Dawwin transcribes", /waits for the graphics card/.test(s.error || ""), s.error);
await ev(`stopReading()`);

// the Arabic interface, and Settings
await ev(`setUiLang('ar')`);
await sleep(500);
await paste(PASTED);
await until((x) => x.n > 0);
check("the Arabic interface runs right to left, with Arabic words", await ev(`document.documentElement.dir === 'rtl' && document.getElementById('btn-read').textContent.includes('القراءة')`));
await a.shot(join(OUT, "read-doc-ar.png"));
await ev(`showView('settings')`);
await sleep(800);
check("Settings has the three voices, each with what it reads", await ev(`[...document.querySelectorAll('#set-reading .setting h3')].map(h => h.firstChild.textContent).join(',')`) === "VoxCPM 2,Supertonic 3,Phonon 31M");
await ev(`document.querySelector('#set-reading .setting button').click()`);
await sleep(800);
check("a voice can be heard before it is downloaded", await ev(`!!window.__readAloud.sample && !window.__readAloud.sample.paused && !window.__readAloud.sample.error`), await ev(`window.__readAloud.sample && [window.__readAloud.sample.src, window.__readAloud.sample.error && window.__readAloud.sample.error.code]`));
await a.shot(join(OUT, "read-settings-ar.png"));

a.close();
finish();
