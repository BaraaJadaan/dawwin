// Dawwin UI. Plain JS over Tauri's global API (no bundler).
"use strict";

const T = window.__TAURI__;
const invoke = T.core.invoke;
const listen = T.event.listen;
const fileSrc = (p) => T.core.convertFileSrc(p);

const $ = (id) => document.getElementById(id);
const ACTIVE = new Set(["queued", "decoding", "loading", "transcribing"]);
const FOLDER_FORMATS = ["txt", "md", "docx", "srt", "vtt", "csv", "tsv", "json"];
const MEDIA_EXT = ["m4a", "mp3", "wav", "ogg", "opus", "oga", "flac", "aac", "wma", "amr", "webm", "mp4", "m4v", "mov", "mkv", "3gp"];

const store = {
  get(k, d) { try { return localStorage.getItem(k) ?? d; } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode */ } },
};

const S = {
  snap: null,
  history: [],
  view: "home",
  current: null,      // the transcript on screen
  spoken: store.get("lang", "ar"), // the language of the recordings
  stamps: store.get("stamps", "1") === "1",
  follow: store.get("follow", "0") === "1",
  setupStep: null,     // the setup step the user opened; null = the first one not done
  modelTab: "download", // how to get the model: "download" | "files"
  update: null,        // the last update check: {checking} | {info} | {error}
  hits: [],
  hit: -1,
  audioFor: null,
};

// ------------------------------------------------------------------ formatting

function ts(sec) {
  const s = Math.max(0, Math.floor(sec || 0));
  const p = (n) => String(n).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}` : `${p(Math.floor(s / 60))}:${p(s % 60)}`;
}
function dur(sec) {
  sec = Math.round(sec || 0);
  const and = t("dur.join");
  if (sec < 60) return t("dur.s", { n: sec });
  const m = Math.floor(sec / 60), s = sec % 60;
  if (m < 10) return s ? t("dur.m", { n: m }) + and + t("dur.s", { n: s }) : t("dur.m", { n: m });
  if (m < 60) return t("dur.m", { n: Math.round(sec / 60) });
  const h = Math.floor(m / 60), mm = m % 60;
  return mm ? t("dur.h", { n: h }) + and + t("dur.m", { n: mm }) : t("dur.h", { n: h });
}
function eta(sec) {
  if (sec == null || !isFinite(sec)) return t("eta.estimating");
  if (sec < 50) return t("eta.lessMin");
  return t("eta.mins", { n: Math.ceil(sec / 60) });
}
function date(ms) {
  const opts = { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" };
  return new Intl.DateTimeFormat(I18N[UI_LANG].locale, opts).format(new Date(ms));
}
/** The device a transcript ran on, as the app stored it ("GPU · NVIDIA …", "CPU (int8)"). */
function deviceLabel(d) {
  if (!d) return "";
  if (d.startsWith("GPU · ")) return t("dev.gpu", { name: shortGpu(d.slice(6)) });
  if (d.startsWith("CPU (int8)")) return t("dev.cpuInt8");
  if (d.startsWith("CPU")) return t("dev.cpu");
  return d;
}
function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}
function shortGpu(g) {
  return (g || "").replace(/^NVIDIA\s+/, "").replace(/^GeForce\s+/, "");
}

// ------------------------------------------------------------------ toast

let toastTimer;
function toast(msg, action) {
  const el = $("toast");
  el.innerHTML = "";
  const span = document.createElement("span");
  span.textContent = msg;
  el.append(span);
  if (action) {
    const b = document.createElement("button");
    b.textContent = action.label;
    b.onclick = () => { action.run(); el.hidden = true; };
    el.append(b);
  }
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (el.hidden = true), action ? 7000 : 3500);
}

// ------------------------------------------------------------------ views

function showView(v) {
  S.view = v;
  for (const id of ["home", "setup", "settings", "transcript"]) $("view-" + id).hidden = id !== v;
  $("btn-settings").classList.toggle("active", v === "settings");
  if (v === "settings") renderSettings();
  renderHistory();
  renderQueue();
  if (v !== "transcript") stopAudio();
}

function setupComplete(snap) {
  return snap && snap.setup.env_ready && snap.setup.model.found;
}

// ------------------------------------------------------------------ status chips

function setChip(el, cls, text) {
  el.className = "chip " + (cls || "");
  el.querySelector(".label").textContent = text;
}

function renderChips() {
  const { engine: e, setup: st } = S.snap;
  const ce = $("chip-engine"), cm = $("chip-model");
  if (st.env_busy) setChip(ce, "busy", t("chip.settingUp"));
  else if (!st.env_ready) setChip(ce, "bad", t("chip.noEngine"));
  else if (e.state === "starting") setChip(ce, "busy", t("chip.starting"));
  else if (e.state === "error") setChip(ce, "bad", t("chip.error"));
  else if (st.run_on !== "auto" || e.device === "cpu" || e.cuda === false) {
    setChip(ce, "warn", t(st.run_on === "cpu-exact" ? "chip.cpu-exact" : "chip.cpu-fast"));
  }
  else if (e.cuda === true) setChip(ce, "ok", t("chip.gpu", { gpu: shortGpu(e.gpu) }));
  else setChip(ce, "", t("chip.engineReady"));

  if (st.download.active) {
    const pct = st.download.total ? Math.floor((100 * st.download.done) / st.download.total) : 0;
    setChip(cm, "busy", t("chip.downloading", { pct }));
  } else if (!st.model.found) setChip(cm, "bad", t("chip.noModel"));
  else if (e.state === "loading") setChip(cm, "busy", t("chip.loading"));
  else if (e.state === "loaded") setChip(cm, "ok", t("chip.loaded"));
  else setChip(cm, "ok", t("chip.ready"));
}

// ------------------------------------------------------------------ queue

function jobById(id) {
  return S.snap?.queue.find((j) => j.id === id);
}

function jobLine(j) {
  switch (j.status) {
    case "queued": return t("job.queued");
    case "decoding": return t("job.decoding");
    case "loading": return t("job.loading");
    case "transcribing": return `${Math.floor(j.progress * 100)}% · ${eta(j.eta_s)}`;
    case "done": return t("job.done", { dur: dur(j.elapsed_s) });
    case "cancelled": return t(j.saved ? "job.cancelledSaved" : "job.cancelled");
    case "failed": return t("job.failed", { err: (j.error || t("job.unknown")).split("\n")[0] });
    default: return j.status;
  }
}

function renderQueue() {
  if (!S.snap) return;
  const jobs = S.snap.queue.filter((j) => j.status !== "done");
  $("queue-section").hidden = jobs.length === 0;
  $("queue-cancel-all").hidden = jobs.filter((j) => ACTIVE.has(j.status)).length < 2;
  const ul = $("queue");
  ul.innerHTML = "";
  for (const j of jobs) {
    const li = document.createElement("li");
    li.className = "item" + (S.current?.id === j.id && S.view === "transcript" ? " active" : "") + (j.status === "failed" ? " failed" : "");
    const indet = j.status === "decoding" || j.status === "loading";
    li.innerHTML = `
      <div class="title" dir="auto">${esc(j.title)}</div>
      <div class="sub">${esc(jobLine(j))}</div>
      ${ACTIVE.has(j.status) ? `<div class="progress ${indet ? "indeterminate" : ""}"><div class="bar" style="width:${indet ? "" : (j.progress * 100).toFixed(1) + "%"}"></div></div>` : ""}
      <button class="icon-btn x" aria-label="${esc(t(ACTIVE.has(j.status) ? "job.cancel" : "job.dismiss"))}">×</button>`;
    li.onclick = () => openTranscript(j.id);
    li.querySelector(".x").onclick = (ev) => {
      ev.stopPropagation();
      if (ACTIVE.has(j.status)) invoke("cancel_job", { id: j.id });
      else invoke("dismiss_job", { id: j.id });
    };
    ul.append(li);
  }
}

// ------------------------------------------------------------------ history

async function loadHistory() {
  S.history = await invoke("list_history");
  renderHistory();
}

function renderHistory() {
  const q = $("history-filter").value.trim().toLowerCase();
  const ul = $("history");
  ul.innerHTML = "";
  const items = S.history.filter((h) => !q || h.title.toLowerCase().includes(q) || h.preview.toLowerCase().includes(q));
  for (const h of items) {
    const li = document.createElement("li");
    li.className = "item" + (S.current?.id === h.id && S.view === "transcript" ? " active" : "");
    li.innerHTML = `
      <div class="title" dir="auto">${esc(h.title)}</div>
      <div class="sub"><span>${esc(date(h.created_ms))}</span><span>·</span><span>${esc(dur(h.duration_s))}</span>${h.partial ? `<span>· ${esc(t("hist.partial"))}</span>` : ""}</div>
      <div class="preview" dir="auto">${esc(h.preview)}</div>`;
    li.onclick = () => openTranscript(h.id);
    ul.append(li);
  }
  $("history-empty").hidden = S.history.length > 0;
}

// ------------------------------------------------------------------ setup view

const STEPS = ["engine", "model", "ready"];
function firstOpenStep(st) {
  return !st.env_ready ? "engine" : !st.model.found ? "model" : "ready";
}

/** Download speed over the last half minute, in bytes per second (null until it is known). */
const dlHist = [];
function downloadRate(d) {
  if (!d.active) { dlHist.length = 0; return null; }
  const now = Date.now();
  const last = dlHist[dlHist.length - 1];
  if (!last || last[1] !== d.done) dlHist.push([now, d.done]);
  while (dlHist.length > 2 && now - dlHist[0][0] > 30000) dlHist.shift();
  if (dlHist.length < 2) return null;
  const [t0, b0] = dlHist[0], [t1, b1] = dlHist[dlHist.length - 1];
  return t1 > t0 ? (b1 - b0) / ((t1 - t0) / 1000) : null;
}
function rateText(r) {
  const s = r >= 1048576 ? `${(r / 1048576).toFixed(1)} MB/s` : `${Math.max(1, Math.round(r / 1024))} KB/s`;
  return `\u2066${s}\u2069`; // a left-to-right island, so "324 KB/s" keeps its order in Arabic
}

function renderSetup() {
  const { setup: st, engine: e } = S.snap;
  const m = st.model, d = st.download;
  const done = { engine: st.env_ready, model: m.found, ready: st.env_ready && m.found };
  const busy = { engine: st.env_busy, model: d.active, ready: false };
  const step = S.setupStep || firstOpenStep(st);

  for (const li of document.querySelectorAll(".stepper li")) {
    const k = li.dataset.step;
    li.classList.toggle("done", done[k]);
    li.classList.toggle("active", k === step);
    li.querySelector(".bullet").textContent = done[k] ? "✓" : String(STEPS.indexOf(k) + 1);
    li.querySelector(".s-state").textContent = t(done[k] ? "step.done" : busy[k] ? "step.busy" : "step.todo");
  }
  for (const k of STEPS) $("panel-" + k).hidden = k !== step;

  // 1. engine
  const torch = e.torch ? ` (torch ${e.torch})` : "";
  $("engine-detail").innerHTML = st.env_ready
    ? tHtml(st.env_backend === "cpu" ? "setup.ready.cpu" : "setup.ready.cuda", { torch, dir: ltr(st.env_dir) })
    : tHtml(st.nvidia_driver ? "setup.todo.gpu" : "setup.todo.cpu", { dir: ltr(st.env_dir) }) + (st.uv ? "" : tHtml("setup.uv"));
  const bs = $("btn-setup");
  bs.textContent = t(st.env_busy ? "setup.settingUp" : st.env_ready ? "setup.reinstall" : "setup.setUp");
  bs.className = "btn " + (st.env_ready ? "btn-ghost" : "btn-primary");
  bs.disabled = st.env_busy;
  $("engine-busy").hidden = !st.env_busy;
  $("engine-status").textContent = st.log[st.log.length - 1] || "";
  $("btn-engine-next").hidden = !st.env_ready;
  $("btn-engine-next").className = "btn " + (st.env_ready ? "btn-primary" : "");
  $("setup-log-box").hidden = !st.log.length;
  const log = $("setup-log");
  if (log.dataset.n !== String(st.log.length)) {
    log.textContent = st.log.join("\n");
    log.dataset.n = String(st.log.length);
    log.scrollTop = log.scrollHeight;
  }
  $("setup-error").hidden = !st.env_error;
  $("setup-error").textContent = st.env_error || "";

  // 2. model
  $("model-done").hidden = !m.found;
  $("model-get").hidden = m.found;
  if (m.found) {
    $("model-detail").innerHTML = tHtml("setup.modelReady", { size: (m.bytes / 1e9).toFixed(1), where: m.source === "hf-cache" ? t("setup.fromCache") : "", dir: ltr(m.dir) });
  }
  for (const b of document.querySelectorAll(".tabs button")) b.setAttribute("aria-checked", String(b.dataset.tab === S.modelTab));
  $("tab-download").hidden = S.modelTab !== "download";
  $("tab-files").hidden = S.modelTab !== "files";
  $("token-state").textContent = t(m.token ? `token.${m.token}` : "token.none");
  $("btn-forget-token").hidden = m.token !== "app";
  $("download-dir").textContent = m.download_dir;
  const bd = $("btn-download");
  bd.textContent = t(d.active ? "dl.pause" : d.done > 0 ? "dl.resume" : "dl.download");
  bd.className = "btn " + (d.active ? "" : "btn-primary");
  bd.disabled = !st.env_ready || (!m.token && !d.active);
  $("download-hint").textContent = !st.env_ready ? t("dl.needEngine") : !m.token ? t("dl.needToken") : "";
  $("btn-download-dir").disabled = d.active;
  $("download-box").hidden = !(d.active || d.done > 0);
  const pct = d.total ? (100 * d.done) / d.total : 0;
  $("download-bar").style.width = pct.toFixed(1) + "%";
  let dl = "";
  if (d.total) {
    dl = t("dl.progress", { done: (d.done / 1e9).toFixed(2), total: (d.total / 1e9).toFixed(2), pct: pct.toFixed(0) });
    const rate = downloadRate(d);
    if (d.retry_wait != null) dl += " · " + t("dl.retry", { wait: d.retry_wait, n: d.retry_attempt });
    else if (!d.active) dl += " · " + t("dl.paused");
    else if (rate > 0) dl += ` · ${rateText(rate)} · ` + t("dl.left", { dur: dur((d.total - d.done) / rate) });
  } else if (d.active) dl = t("dl.connecting");
  $("download-text").textContent = dl;
  $("download-error").hidden = !d.error || d.error === "stopped";
  $("download-error").textContent = d.error || "";

  // 3. ready
  const device = e.device === "cpu" || st.run_on !== "auto" || (st.env_backend === "cpu")
    ? t("ready.cpu")
    : t("ready.gpu", { gpu: shortGpu(e.gpu) || "CUDA" });
  $("ready-text").textContent = t("ready.text", { device });
}

// ------------------------------------------------------------------ settings view

function renderSettings() {
  if (!S.snap) return;
  const { setup: st, engine: e } = S.snap;
  const m = st.model;
  $("set-lang").innerHTML = Object.entries(I18N).map(([k, v]) => `<option value="${k}">${esc(v.name)}</option>`).join("");
  $("set-lang").value = UI_LANG;

  // engine
  const torch = e.torch ? ` (torch ${e.torch})` : "";
  $("set-engine").innerHTML = st.env_ready
    ? tHtml(st.env_backend === "cpu" ? "setup.ready.cpu" : "setup.ready.cuda", { torch, dir: ltr(st.env_dir) })
    : esc(t("set.engineMissing"));
  $("set-reinstall").textContent = t(st.env_busy ? "setup.settingUp" : st.env_ready ? "setup.reinstall" : "setup.setUp");
  $("set-reinstall").disabled = st.env_busy;
  const runOn = st.run_on || "auto";
  $("run-on").value = runOn;
  const key = runOn === "auto" ? (st.nvidia_driver ? "runon.d.auto.gpu" : "runon.d.auto.cpu") : `runon.d.${runOn}`;
  $("runon-detail").textContent = t(key) + (e.note ? " " + e.note : "");
  $("ffmpeg-detail").innerHTML = st.ffmpeg ? tHtml("ffmpeg.yes", { path: ltr(st.ffmpeg) }) : tHtml("ffmpeg.no");

  // model
  $("set-model").innerHTML = m.found
    ? tHtml("setup.modelReady", { size: (m.bytes / 1e9).toFixed(1), where: m.source === "hf-cache" ? t("setup.fromCache") : "", dir: ltr(m.dir) })
    : esc(t("set.noModel"));
  $("set-download").hidden = m.found;
  $("set-model-folder").hidden = !m.found;

  // updates
  $("set-version").textContent = t("set.version", { v: st.app_version });
  $("set-auto-check").checked = !!st.check_updates;

  // transcribing a folder
  const formats = $("set-folder-formats");
  if (formats.dataset.lang !== UI_LANG) {
    formats.dataset.lang = UI_LANG;
    formats.innerHTML = FOLDER_FORMATS.map((f) => `<label class="toggle"><input type="checkbox" value="${f}"> <span>${esc(t("exp." + f))}</span></label>`).join("");
  }
  for (const box of formats.querySelectorAll("input")) box.checked = st.folder_formats.includes(box.value);
  const u = S.update, status = $("set-update-status");
  $("set-check").disabled = !!u?.checking;
  if (!u) status.innerHTML = "";
  else if (u.checking) status.textContent = t("set.checking");
  else if (u.error) status.innerHTML = `<span class="error-text">${esc(u.error)}</span>`;
  else if (u.info.newer) {
    status.innerHTML = `${esc(t("set.newer", { v: u.info.latest }))} `
      + `<button class="linkbtn" id="set-get">${esc(t("set.getIt"))}</button> · `
      + `<button class="linkbtn" id="set-notes">${esc(t("set.notes"))}</button>`;
    $("set-get").onclick = () => invoke("open_url", { url: u.info.installer || u.info.page });
    $("set-notes").onclick = () => invoke("open_url", { url: u.info.page });
  } else status.textContent = t("set.latest");

  $("set-about").textContent = t("set.aboutText", { v: st.app_version });
}

async function checkUpdates(quiet) {
  S.update = { checking: true };
  if (S.view === "settings") renderSettings();
  try {
    const info = await invoke("check_updates");
    S.update = { info };
    if (quiet && info.newer) {
      toast(t("toast.update", { v: info.latest }), { label: t("set.getIt"), run: () => invoke("open_url", { url: info.installer || info.page }) });
    }
  } catch (err) {
    S.update = quiet ? null : { error: String(err) };
  }
  if (S.view === "settings") renderSettings();
}

async function importModel(errorEl) {
  errorEl.hidden = true;
  try {
    onState(await invoke("import_model"));
    if (S.snap.setup.model.found) toast(t("toast.modelReady"));
    if (S.view === "settings") renderSettings();
  } catch (err) {
    errorEl.textContent = String(err);
    errorEl.hidden = false;
  }
}

function renderHomeNote() {
  const el = $("home-note");
  if (setupComplete(S.snap)) { el.innerHTML = ""; return; }
  el.innerHTML = `<div class="notice"><span>${esc(t("home.setupNeeded"))}</span><button class="btn btn-sm btn-primary">${esc(t("home.openSetup"))}</button></div>`;
  el.querySelector("button").onclick = () => showView("setup");
}

// ------------------------------------------------------------------ transcript view

const DIAC = /[ً-ٰٟۖ-ۭـ]/;
function normChar(c) {
  switch (c) {
    case "أ": case "إ": case "آ": case "ٱ": return "ا";
    case "ة": return "ه";
    case "ى": case "ئ": return "ي";
    case "ؤ": return "و";
    default: { const l = c.toLowerCase(); return l.length === 1 ? l : c; }
  }
}
/** Arabic-aware matching: hamza forms, taa marbuta and alif maqsura match, diacritics are ignored. */
function normMap(s) {
  let norm = "";
  const map = [];
  for (let i = 0; i < s.length; i++) {
    if (DIAC.test(s[i])) continue;
    norm += normChar(s[i]);
    map.push(i);
  }
  return { norm, map };
}

function isLive(id) {
  const j = jobById(id);
  return !!j && ACTIVE.has(j.status);
}

async function openTranscript(id) {
  let tx = null;
  try { tx = await invoke("get_transcript", { id }); } catch { /* not started yet */ }
  const j = jobById(id);
  if (!tx && j) {
    tx = { id, title: j.title, source_path: j.path, created_ms: Date.now(), duration_s: j.duration_s, language: j.language, elapsed_s: 0, device: "", model: "", partial: false, segments: [] };
  }
  if (!tx) { toast(t("toast.gone")); await loadHistory(); return; }
  S.current = tx;
  showView("transcript");
  renderTranscript();
}

function renderTranscript() {
  const cur = S.current;
  const live = isLive(cur.id);
  const title = $("t-title");
  title.textContent = cur.title;
  title.contentEditable = live ? "false" : "true";
  $("t-partial").hidden = !cur.partial || live;
  renderMeta();
  renderLiveProgress();
  $("btn-delete").hidden = live;
  const box = $("segments");
  box.dir = cur.language === "en" ? "ltr" : "rtl";
  box.classList.toggle("no-stamps", !S.stamps);
  box.lang = cur.language === "en" ? "en" : "ar";
  box.innerHTML = "";
  if (box.dataset.for !== cur.id) { box.scrollTop = 0; box.dataset.for = cur.id; clearWordHighlight(); }
  wordCache.clear();
  cur.segments.forEach((s, i) => box.append(segRow(s, i, live)));
  if (!cur.segments.length) box.append(emptyLive());
  runSearch(false);
  setupAudio(live);
  maybeAlign(cur);
}

function renderMeta() {
  const c = S.current;
  const bits = [];
  if (c.duration_s) bits.push(dur(c.duration_s));
  bits.push(t(c.language === "en" ? "lang.en" : "lang.ar"));
  if (c.created_ms) bits.push(date(c.created_ms));
  if (c.elapsed_s && !isLive(c.id)) {
    bits.push(t("meta.took", { dur: dur(c.elapsed_s) }) + (c.device ? t("meta.on", { device: deviceLabel(c.device) }) : ""));
  }
  const flagged = c.segments.filter((s) => s.flag === "repeat").length;
  if (flagged) bits.push(t("meta.toCheck", { n: flagged }));
  $("t-meta").textContent = bits.join(" · ");
}

function emptyLive() {
  const d = document.createElement("div");
  d.className = "empty-live";
  const j = jobById(S.current.id);
  d.textContent = t(j && ACTIVE.has(j.status) ? "t.emptyLive" : "t.empty");
  return d;
}

function segRow(s, i, live) {
  const row = document.createElement("div");
  row.className = "seg";
  row.dataset.i = i;
  const time = document.createElement("button");
  time.className = "time";
  time.textContent = ts(s.start);
  time.onclick = () => playFrom(s.start);
  const text = document.createElement("div");
  text.className = "text";
  text.dir = "auto";
  text.textContent = s.text;
  text.spellcheck = false;
  if (!live) {
    text.contentEditable = "plaintext-only";
    // editing works on plain text: drop the search highlights (only then, so a selection survives)
    text.addEventListener("focus", () => { if (text.querySelector("mark")) text.textContent = S.current.segments[i].text; });
    text.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.shiftKey) { ev.preventDefault(); text.blur(); }
      if (ev.key === "Escape") { text.textContent = S.current.segments[i].text; text.blur(); }
    });
    text.addEventListener("blur", () => saveSegment(i, text));
  }
  row.append(time, text);
  if (s.flag === "repeat" || s.edited) {
    const note = document.createElement("div");
    note.className = "note" + (s.flag === "repeat" ? "" : " edited");
    note.dir = I18N[UI_LANG].dir; // the note is in the interface language, not the transcript's
    note.textContent = t(s.flag === "repeat" ? "note.repeat" : "note.edited");
    row.append(note);
  }
  return row;
}

async function saveSegment(i, el) {
  const cur = S.current;
  const seg = cur.segments[i];
  const text = el.innerText.replace(/\s+/g, " ").trim();
  if (text === seg.text) { paintSegment(i); return; }
  try {
    await invoke("update_segment", { id: cur.id, index: i, text });
    seg.text = text;
    seg.edited = true;
    seg.words = []; // timed again by the app
    wordCache.delete(i);
    const row = el.closest(".seg");
    row.replaceWith(segRow(seg, i, false));
    runSearch(false);
  } catch (err) {
    toast(String(err));
    el.textContent = seg.text;
  }
}

function renderLiveProgress() {
  const cur = S.current;
  const j = jobById(cur.id);
  const box = $("t-progress");
  if (!j || !ACTIVE.has(j.status)) { box.hidden = true; return; }
  box.hidden = false;
  const indet = j.status === "decoding" || j.status === "loading" || j.status === "queued";
  box.querySelector(".progress").classList.toggle("indeterminate", indet);
  $("t-bar").style.width = indet ? "" : (j.progress * 100).toFixed(1) + "%";
  let line = jobLine(j);
  if (j.status === "transcribing" && j.duration_s) line += " · " + t("t.of", { a: ts(j.progress * j.duration_s), b: ts(j.duration_s) });
  $("t-status").textContent = line;
}

// ------------------------------------------------------------------ search

function runSearch(scroll) {
  const q = $("search").value.trim();
  S.hits = [];
  const segs = S.current?.segments || [];
  const nq = q ? normMap(q).norm : "";
  segs.forEach((s, i) => {
    if (!nq) return;
    const { norm, map } = normMap(s.text);
    let from = 0, k;
    while ((k = norm.indexOf(nq, from)) !== -1) {
      S.hits.push({ i, a: map[k], b: map[k + nq.length - 1] + 1 });
      from = k + nq.length;
    }
  });
  if (S.hit >= S.hits.length) S.hit = S.hits.length - 1;
  if (S.hit < 0 && S.hits.length) S.hit = 0;
  $("search-count").textContent = q ? (S.hits.length ? `${S.hit + 1} / ${S.hits.length}` : t("t.noMatches")) : "";
  segs.forEach((_, i) => paintSegment(i));
  if (scroll) scrollToHit();
}

function paintSegment(i) {
  const row = $("segments").querySelector(`.seg[data-i="${i}"]`);
  if (!row) return;
  const el = row.querySelector(".text");
  if (document.activeElement === el) return;
  const text = S.current.segments[i].text;
  const hits = S.hits.map((h, n) => ({ ...h, n })).filter((h) => h.i === i);
  if (!hits.length) { el.textContent = text; return; }
  let html = "", at = 0;
  for (const h of hits) {
    html += esc(text.slice(at, h.a)) + `<mark class="${h.n === S.hit ? "current" : ""}">${esc(text.slice(h.a, h.b))}</mark>`;
    at = h.b;
  }
  el.innerHTML = html + esc(text.slice(at));
}

function scrollToHit() {
  const m = $("segments").querySelector("mark.current");
  if (m) m.scrollIntoView({ block: "center", behavior: "smooth" });
}

function stepHit(d) {
  if (!S.hits.length) return;
  S.hit = (S.hit + d + S.hits.length) % S.hits.length;
  runSearch(true);
}

// ------------------------------------------------------------------ audio

function stopAudio() {
  const a = $("audio");
  if (!a.paused) a.pause();
}

async function setupAudio(live) {
  const c = S.current;
  const player = $("player"), a = $("audio"), note = $("player-note");
  if (live) { player.hidden = true; return; }
  if (S.audioFor === c.id && a.src) { player.hidden = false; return; }
  S.audioFor = c.id;
  a.removeAttribute("src");
  a.dataset.fallback = "";
  note.textContent = "";
  let p = null;
  try { p = await invoke("audio_path", { id: c.id }); } catch { /* ignore */ }
  if (S.current?.id !== c.id) return;
  player.hidden = false;
  if (!p) {
    a.hidden = true;
    note.innerHTML = tHtml("audio.missing", { path: c.source_path ? ltr(c.source_path) : t("audio.oldPlace") });
    return;
  }
  a.hidden = false;
  a.src = fileSrc(p);
}

// ------------------------------------------------------------------ hear a word

/** An estimate for lines without word times: place the character by how far into the line it
 *  is, over the stretches with speech (`voice`). */
function estimateTime(s, charIdx) {
  const frac = Math.min(1, Math.max(0, charIdx / Math.max(1, s.text.length)));
  const v = s.voice;
  if (v && v.length >= 2) {
    const x = frac * (v.length - 1), k = Math.floor(x);
    return k >= v.length - 1 ? v[v.length - 1] : v[k] + (v[k + 1] - v[k]) * (x - k);
  }
  return s.start + frac * (s.end - s.start);
}

/** Where to start playing for the word at `charIdx` of line `i`: its own start time (from the
 *  model's alignment), a moment early; for lines without word times, 1.5 s before the estimate. */
function wordTime(i, charIdx) {
  const s = S.current.segments[i];
  if (s.words && s.words.length) {
    let t = s.words[0][1];
    for (const [a, wt] of s.words) { if (a <= charIdx) t = wt; else break; }
    return Math.max(s.start, t - 0.25);
  }
  return Math.max(s.start, estimateTime(s, charIdx) - 1.5);
}

// ------------------------------------------------------------------ follow words while playing

const wordCache = new Map();
/** The words of line `i`: character range [a, b) and start time t. */
function lineWords(i) {
  if (wordCache.has(i)) return wordCache.get(i);
  const s = S.current.segments[i];
  const text = s.text;
  let starts;
  if (s.words && s.words.length) starts = s.words.map(([a, t]) => ({ a, t }));
  else {
    starts = [];
    const re = /\S+/g;
    let m;
    while ((m = re.exec(text))) starts.push({ a: m.index, t: estimateTime(s, m.index) });
  }
  const words = starts.map((w, k) => {
    let b = k + 1 < starts.length ? starts[k + 1].a : text.length;
    while (b > w.a && /\s/.test(text[b - 1])) b--;
    return { a: w.a, b, t: w.t };
  });
  wordCache.set(i, words);
  return words;
}

/** A DOM range over characters [a, b) of an element's text (search marks may split it). */
function textRange(el, a, b) {
  const r = document.createRange();
  const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let pos = 0, started = false;
  for (let n = walk.nextNode(); n; n = walk.nextNode()) {
    const len = n.textContent.length;
    if (!started && a <= pos + len) { r.setStart(n, a - pos); started = true; }
    if (started && b <= pos + len) { r.setEnd(n, b - pos); return r; }
    pos += len;
  }
  return null;
}

let followRaf = 0, shownWord = "", lastUserScroll = 0;
function clearWordHighlight() {
  shownWord = "";
  if (window.CSS && CSS.highlights) CSS.highlights.delete("now-word");
}

function followTick() {
  followRaf = 0;
  const a = $("audio");
  if (!S.follow || a.paused || S.view !== "transcript" || !S.current) return;
  const now = a.currentTime;
  const i = S.current.segments.findIndex((s) => now >= s.start && now < s.end);
  if (i >= 0) {
    const words = lineWords(i);
    let k = -1;
    for (let j = 0; j < words.length; j++) { if (words[j].t <= now + 0.05) k = j; else break; }
    const key = `${S.current.id}:${i}:${k}`;
    const row = $("segments").querySelector(`.seg[data-i="${i}"]`);
    const el = row?.querySelector(".text");
    if (k >= 0 && el && key !== shownWord && document.activeElement !== el && CSS.highlights) {
      const r = textRange(el, words[k].a, words[k].b);
      if (r) { CSS.highlights.set("now-word", new Highlight(r)); shownWord = key; }
    }
    if (row && Date.now() - lastUserScroll > 4000) {
      const box = $("segments").getBoundingClientRect(), rb = row.getBoundingClientRect();
      if (rb.top < box.top + 8 || rb.bottom > box.bottom - 8) row.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }
  followRaf = requestAnimationFrame(followTick);
}

function startFollow() {
  if (S.follow && !followRaf) followRaf = requestAnimationFrame(followTick);
}

/** Saved transcripts from before word timing get their word times once, in the background. */
const aligning = new Set();
function maybeAlign(cur) {
  if (!cur || isLive(cur.id) || aligning.has(cur.id) || !setupComplete(S.snap)) return;
  if (!cur.segments.some((s) => !(s.words && s.words.length) && s.text.trim())) return;
  aligning.add(cur.id);
  invoke("align_transcript", { id: cur.id }).catch(() => {});
}

function charOffset(textEl, node, offset) {
  const r = document.createRange();
  r.selectNodeContents(textEl);
  r.setEnd(node, offset);
  return r.toString().length;
}

/** The time for the start of the selected text, when the selection is inside a transcript line. */
function selectionTime() {
  const sel = document.getSelection();
  if (S.view !== "transcript" || !sel || sel.isCollapsed || !sel.rangeCount) return null;
  const range = sel.getRangeAt(0);
  const startEl = range.startContainer.nodeType === 1 ? range.startContainer : range.startContainer.parentElement;
  const textEl = startEl?.closest(".seg .text");
  if (!textEl || !$("segments").contains(textEl)) return null;
  return wordTime(Number(textEl.closest(".seg").dataset.i), charOffset(textEl, range.startContainer, range.startOffset));
}

let ctxAt = null;
function hideCtx() { $("ctx-menu").hidden = true; }

function wireListen() {
  // right-click on selected text: "Play from here" next to "Copy"
  $("segments").addEventListener("contextmenu", (ev) => {
    const t = selectionTime();
    if (t == null) return; // nothing selected: the usual menu
    ev.preventDefault();
    ctxAt = t;
    const m = $("ctx-menu");
    m.hidden = false;
    const w = m.offsetWidth, h = m.offsetHeight;
    m.style.left = `${Math.min(ev.clientX, innerWidth - w - 8)}px`;
    m.style.top = `${Math.min(ev.clientY, innerHeight - h - 8)}px`;
  });
  $("ctx-play").onclick = () => { hideCtx(); if (ctxAt != null) playFrom(ctxAt); };
  $("ctx-copy").onclick = async () => {
    hideCtx();
    try { await navigator.clipboard.writeText(document.getSelection().toString()); } catch (err) { toast(t("toast.copyFailed", { err })); }
  };
  $("ctx-menu").addEventListener("mousedown", (ev) => ev.preventDefault()); // keep the selection
  document.addEventListener("mousedown", (ev) => { if (!ev.target.closest("#ctx-menu")) hideCtx(); });
  document.addEventListener("keydown", (ev) => { if (ev.key === "Escape") hideCtx(); });
  $("segments").addEventListener("scroll", hideCtx);
  // Ctrl+click a word: play from it without starting to edit
  $("segments").addEventListener("mousedown", (ev) => {
    if (!(ev.ctrlKey || ev.metaKey)) return;
    const textEl = ev.target.closest(".seg .text");
    if (!textEl) return;
    ev.preventDefault();
    const pos = document.caretPositionFromPoint
      ? document.caretPositionFromPoint(ev.clientX, ev.clientY)
      : document.caretRangeFromPoint(ev.clientX, ev.clientY);
    if (!pos) return;
    const node = pos.offsetNode ?? pos.startContainer;
    const off = pos.offset ?? pos.startOffset;
    let idx = charOffset(textEl, node, off);
    const text = S.current.segments[Number(textEl.closest(".seg").dataset.i)].text;
    while (idx > 0 && !/\s/.test(text[idx - 1])) idx--; // the start of the word
    playFrom(wordTime(Number(textEl.closest(".seg").dataset.i), idx));
  });
}

$("audio").addEventListener("error", async () => {
  const a = $("audio");
  const c = S.current;
  if (!c || a.dataset.fallback === "1" || !a.getAttribute("src")) return;
  a.dataset.fallback = "1";
  $("player-note").textContent = t("audio.converting");
  try {
    const p = await invoke("audio_preview", { id: c.id });
    if (S.current?.id !== c.id) return;
    a.src = fileSrc(p);
    $("player-note").textContent = "";
  } catch (err) {
    $("player-note").textContent = t("audio.cannot", { err });
  }
});

function playFrom(sec) {
  const a = $("audio");
  if ($("player").hidden || a.hidden || !a.getAttribute("src")) {
    toast(t(isLive(S.current.id) ? "toast.notFinished" : "toast.noRecording"));
    return;
  }
  const go = () => { a.currentTime = Math.max(0, sec - 0.2); a.play().catch(() => {}); };
  if (a.readyState >= 1) go(); else a.addEventListener("loadedmetadata", go, { once: true });
}

let playingIdx = -1;
$("audio").addEventListener("timeupdate", () => {
  const cur = S.current;
  if (!cur) return;
  const now = $("audio").currentTime;
  const i = cur.segments.findIndex((s) => now >= s.start - 0.2 && now < s.end);
  if (i === playingIdx) return;
  $("segments").querySelector(".seg.playing")?.classList.remove("playing");
  playingIdx = i;
  if (i >= 0) $("segments").querySelector(`.seg[data-i="${i}"]`)?.classList.add("playing");
});

// ------------------------------------------------------------------ actions

async function addFiles(paths) {
  const isMedia = (p) => MEDIA_EXT.includes((p.split(".").pop() || "").toLowerCase());
  const ok = paths.filter(isMedia);
  let skipped = 0;
  for (const p of paths.filter((p) => !isMedia(p))) {
    // a dropped folder is recordings to transcribe, unless it is the model or the model is what is being asked for
    const wantsModel = S.view === "setup" && !S.snap.setup.model.found;
    if (!wantsModel && (await invoke("path_kind", { path: p })) === "folder") {
      await addFolder(p);
      continue;
    }
    try {
      onState(await invoke("import_model_path", { path: p }));
      toast(t("toast.modelFound"));
    } catch (err) {
      if (S.view === "setup" && !S.snap.setup.model.found) {
        $("import-error").textContent = String(err);
        $("import-error").hidden = false;
      } else skipped++;
    }
  }
  if (skipped) toast(t("toast.skipped", { n: skipped }));
  if (!ok.length) return;
  if (!setupComplete(S.snap)) {
    toast(t("toast.setupFirst"));
    showView("setup");
    return;
  }
  const ids = await invoke("enqueue", { paths: ok, language: S.spoken });
  S.snap = await invoke("get_state");
  renderQueue();
  if (ids.length) openTranscript(ids[0]);
}

async function chooseFiles() {
  const paths = await invoke("pick_files");
  if (paths.length) addFiles(paths);
}

async function addFolder(path) {
  if (!setupComplete(S.snap)) {
    toast(t("toast.setupFirst"));
    showView("setup");
    return;
  }
  const { added, already } = await invoke("enqueue_folder", { path, language: S.spoken });
  S.snap = await invoke("get_state");
  renderQueue();
  const parts = [];
  if (added.length) parts.push(t("toast.folderAdded", { n: added.length }));
  if (already) parts.push(t("toast.folderAlready", { n: already }));
  toast(parts.join(" ") || t("toast.folderEmpty"));
}

async function chooseFolder() {
  const path = await invoke("pick_folder");
  if (path) addFolder(path);
}

async function exportAs(fmt) {
  $("export-menu").hidden = true;
  try {
    const path = await invoke("export_transcript", { id: S.current.id, format: fmt, stamps: S.stamps });
    if (path) toast(t("toast.saved", { name: path.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path }) });
  } catch (err) {
    toast(t("toast.exportFailed", { err }));
  }
}

async function copyAll() {
  try {
    const text = await invoke("transcript_text", { id: S.current.id, stamps: S.stamps });
    await navigator.clipboard.writeText(text);
    toast(t("toast.copied", { n: S.current.segments.length }));
  } catch (err) {
    toast(t("toast.copyFailed", { err }));
  }
}

let deleteArmed = null;
async function deleteCurrent() {
  const b = $("btn-delete");
  if (deleteArmed !== S.current.id) {
    deleteArmed = S.current.id;
    b.classList.add("armed");
    toast(t("toast.deleteConfirm"));
    setTimeout(() => { deleteArmed = null; b.classList.remove("armed"); }, 4000);
    return;
  }
  deleteArmed = null;
  b.classList.remove("armed");
  const id = S.current.id;
  await invoke("delete_transcript", { id });
  invoke("dismiss_job", { id });
  S.current = null;
  S.audioFor = null;
  await loadHistory();
  showView("home");
  toast(t("toast.deleted"));
}

async function renameCurrent() {
  const el = $("t-title");
  const cur = S.current;
  const title = el.textContent.replace(/\s+/g, " ").trim();
  if (!cur || title === cur.title) return;
  if (!title) { el.textContent = cur.title; return; }
  try {
    await invoke("rename_transcript", { id: cur.id, title });
    cur.title = title;
    loadHistory();
  } catch (err) {
    el.textContent = cur.title;
    toast(String(err));
  }
}

// ------------------------------------------------------------------ events from the app

function onState(snap) {
  const prev = S.snap;
  S.snap = snap;
  renderChips();
  renderQueue();
  renderHomeNote();
  if (S.view === "setup") renderSetup();
  if (S.view === "settings") renderSettings();
  if (S.view === "transcript" && S.current) {
    const j = jobById(S.current.id);
    const before = prev?.queue.find((x) => x.id === S.current.id);
    renderLiveProgress();
    // the job just finished: reload it from history so it can be edited and played
    if (before && ACTIVE.has(before.status) && j && !ACTIVE.has(j.status)) {
      openTranscript(S.current.id);
    } else if (j && ACTIVE.has(j.status) && !S.current.duration_s && j.duration_s) {
      S.current.duration_s = j.duration_s;
      renderMeta();
    }
  }
  // finished jobs live in History; drop them from "In progress"
  for (const j of snap.queue) if (j.status === "done") invoke("dismiss_job", { id: j.id });
}

function onSegment({ id, segment }) {
  const cur = S.current;
  if (!cur || cur.id !== id || S.view !== "transcript") return;
  const last = cur.segments[cur.segments.length - 1];
  if (last && segment.start <= last.start) return;
  cur.segments.push(segment);
  const box = $("segments");
  box.querySelector(".empty-live")?.remove();
  const nearBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
  box.append(segRow(segment, cur.segments.length - 1, true));
  if (nearBottom) box.scrollTop = box.scrollHeight;
  if ($("search").value.trim()) runSearch(false);
  renderMeta();
}

// ------------------------------------------------------------------ wiring

function wire() {
  wireListen();
  $("btn-new").onclick = () => { showView("home"); };
  $("dropzone").addEventListener("click", (ev) => {
    if (ev.target.closest("#home-folder")) chooseFolder();
    else if (!ev.target.closest(".segmented")) chooseFiles();
  });
  $("dropzone").addEventListener("keydown", (ev) => {
    if (ev.target === $("dropzone") && (ev.key === "Enter" || ev.key === " ")) { ev.preventDefault(); chooseFiles(); }
  });
  $("queue-cancel-all").onclick = () => invoke("cancel_all_jobs");
  for (const b of document.querySelectorAll("#dropzone .segmented button")) {
    b.onclick = (ev) => { ev.stopPropagation(); S.spoken = b.dataset.lang; store.set("lang", S.spoken); paintLang(); };
  }
  paintLang();
  for (const c of document.querySelectorAll("[data-goto=setup]")) {
    c.onclick = () => {
      if (setupComplete(S.snap)) { showView("settings"); return; }
      S.setupStep = null; showView("setup"); renderSetup();
    };
  }
  for (const li of document.querySelectorAll(".stepper li")) {
    li.querySelector("button").onclick = () => { S.setupStep = li.dataset.step; renderSetup(); };
  }
  $("btn-engine-next").onclick = () => { S.setupStep = "model"; renderSetup(); };
  $("btn-model-next").onclick = () => { S.setupStep = "ready"; renderSetup(); };
  $("btn-start").onclick = () => showView("home");
  $("btn-settings").onclick = () => showView("settings");
  $("set-lang").onchange = () => setUiLang($("set-lang").value);
  $("set-reinstall").onclick = () => { invoke("setup_engine"); S.setupStep = "engine"; showView("setup"); renderSetup(); };
  $("set-import").onclick = () => importModel($("set-import-error"));
  $("set-download").onclick = () => { S.setupStep = "model"; S.modelTab = "download"; showView("setup"); renderSetup(); };
  $("set-model-folder").onclick = () => invoke("reveal", { path: S.snap.setup.model.dir });
  $("set-check").onclick = () => checkUpdates(false);
  $("set-auto-check").onchange = async () => onState(await invoke("set_check_updates", { on: $("set-auto-check").checked }));
  $("set-folder-formats").onchange = async () => {
    const formats = [...$("set-folder-formats").querySelectorAll("input:checked")].map((i) => i.value);
    onState(await invoke("set_folder_formats", { formats }));
  };
  $("set-github").onclick = () => invoke("open_url", { url: S.snap.setup.repo_url });
  $("set-issue").onclick = () => invoke("open_url", { url: S.snap.setup.repo_url + "/issues/new" });
  for (const b of document.querySelectorAll(".tabs button")) {
    b.onclick = () => { S.modelTab = b.dataset.tab; renderSetup(); };
  }

  $("btn-setup").onclick = () => invoke("setup_engine");
  $("btn-import").onclick = () => importModel($("import-error"));
  $("run-on").onchange = async () => {
    try { onState(await invoke("set_run_on", { value: $("run-on").value })); toast(t("toast.runOn")); }
    catch (err) { toast(String(err)); }
  };
  $("btn-import-other").onclick = () => importModel($("import-other-error"));
  $("btn-download-dir").onclick = async () => onState(await invoke("choose_download_dir"));
  $("btn-token").onclick = async () => {
    const token = $("hf-token").value.trim();
    if (!token) { $("hf-token").focus(); return; }
    try {
      onState(await invoke("set_hf_token", { token }));
      $("hf-token").value = "";
      toast(t("toast.tokenSaved"));
    } catch (err) { toast(String(err)); }
  };
  $("hf-token").addEventListener("keydown", (ev) => { if (ev.key === "Enter") $("btn-token").click(); });
  $("btn-forget-token").onclick = async () => { onState(await invoke("forget_hf_token")); toast(t("toast.tokenRemoved")); };
  $("ui-lang").onchange = () => setUiLang($("ui-lang").value);
  $("btn-download").onclick = () => invoke(S.snap.setup.download.active ? "stop_download" : "download_model");
  $("btn-recheck").onclick = async () => { onState(await invoke("refresh_setup")); toast(t("toast.checked")); };
  $("btn-log").onclick = () => invoke("open_log");
  $("btn-data").onclick = () => invoke("reveal", { path: S.snap.setup.data_dir + "\\history" });

  $("history-filter").addEventListener("input", renderHistory);

  $("search").addEventListener("input", () => { S.hit = 0; runSearch(true); });
  $("search").addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") { ev.preventDefault(); stepHit(ev.shiftKey ? -1 : 1); }
    if (ev.key === "Escape") { $("search").value = ""; runSearch(false); }
  });
  $("search-next").onclick = () => stepHit(1);
  $("search-prev").onclick = () => stepHit(-1);

  $("follow").checked = S.follow;
  $("follow").onchange = () => {
    S.follow = $("follow").checked;
    store.set("follow", S.follow ? "1" : "0");
    if (S.follow) startFollow(); else clearWordHighlight();
  };
  $("audio").addEventListener("play", startFollow);
  for (const ev of ["wheel", "touchmove", "keydown", "pointerdown"]) {
    $("segments").addEventListener(ev, () => { lastUserScroll = Date.now(); }, { passive: true });
  }
  $("stamps").checked = S.stamps;
  $("stamps").onchange = () => {
    S.stamps = $("stamps").checked;
    store.set("stamps", S.stamps ? "1" : "0");
    $("segments").classList.toggle("no-stamps", !S.stamps);
  };
  $("btn-copy").onclick = copyAll;
  $("btn-export").onclick = (ev) => { ev.stopPropagation(); $("export-menu").hidden = !$("export-menu").hidden; };
  for (const b of $("export-menu").querySelectorAll("button")) b.onclick = () => exportAs(b.dataset.fmt);
  document.addEventListener("click", () => ($("export-menu").hidden = true));
  $("btn-delete").onclick = deleteCurrent;
  $("t-cancel").onclick = () => S.current && invoke("cancel_job", { id: S.current.id });

  const title = $("t-title");
  title.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") { ev.preventDefault(); title.blur(); }
    if (ev.key === "Escape") { title.textContent = S.current.title; title.blur(); }
  });
  title.addEventListener("blur", renameCurrent);

  document.addEventListener("keydown", (ev) => {
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "f" && S.view === "transcript") {
      ev.preventDefault(); $("search").focus(); $("search").select();
    }
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "o") { ev.preventDefault(); chooseFiles(); }
  });

  const overlay = $("drop-overlay");
  listen("tauri://drag-enter", () => { overlay.hidden = false; });
  listen("tauri://drag-leave", () => { overlay.hidden = true; });
  listen("tauri://drag-drop", (e) => { overlay.hidden = true; addFiles(e.payload.paths || []); });

  listen("state", (e) => onState(e.payload));
  listen("segment", (e) => onSegment(e.payload));
  listen("history-changed", () => loadHistory());
  listen("words-ready", async (e) => {
    if (S.current?.id !== e.payload) return;
    try {
      const fresh = await invoke("get_transcript", { id: e.payload });
      fresh.segments.forEach((s, i) => { if (S.current.segments[i]?.text === s.text) S.current.segments[i].words = s.words; });
      wordCache.clear();
    } catch { /* deleted meanwhile */ }
  });
  listen("setup-log", (e) => {
    if (!S.snap) return;
    S.snap.setup.log.push(e.payload);
    if (S.view === "setup") renderSetup();
  });
}

/** Strings with links or code inside, and the language menu. */
function renderStatic() {
  $("how1").innerHTML = tHtml("setup.how1", { link: raw(`<a href="#" data-link="model_url">${esc(t("setup.how1.link"))}</a>`) });
  $("how2").innerHTML = tHtml("setup.how2", { link: raw(`<a href="#" data-link="tokens_url">${esc(t("setup.how2.link"))}</a>`) });
  $("manual-text").innerHTML = tHtml("model.manual", { link: raw(`<a href="#" data-link="files_url">${esc(t("model.manual.link"))}</a>`) });
  for (const a of document.querySelectorAll("a[data-link]")) {
    a.onclick = (ev) => { ev.preventDefault(); invoke("open_url", { url: S.snap.setup[a.dataset.link] }); };
  }
  const sel = $("ui-lang");
  sel.innerHTML = Object.entries(I18N).map(([k, v]) => `<option value="${k}">${esc(v.name)}</option>`).join("");
  sel.value = UI_LANG;
}

/** Switch the interface language and redraw everything that shows text. */
async function setUiLang(lang, tellApp = true) {
  UI_LANG = I18N[lang] ? lang : "en";
  store.set("uiLang", UI_LANG);
  applyI18n();
  renderStatic();
  if (S.snap) {
    renderChips();
    renderQueue();
    renderHistory();
    renderHomeNote();
    if (S.view === "setup") renderSetup();
    if (S.view === "settings") renderSettings();
    if (S.view === "transcript" && S.current) renderTranscript();
  }
  if (tellApp) {
    try { onState(await invoke("set_ui_language", { lang: UI_LANG })); } catch { /* older backend */ }
  }
}

function paintLang() {
  for (const b of document.querySelectorAll("#dropzone .segmented button")) b.setAttribute("aria-checked", String(b.dataset.lang === S.spoken));
}

async function init() {
  // the interface language: the app's setting, else this window's last choice, else Windows' language
  UI_LANG = store.get("uiLang", (navigator.language || "en").toLowerCase().startsWith("ar") ? "ar" : "en");
  applyI18n();
  renderStatic();
  wire();
  const snap = await invoke("get_state");
  const saved = snap.setup.ui_lang;
  onState(snap);
  if (saved && saved !== UI_LANG) await setUiLang(saved, false);
  else if (!saved) await setUiLang(UI_LANG);
  await loadHistory();
  if (!setupComplete(S.snap)) { S.setupStep = null; showView("setup"); renderSetup(); } else showView("home");
  if (S.snap.setup.check_updates) checkUpdates(true);
}

init().catch((err) => toast(t("toast.startup", { err })));
