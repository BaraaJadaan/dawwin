// Tafreegh UI. Plain JS over Tauri's global API (no bundler).
"use strict";

const T = window.__TAURI__;
const invoke = T.core.invoke;
const listen = T.event.listen;
const fileSrc = (p) => T.core.convertFileSrc(p);

const $ = (id) => document.getElementById(id);
const ACTIVE = new Set(["queued", "decoding", "loading", "transcribing"]);
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
  lang: store.get("lang", "ar"),
  stamps: store.get("stamps", "1") === "1",
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
  if (sec < 60) return `${sec} s`;
  const m = Math.floor(sec / 60), s = sec % 60;
  if (m < 10) return s ? `${m} min ${s} s` : `${m} min`;
  if (m < 60) return `${Math.round(sec / 60)} min`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}
function eta(sec) {
  if (sec == null || !isFinite(sec)) return "estimating time…";
  if (sec < 50) return "less than a minute left";
  return `about ${Math.ceil(sec / 60)} min left`;
}
function date(ms) {
  return new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(ms));
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
  for (const id of ["home", "setup", "transcript"]) $("view-" + id).hidden = id !== v;
  renderHistory();
  renderQueue();
  if (v !== "transcript") stopAudio();
}

function setupComplete(snap) {
  return snap && snap.setup.env_ready && snap.setup.model.found;
}

// ------------------------------------------------------------------ status chips

function setChip(el, cls, text, title) {
  el.className = "chip " + (cls || "");
  el.querySelector(".label").textContent = text;
  el.title = title || "";
}

function renderChips() {
  const { engine: e, setup: st } = S.snap;
  const ce = $("chip-engine"), cm = $("chip-model");
  if (st.env_busy) setChip(ce, "busy", "Setting up engine…");
  else if (!st.env_ready) setChip(ce, "bad", "Engine not set up", "Open setup");
  else if (e.state === "starting") setChip(ce, "busy", "Starting engine…");
  else if (e.state === "error") setChip(ce, "bad", "Engine error", e.error || "");
  else if (st.run_on !== "auto" || e.device === "cpu" || e.cuda === false) {
    const exact = st.run_on === "cpu-exact";
    setChip(ce, "warn", exact ? "CPU · exact (slow)" : "CPU · faster mode",
      e.note || (exact ? "Full precision on the processor: very slow" : "8-bit model on the processor: slower than a GPU, slightly less exact"));
  }
  else if (e.cuda === true) setChip(ce, "ok", `GPU · ${shortGpu(e.gpu)}`, `Runs on the GPU with CUDA (torch ${e.torch || "?"}${e.vram_total_gb ? `, ${e.vram_total_gb} GB VRAM` : ""})`);
  else if (e.cuda === false) setChip(ce, "warn", "CPU only (slow)", "No CUDA GPU was found; transcription will be many times slower");
  else setChip(ce, "", "Engine ready", "The engine starts when it is needed");

  if (st.download.active) {
    const pct = st.download.total ? Math.floor((100 * st.download.done) / st.download.total) : 0;
    setChip(cm, "busy", `Downloading model ${pct}%`);
  } else if (!st.model.found) setChip(cm, "bad", "Model not set up", "Download or import the model");
  else if (e.state === "loading") setChip(cm, "busy", "Loading model…");
  else if (e.state === "loaded") setChip(cm, "ok", "Model loaded", `In ${e.device === "cuda" ? "GPU" : "CPU"} memory${e.vram_used_gb ? ` (${e.vram_used_gb} GB)` : ""}; freed after 10 idle minutes`);
  else setChip(cm, "ok", "Model ready", "On disk; it loads when you start a transcription");
}

// ------------------------------------------------------------------ queue

function jobById(id) {
  return S.snap?.queue.find((j) => j.id === id);
}

function jobLine(j) {
  switch (j.status) {
    case "queued": return "Waiting";
    case "decoding": return "Reading audio…";
    case "loading": return "Loading model…";
    case "transcribing": return `${Math.floor(j.progress * 100)}% · ${eta(j.eta_s)}`;
    case "done": return `Done in ${dur(j.elapsed_s)}`;
    case "cancelled": return j.saved ? "Cancelled · partial transcript saved" : "Cancelled";
    case "failed": return "Failed: " + (j.error || "unknown error").split("\n")[0];
    default: return j.status;
  }
}

function renderQueue() {
  if (!S.snap) return;
  const jobs = S.snap.queue.filter((j) => j.status !== "done");
  $("queue-section").hidden = jobs.length === 0;
  const ul = $("queue");
  ul.innerHTML = "";
  for (const j of jobs) {
    const li = document.createElement("li");
    li.className = "item" + (S.current?.id === j.id && S.view === "transcript" ? " active" : "") + (j.status === "failed" ? " failed" : "");
    li.title = j.status === "failed" ? j.error || "" : j.path;
    const indet = j.status === "decoding" || j.status === "loading";
    li.innerHTML = `
      <div class="title" dir="auto">${esc(j.title)}</div>
      <div class="sub">${esc(jobLine(j))}</div>
      ${ACTIVE.has(j.status) ? `<div class="progress ${indet ? "indeterminate" : ""}"><div class="bar" style="width:${indet ? "" : (j.progress * 100).toFixed(1) + "%"}"></div></div>` : ""}
      <button class="icon-btn x" title="${ACTIVE.has(j.status) ? "Cancel" : "Dismiss"}" aria-label="${ACTIVE.has(j.status) ? "Cancel" : "Dismiss"}">×</button>`;
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
      <div class="sub"><span>${esc(date(h.created_ms))}</span><span>·</span><span>${esc(dur(h.duration_s))}</span>${h.partial ? "<span>· partial</span>" : ""}</div>
      <div class="preview" dir="auto">${esc(h.preview)}</div>`;
    li.onclick = () => openTranscript(h.id);
    ul.append(li);
  }
  $("history-empty").hidden = S.history.length > 0;
}

// ------------------------------------------------------------------ setup view

function renderSetup() {
  const { setup: st, engine: e } = S.snap;
  const m = st.model;
  $("step-engine").classList.toggle("done", st.env_ready);
  $("step-model").classList.toggle("done", m.found);

  // 1. engine
  const gpu = st.nvidia_driver ? "PyTorch with CUDA" : "PyTorch (CPU: no NVIDIA GPU found, so it will be slow)";
  let ed;
  if (st.env_ready) {
    ed = `Python 3.12 · ${st.env_backend === "cpu" ? "PyTorch for CPU" : "PyTorch with CUDA"}${e.torch ? ` (torch ${e.torch})` : ""} · ${st.env_dir}`;
  } else {
    ed = `Python 3.12 with ${gpu} and Transformers, installed into ${st.env_dir}. `
      + (st.nvidia_driver ? "About 5 GB on disk; the first time downloads about 3 GB." : "About 1.5 GB on disk.")
      + (st.uv ? "" : " Tafreegh fetches uv, the installer it uses, by itself.");
  }
  $("engine-detail").textContent = ed;
  const bs = $("btn-setup");
  bs.textContent = st.env_busy ? "Setting up…" : st.env_ready ? "Reinstall" : "Set up engine";
  bs.className = "btn " + (st.env_ready ? "btn-ghost" : "btn-primary");
  bs.disabled = st.env_busy;
  const log = $("setup-log");
  log.hidden = !(st.env_busy || st.log.length);
  if (!log.hidden && log.dataset.n !== String(st.log.length)) {
    log.textContent = st.log.join("\n");
    log.dataset.n = String(st.log.length);
    log.scrollTop = log.scrollHeight;
  }
  $("setup-error").hidden = !st.env_error;
  $("setup-error").textContent = st.env_error || "";

  // 2. model
  const where = m.source === "hf-cache" ? "from the Hugging Face cache" : "";
  $("model-detail").textContent = m.found
    ? `Ready · ${(m.bytes / 1e9).toFixed(1)} GB ${where} · ${m.dir}`
    : `${st.model_id}. Download it here, or import the files if you already have them.`;
  $("btn-import-other").hidden = !m.found;
  $("model-get").hidden = m.found;

  const tokenText = {
    app: "A token is saved in Tafreegh.",
    "hf-login": "Using your Hugging Face login (from “hf auth login”). Paste a token only to use another account.",
    env: "Using the HF_TOKEN environment variable.",
    "": "No token yet.",
  }[m.token] ?? "";
  $("token-state").textContent = tokenText;
  $("btn-forget-token").hidden = m.token !== "app";
  $("download-dir").textContent = m.download_dir;

  const d = st.download;
  const bd = $("btn-download");
  bd.textContent = d.active ? "Pause" : d.done > 0 ? "Resume download" : "Download model";
  bd.className = "btn " + (d.active ? "" : "btn-primary");
  bd.disabled = !st.env_ready || (!m.token && !d.active);
  $("download-hint").textContent = !st.env_ready ? "Set up the engine first (step 1)." : !m.token ? "Save a token first." : "";
  $("btn-download-dir").disabled = d.active;
  $("download-box").hidden = !(d.active || d.done > 0);
  const pct = d.total ? (100 * d.done) / d.total : 0;
  $("download-bar").style.width = pct.toFixed(1) + "%";
  $("download-text").textContent = d.total
    ? `${(d.done / 1e9).toFixed(2)} of ${(d.total / 1e9).toFixed(2)} GB (${pct.toFixed(0)}%)${d.retry ? " · " + d.retry : d.active ? "" : " · paused"}`
    : d.active ? "Connecting to Hugging Face…" : "";
  $("download-error").hidden = !d.error || d.error === "stopped";
  $("download-error").textContent = d.error || "";

  $("run-on").value = st.run_on || "auto";
  $("runon-detail").textContent = {
    auto: st.nvidia_driver
      ? "The NVIDIA GPU with CUDA. If its memory is too small, Tafreegh switches to the CPU by itself."
      : "No NVIDIA GPU found, so the CPU in faster mode.",
    "cpu-fast": "The processor, with an 8-bit version of the model: on an 8-core laptop about 3× faster than real time (24 min of audio in about 8 min), slightly less exact than the GPU.",
    "cpu-exact": "The processor at full precision: as exact as the GPU, but about 7× slower than real time.",
  }[st.run_on || "auto"] + (e.note ? " " + e.note : "");

  $("ffmpeg-detail").textContent = st.ffmpeg
    ? `ffmpeg reads every audio and video format (${st.ffmpeg}).`
    : "Built in: wav, mp3, m4a, mp4, flac and ogg (Vorbis). For WhatsApp voice notes (opus) and other formats, install ffmpeg (winget install Gyan.FFmpeg), then press Check again.";
}

async function importModel(errorEl) {
  errorEl.hidden = true;
  try {
    onState(await invoke("import_model"));
    if (S.snap.setup.model.found) toast("Model found. Ready to transcribe.");
  } catch (err) {
    errorEl.textContent = String(err);
    errorEl.hidden = false;
  }
}

function renderHomeNote() {
  const el = $("home-note");
  if (setupComplete(S.snap)) { el.innerHTML = ""; return; }
  el.innerHTML = `<div class="notice"><span>One-time setup is needed before the first transcription.</span><button class="btn btn-sm btn-primary">Open setup</button></div>`;
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
  let t = null;
  try { t = await invoke("get_transcript", { id }); } catch { /* not started yet */ }
  const j = jobById(id);
  if (!t && j) {
    t = { id, title: j.title, source_path: j.path, created_ms: Date.now(), duration_s: j.duration_s, language: j.language, elapsed_s: 0, device: "", model: "", partial: false, segments: [] };
  }
  if (!t) { toast("That transcript is no longer there."); await loadHistory(); return; }
  S.current = t;
  showView("transcript");
  renderTranscript();
}

function renderTranscript() {
  const t = S.current;
  const live = isLive(t.id);
  const title = $("t-title");
  title.textContent = t.title;
  title.contentEditable = live ? "false" : "true";
  $("t-partial").hidden = !t.partial || live;
  renderMeta();
  renderLiveProgress();
  $("btn-delete").hidden = live;
  const box = $("segments");
  box.dir = t.language === "en" ? "ltr" : "rtl";
  box.lang = t.language === "en" ? "en" : "ar";
  box.innerHTML = "";
  if (box.dataset.for !== t.id) { box.scrollTop = 0; box.dataset.for = t.id; }
  t.segments.forEach((s, i) => box.append(segRow(s, i, live)));
  if (!t.segments.length) box.append(emptyLive());
  runSearch(false);
  setupAudio(live);
}

function renderMeta() {
  const t = S.current;
  const bits = [];
  if (t.duration_s) bits.push(dur(t.duration_s));
  bits.push(t.language === "en" ? "English" : "Arabic");
  if (t.created_ms) bits.push(date(t.created_ms));
  if (t.elapsed_s && !isLive(t.id)) bits.push(`transcribed in ${dur(t.elapsed_s)}${t.device ? " on " + t.device : ""}`);
  const flagged = t.segments.filter((s) => s.flag === "repeat").length;
  if (flagged) bits.push(`${flagged} line${flagged > 1 ? "s" : ""} to check`);
  $("t-meta").textContent = bits.join(" · ");
}

function emptyLive() {
  const d = document.createElement("div");
  d.className = "empty-live";
  const j = jobById(S.current.id);
  d.textContent = j && ACTIVE.has(j.status) ? "The transcript appears here, line by line, as it is written." : "This transcript is empty.";
  return d;
}

function segRow(s, i, live) {
  const row = document.createElement("div");
  row.className = "seg";
  row.dataset.i = i;
  const time = document.createElement("button");
  time.className = "time";
  time.textContent = ts(s.start);
  time.title = `Play from ${ts(s.start)}`;
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
    note.textContent = s.flag === "repeat" ? "⚠ The model repeated a word here; the repetition was cut. Listen to check this line." : "Edited";
    row.append(note);
  }
  return row;
}

async function saveSegment(i, el) {
  const t = S.current;
  const seg = t.segments[i];
  const text = el.innerText.replace(/\s+/g, " ").trim();
  if (text === seg.text) { paintSegment(i); return; }
  try {
    await invoke("update_segment", { id: t.id, index: i, text });
    seg.text = text;
    seg.edited = true;
    const row = el.closest(".seg");
    row.replaceWith(segRow(seg, i, false));
    runSearch(false);
  } catch (err) {
    toast(String(err));
    el.textContent = seg.text;
  }
}

function renderLiveProgress() {
  const t = S.current;
  const j = jobById(t.id);
  const box = $("t-progress");
  if (!j || !ACTIVE.has(j.status)) { box.hidden = true; return; }
  box.hidden = false;
  const indet = j.status === "decoding" || j.status === "loading" || j.status === "queued";
  box.querySelector(".progress").classList.toggle("indeterminate", indet);
  $("t-bar").style.width = indet ? "" : (j.progress * 100).toFixed(1) + "%";
  let line = jobLine(j);
  if (j.status === "transcribing" && j.duration_s) line += ` · ${ts(j.progress * j.duration_s)} of ${ts(j.duration_s)}`;
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
  $("search-count").textContent = q ? (S.hits.length ? `${S.hit + 1} / ${S.hits.length}` : "no matches") : "";
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
  const t = S.current;
  const player = $("player"), a = $("audio"), note = $("player-note");
  if (live) { player.hidden = true; return; }
  if (S.audioFor === t.id && a.src) { player.hidden = false; return; }
  S.audioFor = t.id;
  a.removeAttribute("src");
  a.dataset.fallback = "";
  note.textContent = "";
  let p = null;
  try { p = await invoke("audio_path", { id: t.id }); } catch { /* ignore */ }
  if (S.current?.id !== t.id) return;
  player.hidden = false;
  if (!p) {
    a.hidden = true;
    note.textContent = `The original recording is not at ${t.source_path || "its old place"} any more, so it cannot be played.`;
    return;
  }
  a.hidden = false;
  a.src = fileSrc(p);
}

// ------------------------------------------------------------------ hear a word

/** About when a word is said. The model times whole lines (up to 30 s), so the word is placed by
 *  how far into the line it is, over the stretches with speech (`voice`); playback starts 1.5 s
 *  early so the word is heard within the next few seconds. */
function wordTime(i, charIdx) {
  const s = S.current.segments[i];
  const frac = Math.min(1, Math.max(0, charIdx / Math.max(1, s.text.length)));
  const v = s.voice;
  let t;
  if (v && v.length >= 2) {
    const x = frac * (v.length - 1), k = Math.floor(x);
    t = k >= v.length - 1 ? v[v.length - 1] : v[k] + (v[k + 1] - v[k]) * (x - k);
  } else {
    t = s.start + frac * (s.end - s.start);
  }
  return Math.max(s.start, t - 1.5);
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
    try { await navigator.clipboard.writeText(document.getSelection().toString()); } catch (err) { toast("Could not copy: " + err); }
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
  const t = S.current;
  if (!t || a.dataset.fallback === "1" || !a.getAttribute("src")) return;
  a.dataset.fallback = "1";
  $("player-note").textContent = "Converting the recording for playback…";
  try {
    const p = await invoke("audio_preview", { id: t.id });
    if (S.current?.id !== t.id) return;
    a.src = fileSrc(p);
    $("player-note").textContent = "";
  } catch (err) {
    $("player-note").textContent = "This recording cannot be played here: " + err;
  }
});

function playFrom(sec) {
  const a = $("audio");
  if ($("player").hidden || a.hidden || !a.getAttribute("src")) {
    toast(isLive(S.current.id) ? "Playback is available when the transcript is finished." : "The recording is not available to play.");
    return;
  }
  const go = () => { a.currentTime = Math.max(0, sec - 0.2); a.play().catch(() => {}); };
  if (a.readyState >= 1) go(); else a.addEventListener("loadedmetadata", go, { once: true });
}

let playingIdx = -1;
$("audio").addEventListener("timeupdate", () => {
  const t = S.current;
  if (!t) return;
  const now = $("audio").currentTime;
  const i = t.segments.findIndex((s) => now >= s.start - 0.2 && now < s.end);
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
  // a dropped folder may be the model: try to import it
  for (const p of paths.filter((p) => !isMedia(p))) {
    try {
      onState(await invoke("import_model_path", { path: p }));
      toast("Model found in that folder. Ready to transcribe.");
    } catch (err) {
      if (S.view === "setup" && !S.snap.setup.model.found) {
        $("import-error").textContent = String(err);
        $("import-error").hidden = false;
      } else skipped++;
    }
  }
  if (skipped) toast(`Skipped ${skipped} item${skipped > 1 ? "s" : ""}: not audio or video.`);
  if (!ok.length) return;
  if (!setupComplete(S.snap)) {
    toast("Finish the one-time setup first.");
    showView("setup");
    return;
  }
  const ids = await invoke("enqueue", { paths: ok, language: S.lang });
  S.snap = await invoke("get_state");
  renderQueue();
  if (ids.length) openTranscript(ids[0]);
}

async function chooseFiles() {
  const paths = await invoke("pick_files");
  if (paths.length) addFiles(paths);
}

async function exportAs(fmt) {
  $("export-menu").hidden = true;
  try {
    const path = await invoke("export_transcript", { id: S.current.id, format: fmt, stamps: S.stamps });
    if (path) toast(`Saved ${path.split(/[\\/]/).pop()}`, { label: "Show in folder", run: () => invoke("reveal", { path }) });
  } catch (err) {
    toast("Export failed: " + err);
  }
}

async function copyAll() {
  try {
    const text = await invoke("transcript_text", { id: S.current.id, stamps: S.stamps });
    await navigator.clipboard.writeText(text);
    toast(`Copied ${S.current.segments.length} lines`);
  } catch (err) {
    toast("Could not copy: " + err);
  }
}

let deleteArmed = null;
async function deleteCurrent() {
  const b = $("btn-delete");
  if (deleteArmed !== S.current.id) {
    deleteArmed = S.current.id;
    b.classList.add("armed");
    toast("Press delete again to remove this transcript (the recording itself is kept).");
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
  toast("Transcript deleted.");
}

async function renameCurrent() {
  const el = $("t-title");
  const t = S.current;
  const title = el.textContent.replace(/\s+/g, " ").trim();
  if (!t || title === t.title) return;
  if (!title) { el.textContent = t.title; return; }
  try {
    await invoke("rename_transcript", { id: t.id, title });
    t.title = title;
    loadHistory();
  } catch (err) {
    el.textContent = t.title;
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
  const t = S.current;
  if (!t || t.id !== id || S.view !== "transcript") return;
  const last = t.segments[t.segments.length - 1];
  if (last && segment.start <= last.start) return;
  t.segments.push(segment);
  const box = $("segments");
  box.querySelector(".empty-live")?.remove();
  const nearBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
  box.append(segRow(segment, t.segments.length - 1, true));
  if (nearBottom) box.scrollTop = box.scrollHeight;
  if ($("search").value.trim()) runSearch(false);
  renderMeta();
}

// ------------------------------------------------------------------ wiring

function wire() {
  wireListen();
  $("btn-new").onclick = () => { showView("home"); };
  $("dropzone").addEventListener("click", (ev) => { if (!ev.target.closest(".segmented")) chooseFiles(); });
  $("dropzone").addEventListener("keydown", (ev) => { if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); chooseFiles(); } });
  for (const b of document.querySelectorAll(".segmented button")) {
    b.onclick = (ev) => { ev.stopPropagation(); S.lang = b.dataset.lang; store.set("lang", S.lang); paintLang(); };
  }
  paintLang();
  for (const c of document.querySelectorAll("[data-goto=setup]")) c.onclick = () => { showView("setup"); renderSetup(); };

  $("btn-setup").onclick = () => invoke("setup_engine");
  $("btn-import").onclick = () => importModel($("import-error"));
  $("run-on").onchange = async () => {
    try { onState(await invoke("set_run_on", { value: $("run-on").value })); toast("Saved. It applies to the next transcription."); }
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
      toast("Token saved.");
    } catch (err) { toast(String(err)); }
  };
  $("hf-token").addEventListener("keydown", (ev) => { if (ev.key === "Enter") $("btn-token").click(); });
  $("btn-forget-token").onclick = async () => { onState(await invoke("forget_hf_token")); toast("Token removed."); };
  for (const a of document.querySelectorAll("a[data-link]")) {
    a.onclick = (ev) => { ev.preventDefault(); invoke("open_url", { url: S.snap.setup[a.dataset.link] }); };
  }
  $("btn-download").onclick = () => invoke(S.snap.setup.download.active ? "stop_download" : "download_model");
  $("btn-recheck").onclick = async () => { onState(await invoke("refresh_setup")); toast("Checked again."); };
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

  $("stamps").checked = S.stamps;
  $("stamps").onchange = () => { S.stamps = $("stamps").checked; store.set("stamps", S.stamps ? "1" : "0"); };
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
  listen("setup-log", (e) => {
    if (!S.snap) return;
    S.snap.setup.log.push(e.payload);
    if (S.view === "setup") renderSetup();
  });
}

function paintLang() {
  for (const b of document.querySelectorAll(".segmented button")) b.setAttribute("aria-checked", String(b.dataset.lang === S.lang));
}

async function init() {
  wire();
  onState(await invoke("get_state"));
  await loadHistory();
  if (!setupComplete(S.snap)) { showView("setup"); renderSetup(); } else showView("home");
}

init().catch((err) => toast("Startup failed: " + err));
