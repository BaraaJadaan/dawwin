// Downloads behind a proxy, in the real app: started with HTTPS_PROXY naming a stand-in proxy on
// this computer that asks for a sign-in, as a university's or a company's may. Nothing reaches the
// internet: the stand-in answers every request itself. The model download goes to it, says why it
// cannot go on after five tries (in English and in Arabic), with the way to download by hand, and
// keeps trying; the update check goes through it too.
import { createServer } from "node:net";
import { check, finish, makeHome, open, PORT, sleep } from "./lib.mjs";

const asked = [];
const proxy = createServer((socket) => {
  socket.once("data", (d) => {
    asked.push(d.toString("latin1").split("\r\n")[0]);
    socket.end("HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"campus\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
  });
  socket.on("error", () => {});
});
await new Promise((r) => proxy.listen(0, "127.0.0.1", r));
const proxyUrl = `http://127.0.0.1:${proxy.address().port}`;

const home = makeHome("proxy-home", { model_file: undefined });
const a = await open(home, PORT + 5, { HTTPS_PROXY: proxyUrl, HF_ENDPOINT: "" });
const model = (await a.state()).setup.models.find((m) => m.family !== "translator");
await a.ev(`S.pick = ${JSON.stringify(model.id)}; showView('setup'); 1`);
await a.invoke("download_model", { id: model.id });
let d = null;
const started = Date.now();
for (let i = 0; i < 240 && !d?.retry_reason; i++) { await sleep(500); d = (await a.state()).setup.download; }
check("the model download goes through the proxy Windows or HTTPS_PROXY names", asked.some((l) => /^CONNECT huggingface\.co:443/.test(l)), asked.slice(0, 3));
check("after five tries it says why: the proxy wants a sign-in", /sign-in/.test(d?.retry_reason || ""), { reason: d?.retry_reason, attempt: d?.retry_attempt, seconds: Math.round((Date.now() - started) / 1000) });
await sleep(400);
const shown = await a.ev(`({ why: document.getElementById('download-why').hidden ? '' : document.getElementById('download-why').textContent, hand: !document.getElementById('btn-download-hand').hidden })`);
check("the setup page shows it, with \"Download it myself\" next to it", /sign-in/.test(shown.why) && /keeps trying/.test(shown.why) && shown.hand, shown);
check("and it keeps trying", (await a.state()).setup.download.active);
// the reason is written at each try, in the interface's language then: the next one is in Arabic
await a.ev(`setUiLang('ar')`);
let ar = "";
for (let i = 0; i < 150 && !/تسجيل الدخول/.test(ar); i++) { await sleep(500); ar = await a.ev(`document.getElementById('download-why').textContent`); }
check("in Arabic too, from the next try", /تسجيل الدخول/.test(ar) && /يواصل/.test(ar), ar);
await a.ev(`setUiLang('en')`);
await a.invoke("stop_download");
const before = asked.length;
const update = await a.ev(`window.__TAURI__.core.invoke('check_updates').then(() => 'answered', (e) => String(e))`);
check("the update check goes through the same proxy", asked.slice(before).some((l) => /^CONNECT api\.github\.com:443/.test(l)), { asked: asked.slice(before), update });
a.close();
proxy.close();
finish();
