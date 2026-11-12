// Read aloud: open a file, paste text, or listen to a transcript; sentence by sentence, the one
// being read marked, with the player's controls. The sound of each sentence comes from Rust
// (reading/mod.rs), which makes the next ones while this one plays.
"use strict";

const READ = {
  doc: null,
  i: 0,
  playing: false,
  waiting: false,
  error: "",
  token: 0,
  formats: [],
  voices: null,
  lastUserScroll: 0,
  audio: new Audio(),
  // what the tests read: when play was pressed, and when the sound began
  pressedAt: 0,
  startedAt: 0,
};
window.__readAloud = READ;

const readEl = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "on") for (const [ev, f] of Object.entries(v)) e.addEventListener(ev, f);
    else if (k === "text") e.textContent = v;
    else if (v !== undefined && v !== null && v !== false) e.setAttribute(k, v === true ? "" : v);
  }
  for (const kid of kids) if (kid !== null && kid !== undefined) e.append(kid);
  return e;
};

const ICON = {
  play: '<svg class="i-play" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5Z"/></svg><svg class="i-pause" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z"/></svg>',
  back: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M6 5h2v14H6zM20 5.5v13a1 1 0 0 1-1.5.86l-9-6.5a1 1 0 0 1 0-1.72l9-6.5A1 1 0 0 1 20 5.5Z"/></svg>',
  forward: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M16 5h2v14h-2zM4 5.5v13a1 1 0 0 0 1.5.86l9-6.5a1 1 0 0 0 0-1.72l-9-6.5A1 1 0 0 0 4 5.5Z"/></svg>',
  speaker: '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M4 9.5v5h3.5L12 19V5L7.5 9.5H4Zm11.5-.5a4 4 0 0 1 0 6l-1.4-1.4a2 2 0 0 0 0-3.2L15.5 9Zm2.5-2.5a7.5 7.5 0 0 1 0 11l-1.4-1.4a5.5 5.5 0 0 0 0-8.2L18 6.5Z"/></svg>',
};

function readSpeed() {
  return Number(store.get("readSpeed", "1")) || 1;
}

// ------------------------------------------------------------------ the page

function buildReadView() {
  const v = $("view-read");
  if (v.dataset.lang === UI_LANG && v.children.length) return;
  v.dataset.lang = UI_LANG;
  v.innerHTML = "";
  const empty = readEl("div", { class: "read-empty", id: "read-empty" },
    readEl("div", { class: "dropzone read-drop", id: "read-drop", tabindex: "0", role: "button", on: { click: pickReadFile, keydown: (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pickReadFile(); } } } },
      readEl("div", { class: "dz-icon" }),
      readEl("h1", { text: t("read.drop") }),
      readEl("p", {}, readEl("span", { class: "link", text: t("read.open") }), " · ", t("read.formats"))),
    readEl("div", { class: "card read-paste" },
      readEl("textarea", { id: "read-text", class: "input", dir: "auto", placeholder: t("read.paste"), "aria-label": t("read.paste") }),
      readEl("div", { class: "read-paste-row" }, readEl("button", { class: "btn btn-primary", id: "read-text-go", text: t("read.readIt"), on: { click: () => {
        const text = $("read-text").value;
        if (text.trim()) openReading({ text });
      } } }))));
  empty.querySelector(".dz-icon").innerHTML = ICON.speaker.replace('width="16" height="16"', 'width="44" height="44"');
  const doc = readEl("div", { class: "read-doc", id: "read-doc", hidden: true },
    readEl("div", { class: "t-head read-head" },
      readEl("div", { class: "t-title-row" },
        readEl("h1", { class: "t-title", id: "read-title", dir: "auto" }),
        readEl("div", { class: "read-head-tools" },
          readEl("select", { class: "input select select-sm", id: "read-chapters", title: t("scr.chaptersTip"), "aria-label": t("scr.chapters"), hidden: true, on: { change: (e) => goToChapter(Number(e.target.value)) } }),
          readEl("button", { class: "btn btn-sm btn-ghost", id: "read-save", text: t("scr.saveSound"), title: t("scr.saveSoundTip"), on: { click: saveReadingSound } }),
          readEl("button", { class: "btn btn-sm btn-ghost", id: "read-other", text: t("read.other"), on: { click: closeReading } }))),
      readEl("div", { class: "toolbar read-voices", id: "read-voices" })),
    readEl("div", { class: "read-text-box", id: "read-body", on: {
      wheel: () => (READ.lastUserScroll = Date.now()),
      touchmove: () => (READ.lastUserScroll = Date.now()),
      // a click on a sentence reads from there
      click: (e) => { const rs = e.target.closest?.(".rs"); if (rs) readFrom(Number(rs.dataset.i)); },
    } }),
    readEl("div", { class: "error read-error", id: "read-error", dir: "auto", hidden: true },
      readEl("span", { id: "read-error-text" }),
      readEl("button", { class: "btn btn-sm", text: t("read.again"), on: { click: () => readFrom(READ.i) } })),
    readEl("div", { class: "player read-player", id: "read-player" },
      readEl("div", { class: "controls", dir: "ltr" },
        readEl("button", { class: "icon-btn", id: "read-back", title: t("read.backTip"), "aria-label": t("read.back"), on: { click: () => readStep(-1) } }),
        readEl("button", { class: "play", id: "read-play", title: t("read.play"), "aria-label": t("read.play"), on: { click: togglePlay } }),
        readEl("button", { class: "icon-btn", id: "read-forward", title: t("read.forwardTip"), "aria-label": t("read.forward"), on: { click: () => readStep(1) } }),
        readEl("span", { class: "muted small read-pos", id: "read-pos", dir: "auto" }),
        readEl("span", { class: "muted small", id: "read-status", dir: "auto" }),
        readEl("span", { class: "grow" }),
        readEl("button", { class: "speed", id: "read-speed", title: t("read.speed"), "aria-label": t("read.speed"), on: { click: () => {
          const next = SPEEDS[(SPEEDS.indexOf(READ.audio.playbackRate) + 1) % SPEEDS.length] || 1;
          READ.audio.playbackRate = READ.audio.defaultPlaybackRate = next;
          store.set("readSpeed", String(next));
          paintReadPlayer();
        } } }))));
  v.append(empty, doc);
  $("read-back").innerHTML = ICON.back;
  $("read-play").innerHTML = ICON.play;
  $("read-forward").innerHTML = ICON.forward;
  if (READ.doc) renderReadDoc();
}

function paintReadPlayer() {
  if (!$("read-player")) return;
  const a = READ.audio;
  $("read-player").classList.toggle("playing", READ.playing && !a.paused || READ.waiting);
  $("read-speed").textContent = `${a.playbackRate}×`;
  const n = READ.doc ? READ.doc.sentences.length : 0;
  $("read-pos").textContent = n ? t("read.position", { n: READ.i + 1, total: n }) : "";
  $("read-status").textContent = READ.waiting ? t("read.preparing") : "";
  $("read-back").disabled = !n || (READ.i === 0 && a.currentTime < 1.5);
  $("read-forward").disabled = !n || READ.i + 1 >= n;
  $("read-error").hidden = !READ.error;
  $("read-error-text").textContent = READ.error;
}

function voiceLabel(v) {
  return `${v.model} · ${t(v.woman ? "read.woman" : "read.man", { n: v.number })}`;
}

async function loadVoices() {
  try { READ.voices = await invoke("reading_voices"); } catch { READ.voices = null; }
  return READ.voices;
}

function renderVoicePickers() {
  const box = $("read-voices");
  if (!box) return;
  box.innerHTML = "";
  const vs = READ.voices;
  if (!vs) return;
  for (const lang of ["ar", "en"]) {
    const mine = vs.voices.filter((v) => v.lang === lang);
    const label = readEl("label", { class: "read-voice small", title: t("read.voiceTip", { lang: t("read.lang." + lang) }) }, readEl("span", { class: "muted", text: t("read.voice." + lang) }));
    if (!mine.length) {
      label.append(readEl("button", { class: "linkbtn", text: t("read.getVoice"), on: { click: () => showView("settings") } }));
    } else {
      const sel = readEl("select", { class: "input select select-sm", "aria-label": t("read.voice." + lang) });
      for (const v of mine) sel.append(readEl("option", { value: v.key, text: voiceLabel(v) }));
      sel.value = vs.chosen[lang] || mine[0].key;
      sel.onchange = async () => {
        try {
          READ.voices = await invoke("reading_choose_voice", { lang, key: sel.value });
          // what is playing is made again in the new voice
          if (READ.playing) readFrom(READ.i);
        } catch (err) { toast(String(err)); }
      };
      label.append(sel);
    }
    box.append(label);
  }
}

function renderReadDoc() {
  const d = READ.doc;
  $("read-empty").hidden = !!d;
  $("read-doc").hidden = !d;
  if (!d) return;
  $("read-title").textContent = d.title;
  const body = $("read-body");
  body.innerHTML = "";
  body.dir = d.lang === "ar" ? "rtl" : "ltr";
  body.lang = d.lang;
  // a long book has thousands of sentences: plain elements in one go, one click handler for all
  const frag = document.createDocumentFragment();
  let block = -1, el = null;
  d.sentences.forEach((s, i) => {
    if (s.block !== block) {
      block = s.block;
      el = document.createElement(s.kind === "heading" ? "h2" : s.kind === "item" ? "li" : "p");
      el.className = "read-block read-" + s.kind;
      // the paragraph's own direction: its sentences are isolated, so "auto" would see none of them
      const ar = (s.text.match(/[؀-ۿ]/g) || []).length, en = (s.text.match(/[A-Za-z]/g) || []).length;
      el.dir = ar || en ? (ar >= en ? "rtl" : "ltr") : d.lang === "ar" ? "rtl" : "ltr";
      frag.append(el);
    } else el.append(" ");
    const span = document.createElement("span");
    span.className = "rs";
    span.dir = "auto";
    span.dataset.i = String(i);
    span.textContent = s.text;
    el.append(span);
  });
  body.append(frag);
  // a book's chapters, to go to one (1.8.0: e-books)
  const chapters = $("read-chapters");
  chapters.innerHTML = "";
  const heads = d.sentences.map((s, i) => (s.kind === "heading" ? i : -1)).filter((i) => i >= 0);
  for (const i of heads) chapters.append(readEl("option", { value: String(i), text: d.sentences[i].text }));
  chapters.hidden = heads.length < 2;
  renderVoicePickers();
  markSentence(false);
  paintReadPlayer();
}

function markSentence(scroll = true) {
  const body = $("read-body");
  if (!body) return;
  body.querySelector(".rs.now")?.classList.remove("now");
  const el = body.querySelector(`.rs[data-i="${READ.i}"]`);
  if (!el) return;
  el.classList.add("now");
  // the chapter list says which chapter this is
  const chapters = $("read-chapters");
  if (chapters && !chapters.hidden) {
    const at = [...chapters.options].map((o) => Number(o.value)).filter((i) => i <= READ.i).pop();
    if (at !== undefined) chapters.value = String(at);
  }
  // the page follows the reading, unless the reader scrolled away a moment ago
  if (scroll && Date.now() - READ.lastUserScroll > 4000) el.scrollIntoView({ block: "center", behavior: "smooth" });
}

// ------------------------------------------------------------------ reading

async function openReading(source) {
  stopReading();
  try {
    const doc = await invoke("reading_open", { path: null, text: null, transcript: null, language: null, ...source });
    READ.doc = doc;
    READ.i = Math.min(doc.place || 0, doc.sentences.length - 1);
    READ.error = "";
    if (S.view !== "read") showView("read");
    await loadVoices();
    buildReadView();
    renderReadDoc();
    markSentence(true);
  } catch (err) {
    // a PDF wants PDFium first (1.8.0): a download of its own, asked for once
    if (String(err) === "needs-pdfium" && source.path) return getPdfium(source);
    toast(String(err));
  }
}

async function getPdfium(source) {
  if (!confirm(t("scr.pdfiumAsk"))) return;
  toast(t("scr.pdfiumGetting"));
  try {
    await invoke("screen_install_pdfium");
    openReading(source);
  } catch (err) {
    toast(String(err));
  }
}

function goToChapter(i) {
  if (!READ.doc) return;
  if (READ.playing || READ.waiting) return readFrom(i);
  READ.i = i;
  READ.audio.removeAttribute("src");
  READ.lastUserScroll = 0;
  markSentence();
  paintReadPlayer();
  invoke("reading_place", { doc: READ.doc.id, index: i }).catch(() => {});
}

/** The reading saved as a sound file (1.8.0): a job in the list, after asking where. */
async function saveReadingSound() {
  if (!READ.doc) return;
  try {
    const id = await invoke("reading_save_sound", { doc: READ.doc.id });
    if (id) toast(t("scr.saving"));
  } catch (err) {
    toast(String(err));
  }
}

function closeReading() {
  stopReading();
  READ.doc = null;
  buildReadView();
  renderReadDoc();
}

function stopReading() {
  READ.token++;
  READ.playing = false;
  READ.waiting = false;
  READ.audio.pause();
  invoke("reading_stop").catch(() => {});
  paintReadPlayer();
}

async function readFrom(i) {
  const d = READ.doc;
  if (!d || i < 0 || i >= d.sentences.length) return;
  const my = ++READ.token;
  READ.i = i;
  READ.playing = true;
  READ.error = "";
  READ.audio.pause();
  READ.pressedAt = performance.now();
  READ.startedAt = 0;
  markSentence();
  // "preparing" only when it takes a moment
  const slow = setTimeout(() => { if (my === READ.token) { READ.waiting = true; paintReadPlayer(); } }, 250);
  paintReadPlayer();
  try {
    const ready = await invoke("reading_sentence", { doc: d.id, index: i });
    if (my !== READ.token) return;
    READ.audio.src = fileSrc(ready.path);
    READ.audio.playbackRate = READ.audio.defaultPlaybackRate = readSpeed();
    READ.audio.preservesPitch = true;
    READ.device = ready.device;
    await READ.audio.play();
    invoke("reading_place", { doc: d.id, index: i }).catch(() => {});
  } catch (err) {
    if (my !== READ.token) return;
    READ.playing = false;
    READ.error = String(err);
  } finally {
    clearTimeout(slow);
    if (my === READ.token) { READ.waiting = false; paintReadPlayer(); }
  }
}

function togglePlay() {
  if (!READ.doc) return;
  const a = READ.audio;
  if (READ.playing && !a.paused) { a.pause(); READ.playing = false; paintReadPlayer(); return; }
  if (READ.waiting) { stopReading(); return; }
  // paused in the middle of a sentence: go on from there
  if (a.src && a.currentTime > 0 && !a.ended && a.dataset?.i === String(READ.i)) { READ.playing = true; a.play().catch(() => {}); return; }
  readFrom(READ.i);
}

function readStep(d) {
  if (!READ.doc) return;
  // back from the middle of a sentence starts it again
  if (d < 0 && READ.audio.currentTime > 1.5 && READ.playing) return readFrom(READ.i);
  const i = Math.max(0, Math.min(READ.doc.sentences.length - 1, READ.i + d));
  if (READ.playing || READ.waiting) readFrom(i);
  else { READ.i = i; READ.audio.removeAttribute("src"); markSentence(); paintReadPlayer(); invoke("reading_place", { doc: READ.doc.id, index: i }).catch(() => {}); }
}

READ.audio.addEventListener("playing", () => {
  if (!READ.startedAt) READ.startedAt = performance.now();
  paintReadPlayer();
});
READ.audio.addEventListener("loadeddata", () => { READ.audio.dataset.i = String(READ.i); });
READ.audio.addEventListener("pause", paintReadPlayer);
READ.audio.addEventListener("ended", () => {
  if (!READ.playing || !READ.doc) return;
  if (READ.i + 1 < READ.doc.sentences.length) readFrom(READ.i + 1);
  else {
    // the end: next time it starts from the beginning
    READ.playing = false;
    invoke("reading_place", { doc: READ.doc.id, index: READ.doc.sentences.length - 1 }).catch(() => {});
    paintReadPlayer();
  }
});

async function pickReadFile() {
  const path = await invoke("reading_pick_file");
  if (path) openReading({ path });
}

/** Dropped files: the first that can be read aloud is opened. True when one was. */
async function readDropped(paths) {
  if (!READ.formats.length) READ.formats = await invoke("reading_formats");
  const doc = paths.find((p) => READ.formats.includes(extOf(p)));
  if (doc) { openReading({ path: doc }); return true; }
  if (S.view === "read" && paths.length) toast(t("read.unreadable", { name: paths[0].split(/[\\/]/).pop() }));
  return S.view === "read";
}

// ------------------------------------------------------------------ settings

let readArmedDelete = null;
async function renderReadingSettings() {
  const box = $("set-reading");
  if (!box) return;
  const vs = await loadVoices();
  if (!vs) return;
  box.innerHTML = "";
  box.append(readEl("h2", { text: t("read.title") }));
  for (const m of vs.models) {
    const row = readEl("div", { class: "setting", "data-id": m.id });
    const status = m.installed ? t("set.modelOnDisk") : m.downloading ? t("set.modelGoing", { pct: Math.floor((100 * (READ.dl?.[m.id]?.done || m.partial)) / m.size) })
      : m.partial > 0 ? t("set.modelPaused", { pct: Math.floor((100 * m.partial) / m.size) }) : t("set.modelNone");
    const text = readEl("div", { class: "setting-text" },
      readEl("h3", {}, m.name, " ", readEl("span", { class: "muted small", text: sizeText(m.size) })),
      readEl("p", { class: "muted small", text: t("read.about." + m.id) }),
      readEl("p", { class: "muted small" }, readEl("a", { class: "link", href: "#", text: `${t("read.license")}: ${m.license}`, on: { click: (e) => { e.preventDefault(); invoke("open_url", { url: m.license_url }); } } })),
      readEl("p", { class: "muted small status", text: READ.dl?.[m.id]?.state === "program" ? t("read.programFirst", { pct: Math.floor((100 * READ.dl[m.id].done) / (READ.dl[m.id].total || 1)) }) : status }));
    row.append(text);
    for (const [k, lang] of ["ar", "en"].entries()) {
      if (m.samples[k]) {
        row.append(readEl("button", { class: "btn btn-sm btn-ghost", text: t("read.hear." + lang), on: { click: () => {
          stopReading();
          READ.sample = READ.sample || new Audio();
          READ.sample.src = `voices/${m.id}-${lang}.ogg`;
          READ.sample.play().catch(() => {});
        } } }));
      }
    }
    if (!m.installed) {
      row.append(readEl("button", { class: "btn btn-sm", text: t(m.downloading ? "dl.pause" : m.partial > 0 ? "dl.resume" : "dl.download"), on: { click: async () => {
        if (m.downloading) return invoke("reading_pause_voice", { id: m.id });
        if (m.restricted && !m.partial && !confirm(t("read.restricted", { name: m.name }))) { invoke("open_url", { url: m.license_url }); return; }
        try { await invoke("reading_download_voice", { id: m.id }); } catch (err) { toast(String(err)); }
        setTimeout(renderReadingSettings, 300);
      } } }));
    }
    if (m.installed || (m.partial > 0 && !m.downloading)) {
      const del = readEl("button", { class: "btn btn-sm btn-ghost", text: t(readArmedDelete === m.id ? "set.modelDeleteSure" : "set.modelDelete"), on: { click: async () => {
        if (readArmedDelete !== m.id) { readArmedDelete = m.id; renderReadingSettings(); return; }
        readArmedDelete = null;
        try { await invoke("reading_delete_voice", { id: m.id }); } catch (err) { toast(String(err)); }
        renderReadingSettings();
      } } });
      row.append(del);
    }
    box.append(row);
  }
}

// ------------------------------------------------------------------ wiring

/** Tooltips follow the interface language (applyI18n fills text, placeholders and labels). */
function paintReadTips() {
  const listen = $("btn-listen");
  if (listen) listen.title = t("read.listenTip");
}

function wireReader() {
  paintReadTips();
  new MutationObserver(paintReadTips).observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });
  READ.audio.preload = "auto";
  READ.audio.playbackRate = readSpeed();
  $("btn-read").onclick = () => showView("read");
  $("btn-listen")?.addEventListener("click", () => {
    if (!S.current) return;
    // the translation shown under the lines, if there is one; else the transcript itself
    const lang = S.shown || null;
    openReading({ transcript: S.current.id, language: lang });
  });
  document.addEventListener("keydown", (ev) => {
    if (S.view !== "read" || !READ.doc) return;
    const el = ev.target;
    if (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName)) return;
    const rtl = document.documentElement.dir === "rtl";
    if (ev.key === " " && el.tagName !== "BUTTON") { ev.preventDefault(); togglePlay(); }
    else if (ev.key === "ArrowLeft") { ev.preventDefault(); readStep(rtl ? 1 : -1); }
    else if (ev.key === "ArrowRight") { ev.preventDefault(); readStep(rtl ? -1 : 1); }
  });
  // pasting on the page reads what was pasted
  document.addEventListener("paste", (ev) => {
    if (S.view !== "read" || READ.doc || ev.target?.id === "read-text") return;
    const text = ev.clipboardData?.getData("text/plain");
    if (text && text.trim()) { ev.preventDefault(); openReading({ text }); }
  });
  // the small player reads a selection: one voice at a time (1.8.0)
  listen("reading-yield", () => { if (READ.playing || READ.waiting) stopReading(); });
  listen("voice-download", (e) => {
    READ.dl = READ.dl || {};
    READ.dl[e.payload.id] = e.payload;
    if (e.payload.error) toast(e.payload.error);
    if (S.view === "settings") renderReadingSettings();
    if (e.payload.state === "done") loadVoices().then(renderVoicePickers);
  });
}

/** Called by showView: the page is built when first shown, and reading stops when it is left. */
function onReadView(shown) {
  if (shown) { buildReadView(); renderReadDoc(); }
  else if (READ.playing || READ.waiting) stopReading();
}
