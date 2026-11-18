// The Settings page through the real app, clicked as a person does: its sections in the side panel
// in place of the history (sliding over it), one at a time on the page, in English and Arabic, and
// folded to icons; the model in use of a family chosen by the mark on its row; the models folder
// changed with the models and voices moving to it; Back and Escape; the Read aloud page saying how
// to read from any app. With stand-ins for the models (their names and sizes, nothing in them).
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, truncateSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, finish, makeHome, open, OUT, PORT, sleep } from "./lib.mjs";

const home = makeHome("settings-home");
const models = join(home, "models");
const fake = (name, size) => {
  const p = join(models, name);
  writeFileSync(p, Buffer.from("GGUF\x03\0\0\0", "binary"));
  execFileSync("fsutil", ["sparse", "setflag", p], { stdio: "ignore" });
  truncateSync(p, size);
};
fake("MiLMMT-46-4B-v1.0.Q4_K_M.gguf", 2489893152);
fake("MiLMMT-46-1B-v1.0.Q4_K_M.gguf", 806057408);
fake("Ministral-3-3B-Instruct-2512-Q4_K_M.gguf", 2147023008);
fake("Jais-3-0.9B-Q4_K_M.gguf", 666192384);
mkdirSync(join(models, "voices", "phonon-31m"), { recursive: true });
writeFileSync(join(models, "voices", "phonon-31m", "voice.bin"), "a voice");

const a = await open(home, PORT + 90);
/** Pressed, held a moment, let go, at the middle of what `sel` finds. */
const press = async (sel, hold = 120) => {
  const r = await a.ev(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; const b = e.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
  if (!r) return false;
  await a.call("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x, y: r.y });
  await a.call("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: 1 });
  await sleep(hold);
  await a.call("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: 1 });
  return true;
};
const key = async (k, code, vk) => {
  await a.call("Input.dispatchKeyEvent", { type: "keyDown", key: k, code, windowsVirtualKeyCode: vk });
  await a.call("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk });
};
const inSettings = () => a.ev(`document.querySelector('.sidebar').classList.contains('in-settings')`);

await a.ev(`showView('home')`);
await sleep(400);
await press("#btn-settings");
await sleep(140);
await a.shot(join(OUT, "settings-sliding.png"));
await sleep(500);
check("Settings: its sections take the place of the history in the side panel",
  (await inSettings()) && (await a.ev(`document.getElementById('side-main').inert && !document.getElementById('side-set').inert`)));
const pages = await a.ev(`[...document.querySelectorAll('#set-nav [role=tab]')].map((b) => b.dataset.page)`);
check("nine sections, each with its icon", pages.length === 9 && (await a.ev(`document.querySelectorAll('#set-nav svg').length`)) === 9, pages);
for (const p of pages) {
  await press(`#set-nav [data-page="${p}"]`, 60);
  await sleep(320);
  const shown = await a.ev(`[...document.querySelectorAll('.set-page > [data-page]')].filter((s) => s.offsetParent).map((s) => s.dataset.page)`);
  const title = await a.ev(`document.getElementById('set-page-title').textContent`);
  check(`${p}: its own sections alone, under its name`, shown.length > 0 && shown.every((x) => x === p) && title.length > 0, { shown, title });
}

// the model in use: the mark on its row, in Settings
for (const [page, box, family] of [["translate", "#set-translators", "translator"], ["minutes", "#set-minutes", "minutes"]]) {
  await press(`#set-nav [data-page="${page}"]`, 60);
  await sleep(400);
  const marks = await a.ev(`[...document.querySelectorAll('${box} .pick')].map((p) => ({ id: p.closest('.setting').dataset.id, on: p.checked, can: !p.disabled }))`);
  check(`${family}: each model has a mark, the two here can be chosen, one is in use`, marks.length === 3 && marks.filter((m) => m.can).length === 2 && marks.filter((m) => m.on).length === 1, marks);
  const other = marks.find((m) => m.can && !m.on);
  await press(`${box} [data-id="${other.id}"] .pick`, 60);
  await sleep(900);
  const now = (await a.invoke("get_state")).setup.families[family];
  check(`${family}: a click on another's mark puts it in use`, now.variant === other.id, now.variant);
}

// up and down move between the sections
await a.ev(`document.querySelector('#set-nav [aria-selected=true]').focus()`);
await key("ArrowDown", "ArrowDown", 40);
await sleep(300);
check("the arrow keys move between the sections", (await a.ev(`SET.page`)) === "read", await a.ev(`SET.page`));

// the models folder: what is in it, then the models and voices moving to another
await press('#set-nav [data-page="files"]', 60);
await sleep(400);
const shownDir = await a.ev(`document.getElementById('set-models-dir').textContent`);
const held = await a.ev(`document.getElementById('set-models-held').textContent`);
check("the models folder is named, with how much is in it", shownDir === models && /GB/.test(held), { shownDir, held });
const to = join(OUT, "settings-models-moved");
rmSync(to, { recursive: true, force: true });
await a.invoke("set_models_dir_to_path", { path: to });
let st;
for (let i = 0; i < 160; i++) {
  await sleep(250);
  st = (await a.invoke("get_state")).setup;
  if (!st.moving) break;
}
check("the models and the voices move to the new folder, and the folder changes",
  st.download_dir === to && existsSync(join(to, "MiLMMT-46-1B-v1.0.Q4_K_M.gguf")) && existsSync(join(to, "voices", "phonon-31m", "voice.bin")) && !existsSync(join(models, "MiLMMT-46-1B-v1.0.Q4_K_M.gguf")),
  { dir: st.download_dir, moving: st.moving });
check("the models in use are found there", st.families.translator.found && st.families.translator.path.startsWith(to) && st.families.minutes.path.startsWith(to), [st.families.translator.path, st.families.minutes.path]);
check("the page says where they are now", (await a.ev(`document.getElementById('set-models-dir').textContent`)) === to);

// Back, Escape, and the section it was left at
await press("#set-back", 60);
await sleep(500);
check("Back leaves Settings, and the history comes back", (await a.ev(`S.view`)) === "home" && !(await inSettings()));
await press("#btn-settings", 60);
await sleep(500);
check("it opens again at the section last chosen", (await a.ev(`SET.page`)) === "files", await a.ev(`SET.page`));
await a.shot(join(OUT, "settings-en.png"));
await a.ev(`document.activeElement?.blur()`);
await key("Escape", "Escape", 27);
await sleep(500);
check("Escape leaves it too", (await a.ev(`S.view`)) !== "settings" && !(await inSettings()));

// Arabic: right to left, the side panel sliding the other way
await a.ev(`setUiLang('ar')`);
await a.ev(`showView('home')`);
await sleep(400);
await press("#btn-settings");
await sleep(600);
const ar = await a.ev(`({ dir: document.documentElement.dir, first: document.querySelector('#set-nav [role=tab] span').textContent, title: document.getElementById('set-page-title').textContent, back: document.querySelector('#set-back span').textContent })`);
check("in Arabic: right to left, and in Arabic", ar.dir === "rtl" && /[؀-ۿ]/.test(ar.first + ar.title + ar.back), ar);
await a.shot(join(OUT, "settings-ar.png"));

// folded: the sections stay, as icons
await a.ev(`foldSidebar(true)`);
await sleep(400);
check("folded, the sections stay, as icons", (await a.ev(`document.querySelectorAll('#set-nav [role=tab]').length`)) === 9 && (await a.ev(`document.querySelector('.sidebar').offsetWidth`)) < 80);
await a.shot(join(OUT, "settings-folded.png"));
await a.ev(`foldSidebar(false)`);
await a.ev(`setUiLang('en')`);

// Read aloud: how to read from any app, with its shortcut and the player's keys
await a.ev(`showView('read')`);
await sleep(900);
const any = await a.ev(`({ text: document.getElementById('read-anywhere')?.textContent || '', kbd: document.querySelectorAll('#read-anywhere kbd').length })`);
check("Read aloud says how to read from any app, with the shortcut and the keys", /any app/i.test(any.text) && any.kbd >= 5, any);
await press('#read-anywhere [data-any="change"]', 60);
await sleep(500);
check("Change the shortcut opens its section of Settings", (await a.ev(`S.view`)) === "settings" && (await a.ev(`SET.page`)) === "screen");
await a.ev(`showView('read')`);
await sleep(400);
await a.shot(join(OUT, "read-empty.png"));
a.close();
finish();
