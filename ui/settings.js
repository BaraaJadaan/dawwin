// The Settings page: its sections take the place of the history in the side panel, which slides
// over to them, and the page shows one section at a time. Also the models folder (with its move
// to another disk), the size of the interface, and the model in use of each family, chosen by a
// mark on its row. Loaded after app.js, whose functions it builds on.
"use strict";

Object.assign(I18N.en.s, {
  "set.back": "Back",
  "set.p.general": "General",
  "set.p.transcribe": "Transcription",
  "set.p.translate": "Translation",
  "set.p.minutes": "Minutes",
  "set.p.read": "Read aloud",
  "set.p.screen": "Read the screen",
  "set.p.subtitles": "Subtitles",
  "set.p.files": "Files and storage",
  "set.p.about": "About",
  "set.zoom": "Size of the interface",
  "set.zoomIn": "Larger",
  "set.zoomOut": "Smaller",
  "set.translateModels": "Translation models",
  "set.tools": "Tools",
  "set.names": "Speaker names in subtitles",
  "set.modelsFolder": "Where models are kept",
  "set.modelsHeld": "{size} of models and voices here",
  "set.modelsNone": "No models here yet",
  "set.modelsChangeTip": "Every model and voice you have moves to the folder you choose: one on another disk frees the space here",
  "set.moving": "Moving the models to {to}: {pct}%",
  "set.moveFailed": "The models could not be moved, and stayed where they were: {err}",
  "set.pickTip": "Use this one",
  "toast.modelsMoved": "The models are in {to} now.",
  "t.tmodel": "Translation model",
  "t.tmodelTip": "Which of your translation models translates",
  "min.model": "Minutes model",
  "min.modelTip": "Which of the minutes models you have writes them",
});

Object.assign(I18N.ar.s, {
  "set.back": "رجوع",
  "set.p.general": "عام",
  "set.p.transcribe": "التفريغ",
  "set.p.translate": "الترجمة",
  "set.p.minutes": "المحاضر",
  "set.p.read": "القراءة بصوت عالٍ",
  "set.p.screen": "قراءة الشاشة",
  "set.p.subtitles": "الترجمة على الفيديو",
  "set.p.files": "الملفات والتخزين",
  "set.p.about": "حول",
  "set.zoom": "حجم الواجهة",
  "set.zoomIn": "أكبر",
  "set.zoomOut": "أصغر",
  "set.translateModels": "نماذج الترجمة",
  "set.tools": "الأدوات",
  "set.names": "أسماء المتحدثين في الترجمة على الفيديو",
  "set.modelsFolder": "مكان حفظ النماذج",
  "set.modelsHeld": "{size} من النماذج والأصوات هنا",
  "set.modelsNone": "لا نماذج هنا بعد",
  "set.modelsChangeTip": "تنتقل كل النماذج والأصوات التي لديك إلى المجلد الذي تختاره، واختيار مجلد على قرص آخر يُفرغ المساحة هنا",
  "set.moving": "جارٍ نقل النماذج إلى {to}: {pct}٪",
  "set.moveFailed": "تعذّر نقل النماذج، وبقيت في مكانها: {err}",
  "set.pickTip": "استخدم هذا",
  "toast.modelsMoved": "صارت النماذج في {to}.",
  "t.tmodel": "نموذج الترجمة",
  "t.tmodelTip": "أيّ نماذج الترجمة التي لديك يترجم",
  "min.model": "نموذج المحضر",
  "min.modelTip": "أيّ نماذج المحاضر التي لديك يكتبها",
});

/** The sections, in the side panel's order, with their icons (24 × 24). */
const SET_PAGES = [
  ["general", "M3 17v2h6v-2H3ZM3 5v2h10V5H3Zm10 16v-2h8v-2h-8v-2h-2v6h2ZM7 9v2H3v2h4v2h2V9H7Zm14 4v-2H11v2h10Zm-6-4h2V7h4V5h-4V3h-2v6Z"],
  ["transcribe", "M12 14a3 3 0 0 0 3-3V5a3 3 0 1 0-6 0v6a3 3 0 0 0 3 3Zm5-3a5 5 0 0 1-10 0H5a7 7 0 0 0 6 6.92V21h2v-3.08A7 7 0 0 0 19 11h-2Z"],
  ["translate", "m12.87 15.07-2.54-2.51.03-.03A17.5 17.5 0 0 0 14.07 6H17V4h-7V2H8v2H1v2h11.17C11.5 7.92 10.44 9.75 9 11.35 8.07 10.32 7.3 9.19 6.69 8h-2c.73 1.63 1.73 3.17 2.98 4.56l-5.09 5.02L4 19l5-5 3.11 3.11.76-2.04ZM18.5 10h-2L12 22h2l1.12-3h4.75L21 22h2l-4.5-12Zm-2.62 7 1.62-4.33L19.12 17h-3.24Z"],
  ["minutes", "M19 3h-4.18A3 3 0 0 0 12 1a3 3 0 0 0-2.82 2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Zm-7 0a1 1 0 1 1 0 2 1 1 0 0 1 0-2Zm2 14H7v-2h7v2Zm3-4H7v-2h10v2Zm0-4H7V7h10v2Z"],
  ["read", "M4 9.5v5h3.5L12 19V5L7.5 9.5H4Zm11.5-.5a4 4 0 0 1 0 6l-1.4-1.4a2 2 0 0 0 0-3.2L15.5 9Zm2.5-2.5a7.5 7.5 0 0 1 0 11l-1.4-1.4a5.5 5.5 0 0 0 0-8.2L18 6.5Z"],
  ["screen", "M21 3H3a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h7v2H8v2h8v-2h-2v-2h7a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Zm0 14H3V5h18v12ZM6 8h8v2H6V8Zm0 3h12v2H6v-2Z"],
  ["subtitles", "M20 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2ZM4 12h4v2H4v-2Zm10 6H4v-2h10v2Zm6 0h-4v-2h4v2Zm0-4H10v-2h10v2Z"],
  ["files", "M10 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-8l-2-2Z"],
  ["about", "M11 7h2v2h-2V7Zm0 4h2v6h-2v-6Zm1-9a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16Z"],
];

const SET = { page: store.get("setPage", "general"), back: "home" };
if (!SET_PAGES.some(([id]) => id === SET.page)) SET.page = "general";

function buildSetNav() {
  const nav = $("set-nav");
  if (nav.dataset.lang === UI_LANG && nav.children.length) return;
  nav.dataset.lang = UI_LANG;
  nav.innerHTML = SET_PAGES.map(([id, d]) => `
    <button class="btn btn-ghost" role="tab" data-page="${id}" aria-selected="false" title="${esc(t("set.p." + id))}">
      <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true"><path fill="currentColor" d="${d}"/></svg>
      <span>${esc(t("set.p." + id))}</span>
    </button>`).join("");
  for (const b of nav.children) b.onclick = () => settingsPage(b.dataset.page, true);
  // up and down move between the sections, as in a list
  nav.onkeydown = (ev) => {
    if (ev.key !== "ArrowDown" && ev.key !== "ArrowUp") return;
    ev.preventDefault();
    const all = [...nav.children], at = all.indexOf(document.activeElement);
    const next = all[(at + (ev.key === "ArrowDown" ? 1 : all.length - 1)) % all.length];
    next.focus();
    settingsPage(next.dataset.page, false);
  };
}

/** Show one section of Settings; `user`: chosen in the side panel (it is remembered). */
function settingsPage(id, user) {
  SET.page = id;
  if (user) store.set("setPage", id);
  for (const b of $("set-nav").children) {
    const on = b.dataset.page === id;
    b.setAttribute("aria-selected", String(on));
    b.tabIndex = on ? 0 : -1;
  }
  setText($("set-page-title"), t("set.p." + id));
  for (const sec of document.querySelectorAll(".set-page > [data-page]")) {
    const on = sec.dataset.page === id;
    if (on && !sec.classList.contains("on")) {
      sec.classList.add("on");
    } else if (!on) sec.classList.remove("on");
  }
  $("view-settings").scrollTop = 0;
}

/** Settings at one of its sections (a model to download, a voice, the shortcut). */
function openSettings(page) {
  showView("settings", page);
}

{
  const base = showView;
  // eslint-disable-next-line no-global-assign
  showView = function (v, page) {
    if (v === "settings" && S.view !== "settings") SET.back = ["setup", "settings"].includes(S.view) ? "home" : S.view || "home";
    const side = document.querySelector(".sidebar");
    const on = v === "settings";
    side.classList.toggle("in-settings", on);
    $("side-set").inert = !on;
    $("side-main").inert = on;
    if (on) buildSetNav();
    base(v);
    if (on) settingsPage(page || SET.page, !!page);
  };
}

function leaveSettings() {
  const back = SET.back === "transcript" && !S.current ? "home" : SET.back;
  showView(back);
  $("btn-settings").focus({ preventScroll: true });
}

// ------------------------------------------------------------------ what is drawn on its pages

/** The models folder: where it is, what is in it, and the move to another one while it runs. */
function paintModelsFolder(st) {
  setText($("set-models-dir"), st.download_dir);
  setText($("set-models-held"), st.models_bytes ? t("set.modelsHeld", { size: sizeText(st.models_bytes) }) : t("set.modelsNone"));
  const m = st.moving;
  const going = !!m && !m.error;
  const bar = $("set-move-bar"), status = $("set-move-status");
  bar.hidden = !going;
  if (going) {
    const pct = m.total ? Math.floor((100 * m.done) / m.total) : 0;
    bar.classList.toggle("indeterminate", !m.total);
    bar.firstElementChild.style.width = pct + "%";
    setText(status, t("set.moving", { to: m.to, pct }));
  } else if (m?.error) setText(status, t("set.moveFailed", { err: m.error }));
  status.hidden = !m;
  status.classList.toggle("error-text", !!m?.error);
  $("set-download-dir").disabled = going || st.download.active;
  $("set-download-dir").title = t("set.modelsChangeTip");
}

function paintZoom(st) {
  setText($("set-zoom-value"), `${Math.round((st.zoom || 1) * 100)}%`);
}

async function zoomBy(step) {
  const now = S.snap?.setup?.zoom || 1;
  try { await invoke("set_zoom", { level: step ? now + step : 1 }); onState(await invoke("get_state")); } catch (err) { toast(String(err)); }
}

{
  const base = renderSettings;
  renderSettings = function () {
    base();
    if (!S.snap) return;
    paintModelsFolder(S.snap.setup);
    paintZoom(S.snap.setup);
    // with Dawwin's own ffmpeg, its row below says it all: no line above it with nothing to say
    $("ffmpeg-row").hidden = $("ffmpeg-detail").hidden;
    if (S.view === "settings") setText($("set-page-title"), t("set.p." + SET.page));
  };
}

// the model in use of a family with a choice: a mark on its row, not a button to find
{
  const base = renderRows;
  renderRows = function (box, st, models) {
    base(box, st, models);
    const all = [...st.models, ...toolRows(st, st.tools.map((x) => x.id))];
    for (const row of box.children) {
      const m = all.find((x) => x.id === row.dataset.id);
      if (!m) continue;
      const choice = !m.tool && st.models.filter((x) => x.family === m.family).length > 1;
      let pick = row.querySelector(":scope > .pick");
      if (choice && !pick) {
        pick = document.createElement("input");
        pick.type = "radio";
        pick.className = "pick";
        pick.name = "pick-" + m.family;
        pick.onchange = async () => {
          try { onState(await invoke("use_model", { id: row.dataset.id })); } catch (err) { toast(String(err)); renderSettings(); }
        };
        row.prepend(pick);
      }
      if (pick) {
        pick.hidden = !choice;
        pick.checked = !!m.active;
        pick.disabled = !m.on_disk;
        pick.title = t("set.pickTip");
        pick.setAttribute("aria-label", t("set.pickTip"));
      }
      // the mark says it: no button for it
      const use = row.querySelector(".use");
      if (use) use.hidden = true;
      row.classList.toggle("in-use", choice && !!m.active);
    }
  };
}

// ------------------------------------------------------------------ wiring

function wireSettings() {
  $("set-back").onclick = leaveSettings;
  $("set-zoom-in").onclick = () => zoomBy(0.1);
  $("set-zoom-out").onclick = () => zoomBy(-0.1);
  // Escape leaves Settings, unless it closes a menu or leaves a field first
  document.addEventListener("keydown", (ev) => {
    if (ev.key !== "Escape" || S.view !== "settings" || ev.defaultPrevented) return;
    const el = document.activeElement;
    if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT")) return;
    if (document.querySelector(".menu:not([hidden]), .export-menu:not([hidden])")) return;
    leaveSettings();
  });
  listen("models-moved", (e) => toast(t("toast.modelsMoved", { to: e.payload })));
  // the places that sent people to Settings go to the section that has what they need
  openTranslationSettings = () => openSettings("translate");
}

wireSettings();
