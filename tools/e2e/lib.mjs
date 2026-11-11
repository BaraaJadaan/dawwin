// What every test of the real app shares: where things are, a throwaway data folder, the app
// started with its WebView's debugging port open, and a way to run code in its page. Only the
// processes started here are ever stopped. See README.md.
import { execFileSync, spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const OUT = process.env.DAWWIN_E2E_OUT || join(REPO, "tools", "e2e", "out");
export const APP = process.env.DAWWIN_APP || join(REPO, "src-tauri", "target", "debug", "dawwin.exe");
export const FAKE_TRANSLATOR = join(dirname(APP), "examples", "fake_translator.exe");
/** A speech model file (.gguf): the Arabic one also does English. */
export const MODEL = process.env.DAWWIN_E2E_MODEL || "";
/** A folder with Dawwin's own ffmpeg builds unpacked into ffmpeg-audio and ffmpeg-video (from the
 *  release's dawwin-ffmpeg-*.tar.xz): the app uses those, as it does for its users. */
export const TOOLS = process.env.DAWWIN_E2E_TOOLS || "";
/** A full ffmpeg, only to make the sample videos. */
export const FFMPEG = process.env.DAWWIN_E2E_FFMPEG || "ffmpeg";
/** The first of the debugging ports the tests use. */
export const PORT = Number(process.env.DAWWIN_E2E_PORT || 9400);

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export const slash = (p) => p.split(String.fromCharCode(92)).join("/");

let failed = 0;
export function check(name, ok, detail) {
  console.log(`${ok ? "pass" : "FAIL"}  ${name}${detail !== undefined ? "  " + JSON.stringify(detail) : ""}`);
  if (!ok) failed++;
}
export function finish() {
  console.log(failed ? `${failed} FAILED` : "ALL PASS");
  process.exit(failed ? 1 : 0);
}
export function need(what, value) {
  if (!value) {
    console.log(`skipped: set ${what} (see tools/e2e/README.md)`);
    process.exit(0);
  }
}

/** A fresh data folder `name` under OUT, with `settings` and Dawwin's own ffmpeg in it. */
export function makeHome(name, settings = {}) {
  const home = join(OUT, name);
  rmSync(home, { recursive: true, force: true });
  mkdirSync(join(home, "history"), { recursive: true });
  mkdirSync(join(home, "models"), { recursive: true });
  writeFileSync(join(home, "settings.json"), JSON.stringify({ model_file: MODEL || undefined, ui_lang: "en", ...settings }));
  if (TOOLS) {
    for (const t of ["ffmpeg-audio", "ffmpeg-video"]) {
      if (existsSync(join(TOOLS, t))) cpSync(join(TOOLS, t), join(home, "tools", t), { recursive: true });
    }
  }
  return home;
}

/** Make a file with the full ffmpeg (samples only). */
export function ffmpeg(args) {
  execFileSync(FFMPEG, ["-nostdin", "-loglevel", "error", "-y", ...args]);
}

/** Start the app on `home`, with the page's debugging port `port`. */
export async function open(home, port, env = {}) {
  // the interface starts as a new user's, without what the last run left in its storage
  const webview = join(OUT, `webview-${port}`);
  if (!env.KEEP_WEBVIEW) rmSync(webview, { recursive: true, force: true });
  const proc = spawn(APP, [], {
    stdio: "ignore",
    env: { ...process.env, DAWWIN_HOME: home, WEBVIEW2_USER_DATA_FOLDER: webview, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --mute-audio`, ...env },
  });
  let page;
  for (let i = 0; i < 80 && !page; i++) {
    await sleep(500);
    try { page = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page"); } catch { /* not up yet */ }
  }
  if (!page) { proc.kill(); throw new Error("the app did not open"); }
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let n = 0;
  const pending = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
  const call = (method, params = {}) => new Promise((r) => { const id = ++n; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
  const ev = async (expression) => {
    const r = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails).slice(0, 600));
    return r.result?.result?.value;
  };
  await call("Emulation.setDeviceMetricsOverride", { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false });
  await call("Emulation.setFocusEmulationEnabled", { enabled: true });
  await sleep(2500);
  await ev(`setUiLang('en')`);
  const invoke = (cmd, args) => ev(`window.__TAURI__.core.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args || {})})`);
  const app = {
    proc,
    call,
    ev,
    invoke,
    state: () => invoke("get_state"),
    /** Wait until no job is under way. */
    async idle(seconds = 600) {
      for (let i = 0; i < seconds * 4; i++) {
        const busy = await ev(`window.__TAURI__.core.invoke('get_state').then(s=>s.queue.filter(j=>ACTIVE.has(j.status)).length)`);
        if (!busy) return true;
        await sleep(250);
      }
      return false;
    },
    async shot(file) {
      const r = await call("Page.captureScreenshot", { format: "png" });
      writeFileSync(file, Buffer.from(r.result.data, "base64"));
    },
    close() { try { ws.close(); } catch { /* gone */ } proc.kill(); },
  };
  process.on("exit", () => proc.kill());
  return app;
}

process.on("uncaughtException", (e) => { console.log("STOPPED:", e.stack || e.message); process.exit(2); });
