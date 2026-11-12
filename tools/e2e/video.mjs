// Videos with subtitles, in the real app, three times over. With Dawwin's small ffmpeg: a meeting
// video transcribed on the GPU, translated by the stand-in translator, and saved with its subtitles
// as tracks, English first, the picture and the sound copied (and an MKV with SRT tracks); a sound
// recording offers no video. As on a computer with no ffmpeg and no translator: each step and
// export that needs one offers its download, and the app refuses without it. With Dawwin's ffmpeg
// for video: a video transcribed with "burned in", a translated transcript burned in from the
// export menu in both languages, and one stopped half way, which leaves nothing behind.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, rmSync, truncateSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, FAKE_TRANSLATOR, FFMPEG, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);
need("the stand-in translator (cargo build --example fake_translator)", existsSync(FAKE_TRANSLATOR));

/** What the full ffmpeg says of a file: its streams and its length. */
const probe = (file) => { try { execFileSync(FFMPEG, ["-hide_banner", "-i", file], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }); return ""; } catch (e) { return String(e.stderr); } };
/** A stand-in for the smallest translation model: its name and size, and a GGUF header. */
function translationModel(home) {
  const f = join(home, "models", "MiLMMT-46-1B-v1.0.Q4_K_M.gguf");
  writeFileSync(f, Buffer.from("GGUF\x03\0\0\0", "binary"));
  execFileSync("fsutil", ["sparse", "setflag", f], { stdio: "ignore" });
  truncateSync(f, 806057408);
}
const ACTIVE = ["queued", "decoding", "loading", "transcribing", "timing", "translating", "saving_audio", "saving_video"];
const toast = (a) => a.ev(`document.getElementById('toast').hidden ? '' : document.getElementById('toast').innerText`);
const translateJobs = (a) => a.ev(`S.snap.queue.filter(j=>j.kind==='translate'&&['queued','loading','translating'].includes(j.status)).length`);
async function translated(a, id) {
  await a.ev(`openTranscript(${JSON.stringify(id)})`);
  await a.ev(`(async()=>{const m=document.getElementById('t-lang');m.value='en';m.dispatchEvent(new Event('change'));})()`);
  for (let i = 0; i < 240; i++) { await sleep(250); if (!(await translateJobs(a))) break; }
  await sleep(800);
}

// ---- the recordings: an Arabic meeting of the voices of Windows, in a quiet room, on a test picture
const voices = join(OUT, "voices");
if (!existsSync(join(voices, "meeting-ar.wav"))) {
  execFileSync("pwsh", ["-NoProfile", "-File", join(REPO, "tools", "timing", "speak.ps1"), "-Out", voices], { stdio: "inherit" });
}
const work = join(OUT, "video-files");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const room = join(work, "room.wav");
ffmpeg(["-i", join(voices, "meeting-ar.wav"), "-f", "lavfi", "-i", "anoisesrc=c=pink:a=0.003:r=16000:seed=7", "-filter_complex", "[0:a][1:a]amix=inputs=2:duration=first:normalize=0", room]);
const meeting = join(work, "meeting.mp4");
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=640x360:rate=24", "-i", room, "-t", "90", "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", meeting]);
const sound = join(work, "meeting.m4a");
ffmpeg(["-i", room, "-t", "90", "-c:a", "aac", sound]);
const burnDir = join(work, "burn");
mkdirSync(burnDir);
const hd = join(burnDir, "meeting.mp4");
ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30", "-i", room, "-t", "40", "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", hd]);
check("a 90 s test video and a 40 s one in 720p are made", existsSync(meeting) && existsSync(hd));

// ================================================================ subtitle tracks, the small ffmpeg
{
  const home = makeHome("video-tracks-home");
  rmSync(join(home, "tools", "ffmpeg-video"), { recursive: true, force: true });
  translationModel(home);
  const a = await open(home, PORT + 22, { DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_TRANSLATOR_DELAY_MS: "100" });
  const st = (await a.state()).setup;
  check("Dawwin's small ffmpeg is the one in use", /ffmpeg-audio[\\/]ffmpeg\.exe$/.test(st.ffmpeg || "") && st.ffmpeg.includes("video-tracks-home"), st.ffmpeg);

  // ---- transcribe the video
  const id = await a.ev(`(async()=>{ await addFiles([${JSON.stringify(slash(meeting))}]); await new Promise(r=>setTimeout(r,300)); const s = await window.__TAURI__.core.invoke('get_state'); return s.queue.find(j=>!j.kind)?.id; })()`);
  check("the video is transcribed", !!id && (await a.idle(600)));
  const t = await a.invoke("get_transcript", { id });
  check("its sound is transcribed, in Arabic", t.segments?.length >= 3 && /[؀-ۿ]{3}/.test(t.segments.map((s) => s.text).join(" ")), t.segments?.length);
  check("from 90 s of video", Math.abs(t.duration_s - 90) < 1.5, t.duration_s);

  // ---- translate it, then the export menu offers the video
  await translated(a, id);
  await a.ev(`document.getElementById('btn-export').click()`);
  await sleep(200);
  check("the export menu offers the video with subtitles", (await a.ev(`!document.getElementById('export-video').hidden`)) === true);
  await a.shot(join(OUT, "video-export.png"));
  await a.ev(`document.body.click()`);

  // ---- the words placed in time first (subtitles ask for it), then both languages into the video
  await a.invoke("fix_timing", { id });
  await a.idle(300);
  check("the words are placed in time", (await a.invoke("get_transcript", { id })).timed === true);
  const out = join(work, "meeting (English, Arabic).mp4");
  const started = Date.now();
  await a.invoke("export_video_to_path", { id, path: slash(out), view: "both", language: "en" });
  const took = (Date.now() - started) / 1000;
  check("the video with subtitles is saved", existsSync(out), took);
  const info = probe(out);
  const subs = info.split("\n").filter((l) => /Stream #0:\d+(\[\w+\])?\((\w+)\).*Subtitle: mov_text/.test(l));
  check("it has two subtitle tracks, English first, then Arabic", subs.length === 2 && subs[0].includes("(eng)") && subs[1].includes("(ara)"), subs.map((l) => l.trim()));
  check("the picture and the sound are copied, not made again", /Video: h264/.test(info) && /Audio: aac/.test(info));
  check("in seconds", took < 20, took);
  const en = execFileSync(FFMPEG, ["-hide_banner", "-loglevel", "error", "-i", out, "-map", "0:s:0", "-f", "srt", "-"], { encoding: "utf8" });
  check("the English track holds the translation", en.includes("[English]"), en.slice(0, 160));

  // ---- an MKV keeps every stream it has, and gets SRT tracks
  const mkv = join(work, "meeting.mkv");
  ffmpeg(["-i", meeting, "-c", "copy", mkv]);
  const tt = await a.invoke("get_transcript", { id });
  writeFileSync(join(home, "history", "vid-mkv.json"), JSON.stringify({ ...tt, id: "vid-mkv", source_path: mkv }));
  const outMkv = join(work, "meeting (English).mkv");
  await a.invoke("export_video_to_path", { id: "vid-mkv", path: slash(outMkv), view: "translation", language: "en" });
  const mk = probe(outMkv);
  check("an MKV gets an SRT track in English", /Stream #0:\d+(\[\w+\])?\(eng\).*Subtitle: subrip/.test(mk), mk.split("\n").filter((l) => l.includes("Stream")).map((l) => l.trim()));

  // ---- a recording that is sound only has no such item
  writeFileSync(join(home, "history", "aud-1.json"), JSON.stringify({ ...t, id: "aud-1", source_path: sound, translations: [] }));
  await a.ev(`loadHistory()`);
  await a.ev(`openTranscript('aud-1')`);
  await a.ev(`document.getElementById('btn-export').click()`);
  await sleep(200);
  check("a sound recording offers no video", (await a.ev(`document.getElementById('export-video').hidden && document.getElementById('export-burn').hidden`)) === true);
  await a.ev(`document.body.click()`);
  const refused = await a.ev(`window.__TAURI__.core.invoke('export_video_to_path',{id:'aud-1',path:'x.mp4'}).then(()=>'saved',e=>String(e))`);
  check("and refuses to make one", refused === "That recording is not a video.", refused);
  a.close();
}

// ================================================================ nothing installed
{
  const home = makeHome("video-none-home");
  rmSync(join(home, "tools"), { recursive: true, force: true });
  translationModel(home);
  const a = await open(home, PORT + 23, { DAWWIN_TEST_NO_FFMPEG: "1" });
  const st = (await a.state()).setup;
  check("no ffmpeg and no translator here", !st.ffmpeg && !st.burner && !st.translator_found, { ffmpeg: st.ffmpeg, burner: st.burner });
  check("the tools to download, none on the disk (with 1.7, the voice program and its GPU library; with 1.8, PDFium)", st.tools.map((x) => x.id).join() === "translator,ffmpeg-audio,ffmpeg-video,voice,voice-gpu,pdfium" && st.tools.every((x) => !x.on_disk), st.tools.map((x) => [x.id, x.on_disk]));

  await a.ev(`showView('settings')`);
  await sleep(400);
  const rows = await a.ev(`[...document.querySelectorAll('#set-tools .setting, #set-translator-tool .setting')].map(r=>r.innerText.replace(/\\s+/g,' ').trim())`);
  check("Settings has a row for each tool", rows.length === 3, rows);
  check("each says it is not downloaded, with a Download button", rows.every((r) => /Not downloaded/.test(r) && /Download/.test(r)), rows);
  check("the line about reading formats says to download ffmpeg", /download ffmpeg/.test(await a.ev(`document.getElementById('ffmpeg-detail').innerText`)));
  await a.ev(`document.getElementById('set-tools').scrollIntoView({block:'center'})`);
  await sleep(300);
  await a.shot(join(OUT, "video-tools-none.png"));

  await a.ev(`showView('home')`);
  await a.ev(`setStep('video', true); document.getElementById('step-video-how').value='burn'; document.getElementById('step-video-how').dispatchEvent(new Event('change'))`);
  await sleep(200);
  check("the video step offers burning in", (await a.ev(`!document.getElementById('step-video-how').disabled && S.steps.burn`)) === true);
  await a.ev(`stepsReady(stepsForJob())`);
  let tt = await toast(a);
  check("starting it offers ffmpeg for video, with its size", /Burning subtitles in needs ffmpeg for video, a [\d.]+ MB download\./.test(tt) && /Download/.test(tt), tt);
  await a.ev(`document.getElementById('step-video-how').value='tracks'; document.getElementById('step-video-how').dispatchEvent(new Event('change'))`);
  await a.ev(`stepsReady(stepsForJob())`);
  tt = await toast(a);
  check("as tracks, it offers the small ffmpeg", /Subtitles in a video need ffmpeg, a [\d.]+ MB download\./.test(tt), tt);
  await a.ev(`setStep('video', false); setStep('translate', true)`);
  await a.ev(`stepsReady(stepsForJob())`);
  tt = await toast(a);
  check("translating offers the translator", /Translating needs the translator, a [\d.]+ MB download\./.test(tt), tt);
  await a.ev(`setStep('translate', false)`);

  // a transcript of a video: both video items, each offering what it needs
  writeFileSync(join(home, "history", "vid-1.json"), JSON.stringify({
    id: "vid-1", title: "meeting", source_path: hd, created_ms: Date.now(), duration_s: 40, language: "ar", elapsed_s: 1, device: "CPU", model: "x", partial: false, timed: true,
    segments: [{ start: 1, end: 4, text: "طيب، خلونا نبدأ الاجتماع: عندنا demo اليوم." }], translations: [],
  }));
  await a.ev(`loadHistory()`);
  await a.ev(`openTranscript('vid-1')`);
  await sleep(300);
  await a.ev(`document.getElementById('btn-export').click()`);
  await sleep(200);
  check("the export menu has both video items", (await a.ev(`!document.getElementById('export-video').hidden && !document.getElementById('export-burn').hidden`)) === true);
  await a.ev(`document.getElementById('export-burn').click()`);
  await sleep(200);
  tt = await toast(a);
  check("burning in from the menu offers ffmpeg for video", /needs ffmpeg for video/.test(tt), tt);
  await a.shot(join(OUT, "video-offer-toast.png"));
  const refused = await a.ev(`window.__TAURI__.core.invoke('export_burned_to_path',{id:'vid-1',path:'x.mp4'}).then(()=>'started',e=>String(e))`);
  check("and the app refuses it without one", refused === "Burning subtitles in needs the video ffmpeg. Download it in Settings.", refused);
  const tracks = await a.ev(`window.__TAURI__.core.invoke('export_video_to_path',{id:'vid-1',path:'x.mp4'}).then(()=>'saved',e=>String(e))`);
  check("and tracks too", tracks === "Adding subtitles to a video needs ffmpeg. Download it in Settings.", tracks);
  const tr = await a.ev(`window.__TAURI__.core.invoke('translate',{id:'vid-1',language:'en'}).then(()=>'started',e=>String(e))`);
  check("translating without the translator says where to get it", tr === "The translator is not downloaded. Download it in Settings.", tr);
  a.close();
}

// ================================================================ burned in, Dawwin's ffmpeg for video
{
  const home = makeHome("video-burn-home");
  translationModel(home);
  const a = await open(home, PORT + 24, { DAWWIN_TEST_TRANSLATOR: FAKE_TRANSLATOR, FAKE_TRANSLATOR_DELAY_MS: "20" });
  const st = (await a.state()).setup;
  check("Dawwin's ffmpeg for video is in use, and it burns", /ffmpeg-video[\\/]ffmpeg\.exe$/.test(st.burner || "") && st.burner === st.ffmpeg && st.ffmpeg.includes("video-burn-home"), st.burner);

  // the steps: transcribe, then burn the transcript in
  await a.ev(`S.steps = {...DEFAULT_STEPS, transcribe: true, video: true, burn: true}; paintSteps()`);
  const id = await a.ev(`(async()=>{ await addFiles([${JSON.stringify(slash(hd))}]); await new Promise(r=>setTimeout(r,300)); const s = await window.__TAURI__.core.invoke('get_state'); return s.queue.find(j=>!j.kind)?.id; })()`);
  let sawBurning = null, j = null;
  for (let i = 0; i < 2400; i++) {
    j = await a.ev(`window.__TAURI__.core.invoke('get_state').then(s=>s.queue.find(j=>j.id===${JSON.stringify(id)})||null)`);
    if (j?.status === "saving_video" && j.progress > 0 && j.progress < 1 && !sawBurning) {
      sawBurning = { progress: j.progress, line: await a.ev(`[...document.querySelectorAll('#queue li')].map(l=>l.innerText.replace(/\\s+/g,' ')).join(' / ')`) };
      await a.shot(join(OUT, "video-burn-progress.png"));
    }
    if (!j || !ACTIVE.includes(j.status)) break;
    await sleep(250);
  }
  // the interface moves a finished job into History at once: gone after being seen is done
  check("the job ends done", !j || j.status === "done", j && `${j.status} ${j.error || ""}`);
  check("the list shows how far burning is", !!sawBurning && /Burning the subtitles in/.test(sawBurning.line) && /\d+%/.test(sawBurning.line), sawBurning);
  const burned = join(burnDir, "meeting (Arabic, burned in).mp4");
  check("the video is saved next to the original", existsSync(burned), readdirSync(burnDir));
  const info = probe(burned);
  // a 720p picture is enlarged to 1080p before the text is drawn: Sharp text on small videos, on by default (1.4)
  check("an H.264 MP4 of the same length, enlarged to 1080p for sharp text, with no subtitle track", /Video: h264/.test(info) && /1920x1080/.test(info) && /Duration: 00:00:(39|40)/.test(info) && !/Subtitle:/.test(info), info.split("\n").filter((l) => /Duration|Stream/.test(l)).map((l) => l.trim()));
  if (existsSync(burned)) ffmpeg(["-ss", "8", "-i", burned, "-frames:v", "1", join(OUT, "video-burn-frame-steps.png")]);

  // the export menu: translated, both languages burned in
  await translated(a, id);
  const both = join(burnDir, "meeting both.mp4");
  const jobId = await a.invoke("export_burned_to_path", { id, path: slash(both), view: "both", language: "en" });
  let last, savedToast = "";
  for (let i = 0; i < 1200; i++) {
    last = await a.ev(`window.__TAURI__.core.invoke('get_state').then(s=>s.queue.find(j=>j.id===${JSON.stringify(jobId)})||null)`);
    const tt = await toast(a);
    if (/Saved/.test(tt || "")) savedToast = tt;
    if (!last || last.status !== "saving_video") break;
    await sleep(250);
  }
  for (let i = 0; i < 20 && !savedToast; i++) { const tt = await toast(a); if (/Saved/.test(tt || "")) savedToast = tt; await sleep(100); }
  check("the burn job from the menu ends done, with its file", (!last || last.status === "done") && existsSync(both), last && { status: last.status, error: last.error, outputs: last.outputs });
  if (existsSync(both)) ffmpeg(["-ss", "8", "-i", both, "-frames:v", "1", join(OUT, "video-burn-frame-both.png")]);
  check("a toast says it is saved", /Saved/.test(savedToast), savedToast);

  // stopped in the middle: nothing left behind
  const stopped = join(burnDir, "stopped.mp4");
  const sid = await a.invoke("export_burned_to_path", { id, path: slash(stopped), view: "original" });
  for (let i = 0; i < 100; i++) {
    const p = await a.ev(`S.snap.queue.find(j=>j.id===${JSON.stringify(sid)})?.progress||0`);
    if (p > 0.05) break;
    await sleep(100);
  }
  await a.ev(`[...document.querySelectorAll('#queue li')].find(l=>/Burning/.test(l.innerText))?.querySelector('.x')?.click()`);
  for (let i = 0; i < 60; i++) {
    last = await a.ev(`S.snap.queue.find(j=>j.id===${JSON.stringify(sid)})||null`);
    if (last && last.status !== "saving_video") break;
    await sleep(200);
  }
  check("stopping it from the list cancels it", last?.status === "cancelled", last?.status);
  await sleep(500);
  check("and leaves no file", !existsSync(stopped) && !existsSync(join(burnDir, "stopped.part.mp4")), readdirSync(burnDir));
  a.close();
}
finish();
