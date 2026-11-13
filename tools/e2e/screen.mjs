// Read anything on the screen (1.8.0), with the stand-in voice (`cargo build --example fake_voice`):
// the shortcut's settings (a taken shortcut said, with free ones offered), say it like this, the
// Settings section in English and Arabic with its tooltips, Start with Windows (in a test registry
// key, never the real Run key), the small player reading a selection (pause, back, forward, the
// shortcut again to stop, Read aloud in the window stopping meanwhile, the selection forgotten with
// its sound), closing to the notification area and starting again, PDFium downloaded from a local
// stand-in for the release, PDFs, an e-book's chapters and its place, a scan and a picture of text,
// saving as sound as a job in the list, and closing with the shortcut off quitting Dawwin.
//
// The taking of the selection from another program is not here (it would take the keyboard): see
// tools/trials/selection. Files: DAWWIN_E2E_PDFS, a folder made by tools/trials/pdf/make_pdfs.py
// (and make_word.ps1). PDFium: the release's dawwin-pdfium-*.tar.xz in DAWWIN_E2E_TOOLS.
import { execFileSync, spawn } from "node:child_process";
import { createServer } from "node:http";
import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { APP, check, finish, makeHome, open, OUT, PORT, sleep, TOOLS } from "./lib.mjs";

const FAKE = join(dirname(APP), "examples", "fake_voice.exe");
if (!existsSync(FAKE)) {
  console.log("skipped: build the stand-in voice first (cargo build --example fake_voice)");
  process.exit(0);
}
const PDFS = process.env.DAWWIN_E2E_PDFS || "";
const KEY = "Software\\Dawwin-e2e-screen\\Run";
const port = PORT + 50;
const home = makeHome("screen-home");
const regValue = () => {
  try { return execFileSync("reg", ["query", "HKCU\\" + KEY, "/v", "Dawwin"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }); } catch { return ""; }
};
const cleanReg = () => { try { execFileSync("reg", ["delete", "HKCU\\Software\\Dawwin-e2e-screen", "/f"], { stdio: "ignore" }); } catch { /* not there */ } };

// the release's PDFium pack, served as the release would
const pack = TOOLS && existsSync(TOOLS) ? readdirSync(TOOLS).find((f) => /^dawwin-pdfium-.*\.tar\.xz$/.test(f)) : null;
let server = null;
let toolsUrl = "";
if (pack) {
  server = createServer((req, res) => {
    const f = join(TOOLS, decodeURIComponent(req.url.split("/").pop()));
    if (!existsSync(f)) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { "Content-Length": statSync(f).size });
    res.end(readFileSync(f));
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  toolsUrl = `http://127.0.0.1:${server.address().port}`;
}

const env = {
  DAWWIN_TEST_VOICE: FAKE, FAKE_VOICE_DELAY_MS: "250", DAWWIN_TEST_AUTOSTART_KEY: KEY, DAWWIN_TOOLS_URL: toolsUrl,
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --mute-audio --autoplay-policy=no-user-gesture-required`,
};

/** What a click on the main window's X sends (WM_CLOSE), to this test's own Dawwin: its largest
 *  visible window, not the small player. */
function closeMainWindow(pid) {
  const ps = `Add-Type -Name W -Namespace U -MemberDefinition @'
public delegate bool P(System.IntPtr h, System.IntPtr l);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool EnumWindows(P p, System.IntPtr l);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint pid);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool IsWindowVisible(System.IntPtr h);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool GetWindowRect(System.IntPtr h, out RECT r);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr h, uint m, System.IntPtr w, System.IntPtr l);
public struct RECT { public int L, T, R, B; }
'@
$best = [IntPtr]::Zero; $area = 0
[U.W]::EnumWindows({ param($h, $l) $p = 0; [void][U.W]::GetWindowThreadProcessId($h, [ref]$p); if ($p -eq ${pid} -and [U.W]::IsWindowVisible($h)) { $r = New-Object U.W+RECT; [void][U.W]::GetWindowRect($h, [ref]$r); $a = ($r.R - $r.L) * ($r.B - $r.T); if ($a -gt $script:area) { $script:area = $a; $script:best = $h } }; $true }, [IntPtr]::Zero) | Out-Null
[void][U.W]::PostMessage($best, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)`;
  execFileSync("powershell", ["-NoProfile", "-Command", ps]);
}

cleanReg();
let a = await open(home, port, env);
const alive = (p) => p.exitCode === null && p.signalCode === null;
const visible = () => a.ev("window.__TAURI__.window.getCurrentWindow().isVisible()");

/** The small player's page, through the same debugging port. */
async function player() {
  for (let i = 0; i < 40; i++) {
    const t = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((x) => x.type === "page" && x.url.includes("player.html"));
    if (t) {
      const ws = new WebSocket(t.webSocketDebuggerUrl);
      await new Promise((r) => (ws.onopen = r));
      let n = 0;
      const pending = new Map();
      ws.onmessage = (m) => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
      const ev = (expression) => new Promise((r) => { const id = ++n; pending.set(id, (d) => r(d.result?.result?.value)); ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } })); });
      return { ws, ev };
    }
    await sleep(250);
  }
  throw new Error("no player");
}

// ---- the shortcut's settings
let s = await a.invoke("screen_state");
check("off until turned on, Ctrl+Shift+Space by default", !s.settings.on && s.settings.shortcut === "Ctrl+Shift+Space" && s.shortcut.state === "off", s.shortcut);
s = await a.invoke("screen_set_on", { on: true });
check("turned on: registered, or its being taken said with free ones", s.shortcut.state === "on" || (s.shortcut.state === "taken" && !!s.error && s.free.length > 0), s.error || s.shortcut);
if (s.shortcut.state === "taken") {
  s = await a.invoke("screen_set_shortcut", { keys: s.free[0] });
  check("a free one offered is taken at once", s.shortcut.state === "on", s.shortcut);
}
s = await a.invoke("screen_set_shortcut", { keys: "Shift+R" });
check("a key alone is refused, the shortcut before stays", !!s.error && s.shortcut.state === "on", s.error);
s = await a.invoke("screen_set_say", { list: [{ lang: "ar", written: "AI", said: "الذكاء الاصطناعي" }, { lang: "en", written: "", said: "x" }] });
check("say it like this kept, empty rows dropped", s.settings.say.length === 1);

// ---- Settings, both languages
await a.ev(`showView("settings")`);
await sleep(600);
const en = await a.ev(`document.getElementById("set-screen").innerText`);
check("Settings shows the section", en.includes("Read anything on the screen") && en.includes("Say it like this"), en.split("\n")[0]);
check("its controls have tooltips", (await a.ev(`document.getElementById("scr-on-label").title`)).includes("kept nowhere"));
await a.ev(`setUiLang('ar')`);
await sleep(600);
check("and in Arabic, right to left", (await a.ev(`document.getElementById("set-screen").innerText`)).includes("قراءة أي نص على الشاشة") && (await a.ev("document.documentElement.dir")) === "rtl");
check("tooltips follow the language", (await a.ev(`document.getElementById("scr-copy-label").title`)).includes("Ctrl+C"));
await a.ev(`setUiLang('en')`);

// ---- How to use: a part of its own, with its picture, in both languages
for (const lang of ["en", "ar"]) {
  await a.ev(`setUiLang(${JSON.stringify(lang)}); document.getElementById("guide-items").dataset.lang = ""; showView("guide")`);
  await sleep(700);
  const parts = await a.ev(`[...document.querySelectorAll('#guide-items h2')].map(h => h.textContent)`);
  const pic = await a.ev(`[...document.querySelectorAll('#guide-items img')].some(i => i.src.endsWith("/${lang}/screen.webp") && i.naturalWidth > 0)`);
  check(`How to use has thirteen parts, Read anything on the screen after Read aloud (${lang})`, parts.length === 13 && parts.findIndex((p) => p.includes(lang === "ar" ? "اقرأ أي نص على الشاشة" : "Read anything on the screen")) === parts.findIndex((p) => p.includes(lang === "ar" ? "اسمع أي نص مقروءًا" : "Have any text read aloud")) + 1 && pic, parts);
}
await a.ev(`setUiLang('en')`);

// ---- Start with Windows, in the test key
await a.invoke("screen_set_start_with_windows", { on: true });
// the path as Windows writes it, however DAWWIN_APP was given
check("Start with Windows writes this Dawwin, hidden", regValue().includes(`"${resolve(APP)}" --hidden`), regValue().trim().split("\n").pop());

// ---- the small player: a note alone
await a.invoke("test_player_note", { note: "nothing_selected" });
let p = await player();
await sleep(600);
check("the player says when nothing is selected", (await p.ev(`document.getElementById("msg").textContent`)).includes("Nothing is selected"));
check("its buttons have tooltips", (await p.ev(`document.getElementById("close").title`)) === "Stop and close");
await sleep(4500);
check("a note alone goes away by itself", (await p.ev("window.__TAURI__.window.getCurrentWindow().isVisible()")) === false);

// ---- a selection read in the player, while Read aloud in the window stops
await a.ev(`openReading({ text: "Read aloud in the window. It has three sentences. This is the last one." })`);
await sleep(800);
await a.ev(`readFrom(0)`);
await sleep(1500);
check("Read aloud plays in the window", await a.ev("window.__readAloud.playing"));
const note = await a.invoke("test_screen_read", { text: "يعمل AI هنا على الحاسوب. Then an English sentence follows. وجملة ثالثة أخيرة." });
check("a press with text reads it", note === "reading", note);
await sleep(1500);
const pv = () => p.ev(`(() => { const P = window.__player; return { n: P.doc ? P.doc.sentences.length : 0, i: P.i, playing: P.playing, paused: P.audio.paused, id: P.doc?.id, first: P.doc?.sentences[0]?.text, msg: document.getElementById("msg").textContent, visible: null }; })()`);
let st = await pv();
check("the player reads it, sentence by sentence", st.n === 3 && st.playing, st);
check("say it like this was applied", st.first === "يعمل الذكاء الاصطناعي هنا على الحاسوب.", st.first);
check("Read aloud in the window stopped", !(await a.ev("window.__readAloud.playing")));
const docId = st.id;
await p.ev(`document.getElementById("pause").click()`);
await sleep(400);
st = await pv();
check("pause", !st.playing && st.paused, st);
await p.ev(`document.getElementById("resume").click()`);
await sleep(600);
check("and on again", (await pv()).playing);
await p.ev(`document.getElementById("forward").click()`);
await sleep(600);
st = await pv();
check("forward a sentence", st.i >= 1, st);
await p.ev(`window.__player.audio.currentTime = 0; document.getElementById("back").click()`);
await sleep(600);
st = await pv();
check("back a sentence", st.i === 0 || st.i < 2, st);
const again = await a.invoke("test_screen_read", { text: "whatever is selected now" });
await sleep(800);
st = await pv();
check("the shortcut again stops", again === "stopped" && st.n === 0 && !st.playing, { again, st });
const cache = join(home, "cache", "reading", docId);
check("the selection is forgotten with the sound made of it", !existsSync(cache));
const prefs = existsSync(join(home, "reading.json")) ? readFileSync(join(home, "reading.json"), "utf8") : "";
check("and no place of it is kept", !prefs.includes("selection:"));
// to its end, by itself
await a.invoke("test_screen_read", { text: "One short sentence." });
for (let i = 0; i < 40 && (await pv()).n; i++) await sleep(250);
check("a reading that ends lets the player go", (await pv()).n === 0);
p.ws.close();

// ---- closing to the notification area, and starting again
closeMainWindow(a.proc.pid);
await sleep(1500);
check("closing hides the window, and Dawwin stays", alive(a.proc) && (await visible()) === false);
const second = spawn(APP, [], { stdio: "ignore", env: { ...process.env, DAWWIN_HOME: home, WEBVIEW2_USER_DATA_FOLDER: join(OUT, "webview-screen-2") } });
await sleep(4000);
check("starting it again opens the window, and the second one quits", (await visible()) === true && !alive(second));

// ---- files in Read aloud
const rs = () => a.ev(`(() => { const R = window.__readAloud; return { n: R.doc ? R.doc.sentences.length : 0, i: R.i, title: R.doc?.title, text: R.doc ? R.doc.sentences.slice(0, 3).map(s => s.text).join(" ") : "", chapters: document.getElementById("read-chapters")?.hidden ? 0 : document.getElementById("read-chapters")?.options.length }; })()`);
const openFile = (path) => a.ev(`window.__TAURI__.core.invoke("reading_open", { path: ${JSON.stringify(path)}, text: null, transcript: null, language: null }).then(() => "ok", (e) => String(e))`);
if (!PDFS) {
  console.log("skipped: the files (set DAWWIN_E2E_PDFS to a folder made by tools/trials/pdf/make_pdfs.py)");
} else {
  check("Read aloud offers the new kinds", (await a.invoke("reading_formats")).includes("epub"));
  await a.ev(`openReading({ path: ${JSON.stringify(join(PDFS, "book.epub"))} })`);
  await sleep(1200);
  let r = await rs();
  check("an e-book opens with its chapters to choose", r.n > 2 && r.chapters === 2 && r.text.startsWith("الفصل الأول"), r);
  await a.ev(`goToChapter(Number(document.getElementById("read-chapters").options[1].value))`);
  const at = (await rs()).i;
  await a.ev(`closeReading()`);
  await a.ev(`openReading({ path: ${JSON.stringify(join(PDFS, "book.epub"))} })`);
  await sleep(1000);
  check("and comes back to where it was", (await rs()).i === at && at > 0, { at, now: (await rs()).i });
  const before = await openFile(join(PDFS, "chrome.pdf"));
  check("a PDF asks for PDFium first", before === "needs-pdfium", before);
  if (!pack) {
    console.log("skipped: PDFium (put the release's dawwin-pdfium-*.tar.xz in DAWWIN_E2E_TOOLS)");
  } else {
    const got = await a.ev(`window.__TAURI__.core.invoke("screen_install_pdfium").then(() => "ok", (e) => String(e))`);
    check("PDFium comes from the release, checked", got === "ok", got);
    for (const [f, want] of [["chrome.pdf", "تجربة قراءة PDF مرحبًا، هذه فقرة مختلقة"], ["word.pdf", "مرحبًا، هذه فقرة مختلقة"], ["scan.pdf", "تجربة قراءة PDF"], ["scan.png", "تجربة قراءة PDF"]]) {
      if (!existsSync(join(PDFS, f))) { console.log(`skipped: ${f}`); continue; }
      await a.ev(`openReading({ path: ${JSON.stringify(join(PDFS, f))} })`);
      await sleep(f.startsWith("scan") ? 2500 : 1200);
      r = await rs();
      check(`opens ${f}`, r.text.includes(want), r.text.slice(0, 80));
    }
  }
  // ---- save as sound: a job in the list
  await a.ev(`openReading({ path: ${JSON.stringify(join(PDFS, "book.epub"))} })`);
  await sleep(1000);
  const docNow = await a.ev("window.__readAloud.doc.id");
  const dest = join(OUT, "screen-book.m4b");
  rmSync(dest, { force: true });
  const job = await a.ev(`window.__TAURI__.core.invoke("reading_save_sound_to_path", { doc: ${JSON.stringify(docNow)}, path: ${JSON.stringify(dest)} }).catch((e) => "error: " + e)`);
  check("saving as sound starts a job in the list", typeof job === "string" && !job.startsWith("error"), job);
  // a finished job leaves the list at once, and a message says where the file is
  let row = "", told = false, failed = null;
  for (let i = 0; i < 1200 && !told && !failed; i++) {
    const w = await a.ev(`({ row: document.querySelector('#queue li[data-key="${job}"]')?.innerText || "", told: S.told.has(${JSON.stringify(job)}), failed: S.snap.queue.find(j => j.id === ${JSON.stringify(job)} && j.status === "failed")?.error || null })`);
    if (w.row) row = w.row;
    told = w.told;
    failed = w.failed;
    await sleep(50);
  }
  check("the job shows in the list while it works, with how far it is", row.includes("Saving the reading as sound") && /\d+%/.test(row), row);
  const m4b = existsSync(dest) ? readFileSync(dest) : Buffer.alloc(0);
  check("it ends with the book saved, with its chapters, and says so", told && !failed && m4b.includes(Buffer.from("chpl")) && m4b.includes(Buffer.from("Chapter two")), { told, failed, bytes: m4b.length });
}

// ---- off: closing quits as before
s = await a.invoke("screen_set_on", { on: false });
check("off: the shortcut let go, Start with Windows removed", s.shortcut.state === "off" && !regValue().includes("--hidden"));
closeMainWindow(a.proc.pid);
for (let i = 0; i < 20 && alive(a.proc); i++) await sleep(500);
check("with the shortcut off, closing the window quits Dawwin", !alive(a.proc));
if (alive(a.proc)) a.close();

// ---- started by Windows, hidden in the notification area; started again from Start, its window opens
{
  const settingsFile = join(home, "settings.json");
  const st = JSON.parse(readFileSync(settingsFile, "utf8"));
  st.screen = { ...(st.screen || {}), on: true, start_with_windows: true };
  writeFileSync(settingsFile, JSON.stringify(st));
  await sleep(1500);
  const hidden = spawn(APP, ["--hidden"], { stdio: "ignore", env: { ...process.env, ...env, DAWWIN_HOME: home, WEBVIEW2_USER_DATA_FOLDER: join(OUT, "webview-screen-hidden") } });
  let page;
  for (let i = 0; i < 80 && !page; i++) {
    await sleep(500);
    try { page = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page" && !t.url.includes("player")); } catch { /* not up yet */ }
  }
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let n = 0;
  const pending = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
  const ev = (expression) => new Promise((r) => { const id = ++n; pending.set(id, (d) => r(d.result?.result?.value)); ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } })); });
  await sleep(3000);
  const shown = () => ev("window.__TAURI__.window.getCurrentWindow().isVisible()");
  check("started with --hidden and the shortcut on, it waits without its window", (await shown()) === false && alive(hidden));
  const fromStart = spawn(APP, [], { stdio: "ignore", env: { ...process.env, DAWWIN_HOME: home, WEBVIEW2_USER_DATA_FOLDER: join(OUT, "webview-screen-again") } });
  await sleep(4000);
  const state = await ev("window.__TAURI__.core.invoke('screen_state')");
  check("started again from Start: the hidden one opens its window, the second quits, the shortcut is its own", (await shown()) === true && !alive(fromStart) && state.shortcut.state === "on", { second: alive(fromStart), shortcut: state.shortcut });

  // ---- Quit in the notification area while a reading is saved as sound: the save stops, nothing is left
  const long = Array.from({ length: 80 }, (_, i) => `هذه الجملة المختلقة رقم ${i + 1} للتجربة.`).join(" ");
  const opened = await ev(`window.__TAURI__.core.invoke("reading_open", { path: null, text: ${JSON.stringify(long)}, transcript: null, language: null }).then((d) => d.id, (e) => "error: " + e)`);
  const dest = join(OUT, "screen-quit.m4a");
  rmSync(dest, { force: true });
  const job = await ev(`window.__TAURI__.core.invoke("reading_save_sound_to_path", { doc: ${JSON.stringify(opened)}, path: ${JSON.stringify(dest)} }).catch((e) => "error: " + e)`);
  let status = "";
  for (let i = 0; i < 100 && status !== "saving_audio"; i++) {
    await sleep(100);
    status = await ev(`S.snap?.queue.find((j) => j.id === ${JSON.stringify(job)})?.status || ""`);
  }
  ev(`window.__TAURI__.core.invoke("test_tray_quit")`).catch(() => {});
  for (let i = 0; i < 40 && alive(hidden); i++) await sleep(500);
  const left = readdirSync(home, { recursive: true }).map(String).filter((f) => /reading-\d+\.wav$|\.part\.m4a$|-sound-\d+$/.test(f));
  check("Quit in the notification area mid-save: the save stops, Dawwin quits and leaves nothing behind", typeof job === "string" && !job.startsWith("error") && status === "saving_audio" && !alive(hidden) && !existsSync(dest) && !existsSync(dest.replace(/\.m4a$/, ".part.m4a")) && left.length === 0, { job, status, alive: alive(hidden), left });
  ws.close();
  if (alive(hidden)) hidden.kill();
}
cleanReg();
server?.close();
finish();
