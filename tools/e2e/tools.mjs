// The downloads of the tools, in the real app, from a local server standing in for the release and
// for Hugging Face (DAWWIN_TOOLS_URL, HF_ENDPOINT): nothing reaches the internet. First, every file
// checked against its pinned SHA-256: the small ffmpeg from Settings (paused and resumed), ffmpeg
// for video from the offer burning in makes, burning in with it, deleting it, and a damaged
// download. Then the first translation model, which brings the translator along before it (the
// model a stand-in, so the downloads are trusted as they come).
//
// The ffmpegs served are the release's own .tar.xz files, from DAWWIN_E2E_TOOLS. So is the
// translator when its .tar.xz is there; else a stand-in pack of the same name and size, with the
// stand-in translator (src-tauri/examples/fake_translator.rs) as its llama-server.exe.
import { execFileSync, spawnSync } from "node:child_process";
import { createReadStream, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, truncateSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { check, FAKE_TRANSLATOR, FFMPEG, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL); // only so that the app has a model to name: nothing is transcribed
need("DAWWIN_E2E_TOOLS", TOOLS);
const packs = Object.fromEntries(readdirSync(TOOLS).filter((f) => /^dawwin-.*\.tar\.xz$/.test(f)).map((f) => [f, join(TOOLS, f)]));
const packOf = (id) => Object.keys(packs).find((f) => f.startsWith(`dawwin-${id === "translator" ? "translator" : id}-`));
need("the release's dawwin-ffmpeg-audio-*.tar.xz and dawwin-ffmpeg-video-*.tar.xz in DAWWIN_E2E_TOOLS", packOf("ffmpeg-audio") && packOf("ffmpeg-video"));
const realTranslator = !!packOf("translator");
need("the stand-in translator (cargo build --example fake_translator)", realTranslator || existsSync(FAKE_TRANSLATOR));

const work = join(OUT, "tools-files");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });

// ---- a stand-in for the translator's pack: a .tar.xz of exactly the size the app expects, which
// it unpacks as it would the real one. Its tar is followed by zeros, and the xz holds it as
// stored chunks, more of them to make up the last few bytes.
const CRC = new Int32Array(256).map((_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c; });
const crc32 = (buf) => { let c = -1; for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ -1) >>> 0; };
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n >>> 0); return b; };
const varint = (n) => { const out = []; while (n >= 0x80) { out.push((n % 128) | 0x80); n = Math.floor(n / 128); } out.push(n); return Buffer.from(out); };
function tarEntry(name, data) {
  const h = Buffer.alloc(512);
  h.write(name, 0);
  h.write("0000755\0", 100);
  h.write("0000000\0", 108);
  h.write("0000000\0", 116);
  h.write(data.length.toString(8).padStart(11, "0") + "\0", 124);
  h.write("00000000000\0", 136);
  h.write("        ", 148);
  h.write("0", 156);
  h.write("ustar\0", 257);
  h.write("00", 263);
  let sum = 0;
  for (const b of h) sum += b;
  h.write(sum.toString(8).padStart(6, "0") + "\0 ", 148);
  return Buffer.concat([h, data, Buffer.alloc((512 - (data.length % 512)) % 512)]);
}
function standInPack(size) {
  const dlls = ["llama-server-impl.dll", "llama-common.dll", "llama.dll", "mtmd.dll", "libomp.dll", "ggml.dll", "ggml-base.dll", "ggml-vulkan.dll", "ggml-cpu-x64.dll", "ggml-cpu-sse42.dll",
    "ggml-cpu-sandybridge.dll", "ggml-cpu-ivybridge.dll", "ggml-cpu-piledriver.dll", "ggml-cpu-haswell.dll", "ggml-cpu-skylakex.dll", "ggml-cpu-cannonlake.dll", "ggml-cpu-cascadelake.dll",
    "ggml-cpu-icelake.dll", "ggml-cpu-cooperlake.dll", "ggml-cpu-alderlake.dll", "ggml-cpu-sapphirerapids.dll", "ggml-cpu-zen4.dll"];
  const tar = Buffer.concat([
    tarEntry("llama-server.exe", readFileSync(FAKE_TRANSLATOR)),
    ...dlls.map((d) => tarEntry(d, Buffer.from(`a stand-in for ${d}\n`))),
    tarEntry("LICENSE-llama.cpp.txt", Buffer.from("A stand-in for llama.cpp's license, for a test.\n")),
    Buffer.alloc(1024),
  ]);
  const flags = Buffer.from([0, 1]); // the check is a CRC-32
  const streamHeader = Buffer.concat([Buffer.from([0xfd, 0x37, 0x7a, 0x58, 0x5a, 0]), flags, u32(crc32(flags))]);
  const bh = Buffer.from([2, 0, 0x21, 1, 24, 0, 0, 0]); // 12 bytes; one filter, LZMA2, a 16 MiB dictionary
  const blockHeader = Buffer.concat([bh, u32(crc32(bh))]);
  // all but the chunks: stream header 12, block header 12, check 4, index 16, footer 12
  const room = size - 56;
  const chunksFor = (t) => Math.ceil(t / 65536);
  const T = Math.floor((room - 1 - 3 * chunksFor(room)) / 512) * 512;
  const r = room - (T + 3 * chunksFor(T) + 1);
  const k = chunksFor(T) + Math.floor(r / 3), pad = r % 3;
  const data = Buffer.concat([tar, Buffer.alloc(T - tar.length)]);
  const chunks = [];
  for (let i = 0, at = 0; i < k; i++) {
    const n = Math.floor(T / k) + (i < T % k ? 1 : 0);
    chunks.push(Buffer.from([i ? 2 : 1, (n - 1) >> 8, (n - 1) & 0xff]), data.subarray(at, at + n));
    at += n;
  }
  const compressed = Buffer.concat([...chunks, Buffer.from([0])]);
  const unpadded = blockHeader.length + compressed.length + 4;
  let index = Buffer.concat([Buffer.from([0]), varint(1), varint(unpadded), varint(T)]);
  index = Buffer.concat([index, Buffer.alloc((4 - (index.length % 4)) % 4)]);
  index = Buffer.concat([index, u32(crc32(index))]);
  const backward = u32(index.length / 4 - 1);
  const footer = Buffer.concat([u32(crc32(Buffer.concat([backward, flags]))), backward, flags, Buffer.from("YZ")]);
  const xz = Buffer.concat([streamHeader, blockHeader, compressed, Buffer.alloc(pad), u32(crc32(data)), index, footer]);
  if (xz.length !== size || (blockHeader.length + compressed.length + pad) % 4) throw new Error(`the stand-in pack came out ${xz.length} bytes, not ${size}`);
  return xz;
}

// ---- the stand-in server: ranges, a set speed, and a file served damaged on request
const fakeModel = join(work, "MiLMMT-46-1B-v1.0.Q4_K_M.gguf");
writeFileSync(fakeModel, Buffer.from("GGUF\x03\0\0\0", "binary"));
execFileSync("fsutil", ["sparse", "setflag", fakeModel], { stdio: "ignore" });
truncateSync(fakeModel, 806057408);
const damaged = new Set();
const served = [];
let bytesPerSecond = 3e6;
let translatorPack = null; // made once the app says how large it is
const server = createServer((req, res) => {
  const name = decodeURIComponent(req.url.split("/").pop());
  served.push(name);
  const path = name.endsWith(".gguf") ? fakeModel : packs[name] || (/^dawwin-translator-.*\.tar\.xz$/.test(name) ? translatorPack : null);
  if (!path) { res.writeHead(404); res.end(); return; }
  const size = statSync(path).size;
  const m = /bytes=(\d+)-/.exec(req.headers.range || "");
  const from = m ? Number(m[1]) : 0;
  res.writeHead(m ? 206 : 200, { "Content-Length": size - from, ...(m ? { "Content-Range": `bytes ${from}-${size - 1}/${size}` } : {}) });
  if (name.endsWith(".gguf")) { createReadStream(path, { start: from }).pipe(res); return; }
  const data = readFileSync(path).subarray(from);
  if (damaged.has(name)) data[data.length - 100] ^= 0xff;
  let at = 0;
  const tick = setInterval(() => {
    const n = Math.min(data.length - at, Math.ceil(bytesPerSecond / 10));
    res.write(data.subarray(at, at + n));
    at += n;
    if (at >= data.length) { clearInterval(tick); res.end(); }
  }, 100);
  res.on("close", () => clearInterval(tick));
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

/** The app on a data folder with no tools, its downloads from the stand-in server. */
async function start(name, port, env = {}) {
  const home = makeHome(name);
  rmSync(join(home, "tools"), { recursive: true, force: true });
  return { home, a: await open(home, port, { DAWWIN_TEST_NO_FFMPEG: "1", DAWWIN_TOOLS_URL: `${base}/tools`, HF_ENDPOINT: base, ...env }) };
}
/** What toasts said, from here on. */
const hearToasts = (a) => a.ev(`window.__toasts = []; { const said = toast; toast = (m, x) => { window.__toasts.push(m); said(m, x); }; } 1`);
const row = (a, id) => a.ev(`(document.querySelector('[data-id="${id}"]')?.innerText||'').replace(/\\s+/g,' ').trim()`);
async function until(a, expr, ms = 120000) {
  for (let i = 0; i < ms / 200; i++) { if (await a.ev(expr)) return true; await sleep(200); }
  return false;
}

// ================================================================ checked downloads
{
  const video = join(work, "meeting.mp4");
  ffmpeg(["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30:duration=12", "-f", "lavfi", "-i", "sine=frequency=300:duration=12", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", video]);
  const { home, a } = await start("tools-checked-home", PORT + 25);
  writeFileSync(join(home, "history", "vid-1.json"), JSON.stringify({
    id: "vid-1", title: "meeting", source_path: video, created_ms: Date.now(), duration_s: 12, language: "ar", elapsed_s: 1, device: "CPU", model: "x", partial: false, timed: true,
    segments: [{ start: 1, end: 6, text: "طيب، خلونا نبدأ الاجتماع: عندنا demo اليوم." }, { start: 7, end: 11, text: "Okay, let's start the meeting." }], translations: [],
  }));
  await a.ev(`loadHistory()`);
  await hearToasts(a);
  let st = (await a.state()).setup;
  translatorPack = join(work, "translator-stand-in.tar.xz");
  if (!realTranslator) writeFileSync(translatorPack, standInPack(st.tools.find((x) => x.id === "translator").size));
  const sizes = st.tools.filter((x) => packOf(x.id)).map((x) => [x.id, x.size, statSync(packs[packOf(x.id)]).size]);
  check("the sizes in Settings are those of the release's files", sizes.length >= 2 && sizes.every(([, ours, file]) => ours === file), sizes);
  check("no ffmpeg here", !st.ffmpeg && !st.burner);

  // the small one, from its row in Settings: paused, and resumed
  await a.ev(`showView('settings')`);
  await sleep(300);
  bytesPerSecond = 2e5;
  await a.ev(`document.querySelector('[data-id="ffmpeg-audio"] .get').click()`);
  await until(a, `S.snap.setup.download.active && S.snap.setup.download.done > 0`, 20000);
  const going = await row(a, "ffmpeg-audio");
  check("its row shows the download going", /Downloading \d+%/.test(going) && /Pause/.test(going), going);
  await a.ev(`document.getElementById('set-tools').scrollIntoView({block:'center'})`);
  await a.shot(join(OUT, "tools-going.png"));
  bytesPerSecond = 3e6;
  await a.ev(`document.querySelector('[data-id="ffmpeg-audio"] .get').click()`); // pause
  await until(a, `!S.snap.setup.download.active`, 20000);
  await sleep(300);
  const paused = await row(a, "ffmpeg-audio");
  check("paused, it keeps what came and offers to resume", /Paused at \d+%/.test(paused) && /Resume/.test(paused), paused);
  await a.ev(`document.querySelector('[data-id="ffmpeg-audio"] .get').click()`); // resume
  check("resumed, it finishes", await until(a, `S.snap.setup.tools.find(x=>x.id==='ffmpeg-audio').on_disk`, 60000));
  await sleep(500);
  st = (await a.state()).setup;
  check("the app now uses Dawwin's ffmpeg", /tools[\\/]ffmpeg-audio[\\/]ffmpeg\.exe$/.test(st.ffmpeg || ""), st.ffmpeg);
  check("its row says it is in use", /In use/.test(await row(a, "ffmpeg-audio")), await row(a, "ffmpeg-audio"));
  check("a toast says it is ready", (await a.ev(`window.__toasts`)).includes("ffmpeg is ready."), await a.ev(`window.__toasts`));
  check("the download is gone, the folder holds the program and its licenses", readdirSync(join(home, "tools")).join() === "ffmpeg-audio" && ["ffmpeg.exe", "LICENSE-ffmpeg.txt", "README.txt"].every((f) => existsSync(join(home, "tools", "ffmpeg-audio", f))), readdirSync(join(home, "tools")));
  check("the small one cannot burn subtitles in", !st.burner);

  // the video one, from the offer burning in makes
  await a.ev(`openTranscript('vid-1')`);
  await sleep(300);
  await a.ev(`document.getElementById('btn-export').click()`);
  await a.ev(`document.getElementById('export-burn').click()`);
  await sleep(200);
  check("burning in offers ffmpeg for video", /needs ffmpeg for video/.test(await a.ev(`document.getElementById('toast').innerText`)), await a.ev(`document.getElementById('toast').innerText`));
  await a.ev(`document.querySelector('#toast button').click()`);
  check("the offer's button downloads ffmpeg for video", await until(a, `S.snap.setup.tools.find(x=>x.id==='ffmpeg-video').on_disk`, 60000));
  await sleep(500);
  st = (await a.state()).setup;
  check("the larger one takes over, and it burns", /ffmpeg-video[\\/]ffmpeg\.exe$/.test(st.ffmpeg || "") && st.burner === st.ffmpeg, { ffmpeg: st.ffmpeg, burner: st.burner });
  check("toasts said downloading, then ready", await a.ev(`window.__toasts.includes('Downloading ffmpeg for video…') && window.__toasts.includes('ffmpeg for video is ready.')`), await a.ev(`window.__toasts`));

  // burning in with Dawwin's own ffmpeg
  const out = join(work, "burned.mp4");
  const t0 = Date.now();
  const jid = await a.invoke("export_burned_to_path", { id: "vid-1", path: slash(out), view: "original" });
  await until(a, `window.__TAURI__.core.invoke('get_state').then(s=>!s.queue.some(j=>j.id===${JSON.stringify(jid)} && j.status==='saving_video'))`, 300000);
  const took = (Date.now() - t0) / 1000;
  let info = "";
  try { execFileSync(FFMPEG, ["-hide_banner", "-i", out], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }); } catch (e) { info = String(e.stderr); }
  // the 720p video enlarged to 1080p for sharp text (on by default since 1.4)
  check("Dawwin's ffmpeg burns the subtitles in", existsSync(out) && /Video: h264/.test(info) && /1920x1080/.test(info), { took, error: await a.ev(`S.snap.queue.find(j=>j.id===${JSON.stringify(jid)})?.error||null`) });
  if (existsSync(out)) ffmpeg(["-ss", "3", "-i", out, "-frames:v", "1", join(OUT, "tools-burned-frame.png")]);

  // deleting the video one: two presses, then the small one again
  await a.ev(`showView('settings')`);
  await sleep(300);
  await a.ev(`document.querySelector('[data-id="ffmpeg-video"] .del').click()`);
  await sleep(150);
  const sure = await a.ev(`document.querySelector('[data-id="ffmpeg-video"] .del').innerText`);
  check("delete asks to be pressed again", sure === "Press again to delete" && existsSync(join(home, "tools", "ffmpeg-video")), sure);
  await a.ev(`document.querySelector('[data-id="ffmpeg-video"] .del').click()`);
  check("deleted, the small one is in use again", await until(a, `!S.snap.setup.tools.find(x=>x.id==='ffmpeg-video').on_disk && /ffmpeg-audio/.test(S.snap.setup.ffmpeg||'')`, 10000));
  check("and its folder is gone", !existsSync(join(home, "tools", "ffmpeg-video")));

  // a damaged download is thrown away, and said
  damaged.add(packOf("ffmpeg-video"));
  await a.invoke("download_model", { id: "ffmpeg-video" });
  await until(a, `!S.snap.setup.download.active && S.snap.setup.download.error`, 60000);
  const err = await a.ev(`S.snap.setup.download.error`);
  check("a damaged download is refused", /arrived damaged/.test(err || ""), err);
  check("and nothing of it stays", !readdirSync(join(home, "tools")).some((f) => f.includes("ffmpeg-video")), readdirSync(join(home, "tools")));
  damaged.clear();
  await a.shot(join(OUT, "tools-settings.png"));
  a.close();
}

// ================================================================ the translator comes first
{
  const { home, a } = await start("tools-translator-home", PORT + 26, { DAWWIN_TEST_TRUST_DOWNLOAD: "1", FAKE_TRANSLATOR_DELAY_MS: "50" });
  const LINES = ["طيب، خلينا نبدأ: عندنا ثلاث نقاط اليوم.", "زر الـ login ما يشتغل على الجوال!"];
  writeFileSync(join(home, "history", "tr-1.json"), JSON.stringify({
    id: "tr-1", title: "اجتماع", source_path: join(work, "gone.m4a"), created_ms: Date.now(), duration_s: 20, language: "ar", elapsed_s: 1, device: "CPU", model: "x", partial: false, timed: true,
    segments: LINES.map((text, i) => ({ start: i * 10, end: i * 10 + 8, text })), translations: [],
  }));
  await a.ev(`loadHistory()`);
  await a.ev(`showView('settings')`);
  await sleep(300);
  bytesPerSecond = 4e6;
  await a.ev(`document.querySelector('[data-id="milmmt-1b"] .get').click()`);
  await until(a, `S.snap.setup.download.part==='translator' && S.snap.setup.download.done > 0`, 20000);
  const model = await row(a, "milmmt-1b"), tool = await row(a, "translator");
  check("the model's row says the translator comes first", /Downloading the translator \d+%/.test(model), model);
  check("and the translator's row shows it going", /Downloading \d+%/.test(tool), tool);
  await a.ev(`document.getElementById('set-translators').scrollIntoView({block:'center'})`);
  await a.shot(join(OUT, "tools-translator-first.png"));
  check("then the model downloads", await until(a, `S.snap.setup.download.active && !S.snap.setup.download.part && S.snap.setup.download.model==='milmmt-1b'`, 60000));
  check("and both end on the disk", await until(a, `!S.snap.setup.download.active && S.snap.setup.translator_found && S.snap.setup.families.translator?.found`, 180000), await a.ev(`S.snap.setup.download.error`));
  check("the translator's folder has its server and its license", existsSync(join(home, "tools", "translate", "llama-server.exe")) && existsSync(join(home, "tools", "translate", "LICENSE-llama.cpp.txt")), existsSync(join(home, "tools", "translate")) && readdirSync(join(home, "tools", "translate")));
  check("the translator's row says downloaded", /Downloaded/.test(await row(a, "translator")), await row(a, "translator"));
  check("they came from the stand-in server", served.some((f) => f.startsWith("dawwin-translator-")) && served.includes("MiLMMT-46-1B-v1.0.Q4_K_M.gguf"), served);
  if (realTranslator) {
    const r = spawnSync(join(home, "tools", "translate", "llama-server.exe"), ["--version"], { encoding: "utf8" });
    const v = (r.stdout || "") + (r.stderr || "");
    const build = /-b(\d+)\.tar\.xz$/.exec(packOf("translator"))?.[1] || "version";
    check("and it starts, the build its pack names", r.status === 0 && v.includes(build), v.split("\n").find((l) => /version/i.test(l)));
  } else {
    // the stand-in translator answers as llama.cpp's server does: the app translates with it
    const jid = await a.invoke("translate", { id: "tr-1", language: "en" });
    await until(a, `window.__TAURI__.core.invoke('get_state').then(s=>!s.queue.some(j=>j.id===${JSON.stringify(jid)} && ['queued','loading','translating'].includes(j.status)))`, 60000);
    const job = (await a.state()).queue.find((j) => j.id === jid);
    const tr = (await a.invoke("get_transcript", { id: "tr-1" })).translations?.[0];
    check("and the app translates with the one that came", tr?.lines.length === 2 && tr.lines.every((l, i) => l.text === "[English] " + LINES[i]), { lines: tr?.lines, job: job && [job.status, job.error] });
  }
  a.close();
}

server.close();
finish();
