// 1.8.0 in the real app: the shortcut's settings, closing to the notification area, a second start,
// Start with Windows (in a test registry key, never the real Run key), the small player, and
// PDF, EPUB and pictures of text. In a throwaway data folder, with the stand-in reader.
//
//   python tools/trials/pdf/make_pdfs.py <dir>/trial-b      (and make_word.ps1 for word.pdf)
//   node tools/e2e/screen.mjs <dir> src-tauri/target/debug/dawwin.exe
//
// <dir>/pdfium/bin holds pdfium.dll (pdfium-binaries chromium/8152). Debugging port 9901.
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const [SP, EXE] = process.argv.slice(2);
const PORT = 9901;
const KEY = "Software\\Dawwin-test-harness-1-8\\Run";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const home = join(SP, "screen-home");
rmSync(home, { recursive: true, force: true });
mkdirSync(home, { recursive: true });
writeFileSync(join(home, "settings.json"), JSON.stringify({ ui_lang: "en" }));
const env = { ...process.env, DAWWIN_HOME: home, DAWWIN_TEST_READER: "1", DAWWIN_TEST_AUTOSTART_KEY: KEY, DAWWIN_TEST_PDFIUM: join(SP, "pdfium", "bin"), WEBVIEW2_USER_DATA_FOLDER: join(SP, "webview-screen"), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT} --mute-audio` };
const start = (args = []) => spawn(EXE, args, { stdio: "ignore", env });
const alive = (p) => p.exitCode === null && p.signalCode === null;
const results = [];
const check = (name, ok, detail = "") => {
  results.push(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  (" + detail + ")" : ""}`);
  console.log(results.at(-1));
};

async function connect(filter) {
  for (let i = 0; i < 80; i++) {
    await sleep(500);
    try {
      const page = (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).find((t) => t.type === "page" && filter(t));
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((r) => (ws.onopen = r));
        let n = 0;
        const pending = new Map();
        ws.onmessage = (m) => {
          const d = JSON.parse(m.data);
          if (pending.has(d.id)) {
            pending.get(d.id)(d);
            pending.delete(d.id);
          }
        };
        const ev = (expression) =>
          new Promise((r) => {
            const id = ++n;
            pending.set(id, (d) => r(d.result?.result?.value ?? d.result?.exceptionDetails?.exception?.description));
            ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
          });
        return { ws, ev, invoke: (c, a) => ev(`window.__TAURI__.core.invoke(${JSON.stringify(c)}, ${JSON.stringify(a || {})}).catch(e => ({ error: e }))`) };
      }
    } catch {}
  }
  throw new Error("no page");
}
const regValue = () => {
  try {
    return execFileSync("reg", ["query", "HKCU\\" + KEY, "/v", "Dawwin"], { encoding: "utf8" });
  } catch {
    return "";
  }
};


// what a click on the window's X sends: WM_CLOSE, to this test's own Dawwin only
const closeWindow = (pid) =>
  execFileSync("powershell", ["-NoProfile", "-Command", `Add-Type -Name W -Namespace U -MemberDefinition '[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);'; $h = (Get-Process -Id ${pid}).MainWindowHandle; [U.W]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null; "$h"`], { encoding: "utf8" }).trim();

const app = start();
const main = await connect((t) => t.url.startsWith("http://tauri.localhost") && !t.url.includes("player"));
await sleep(3000);
check("the window opens at start", (await main.ev("window.__TAURI__.window.getCurrentWindow().isVisible()")) === true);
let s = await main.invoke("screen_state");
check("off until turned on, Ctrl+Alt+R by default", s.settings.on === false && s.settings.shortcut === "Ctrl+Alt+R" && s.shortcut.state === "off", JSON.stringify(s.shortcut));
s = await main.invoke("screen_set_on", { on: true });
check("turned on: the shortcut is registered, or its being taken is said with free ones", s.shortcut.state === "on" || (s.shortcut.state === "taken" && !!s.error && s.free.length > 0), `${s.shortcut.state}; ${s.error ?? ""}; free: ${s.free.join(", ")}`);
if (s.shortcut.state === "taken") {
  const pick = s.free[0];
  s = await main.invoke("screen_set_shortcut", { keys: pick });
  check("a free one offered can be chosen", s.shortcut.state === "on" && s.settings.shortcut === pick, JSON.stringify(s.shortcut));
}
s = await main.invoke("screen_set_shortcut", { keys: "Shift+R" });
check("a key alone is refused, and the shortcut before stays", !!s.error && s.shortcut.state === "on", s.error);
s = await main.invoke("screen_set_say", { list: [{ lang: "ar", written: "AI", said: "الذكاء الاصطناعي" }, { lang: "en", written: "", said: "x" }] });
check("say it like this is kept, empty rows dropped", s.settings.say.length === 1 && s.settings.say[0].said === "الذكاء الاصطناعي");
// Settings in the interface, both languages
await main.ev(`document.getElementById("btn-settings").click()`);
await sleep(800);
const en = await main.ev(`document.getElementById("set-screen").innerText`);
check("Settings shows the section in English", en.includes("Read anything on the screen") && en.includes("Say it like this"), en.split("\n").slice(0, 3).join(" / "));
const tip = await main.ev(`document.getElementById("scr-on-label").title`);
check("its controls have tooltips", tip.includes("kept nowhere"), tip);
await main.invoke("set_ui_language", { lang: "ar" });
await main.ev(`location.reload()`);
main.ws.close();
await sleep(3000);
const main2 = await connect((t) => t.url.startsWith("http://tauri.localhost") && !t.url.includes("player"));
await sleep(1500);
await main2.ev(`document.getElementById("btn-settings").click()`);
await sleep(800);
const ar = await main2.ev(`document.getElementById("set-screen").innerText`);
const dir = await main2.ev(`document.documentElement.dir`);
check("and in Arabic, right to left", ar.includes("قراءة أي نص على الشاشة") && dir === "rtl", ar.split("\n").slice(0, 2).join(" / "));
await main2.invoke("set_ui_language", { lang: "en" });
// Start with Windows, in the test key
s = await main2.invoke("screen_set_start_with_windows", { on: true });
const exe = EXE.replaceAll("/", "\\");
check("Start with Windows writes this Dawwin, hidden", regValue().includes(`"${exe}" --hidden`), regValue().trim().split("\n").pop());
// the small player, without taking the keyboard
await main2.invoke("test_player_note", { note: "nothing_selected" });
const player = await connect((t) => t.url.includes("player.html"));
await sleep(800);
check("the small player shows the note", (await player.ev(`document.getElementById("msg").textContent`)).includes("Nothing is selected"));
check("its buttons have tooltips", (await player.ev(`document.getElementById("close").title`)) === "Stop and close");
await sleep(4500);
check("a note alone goes away by itself", (await player.ev("window.__TAURI__.window.getCurrentWindow().isVisible()")) === false);
player.ws.close();
// close: to the notification area while the shortcut is on
closeWindow(app.pid);
await sleep(1500);
check("closing hides the window and Dawwin stays", alive(app) && (await main2.ev("window.__TAURI__.window.getCurrentWindow().isVisible()")) === false);
const second = start();
await sleep(4000);
check("starting it again opens the window, and the second one quits", (await main2.ev("window.__TAURI__.window.getCurrentWindow().isVisible()")) === true && !alive(second));
// files
for (const [file, want] of [
  ["book.epub", "الفصل الأول"],
  ["chrome.pdf", "تجربة قراءة PDF"],
  ["word.pdf", "مرحبًا، هذه فقرة مختلقة"],
  ["scan.pdf", "تجربة قراءة PDF"],
  ["scan.png", "تجربة قراءة PDF"],
]) {
  const o = await main2.invoke("screen_open_file", { path: join(SP, "trial-b", file) });
  const text = o.parts ? o.parts.map((p) => p.title + "\n" + p.text).join("\n") : JSON.stringify(o);
  check(`opens ${file}`, text.includes(want), (o.parts ? `${o.kind}, ${o.parts.length} parts, recognised ${o.recognised}: ` : "") + text.replace(/\s+/g, " ").slice(0, 70));
}
const r = await main2.invoke("screen_read_text", { text: "يعمل AI هنا.", path: null });
check("a text goes to the reader", r === null || r === undefined, JSON.stringify(r));
// off, and closing quits as before
s = await main2.invoke("screen_set_on", { on: false });
check("off: the shortcut is let go and Start with Windows removed", s.shortcut.state === "off" && !regValue().includes("--hidden"));
closeWindow(app.pid);
for (let i = 0; i < 20 && alive(app); i++) await sleep(500);
check("with the shortcut off, closing the window quits Dawwin", !alive(app));
if (alive(app)) app.kill();
try {
  execFileSync("reg", ["delete", "HKCU\\Software\\Dawwin-test-harness-1-8", "/f"]);
} catch {}
console.log(`\n${results.filter((x) => x.startsWith("PASS")).length} of ${results.length} passed`);
process.exit(0);
