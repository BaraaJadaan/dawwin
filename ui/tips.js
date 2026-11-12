// Short descriptions on hover, where a name cannot say everything: what a step does, what an
// export format is, what the status at the top means. They are the system's own (the `title`
// attribute, which the webview shows as a native tooltip), one sentence each, from the string
// tables, in the interface's language. The same text is given to assistive technology
// (`aria-description`), so it does not depend on a mouse. A button that is only a picture shows
// its name on hover. Controls whose name says it all get nothing.
"use strict";

/** What is described: an element's id, the key of its text, and whether the hover goes on the
 *  label around it (a tick box and its words) rather than the control itself. */
const TIPS = [
  ["step-silence", "tip.silence", true],
  ["step-save-audio", "tip.saveAudio", true],
  ["step-transcribe", "tip.transcribe", true],
  ["step-translate", "tip.translate", true],
  ["step-video", "tip.video", true],
  ["step-video-how", "tip.videoHow"],
  ["chip-status", "tip.status"],
  ["follow", "tip.follow", true],
  ["stamps", "tip.stamps", true],
  ["t-lang", "tip.translation"],
  ["btn-fix-timing", "tip.fixTiming"],
  ["btn-retranslate", "tip.retranslate"],
  ["export-video", "tip.exp.video"],
  ["export-burn", "tip.exp.burn"],
  ["export-audio", "tip.exp.audio"],
  ["export-audio-short", "tip.exp.audioShort"],
  ["run-on", "tip.runOn"],
  ["set-save-where", "tip.saveWhere"],
  ["home-folder", "tip.folder"],
  // what came with 1.4 (strings in who.js)
  ["step-speakers", "tip.speakers", true],
  ["spoken", "tip.spoken"],
  ["search-who", "tip.searchWho"],
  ["btn-more", "tip.more"],
  ["words-add", "tip.words"],
  ["look-font", "tip.font"],
  ["look-sharp", "tip.sharp", true],
  ["look-preview", "tip.look"],
];
/** The export formats, by their button's data-fmt. */
const FORMAT_TIPS = ["docx", "txt", "md", "srt", "vtt", "csv", "tsv", "json"];
/** Buttons that are only a picture: their name (aria-label) is shown on hover. */
const PICTURE_BUTTONS = ".icon-btn, #btn-play, #btn-speed, #btn-mute, .side-toggle";

function describe(el, text) {
  if (!el) return;
  if (el.getAttribute("title") !== text) el.setAttribute("title", text);
  if (el.getAttribute("aria-description") !== text) el.setAttribute("aria-description", text);
}

function nameOnHover(root) {
  const els = root.matches?.(PICTURE_BUTTONS) ? [root] : [...(root.querySelectorAll?.(PICTURE_BUTTONS) || [])];
  for (const el of els) {
    const name = el.getAttribute("aria-label");
    if (name && el.getAttribute("title") !== name) el.setAttribute("title", name);
  }
}

/** The spoken-language menus: on "Arabic", that the dialects are covered and English words are
 *  written in English. */
function tipLanguages() {
  for (const menu of [$("spoken"), $("setup-spoken")]) {
    if (!menu) continue;
    const ar = menu.querySelector('option[value="ar"]');
    if (ar) ar.title = t("tip.arabic");
    if (menu.value === "ar") describe(menu, t("tip.arabic"));
    else { menu.removeAttribute("title"); menu.removeAttribute("aria-description"); }
  }
}

function applyTips() {
  for (const [id, key, onLabel] of TIPS) {
    const el = $(id);
    if (!el) continue;
    const text = t(key);
    if (onLabel) {
      const label = el.closest("label");
      if (label) label.title = text;
      el.setAttribute("aria-description", text);
    } else describe(el, text);
  }
  for (const fmt of FORMAT_TIPS) describe(document.querySelector(`#export-menu [data-fmt="${fmt}"]`), t(`tip.exp.${fmt}`));
  tipLanguages();
  nameOnHover(document.body);
}

// in step with the language, and with what the page draws later
if (typeof applyI18n === "function") {
  const drawn = applyI18n;
  applyI18n = function () { drawn.apply(this, arguments); applyTips(); };
}
new MutationObserver((changes) => {
  for (const c of changes) {
    if (c.type === "attributes") nameOnHover(c.target);
    else for (const n of c.addedNodes) if (n.nodeType === 1) nameOnHover(n);
    if (c.target.id === "spoken" || c.target.id === "setup-spoken") tipLanguages();
  }
}).observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ["aria-label"] });
for (const id of ["spoken", "setup-spoken"]) $(id)?.addEventListener("change", tipLanguages);
applyTips();
