// Dawwin UI. Plain JS over Tauri's global API (no bundler).
"use strict";

const T = window.__TAURI__;
const invoke = T.core.invoke;
const listen = T.event.listen;
const fileSrc = (p) => T.core.convertFileSrc(p);

const $ = (id) => document.getElementById(id);
const ACTIVE = new Set(["queued", "decoding", "loading", "transcribing", "translating", "saving_audio", "saving_video"]);
const FOLDER_FORMATS = ["txt", "md", "docx", "srt", "vtt", "csv", "tsv", "json"];
const MEDIA_EXT = ["m4a", "mp3", "wav", "ogg", "opus", "oga", "flac", "aac", "wma", "amr", "webm", "mp4", "m4v", "mov", "mkv", "3gp"];

const store = {
  get(k, d) { try { return localStorage.getItem(k) ?? d; } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode */ } },
};

const DEFAULT_STEPS = { remove_silence: false, save_audio: false, transcribe: true, translate: false, target: "", video: false, burn: false };
function loadSteps() {
  try { return { ...DEFAULT_STEPS, ...JSON.parse(store.get("steps", "{}")) }; } catch { return { ...DEFAULT_STEPS }; }
}

const S = {
  snap: null,
  history: [],
  view: "home",
  current: null,      // the transcript on screen
  languages: new Map(), // the languages recordings can be in, by code
  targets: new Map(),   // the languages a transcript can be translated into, by code
  shown: "",            // the translation shown under the lines of the transcript on screen ("": none)
  scope: store.get("scope", "translation"), // what export and copy take while a translation is shown
  steps: loadSteps(),   // what happens to recordings dropped on the window
  told: new Set(),      // jobs whose saved files were announced
  spoken: store.get("lang", "ar"), // the language of the recordings
  stamps: store.get("stamps", "1") === "1",
  follow: store.get("follow", "0") === "1",
  setupStep: null,     // the setup step the user opened; null = the first one not done
  pick: null,          // the model chosen on the setup page
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
  for (const id of ["home", "setup", "settings", "transcript", "guide"]) $("view-" + id).hidden = id !== v;
  $("btn-settings").classList.toggle("active", v === "settings");
  $("btn-guide").classList.toggle("active", v === "guide");
  if (v === "settings") { renderSettings(); checkOldEngine(); }
  if (v === "guide") renderGuide();
  if (v === "home") paintSteps();
  renderHistory();
  renderQueue();
  if (v !== "transcript") stopAudio();
}

// ------------------------------------------------------------------ how to use

/** Each part of the app: a picture of it (in the interface language), and what to do with it. */
const GUIDE = ["drop", "steps", "live", "listen", "translate", "export", "settings"];

function renderGuide() {
  const box = $("guide-items");
  if (box.dataset.lang === UI_LANG) return;
  box.dataset.lang = UI_LANG;
  box.innerHTML = GUIDE.map((g, i) => `
    <section class="card guide-item">
      <img src="guide/${UI_LANG}/${g}.webp" alt="${esc(t(`guide.${g}Title`))}">
      <div class="guide-text">
        <h2><span class="step-no">${i + 1}</span>${esc(t(`guide.${g}Title`))}</h2>
        <p>${esc(t(`guide.${g}Text`))}</p>
      </div>
    </section>`).join("");
  for (const img of box.querySelectorAll("img")) img.onerror = () => img.remove();
}

const writeToMe = () => invoke("open_url", { url: "mailto:braajad1@gmail.com?subject=Dawwin" });

/** Fold the side panel into a rail of icons, or open it again; remembered for next time. */
function foldSidebar(folded) {
  document.querySelector(".sidebar").classList.toggle("collapsed", folded);
  $("btn-side").setAttribute("aria-expanded", String(!folded));
  for (const b of [$("btn-new"), $("btn-guide"), $("btn-settings")]) {
    if (folded) b.setAttribute("aria-label", b.querySelector("span").textContent);
    else b.removeAttribute("aria-label");
  }
  store.set("sideFolded", folded ? "1" : "0");
}

// ------------------------------------------------------------------ steps

/** The steps as a job takes them: the language to translate into, and a sound saved when it is all there is. */
function stepsForJob() {
  const s = S.steps;
  return {
    remove_silence: s.remove_silence,
    save_audio: s.save_audio || (s.remove_silence && !s.transcribe),
    transcribe: s.transcribe,
    translate: s.transcribe && s.translate ? stepTarget() : null,
    video: s.transcribe && s.video,
    burn: s.transcribe && s.video && s.burn,
  };
}

/** The language to translate into: the one chosen, else English, or Arabic for recordings in English. */
function stepTarget() {
  const t = S.steps.target;
  if (t && S.targets.has(t) && t !== S.spoken) return t;
  return S.spoken === "en" ? "ar" : "en";
}

/** Turn a step on or off, and the ones that go with it: translating and the video need a transcript. */
function setStep(key, on) {
  const s = S.steps;
  s[key] = on;
  if ((key === "translate" || key === "video") && on) s.transcribe = true;
  if (key === "transcribe" && !on) { s.translate = false; s.video = false; }
  // with nothing else to do, taking the silences out means saving the sound
  if (s.remove_silence && !s.transcribe) s.save_audio = true;
  store.set("steps", JSON.stringify(s));
  paintSteps();
}

function paintSteps() {
  const s = S.steps;
  $("step-silence").checked = s.remove_silence;
  $("step-save-audio").checked = s.save_audio || (s.remove_silence && !s.transcribe);
  $("step-save-audio").disabled = s.remove_silence && !s.transcribe;
  $("step-transcribe").checked = s.transcribe;
  $("step-translate").checked = s.transcribe && s.translate;
  $("step-video").checked = s.transcribe && s.video;
  $("step-video-how").value = s.burn ? "burn" : "tracks";
  $("step-video-how").disabled = !(s.transcribe && s.video);
  $("spoken").disabled = !s.transcribe;
  const menu = $("step-target");
  const key = `${UI_LANG}:${S.spoken}`;
  if (menu.dataset.key !== key) {
    menu.dataset.key = key;
    const codes = [...S.targets.keys()].filter((c) => c !== S.spoken);
    const first = ["en", "ar"].filter((c) => codes.includes(c));
    const rest = codes.filter((c) => !first.includes(c)).sort((a, b) => langName(a).localeCompare(langName(b), I18N[UI_LANG].locale));
    menu.innerHTML = [...first, ...rest].map((c) => `<option value="${c}">${esc(langName(c))}</option>`).join("");
  }
  menu.value = stepTarget();
  menu.disabled = !(s.transcribe && s.translate);
  for (const box of document.querySelectorAll(".step")) box.classList.toggle("on", !!box.querySelector("input:checked"));
}

function wireSteps() {
  const keys = { "step-silence": "remove_silence", "step-save-audio": "save_audio", "step-transcribe": "transcribe", "step-translate": "translate", "step-video": "video" };
  for (const [id, key] of Object.entries(keys)) $(id).onchange = () => setStep(key, $(id).checked);
  $("step-target").onchange = () => { S.steps.target = $("step-target").value; store.set("steps", JSON.stringify(S.steps)); };
  $("step-video-how").onchange = () => { S.steps.burn = $("step-video-how").value === "burn"; store.set("steps", JSON.stringify(S.steps)); };
}

/** Whether what is about to start can: a model for each step that needs one. False after saying what is missing. */
function stepsReady(steps) {
  if (!steps.transcribe && !steps.save_audio) { toast(t("toast.noSteps")); return false; }
  if (steps.transcribe && !setupComplete(S.snap)) {
    toast(t("toast.setupFirst"));
    showView("setup");
    return false;
  }
  if (steps.translate && !S.snap.setup.families.translator?.found) {
    toast(t("toast.needTranslator"), { label: t("toast.getTranslator"), run: openTranslationSettings });
    return false;
  }
  if (steps.translate && !S.snap.setup.translator_found) return offerTool("translator", "toast.needTranslatorTool");
  if (steps.video && steps.burn && !S.snap.setup.burner) return offerTool("ffmpeg-video", "toast.needBurner");
  if (steps.video && !S.snap.setup.ffmpeg) return offerTool("ffmpeg-audio", "toast.needFfmpeg");
  return true;
}

/** A tool that what was asked for needs is not here: say so, with its download a press away. False. */
function offerTool(id, message) {
  const tool = S.snap.setup.tools.find((x) => x.id === id);
  const name = t(`tool.${id}.long`);
  toast(t(message, { size: sizeText(tool?.size || 0) }), {
    label: t("dl.download"),
    run: () => {
      invoke("download_model", { id });
      toast(t("toast.toolDownloading", { name }));
    },
  });
  return false;
}

/** "1.3 MB", or "2.5 GB" for a model. */
function sizeText(bytes) {
  return bytes < 1e9 ? t("set.sizeMb", { size: (bytes / 1e6).toFixed(1) }) : t("set.modelSize", { size: gb(bytes) });
}

function setupComplete(snap) {
  return snap && snap.setup.engine_found && !!spokenModel(snap.setup);
}

/** The models that can transcribe a language, the best first. */
function modelsFor(st, code = S.spoken) {
  return (S.languages.get(code)?.models || []).map((id) => st.models.find((m) => m.id === id)).filter(Boolean);
}

/** The model file a recording in this language would use, or null when it has none yet. */
function spokenModel(st, code = S.spoken) {
  for (const m of modelsFor(st, code)) if (st.families[m.family]?.found) return st.families[m.family];
  return null;
}

let languageNames = null;
function langName(code) {
  if (code === "ar" || code === "en") return t("lang." + code);
  if (languageNames?.lang !== UI_LANG) languageNames = { lang: UI_LANG, names: new Intl.DisplayNames([I18N[UI_LANG].locale], { type: "language" }) };
  const name = languageNames.names.of(code === "jw" ? "jv" : code);
  // Windows does not know them all by name
  return name && name !== code ? name : S.languages.get(code)?.name || S.targets.get(code)?.name || code;
}

// ------------------------------------------------------------------ translations

function translationOf(tx, lang) {
  return (tx?.translations || []).find((x) => x.language === lang) || null;
}
/** Line `i` holds a translation of what it says now. */
function translatedNow(tr, i, seg) {
  const l = tr?.lines?.[i];
  return !!l && !!l.text.trim() && l.from === seg.text;
}
/** How many lines of a transcript have no translation into `lang` of what they say now. */
function untranslated(tx, lang) {
  const tr = translationOf(tx, lang);
  return tx.segments.filter((s, i) => s.text.trim() && !translatedNow(tr, i, s)).length;
}
/** The translation job of a transcript that is waiting or running, into `lang` or any language. */
function translateJob(id, lang) {
  return S.snap?.queue.find((j) => j.kind === "translate" && j.transcript === id && (!lang || j.target === lang) && ACTIVE.has(j.status)) || null;
}

// ------------------------------------------------------------------ status chips

function setChip(el, cls, text) {
  el.className = "chip " + (cls || "");
  el.querySelector(".label").textContent = text;
}

function renderChips() {
  const { engine: e, setup: st } = S.snap;
  const chip = $("chip-status"), d = st.download;
  if (e.state === "missing") return setChip(chip, "bad", t("chip.noEngine"));
  if (e.state === "error") return setChip(chip, "bad", t("chip.error"));
  if (d.active) {
    const pct = d.total ? Math.floor((100 * d.done) / d.total) : 0;
    // a tool, or the translator before the first translation model, says what it is
    const tool = st.tools.find((x) => x.id === (d.part || d.model));
    return setChip(chip, "busy", tool ? t("chip.downloadingTool", { name: t(`tool.${tool.id}.long`), pct }) : t("chip.downloading", { pct }));
  }
  const tj = S.snap.queue.find((j) => j.kind === "translate" && j.status === "translating");
  if (tj) return setChip(chip, "busy", t("chip.translating", { pct: Math.floor(tj.progress * 100) }));
  if (!spokenModel(st)) return setChip(chip, "bad", t("chip.noModel"));
  if (e.state === "starting") return setChip(chip, "busy", t("chip.starting"));
  if (e.state === "loading") return setChip(chip, "busy", t("chip.loading"));
  // the GPU's name runs left to right, also inside an Arabic sentence
  const where = st.run_on !== "cpu" && e.device !== "cpu" && e.gpu ? `\u2066${shortGpu(e.gpu)}\u2069` : t("chip.cpu");
  setChip(chip, e.note ? "warn" : "ok", `${where} · ${t(e.state === "loaded" ? "chip.loaded" : "chip.ready")}`);
}

// ------------------------------------------------------------------ queue

function jobById(id) {
  return S.snap?.queue.find((j) => j.id === id);
}

function jobLine(j) {
  switch (j.status) {
    case "queued": return t("job.queued");
    case "decoding": return t("job.decoding");
    case "loading": return t(j.kind === "translate" ? "job.loadingTranslator" : "job.loading");
    case "transcribing": return `${Math.floor(j.progress * 100)}% · ${eta(j.eta_s)}`;
    case "translating": return `${Math.floor(j.progress * 100)}% · ${eta(j.eta_s)}`;
    case "saving_audio": return t("job.savingAudio");
    case "saving_video": return j.steps?.burn ? `${Math.floor(j.progress * 100)}% · ${eta(j.eta_s)}` : t("job.savingVideo");
    case "done": return t("job.done", { dur: dur(j.elapsed_s) });
    case "cancelled": return t(j.kind === "translate" ? "job.cancelledKept" : j.saved ? "job.cancelledSaved" : "job.cancelled");
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
    const of = j.transcript || j.id;
    li.className = "item" + (S.current?.id === of && S.view === "transcript" ? " active" : "") + (j.status === "failed" ? " failed" : "");
    // burning the subtitles in takes a while, and says how far it is
    const indet = ["decoding", "loading", "saving_audio", "saving_video"].includes(j.status) && !(j.status === "saving_video" && j.steps?.burn);
    const doing = j.kind === "translate" ? t("job.translating", { lang: langName(j.target) }) : j.status === "saving_video" && j.steps?.burn ? t("job.burning") : "";
    const what = doing ? `<div class="sub">${esc(doing)}</div>` : "";
    li.innerHTML = `
      <div class="title" dir="auto">${esc(j.title)}</div>
      ${what}
      <div class="sub">${esc(jobLine(j))}</div>
      ${ACTIVE.has(j.status) ? `<div class="progress ${indet ? "indeterminate" : ""}"><div class="bar" style="width:${indet ? "" : (j.progress * 100).toFixed(1) + "%"}"></div></div>` : ""}
      <button class="icon-btn x" aria-label="${esc(t(ACTIVE.has(j.status) ? "job.cancel" : "job.dismiss"))}">×</button>`;
    // a job that only saves the sound has no transcript: it shows the file
    li.onclick = () => (j.kind === "burn" ? openTranscript(of) : j.kind !== "translate" && !j.steps?.transcribe ? j.outputs?.length && invoke("reveal", { path: j.outputs[0] }) : openTranscript(of, j.target));
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

const STEPS = ["choose", "download", "ready"];
function firstOpenStep(st) {
  if (spokenModel(st)) return "ready";
  return speechDownload(st).active || modelsFor(st).some((m) => m.partial > 0) ? "download" : "choose";
}

/** The download as setting up sees it: one of a tool or a translation model is not its business. */
function speechDownload(st) {
  const d = st.download;
  const speech = st.models.some((m) => m.id === d.model && m.family !== "translator");
  return speech ? d : { ...d, active: false, verifying: false, retry_wait: null, error: null };
}

/** The model the download step is about: the one downloading, one left paused, or the one picked. */
function downloadingModel(st) {
  const id = speechDownload(st).active ? st.download.model : modelsFor(st).find((m) => m.partial > 0 && !m.on_disk)?.id;
  return st.models.find((m) => m.id === (id || S.pick)) || modelsFor(st)[0];
}

function modelName(m) {
  return t(`model.${m.id}.name`);
}
function gb(bytes) {
  return (bytes / 1e9).toFixed(1);
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
  const found = !!spokenModel(st), d = speechDownload(st), choices = modelsFor(st);
  const started = d.active || choices.some((x) => x.partial > 0);
  const done = { choose: found || started, download: found, ready: found && st.engine_found };
  const busy = { choose: false, download: d.active, ready: false };
  const step = S.setupStep || firstOpenStep(st);

  for (const li of document.querySelectorAll(".stepper li")) {
    const k = li.dataset.step;
    li.classList.toggle("done", done[k]);
    li.classList.toggle("active", k === step);
    li.querySelector(".bullet").textContent = done[k] ? "✓" : String(STEPS.indexOf(k) + 1);
    li.querySelector(".s-state").textContent = t(done[k] ? "step.done" : busy[k] ? "step.busy" : "step.todo");
  }
  for (const k of STEPS) $("panel-" + k).hidden = k !== step;

  // 1. choose
  paintSpoken();
  if (!choices.some((x) => x.id === S.pick)) {
    S.pick = (choices.find((x) => d.active && x.id === d.model) || choices.find((x) => x.partial > 0 || x.on_disk) || choices[0])?.id;
  }
  renderModelCards(st);
  const picked = st.models.find((x) => x.id === S.pick);
  $("download-dir").textContent = st.download_dir;
  $("btn-download-dir").disabled = d.active;
  $("btn-choose-go").disabled = d.active;
  $("btn-choose-go").textContent = t(picked?.on_disk ? "choose.use" : picked?.partial > 0 ? "dl.resume" : "dl.download");

  // 2. download
  const dm = downloadingModel(st);
  if (dm) {
    const doneBytes = d.active ? d.done : dm.partial;
    const total = dm.size;
    const pct = total ? (100 * doneBytes) / total : 0;
    $("download-title").textContent = t("dl.title", { name: modelName(dm), size: gb(dm.size) });
    $("download-bar").style.width = pct.toFixed(1) + "%";
    let line = t("dl.progress", { done: (doneBytes / 1e9).toFixed(2), total: (total / 1e9).toFixed(2), pct: pct.toFixed(0) });
    if (d.verifying) line = t("dl.verifying");
    else if (d.retry_wait != null) line += " · " + t("dl.retry", { wait: d.retry_wait, n: d.retry_attempt });
    else if (!d.active) line += " · " + t("dl.paused");
    else {
      const rate = downloadRate(d);
      if (rate > 0) line += ` · ${rateText(rate)} · ` + t("dl.left", { dur: dur((total - doneBytes) / rate) });
    }
    $("download-text").textContent = line;
    const bd = $("btn-download");
    bd.textContent = t(d.active ? "dl.pause" : "dl.resume");
    bd.className = "btn " + (d.active ? "" : "btn-primary");
    bd.dataset.model = dm.id;
    $("btn-download-back").hidden = d.active;
  }
  $("download-error").hidden = !d.error;
  $("download-error").textContent = d.error || "";
  $("download-box").hidden = !!d.error && !d.active && !(dm && dm.partial > 0);

  // 3. ready
  const device = e.device === "cpu" || st.run_on === "cpu" || !e.gpu ? t("ready.cpu") : t("ready.gpu", { gpu: shortGpu(e.gpu) });
  $("ready-text").textContent = t("ready.text", { device });
}

function renderModelCards(st) {
  const box = $("model-cards"), choices = modelsFor(st), key = `${UI_LANG}:${choices.map((m) => m.id)}`;
  if (box.dataset.key !== key) {
    box.dataset.key = key;
    box.innerHTML = choices.map((m) => `
      <button type="button" class="model-card" role="radio" data-id="${m.id}">
        <h3>${esc(modelName(m))}</h3>
        <p class="muted small">${esc(t("set.modelSize", { size: gb(m.size) }))} · ${esc(t(`model.${m.id}.detail`))}</p>
        <p class="status small"></p>
      </button>`).join("");
    for (const b of box.children) {
      b.onclick = () => { S.pick = b.dataset.id; renderSetup(); };
    }
  }
  for (const b of box.children) {
    const m = st.models.find((x) => x.id === b.dataset.id);
    const on = S.pick === m.id;
    b.classList.toggle("on", on);
    b.setAttribute("aria-checked", String(on));
    b.querySelector(".status").textContent = m.on_disk
      ? t("set.modelOnDisk")
      : m.partial > 0 ? t("set.modelPaused", { pct: Math.floor((100 * m.partial) / m.size) }) : "";
  }
}

// ------------------------------------------------------------------ settings view

function renderSettings() {
  if (!S.snap) return;
  const { setup: st, engine: e } = S.snap;
  $("set-lang").innerHTML = Object.entries(I18N).map(([k, v]) => `<option value="${k}">${esc(v.name)}</option>`).join("");
  $("set-lang").value = UI_LANG;

  // engine
  $("run-on").value = st.run_on || "auto";
  let detail = st.run_on === "cpu" ? t("runon.d.cpu") : e.gpu ? t("runon.d.gpu", { gpu: shortGpu(e.gpu) }) : t("runon.d.nogpu");
  if (e.note) detail += " " + t(`note.${e.note}`);
  $("runon-detail").textContent = detail;
  // an ffmpeg of the computer's own is named; one of Dawwin's says "In use" in its row
  const ours = st.tools.some((x) => x.active && x.id.startsWith("ffmpeg"));
  $("ffmpeg-detail").innerHTML = ours ? "" : st.ffmpeg ? tHtml("ffmpeg.yes", { path: ltr(st.ffmpeg) }) : tHtml("ffmpeg.no");
  $("ffmpeg-detail").hidden = ours;

  // model
  renderModelRows(st);
  const own = Object.values(st.families).filter((f) => f.found && !f.variant);
  $("set-custom").hidden = !own.length;
  $("set-custom-text").innerHTML = own.map((f) => tHtml("set.customModel", { path: ltr(f.path) })).join("<br>");
  if (st.download.error) {
    $("set-import-error").textContent = st.download.error;
    $("set-import-error").hidden = false;
  }
  $("set-download-dir").disabled = st.download.active;

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

/** The tools as rows like the models': `family` keeps the two ffmpegs a choice of one. */
function toolRows(st, ids) {
  return st.tools.filter((x) => ids.includes(x.id)).map((x) => ({ ...x, family: x.id.startsWith("ffmpeg") ? "tool-ffmpeg" : "tool-" + x.id, tool: true }));
}

function renderModelRows(st) {
  renderRows($("set-models"), st, st.models.filter((m) => m.family !== "translator"));
  renderRows($("set-translators"), st, st.models.filter((m) => m.family === "translator"));
  renderRows($("set-translator-tool"), st, toolRows(st, ["translator"]));
  renderRows($("set-tools"), st, toolRows(st, ["ffmpeg-audio", "ffmpeg-video"]));
}

function renderRows(box, st, models) {
  const d = st.download;
  const all = [...st.models, ...toolRows(st, st.tools.map((x) => x.id))];
  if (box.dataset.lang !== UI_LANG || box.children.length !== models.length) {
    box.dataset.lang = UI_LANG;
    box.innerHTML = models.map((m) => `
      <div class="setting" data-id="${m.id}">
        <div class="setting-text">
          <h3>${esc(t(`${m.tool ? "tool" : "model"}.${m.id}.long`))} <span class="muted small">${esc(sizeText(m.size))}</span></h3>
          <p class="muted small">${esc(t(`${m.tool ? "tool" : "model"}.${m.id}.detail`))}</p>
          <p class="muted small status"></p>
        </div>
        <button class="btn btn-sm use" >${esc(t("set.modelUse"))}</button>
        <button class="btn btn-sm get"></button>
        <button class="btn btn-sm btn-ghost del"></button>
      </div>`).join("");
    for (const row of box.children) {
      const id = row.dataset.id;
      row.querySelector(".use").onclick = async () => {
        try { onState(await invoke("use_model", { id })); } catch (err) { toast(String(err)); }
      };
      row.querySelector(".get").onclick = () => {
        const dl = S.snap.setup.download;
        invoke(dl.active && (dl.model === id || dl.part === id) ? "stop_download" : "download_model", { id });
      };
      row.querySelector(".del").onclick = () => deleteModel(id, row.querySelector(".del"));
    }
  }
  for (const row of box.children) {
    const m = all.find((x) => x.id === row.dataset.id);
    // the translator also downloads as the first part of a translation model
    const going = d.active && (d.model === m.id || d.part === m.id);
    const first = d.active && d.model === m.id && d.part;
    const pctOf = (n, of = m.size) => Math.floor((100 * n) / of);
    // "use" only means something where there is a choice of file for the same languages
    const choice = all.filter((x) => x.family === m.family).length > 1;
    row.querySelector(".status").textContent = first
      ? t("dl.translatorFirst", { pct: pctOf(d.done, d.total) })
      : going
      ? (d.verifying ? t("dl.verifying") : t("set.modelGoing", { pct: pctOf(d.done) }))
      : m.active && choice ? t("set.modelInUse")
      : m.on_disk ? t("set.modelOnDisk")
      : m.partial > 0 ? t("set.modelPaused", { pct: pctOf(m.partial) })
      : t("set.modelNone");
    row.querySelector(".use").hidden = m.tool || !m.on_disk || m.active || !choice;
    const get = row.querySelector(".get");
    get.hidden = m.on_disk;
    get.disabled = d.active && !going;
    get.textContent = t(going ? "dl.pause" : m.partial > 0 ? "dl.resume" : "dl.download");
    const del = row.querySelector(".del");
    del.hidden = going || !(m.on_disk || m.partial > 0);
    if (armedDelete !== m.id) del.textContent = t("set.modelDelete");
  }
}

/** Downloading a model can take hours: deleting one takes two presses. */
let armedDelete = null;
async function deleteModel(id, button) {
  if (armedDelete !== id) {
    armedDelete = id;
    button.textContent = t("set.modelDeleteSure");
    setTimeout(() => { if (armedDelete === id) { armedDelete = null; if (S.view === "settings") renderSettings(); } }, 4000);
    return;
  }
  armedDelete = null;
  try { onState(await invoke("delete_model", { id })); } catch (err) { toast(String(err)); }
}

/** What the engine of older versions left behind, offered for removal once per visit to Settings. */
async function checkOldEngine() {
  let size = 0;
  try { size = await invoke("old_engine_size"); } catch { /* nothing to offer */ }
  $("set-old-box").hidden = size < 5e7;
  $("set-old-text").textContent = t("set.oldText", { size: gb(size) });
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
    toast(t(spokenModel(S.snap.setup) ? "toast.modelReady" : "toast.modelFound"));
    if (S.view === "settings") renderSettings();
  } catch (err) {
    errorEl.textContent = String(err);
    errorEl.hidden = false;
  }
}

function renderHomeNote() {
  const el = $("home-note"), st = S.snap.setup;
  if (setupComplete(S.snap)) { el.innerHTML = ""; return; }
  if (!st.engine_found) { el.innerHTML = `<div class="notice"><span>${esc(t("home.engineMissing"))}</span></div>`; return; }
  const first = modelsFor(st)[0], another = Object.values(st.families).some((f) => f.found);
  const text = another && first ? t("home.needsModel", { lang: langName(S.spoken), size: gb(first.size) }) : t("home.setupNeeded");
  el.innerHTML = `<div class="notice"><span>${esc(text)}</span><button class="btn btn-sm btn-primary">${esc(t(another ? "home.getModel" : "home.openSetup"))}</button></div>`;
  el.querySelector("button").onclick = () => { S.setupStep = null; showView("setup"); renderSetup(); };
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

/** Open a transcript; with `lang`, with its translation into that language under the lines. */
async function openTranscript(id, lang) {
  let tx = null;
  try { tx = await invoke("get_transcript", { id }); } catch { /* not started yet */ }
  const j = jobById(id);
  if (!tx && j) {
    tx = { id, title: j.title, source_path: j.path, created_ms: Date.now(), duration_s: j.duration_s, language: j.language, elapsed_s: 0, device: "", model: "", partial: false, segments: [], translations: [] };
  }
  if (!tx) { toast(t("toast.gone")); await loadHistory(); return; }
  if (S.current?.id !== id || lang) {
    // the translation last shown with this transcript, while it has one
    const last = lang || store.get("shown:" + id, "");
    S.shown = last && (translationOf(tx, last) || translateJob(id, last)) ? last : "";
  }
  S.current = tx;
  showView("transcript");
  renderTranscript();
}

/** Read the transcript on screen again (a translation of it was saved), keeping the place. */
async function reloadCurrent() {
  const id = S.current?.id;
  if (!id) return;
  try { S.current = await invoke("get_transcript", { id }); } catch { return; }
  if (S.view === "transcript" && S.current.id === id) renderTranscript();
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
  paintTranslationMenu();
  $("btn-delete").hidden = live;
  const box = $("segments");
  box.dir = S.languages.get(cur.language)?.right_to_left ? "rtl" : "ltr";
  box.classList.toggle("no-stamps", !S.stamps);
  box.lang = cur.language === "jw" ? "jv" : cur.language;
  const same = box.dataset.for === cur.id, keep = box.scrollTop;
  box.innerHTML = "";
  if (!same) { box.dataset.for = cur.id; clearWordHighlight(); }
  wordCache.clear();
  cur.segments.forEach((s, i) => box.append(segRow(s, i, live)));
  if (!cur.segments.length) box.append(emptyLive());
  box.scrollTop = same ? keep : 0;
  runSearch(false);
  setupAudio(live);
}

/** The translation menu: none, the languages it is translated into, then all the others. */
function paintTranslationMenu() {
  const cur = S.current, menu = $("t-lang");
  const done = (cur.translations || []).map((x) => x.language).filter((l) => S.targets.has(l) && l !== cur.language);
  if (S.shown && !done.includes(S.shown)) done.push(S.shown); // a translation that is starting
  const rest = [...S.targets.keys()].filter((c) => c !== cur.language && !done.includes(c));
  const first = ["en", "ar"].filter((c) => rest.includes(c));
  const others = rest.filter((c) => !first.includes(c)).map((c) => [c, langName(c)]).sort((a, b) => a[1].localeCompare(b[1], I18N[UI_LANG].locale));
  const opt = (c) => `<option value="${c}">${esc(langName(c))}</option>`;
  const line = `<option disabled>──────────</option>`;
  const key = `${UI_LANG}:${cur.id}:${done}`;
  if (menu.dataset.key !== key) {
    menu.dataset.key = key;
    menu.innerHTML = `<option value="">${esc(t("t.noTranslation"))}</option>`
      + (done.length ? line + done.map(opt).join("") : "")
      + line + first.map(opt).join("") + others.map(([c]) => opt(c)).join("");
  }
  menu.value = S.shown;
  menu.disabled = isLive(cur.id);
  $("btn-retranslate").hidden = !S.shown || !!translateJob(cur.id, S.shown) || !untranslated(cur, S.shown);
}

/** Show the transcript's translation into `lang` (or none), and translate it if it has none yet. */
async function showTranslation(lang) {
  const cur = S.current;
  S.shown = lang;
  store.set("shown:" + cur.id, lang);
  const any = translationOf(cur, lang)?.lines?.some((l) => l.text);
  if (lang && !any && !translateJob(cur.id, lang) && !(await startTranslation(lang))) {
    S.shown = "";
    store.set("shown:" + cur.id, "");
  }
  renderTranscript();
}

async function startTranslation(lang) {
  if (!S.snap.setup.families.translator?.found) {
    toast(t("toast.needTranslator"), { label: t("toast.getTranslator"), run: openTranslationSettings });
    return false;
  }
  if (!S.snap.setup.translator_found) return offerTool("translator", "toast.needTranslatorTool");
  try {
    await invoke("translate", { id: S.current.id, language: lang });
    onState(await invoke("get_state"));
    return true;
  } catch (err) {
    toast(String(err));
    return false;
  }
}

function openTranslationSettings() {
  showView("settings");
  $("set-translators").scrollIntoView({ block: "center" });
}

function renderMeta() {
  const c = S.current;
  const bits = [];
  if (c.duration_s) bits.push(dur(c.duration_s));
  bits.push(langName(c.language));
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
    text.addEventListener("focus", () => {
      if (!text.querySelector("mark")) return;
      text.textContent = S.current.segments[i].text;
      shownWord = "";
    });
    text.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.shiftKey) { ev.preventDefault(); text.blur(); }
      if (ev.key === "Escape") { text.textContent = S.current.segments[i].text; text.blur(); }
    });
    text.addEventListener("blur", () => saveSegment(i, text));
  }
  row.append(time, text);
  const tr = S.shown ? translationOf(S.current, S.shown) : null;
  const outdated = !!tr?.lines?.[i]?.text && tr.lines[i].from !== s.text;
  if (S.shown) row.append(translationLine(i, live));
  if (s.flag === "repeat" || s.edited || outdated) {
    const note = document.createElement("div");
    note.className = "note" + (s.flag === "repeat" ? "" : " edited");
    note.dir = I18N[UI_LANG].dir; // the note is in the interface language, not the transcript's
    const notes = [];
    if (s.flag === "repeat") notes.push(t("note.repeat"));
    else if (s.edited) notes.push(t("note.edited"));
    if (outdated) notes.push(t("note.outdated"));
    note.textContent = notes.join(" · ");
    row.append(note);
  }
  return row;
}

/** The line's translation, under it; it can be corrected by hand when no translation is running. */
function translationLine(i, live) {
  const cur = S.current, seg = cur.segments[i], line = translationOf(cur, S.shown)?.lines?.[i];
  const el = document.createElement("div");
  el.className = "trans";
  el.dir = S.targets.get(S.shown)?.right_to_left ? "rtl" : "ltr";
  el.lang = S.shown;
  el.spellcheck = false;
  paintTranslationLine(el, line, seg);
  if (!live && !translateJob(cur.id, S.shown)) {
    el.contentEditable = "plaintext-only";
    el.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.shiftKey) { ev.preventDefault(); el.blur(); }
      if (ev.key === "Escape") { el.textContent = translationOf(S.current, S.shown)?.lines?.[i]?.text || ""; el.blur(); }
    });
    el.addEventListener("blur", () => saveTranslation(i, el));
  }
  return el;
}

function paintTranslationLine(el, line, seg) {
  el.textContent = line?.text || "";
  el.classList.toggle("outdated", !!line?.text && line.from !== seg.text);
  el.classList.toggle("pending", !line?.text && !!seg.text.trim());
}

async function saveTranslation(i, el) {
  const cur = S.current, lang = S.shown, tr = translationOf(cur, lang);
  const text = el.innerText.replace(/\s+/g, " ").trim();
  const before = tr?.lines?.[i]?.text || "";
  if (!tr || text === before) { el.textContent = before; return; }
  try {
    await invoke("update_translation", { id: cur.id, language: lang, index: i, text });
    while (tr.lines.length < cur.segments.length) tr.lines.push({ text: "", from: "", edited: false });
    tr.lines[i] = { text, from: cur.segments[i].text, edited: true };
    el.closest(".seg").replaceWith(segRow(cur.segments[i], i, false));
    paintTranslationMenu();
  } catch (err) {
    toast(String(err));
    el.textContent = before;
  }
}

/** A line the translation running in the background just finished. */
function onTranslated({ id, language, index, text, from }) {
  const cur = S.current;
  if (!cur || cur.id !== id) return;
  let tr = translationOf(cur, language);
  if (!tr) {
    tr = { language, model: "", lines: [] };
    (cur.translations ||= []).push(tr);
  }
  while (tr.lines.length < cur.segments.length) tr.lines.push({ text: "", from: "", edited: false });
  if (cur.segments[index]?.text !== from) return; // edited meanwhile: it waits for another turn
  tr.lines[index] = { text, from, edited: false };
  if (S.view !== "transcript" || S.shown !== language) return;
  const el = $("segments").querySelector(`.seg[data-i="${index}"] .trans`);
  if (el && document.activeElement !== el) paintTranslationLine(el, tr.lines[index], cur.segments[index]);
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
    if (S.shown) paintTranslationMenu();
    runSearch(false);
  } catch (err) {
    toast(String(err));
    el.textContent = seg.text;
  }
}

function renderLiveProgress() {
  const cur = S.current;
  const own = jobById(cur.id);
  // the transcript being written, else a translation of it
  const j = own && ACTIVE.has(own.status) ? own : translateJob(cur.id, S.shown) || translateJob(cur.id);
  const box = $("t-progress");
  if (!j) { box.hidden = true; return; }
  box.hidden = false;
  $("t-cancel").dataset.job = j.id;
  const indet = j.status === "decoding" || j.status === "loading" || j.status === "queued";
  box.querySelector(".progress").classList.toggle("indeterminate", indet);
  $("t-bar").style.width = indet ? "" : (j.progress * 100).toFixed(1) + "%";
  let line = jobLine(j);
  if (j.status === "transcribing" && j.duration_s) line += " · " + t("t.of", { a: ts(j.progress * j.duration_s), b: ts(j.duration_s) });
  if (j.kind === "translate") {
    line = t("job.translating", { lang: langName(j.target) }) + " · " + line;
    if (j.device) line += " · " + deviceLabel(j.device);
  }
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
  shownWord = ""; // the highlight sits on the text nodes replaced here: draw it again
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
    $("controls").hidden = true;
    note.innerHTML = tHtml("audio.missing", { path: c.source_path ? ltr(c.source_path) : t("audio.oldPlace") });
    return;
  }
  $("controls").hidden = false;
  a.src = fileSrc(p);
  paintPlayer();
}

// ------------------------------------------------------------------ the player

const SPEEDS = [1, 1.25, 1.5, 1.75, 2, 0.75];

/** A range input's filled part, for the track's colours. */
function fillTo(input) {
  const pct = (100 * (Number(input.value) - Number(input.min))) / (Number(input.max) - Number(input.min) || 1);
  input.style.setProperty("--pct", `${pct}%`);
}

let seeking = false;
function paintPlayer() {
  const a = $("audio"), seek = $("seek");
  const total = isFinite(a.duration) ? a.duration : 0;
  $("clock-now").textContent = ts(a.currentTime);
  $("clock-total").textContent = ts(total);
  if (!seeking) seek.value = total ? Math.round((1000 * a.currentTime) / total) : 0;
  fillTo(seek);
  $("player").classList.toggle("playing", !a.paused);
  $("btn-speed").textContent = `${a.playbackRate}×`;
}

function wirePlayer() {
  const a = $("audio"), seek = $("seek");
  a.playbackRate = Number(store.get("speed", "1")) || 1;
  a.defaultPlaybackRate = a.playbackRate;
  $("btn-play").onclick = () => (a.paused ? a.play().catch(() => {}) : a.pause());
  seek.addEventListener("input", () => {
    seeking = true;
    if (isFinite(a.duration)) a.currentTime = (Number(seek.value) / 1000) * a.duration;
    fillTo(seek);
  });
  seek.addEventListener("change", () => { seeking = false; });
  $("btn-speed").onclick = () => {
    const next = SPEEDS[(SPEEDS.indexOf(a.playbackRate) + 1) % SPEEDS.length] || 1;
    a.playbackRate = a.defaultPlaybackRate = next;
    store.set("speed", String(next));
    paintPlayer();
  };
  for (const ev of ["timeupdate", "play", "pause", "durationchange", "loadedmetadata", "ratechange", "emptied"]) a.addEventListener(ev, paintPlayer);
  // the space bar plays and pauses, except where it types a space
  document.addEventListener("keydown", (ev) => {
    if (ev.key !== " " || S.view !== "transcript" || $("player").hidden || $("controls").hidden) return;
    const el = ev.target;
    if (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(el.tagName)) return;
    ev.preventDefault();
    $("btn-play").click();
  });
  paintPlayer();
}

// ------------------------------------------------------------------ volume

const VOLUME_DEFAULT = 60;
// The bar runs on a curve, because hearing does: most of its length covers the quiet end.
const barToVolume = (bar) => (bar / 100) ** 2;
const volumeToBar = (volume) => Math.round(Math.sqrt(volume) * 100);

function wireVolume() {
  const a = $("audio"), bar = $("volume-bar");
  const saved = Number(store.get("volume", VOLUME_DEFAULT));
  a.volume = barToVolume(saved >= 0 && saved <= 100 ? saved : VOLUME_DEFAULT);
  a.muted = store.get("muted", "0") === "1";
  const paint = () => {
    bar.value = a.muted ? 0 : volumeToBar(a.volume);
    fillTo(bar);
    $("volume").classList.toggle("silent", a.muted || a.volume === 0);
  };
  bar.oninput = () => { a.muted = false; a.volume = barToVolume(Number(bar.value)); };
  $("btn-mute").onclick = () => {
    if (a.volume === 0) { a.muted = false; a.volume = barToVolume(VOLUME_DEFAULT); } else a.muted = !a.muted;
  };
  a.addEventListener("volumechange", () => {
    store.set("volume", volumeToBar(a.volume));
    store.set("muted", a.muted ? "1" : "0");
    paint();
  });
  paint();
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
    if (el && el.textContent !== S.current.segments[i].text) {
      // the line is being edited: its characters no longer match the timings of its words
      if (shownWord) clearWordHighlight();
    } else if (k >= 0 && el && key !== shownWord && CSS.highlights) {
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
  if ($("player").hidden || $("controls").hidden || !a.getAttribute("src")) {
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
    const wantsModel = S.view === "setup" && !spokenModel(S.snap.setup);
    const kind = await invoke("path_kind", { path: p });
    if (kind === "folder" && !wantsModel) {
      await addFolder(p);
      continue;
    }
    if (kind === "other" && !wantsModel) {
      skipped++;
      continue;
    }
    try {
      onState(await invoke("import_model_path", { path: p }));
      toast(t("toast.modelFound"));
    } catch (err) {
      if (S.view === "setup" && !spokenModel(S.snap.setup)) {
        $("import-error").textContent = String(err);
        $("import-error").hidden = false;
      } else skipped++;
    }
  }
  if (skipped) toast(t("toast.skipped", { n: skipped }));
  if (!ok.length) return;
  const steps = stepsForJob();
  if (!stepsReady(steps)) return;
  const ids = await invoke("enqueue", { paths: ok, language: S.spoken, steps });
  S.snap = await invoke("get_state");
  renderQueue();
  if (ids.length && steps.transcribe) openTranscript(ids[0]);
}

async function chooseFiles() {
  const paths = await invoke("pick_files");
  if (paths.length) addFiles(paths);
}

async function addFolder(path) {
  const steps = stepsForJob();
  if (!stepsReady(steps)) return;
  const { added, already } = await invoke("enqueue_folder", { path, language: S.spoken, steps });
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

/** What export and copy take: the transcript, or with a translation shown, the choice in the export menu. */
function exportView() {
  const tr = S.shown ? translationOf(S.current, S.shown) : null;
  return tr && tr.lines.some((l) => l.text) ? { view: S.scope, language: S.shown } : { view: "original", language: null };
}

const VIDEO_EXT = ["mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "3gp", "ts", "mts", "m2ts", "mpg", "mpeg"];

function paintExportScope() {
  const box = $("export-scope");
  // subtitles into the video it was made from, when it is a video
  const ext = (S.current?.source_path || "").split(".").pop().toLowerCase();
  $("export-video").hidden = $("export-burn").hidden = !VIDEO_EXT.includes(ext);
  box.hidden = !exportView().language;
  for (const b of box.querySelectorAll("button")) {
    const on = b.dataset.scope === S.scope;
    b.classList.toggle("on", on);
    b.setAttribute("aria-checked", String(on));
  }
}

async function exportAs(fmt) {
  $("export-menu").hidden = true;
  try {
    const path = await invoke("export_transcript", { id: S.current.id, format: fmt, stamps: S.stamps, ...exportView() });
    if (path) toast(t("toast.saved", { name: path.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path }) });
  } catch (err) {
    toast(t("toast.exportFailed", { err }));
  }
}

async function exportAudio(withoutSilences) {
  $("export-menu").hidden = true;
  try {
    const path = await invoke("export_audio", { id: S.current.id, withoutSilences });
    if (path) toast(t("toast.saved", { name: path.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path }) });
  } catch (err) {
    toast(t("toast.exportFailed", { err }));
  }
}

async function exportVideo() {
  $("export-menu").hidden = true;
  if (!S.snap.setup.ffmpeg) return offerTool("ffmpeg-audio", "toast.needFfmpeg");
  try {
    const path = await invoke("export_video", { id: S.current.id, ...exportView() });
    if (path) toast(t("toast.saved", { name: path.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path }) });
  } catch (err) {
    toast(t("toast.exportFailed", { err }));
  }
}

/** The subtitles burned into the picture: a job in the list, which says when the file is saved. */
async function exportBurned() {
  $("export-menu").hidden = true;
  if (!S.snap.setup.burner) return offerTool("ffmpeg-video", "toast.needBurner");
  try {
    if (await invoke("export_burned", { id: S.current.id, ...exportView() })) toast(t("toast.burning"));
  } catch (err) {
    toast(t("toast.exportFailed", { err }));
  }
}

async function copyAll() {
  try {
    const text = await invoke("transcript_text", { id: S.current.id, stamps: S.stamps, ...exportView() });
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
  if (!(prev && spokenModel(prev.setup)) && spokenModel(snap.setup)) S.setupStep = null;
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
    // a translation of it ended: what it saved can now be corrected by hand
    const ended = (prev?.queue || []).filter((x) => x.kind === "translate" && x.transcript === S.current.id && ACTIVE.has(x.status))
      .map((x) => snap.queue.find((y) => y.id === x.id)).filter((y) => !y || !ACTIVE.has(y.status));
    if (ended.length) {
      for (const y of ended) if (y?.status === "failed") toast(t("toast.translateFailed", { err: (y.error || t("job.unknown")).split("\n")[0] }));
      reloadCurrent();
    } else paintTranslationMenu();
    // a translation the steps started: shown under the lines as it comes
    const begun = snap.queue.find((x) => x.kind === "translate" && x.transcript === S.current.id && ACTIVE.has(x.status) && !prev?.queue.some((y) => y.id === x.id));
    if (begun && !S.shown) {
      S.shown = begun.target;
      renderTranscript();
    }
  }
  // a tool that finished downloading is ready for what it was wanted for
  const was = prev?.setup.download, now = snap.setup.download;
  if (was?.active && !now.active && !now.error && snap.setup.tools.some((x) => x.id === was.model && x.on_disk)) {
    toast(t("toast.toolReady", { name: t(`tool.${was.model}.long`) }));
  }
  // the files a job saved: said once, with the way to them
  for (const j of snap.queue) {
    if (j.outputs?.length && !ACTIVE.has(j.status) && !S.told.has(j.id)) {
      S.told.add(j.id);
      const path = j.outputs[j.outputs.length - 1];
      toast(t("toast.saved", { name: path.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path }) });
    }
  }
  if (S.view === "home") paintSteps();
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
  wireVolume();
  wirePlayer();
  $("btn-new").onclick = () => { showView("home"); };
  $("dropzone").addEventListener("click", (ev) => {
    if (ev.target.closest("#home-folder")) chooseFolder();
    else if (!ev.target.closest(".lang")) chooseFiles();
  });
  $("dropzone").addEventListener("keydown", (ev) => {
    if (ev.target === $("dropzone") && (ev.key === "Enter" || ev.key === " ")) { ev.preventDefault(); chooseFiles(); }
  });
  $("queue-cancel-all").onclick = () => invoke("cancel_all_jobs");
  for (const menu of [$("spoken"), $("setup-spoken")]) menu.onchange = () => setSpoken(menu.value);
  paintSpoken();
  for (const c of document.querySelectorAll("[data-goto=setup]")) {
    c.onclick = () => {
      if (setupComplete(S.snap)) { showView("settings"); return; }
      S.setupStep = null; showView("setup"); renderSetup();
    };
  }
  for (const li of document.querySelectorAll(".stepper li")) {
    li.querySelector("button").onclick = () => { S.setupStep = li.dataset.step; renderSetup(); };
  }
  $("btn-side").onclick = () => foldSidebar(!document.querySelector(".sidebar").classList.contains("collapsed"));
  foldSidebar(store.get("sideFolded", "0") === "1");
  $("btn-start").onclick = () => showView("home");
  $("btn-settings").onclick = () => showView("settings");
  $("btn-guide").onclick = () => showView("guide");
  $("guide-mail").onclick = writeToMe;
  $("set-mail").onclick = writeToMe;
  $("guide-github").onclick = () => invoke("open_url", { url: S.snap.setup.repo_url });
  $("guide-issue").onclick = () => invoke("open_url", { url: S.snap.setup.repo_url + "/issues/new" });
  wireSteps();
  $("set-lang").onchange = () => setUiLang($("set-lang").value);

  $("set-import").onclick = () => importModel($("set-import-error"));
  $("set-download-dir").onclick = async () => onState(await invoke("choose_download_dir"));
  $("set-model-folder").onclick = () => {
    const st = S.snap.setup, m = spokenModel(st);
    invoke("reveal", { path: m ? m.path : st.download_dir });
  };
  $("set-old-remove").onclick = async () => {
    $("set-old-remove").disabled = true;
    try { await invoke("remove_old_engine"); $("set-old-box").hidden = true; toast(t("toast.oldRemoved")); }
    catch (err) { toast(String(err)); }
    $("set-old-remove").disabled = false;
  };
  $("set-check").onclick = () => checkUpdates(false);
  $("set-auto-check").onchange = async () => onState(await invoke("set_check_updates", { on: $("set-auto-check").checked }));
  $("set-folder-formats").onchange = async () => {
    const formats = [...$("set-folder-formats").querySelectorAll("input:checked")].map((i) => i.value);
    onState(await invoke("set_folder_formats", { formats }));
  };
  $("set-github").onclick = () => invoke("open_url", { url: S.snap.setup.repo_url });
  $("set-issue").onclick = () => invoke("open_url", { url: S.snap.setup.repo_url + "/issues/new" });
  $("btn-import").onclick = () => importModel($("import-error"));
  $("run-on").onchange = async () => {
    try { onState(await invoke("set_run_on", { value: $("run-on").value })); toast(t("toast.runOn")); }
    catch (err) { toast(String(err)); }
  };
  $("btn-download-dir").onclick = async () => onState(await invoke("choose_download_dir"));
  $("btn-download").onclick = () => {
    if (S.snap.setup.download.active) invoke("stop_download");
    else invoke("download_model", { id: $("btn-download").dataset.model });
  };
  $("btn-download-back").onclick = () => { S.setupStep = "choose"; renderSetup(); };
  $("btn-choose-go").onclick = async () => {
    const m = S.snap.setup.models.find((x) => x.id === S.pick);
    if (!m) return;
    if (m.on_disk) {
      try { onState(await invoke("use_model", { id: m.id })); S.setupStep = "ready"; renderSetup(); }
      catch (err) { $("import-error").textContent = String(err); $("import-error").hidden = false; }
      return;
    }
    S.setupStep = "download";
    invoke("download_model", { id: m.id });
    renderSetup();
  };
  $("btn-manual").onclick = () => {
    const m = S.snap.setup.models.find((x) => x.id === S.pick);
    if (m) invoke("open_url", { url: m.page });
  };
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
  $("btn-export").onclick = (ev) => {
    ev.stopPropagation();
    paintExportScope();
    $("export-menu").hidden = !$("export-menu").hidden;
  };
  for (const b of $("export-menu").querySelectorAll("button[data-fmt]")) b.onclick = () => exportAs(b.dataset.fmt);
  $("export-video").onclick = exportVideo;
  $("export-burn").onclick = exportBurned;
  $("export-audio").onclick = () => exportAudio(false);
  $("export-audio-short").onclick = () => exportAudio(true);
  listen("video-saving", () => toast(t("toast.videoSaving")));
  listen("audio-saving", () => toast(t("toast.audioSaving")));
  for (const b of $("export-scope").querySelectorAll("button")) {
    b.onclick = (ev) => {
      ev.stopPropagation(); // the menu stays open for the format
      S.scope = b.dataset.scope;
      store.set("scope", S.scope);
      paintExportScope();
    };
  }
  document.addEventListener("click", () => ($("export-menu").hidden = true));
  $("btn-delete").onclick = deleteCurrent;
  $("t-cancel").onclick = () => S.current && invoke("cancel_job", { id: $("t-cancel").dataset.job || S.current.id });
  $("t-lang").onchange = () => showTranslation($("t-lang").value);
  $("btn-retranslate").onclick = async () => {
    if (S.current && S.shown && (await startTranslation(S.shown))) paintTranslationMenu();
  };

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
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "b") { ev.preventDefault(); $("btn-side").click(); }
  });

  const overlay = $("drop-overlay");
  listen("tauri://drag-enter", () => { overlay.hidden = false; });
  listen("tauri://drag-leave", () => { overlay.hidden = true; });
  listen("tauri://drag-drop", (e) => { overlay.hidden = true; addFiles(e.payload.paths || []); });

  listen("state", (e) => onState(e.payload));
  listen("segment", (e) => onSegment(e.payload));
  listen("translated", (e) => onTranslated(e.payload));
  listen("history-changed", () => loadHistory());
  listen("transcript-started", (e) => {
    if (S.current?.id === e.payload) openTranscript(e.payload);
  });
}

/** Strings with links or code inside, and the language menu. */

/** Switch the interface language and redraw everything that shows text. */
async function setUiLang(lang, tellApp = true) {
  UI_LANG = I18N[lang] ? lang : "en";
  store.set("uiLang", UI_LANG);
  applyI18n();
  paintSpoken();
  foldSidebar(document.querySelector(".sidebar").classList.contains("collapsed"));
  if (S.snap) {
    renderChips();
    renderQueue();
    renderHistory();
    renderHomeNote();
    paintSteps();
    if (S.view === "setup") renderSetup();
    if (S.view === "settings") renderSettings();
    if (S.view === "guide") renderGuide();
    if (S.view === "transcript" && S.current) renderTranscript();
  }
  if (tellApp) {
    try { onState(await invoke("set_ui_language", { lang: UI_LANG })); } catch { /* older backend */ }
  }
}

/** Fill the language menus, with the two this app is mostly used for on top. */
function paintSpoken() {
  for (const menu of [$("spoken"), $("setup-spoken")]) {
    if (menu.dataset.lang !== UI_LANG) {
      menu.dataset.lang = UI_LANG;
      const others = [...S.languages.keys()]
        .filter((c) => c !== "ar" && c !== "en")
        .map((c) => [c, langName(c)])
        .sort((a, b) => a[1].localeCompare(b[1], I18N[UI_LANG].locale));
      menu.innerHTML = `<option value="ar">${esc(t("home.ar"))}</option><option value="en">${esc(t("home.en"))}</option><option disabled>──────────</option>`
        + others.map(([c, name]) => `<option value="${c}">${esc(name)}</option>`).join("");
    }
    menu.value = S.spoken;
  }
}

function setSpoken(code) {
  S.spoken = code;
  S.setupStep = null;
  store.set("lang", code);
  paintSpoken();
  paintSteps();
  renderChips();
  renderHomeNote();
  if (S.view === "setup") renderSetup();
}

async function init() {
  // the interface language: the app's setting, else this window's last choice, else Windows' language
  UI_LANG = store.get("uiLang", (navigator.language || "en").toLowerCase().startsWith("ar") ? "ar" : "en");
  applyI18n();
  S.languages = new Map((await invoke("spoken_languages")).map((l) => [l.code, l]));
  S.targets = new Map((await invoke("translation_targets")).map((l) => [l.code, l]));
  // the first time: the language of this Windows, when that is one more than Arabic and English
  const windows = (navigator.language || "").split("-")[0].toLowerCase();
  if (store.get("lang", null) === null && windows !== "en" && S.languages.has(windows)) S.spoken = windows;
  if (!S.languages.has(S.spoken)) S.spoken = "ar";
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
