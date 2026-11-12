// The native dialogs of recording, read and answered through UI Automation (`dialog.ps1`, which
// presses a task dialog's button with its own message, without the mouse or the focus): the notice
// before the first recording, in English and Arabic, and again with true words when the recordings
// would go to OneDrive; closing Dawwin while it records (asks; Keep recording; Stop and quit keeps
// the file whole). Made-up tracks from `record.mjs` (run it first).
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, finish, makeHome, MODEL, need, open, OUT, PORT, REPO, sleep } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
const ME = join(OUT, "record-media", "me.wav"), THEM = join(OUT, "record-media", "them.wav");
need("the tracks record.mjs makes (run it first)", existsSync(ME) && existsSync(THEM));
const TEST = `me=${ME}|them=${THEM}`;
const HERE = join(REPO, "tools", "e2e");
const ps = (script, args) => execFileSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", join(HERE, script), ...args.map(String)], { encoding: "utf8" }).trim();
// rfd's task dialog: the first of two buttons is 1000, the second 1001
const dialog = (pid, title, button, first) => { try { return ps("dialog.ps1", [pid, title, button, first ? 1000 : 1001]); } catch (e) { return "ERROR " + e.stdout; } };

let port = PORT + 80;
for (const lang of ["en", "ar"]) {
  const home = makeHome("record-dialogs-" + lang, { ui_lang: lang });
  const a = await open(home, port++, { DAWWIN_TEST_RECORD: TEST });
  await a.ev(`setUiLang(${JSON.stringify(lang)})`);
  const T = lang === "en"
    ? { notice: "Before you record", record: "Record", cancel: "Cancel", close: "Dawwin", keep: "Keep recording", quit: "Stop and quit", consent: "consent of everyone" }
    : { notice: "قبل أن تسجّل", record: "سجّل", cancel: "إلغاء", close: "دَوِّن", keep: "تابع التسجيل", quit: "أوقفه وأغلق", consent: "موافقة جميع المشاركين" };
  // the notice: Cancel, then Record, then never again
  await a.ev(`(window.__notice = window.__TAURI__.core.invoke('recording_notice'), true)`);
  const said = dialog(a.proc.pid, T.notice, T.cancel, false);
  check(`${lang}: the notice says recording may need everyone's consent, and that it stays on this computer`, said.includes(T.consent), said);
  check(`${lang}: Cancel does not record`, (await a.ev(`window.__notice`)) === false);
  await a.ev(`(window.__notice = window.__TAURI__.core.invoke('recording_notice'), true)`);
  dialog(a.proc.pid, T.notice, T.record, true);
  check(`${lang}: Record goes on, and the notice is shown once`, (await a.ev(`window.__notice`)) === true && JSON.parse(readFileSync(join(home, "recording.json"), "utf8")).notice_shown === true);
  check(`${lang}: once shown, it is not shown again`, (await a.invoke("recording_notice")) === true);
  // closing while it records
  const id = await a.invoke("start_recording", { language: "en", steps: null });
  await sleep(3000);
  ps("closewin.ps1", [a.proc.pid]);
  const asked = dialog(a.proc.pid, T.close, T.keep, false);
  check(`${lang}: closing while it records asks first`, asked.length > 20, asked);
  await sleep(1500);
  const still = await a.invoke("recording_state");
  check(`${lang}: Keep recording keeps it going`, still?.id === id && !still.status.ended, still?.status?.seconds);
  ps("closewin.ps1", [a.proc.pid]);
  dialog(a.proc.pid, T.close, T.quit, true);
  let gone = false;
  for (let i = 0; i < 40 && !gone; i++) { await sleep(250); try { process.kill(a.proc.pid, 0); } catch { gone = true; } }
  check(`${lang}: Stop and quit closes Dawwin`, gone);
  const folder = join(home, "recordings");
  const journal = readdirSync(folder).find((f) => f.endsWith(".dawwin-recording.json"));
  const j = journal && JSON.parse(readFileSync(join(folder, journal), "utf8"));
  check(`${lang}: the recording is kept whole, to be transcribed at the next start`, j?.state === "stopped" && readdirSync(folder).some((f) => f.endsWith(".wav")), j?.state);
}

// recordings that would go to OneDrive: the notice is shown again, and says so
{
  const synced = join(OUT, "record-dialogs-synced", "OneDrive");
  mkdirSync(synced, { recursive: true });
  const home = makeHome("record-dialogs-onedrive", { save_dir: synced });
  writeFileSync(join(home, "recording.json"), JSON.stringify({ sources: "both", notice_shown: true }));
  const a = await open(home, port++, { DAWWIN_TEST_RECORD: TEST });
  await a.ev(`(window.__notice = window.__TAURI__.core.invoke('recording_notice'), true)`);
  const said = dialog(a.proc.pid, "Before you record", "Cancel", false);
  check("the notice is shown again when the folder is in OneDrive, and says OneDrive uploads the recording", said.includes("OneDrive will upload"), said);
  a.close();
}

finish();
