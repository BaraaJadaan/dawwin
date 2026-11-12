// The Minutes panel: a second tab beside a transcript's lines, with its summary, decisions, action
// items, topics and open questions, written on this computer by a minutes model. It extends a few
// of app.js's functions (the steps, the queue's lines, the transcript view) by wrapping them, so
// that app.js stays as the other releases left it.
"use strict";

Object.assign(I18N.en.s, {
  "min.tabLines": "Transcript",
  "min.tab": "Minutes",
  "min.write": "Write the minutes",
  "min.again": "Write again",
  "min.againTip": "Writes the minutes again from the transcript as it is now",
  "min.againArmed": "Replace my corrections?",
  "min.againConfirm": "Writing again replaces the minutes, with the corrections you made. Press again to go ahead.",
  "min.continue": "Continue",
  "min.stopped": { one: "Stopped after part {n} of {total}. What was done is kept.", other: "Stopped after part {n} of {total}. What was done is kept." },
  "min.language": "Language of the minutes",
  "min.languageTip": "The minutes can be written in another language than the transcript's",
  "min.stop": "Stop",
  "min.writing": "Writing the minutes · {pct}% · {eta}",
  "min.loading": "Loading the minutes model…",
  "min.waiting": "Waiting for its turn…",
  "min.summary": "Summary",
  "min.decisions": "Decisions",
  "min.actions": "Action items",
  "min.topics": "Topics",
  "min.questions": "Open questions",
  "min.none": "None",
  "min.who": "Who",
  "min.due": "By when",
  "min.jumpTip": "Show where this was said",
  "min.private": "Written on this computer by {model}. Nothing was sent anywhere.",
  "min.privateBefore": "The minutes are written on this computer. Nothing is sent anywhere.",
  "min.changed": "The transcript was corrected after these minutes were written.",
  "min.empty": "One press writes a short summary, the decisions, who does what by when, the topics and the questions left open.",
  "min.lecture": "This recording is a talk, not a meeting: a summary and its topics.",
  "min.needModel": "Minutes are written by a model that runs on this computer. Choose one to download:",
  "min.download": "Download",
  "min.downloading": "Downloading… {pct}%",
  "min.fitsGpu": "Fits this computer",
  "min.fitsCpu": "Runs on this computer's processor, slower",
  "min.tooBig": "Too large for this computer",
  "min.fitTip": "Measured against this computer's memory and graphics memory",
  "min.export": "Export",
  "min.alone": "Minutes",
  "min.withLines": "With the transcript",
  "min.scopeTip": "The minutes alone, or followed by the whole transcript",
  "min.failed": "The minutes could not be written: {err}",
  "min.saveFailed": "Your correction could not be saved: {err}",
  "job.minutes": "Writing the minutes",
  "job.loadingMinutes": "Loading the minutes model…",
  "job.cancelledNotes": "Stopped · what was done is kept",
  "steps.minutes": "Write the minutes",
  "steps.minutesTip": "After the transcript, a summary, the decisions and the action items",
  "drop.minutes": "write the minutes",
  "toast.needMinutes": "Minutes need a minutes model.",
  "toast.getMinutes": "Choose one",
  "set.minutes": "Minutes",
  "model.min-gemma4-12b.name": "Gemma 4 12B",
  "model.min-gemma4-12b.long": "Gemma 4 12B",
  "model.min-gemma4-12b.detail": "Google's Gemma 4, the largest that was measured. The best minutes, slower.",
  "model.min-qwen35-9b.name": "Qwen3.5 9B",
  "model.min-qwen35-9b.long": "Qwen3.5 9B",
  "model.min-qwen35-9b.detail": "Alibaba's Qwen3.5. Fits a graphics card with 8 GB.",
  "model.min-qwen35-4b.name": "Qwen3.5 4B",
  "model.min-qwen35-4b.long": "Qwen3.5 4B",
  "model.min-qwen35-4b.detail": "Nearly as good at half the size.",
  "model.min-qwen35-2b.name": "Qwen3.5 2B",
  "model.min-qwen35-2b.long": "Qwen3.5 2B",
  "model.min-qwen35-2b.detail": "The smallest that still gets the decisions and actions right. For computers without a graphics card.",
  "tool.translator.long": "Translation and minutes tool",
  "tool.translator.detail": "llama.cpp, which runs the translation and minutes models. It comes with the first one.",
});
Object.assign(I18N.ar.s, {
  "min.tabLines": "النص",
  "min.tab": "المحضر",
  "min.write": "اكتب المحضر",
  "min.again": "اكتب من جديد",
  "min.againTip": "يكتب المحضر من جديد من النص كما هو الآن",
  "min.againArmed": "أأستبدل تصحيحاتي؟",
  "min.againConfirm": "الكتابة من جديد تستبدل المحضر ومعه ما صحّحته. اضغط مرة أخرى للمتابعة.",
  "min.continue": "تابِع",
  "min.stopped": "توقّف بعد الجزء {n} من {total}. حُفظ ما أُنجز.",
  "min.language": "لغة المحضر",
  "min.languageTip": "يمكن أن يُكتب المحضر بلغة غير لغة النص",
  "min.stop": "إيقاف",
  "min.writing": "جارٍ كتابة المحضر · {pct}٪ · {eta}",
  "min.loading": "جارٍ تحميل نموذج المحاضر…",
  "min.waiting": "في انتظار دوره…",
  "min.summary": "الملخّص",
  "min.decisions": "القرارات",
  "min.actions": "المهام",
  "min.topics": "المواضيع",
  "min.questions": "أسئلة بلا جواب",
  "min.none": "لا شيء",
  "min.who": "مَن",
  "min.due": "متى",
  "min.jumpTip": "أرِني أين قيل هذا",
  "min.private": "كُتب على هذا الحاسوب بنموذج {model}. لم يُرسَل شيء إلى أي مكان.",
  "min.privateBefore": "يُكتب المحضر على هذا الحاسوب. لا يُرسَل شيء إلى أي مكان.",
  "min.changed": "صُحّح النص بعد كتابة هذا المحضر.",
  "min.empty": "بضغطة واحدة: ملخّص قصير، والقرارات، ومن يفعل ماذا ومتى، والمواضيع، والأسئلة التي بقيت بلا جواب.",
  "min.lecture": "هذا التسجيل محاضرة أو حديث، لا اجتماع: ملخّص ومواضيعه.",
  "min.needModel": "يكتب المحضرَ نموذجٌ يعمل على هذا الحاسوب. اختر واحدًا لتنزيله:",
  "min.download": "تنزيل",
  "min.downloading": "جارٍ التنزيل… {pct}٪",
  "min.fitsGpu": "يناسب هذا الحاسوب",
  "min.fitsCpu": "يعمل على معالج هذا الحاسوب، أبطأ",
  "min.tooBig": "أكبر من أن يعمل على هذا الحاسوب",
  "min.fitTip": "بحسب ذاكرة هذا الحاسوب وذاكرة بطاقة الرسوم",
  "min.export": "تصدير",
  "min.alone": "المحضر",
  "min.withLines": "مع النص",
  "min.scopeTip": "المحضر وحده، أو يتبعه النص كاملًا",
  "min.failed": "تعذّرت كتابة المحضر: {err}",
  "min.saveFailed": "تعذّر حفظ تصحيحك: {err}",
  "job.minutes": "كتابة المحضر",
  "job.loadingMinutes": "جارٍ تحميل نموذج المحاضر…",
  "job.cancelledNotes": "أُوقف · حُفظ ما أُنجز",
  "steps.minutes": "اكتب المحضر",
  "steps.minutesTip": "بعد النص: ملخّص، والقرارات، والمهام",
  "drop.minutes": "كتابة المحضر",
  "toast.needMinutes": "يحتاج المحضر إلى نموذج محاضر.",
  "toast.getMinutes": "اختر واحدًا",
  "set.minutes": "المحاضر",
  "model.min-gemma4-12b.name": "Gemma 4 12B",
  "model.min-gemma4-12b.long": "Gemma 4 12B",
  "model.min-gemma4-12b.detail": "أكبر ما قيس، من Google. أفضل المحاضر، وأبطأ.",
  "model.min-qwen35-9b.name": "Qwen3.5 9B",
  "model.min-qwen35-9b.long": "Qwen3.5 9B",
  "model.min-qwen35-9b.detail": "من Alibaba. يناسب بطاقة رسوم بذاكرة 8 غيغابايت.",
  "model.min-qwen35-4b.name": "Qwen3.5 4B",
  "model.min-qwen35-4b.long": "Qwen3.5 4B",
  "model.min-qwen35-4b.detail": "يقارب الأكبر جودةً بنصف الحجم.",
  "model.min-qwen35-2b.name": "Qwen3.5 2B",
  "model.min-qwen35-2b.long": "Qwen3.5 2B",
  "model.min-qwen35-2b.detail": "الأصغر الذي يصيب القرارات والمهام. للحواسيب بلا بطاقة رسوم.",
  "tool.translator.long": "أداة الترجمة والمحاضر",
  "tool.translator.detail": "‏llama.cpp، الذي يشغّل نماذج الترجمة والمحاضر. يأتي مع أول نموذج منها.",
});

ACTIVE.add("writing");

const Minutes = {
  tab: store.get("minutesTab", "lines"), // the tab last open: "lines" | "minutes"
  info: null,                            // what each minutes model needs, and this computer's memory
  scope: store.get("minutesScope", "alone"),
  armed: false,
  language: "",                          // the language picked for the next run ("": the transcript's)
};

/** The minutes job of the transcript on screen, while it is in the list. */
function minutesJob(id = S.current?.id) {
  return S.snap?.queue.find((j) => j.kind === "minutes" && j.transcript === id);
}

function minutesModels() {
  return (S.snap?.setup.models || []).filter((m) => m.family === "minutes");
}

function minutesSpec(id) {
  return Minutes.info?.models.find((m) => m.id === id);
}

/** Whether a minutes model fits this computer: on its GPU, on its processor, or not at all. */
function minutesFit(m) {
  const spec = minutesSpec(m.id);
  if (!spec) return "";
  const vram = (S.snap?.engine.vram_total_gb || 0) * 1e9, ram = Minutes.info?.memory || 0;
  if (vram && spec.needs <= vram * 0.92) return "gpu";
  if (!ram || spec.needs <= ram * 0.6) return "cpu";
  return "big";
}

// ------------------------------------------------------------------ the tabs

function buildMinutesPanel() {
  const head = document.querySelector("#view-transcript .t-head");
  const tabs = document.createElement("div");
  tabs.className = "t-tabs";
  tabs.id = "t-tabs";
  tabs.setAttribute("role", "tablist");
  tabs.innerHTML = `<button type="button" role="tab" data-tab="lines"></button><button type="button" role="tab" data-tab="minutes"></button>`;
  head.insertBefore(tabs, head.querySelector(".toolbar"));
  for (const b of tabs.querySelectorAll("button")) b.onclick = () => showMinutesTab(b.dataset.tab);

  const panel = document.createElement("div");
  panel.id = "minutes";
  panel.className = "minutes";
  panel.hidden = true;
  $("segments").after(panel);
}

function showMinutesTab(tab) {
  Minutes.tab = tab;
  store.set("minutesTab", tab);
  renderMinutes();
}

// ------------------------------------------------------------------ the panel

function renderMinutes() {
  const cur = S.current, panel = $("minutes"), tabs = $("t-tabs");
  if (!cur || !panel) return;
  const on = Minutes.tab === "minutes";
  for (const b of tabs.querySelectorAll("button")) {
    b.textContent = t(b.dataset.tab === "lines" ? "min.tabLines" : "min.tab");
    b.classList.toggle("on", b.dataset.tab === Minutes.tab);
    b.setAttribute("aria-selected", String(b.dataset.tab === Minutes.tab));
  }
  // a transcript that is still being written has no minutes yet
  tabs.hidden = isLive(cur.id);
  const show = on && !tabs.hidden;
  $("segments").hidden = show;
  document.querySelector("#view-transcript .t-head .toolbar").hidden = show;
  panel.hidden = !show;
  if (!show) return;
  const m = cur.minutes?.written_ms ? cur.minutes : null;
  const job = minutesJob();
  const running = job && ACTIVE.has(job.status);
  panel.dir = I18N[UI_LANG].dir;
  panel.innerHTML = "";
  panel.append(minutesBar(m, running));
  if (running) panel.append(minutesProgress(job));
  else if (cur.minutes?.draft?.notes?.length && cur.minutes.draft.parts?.length > cur.minutes.draft.notes.length) panel.append(minutesStopped(cur.minutes.draft));
  if (!minutesModels().some((x) => x.on_disk) && !m) {
    panel.append(minutesOffer());
    return;
  }
  if (!m) {
    const p = document.createElement("p");
    p.className = "minutes-empty";
    p.textContent = t("min.empty");
    panel.append(p, privateLine(null));
    return;
  }
  panel.append(minutesBody(m), privateLine(m));
}

/** Write / write again, the language, and the export menu. */
function minutesBar(m, running) {
  const cur = S.current;
  const bar = document.createElement("div");
  bar.className = "toolbar minutes-bar";
  const write = document.createElement("button");
  write.className = "btn btn-sm" + (m ? "" : " btn-primary") + (Minutes.armed ? " armed" : "");
  write.textContent = Minutes.armed ? t("min.againArmed") : t(m ? "min.again" : "min.write");
  if (m) write.title = t("min.againTip");
  write.disabled = !!running;
  write.onclick = () => writeMinutes(m);

  const lang = document.createElement("select");
  lang.className = "input select select-sm";
  lang.setAttribute("aria-label", t("min.language"));
  lang.title = t("min.languageTip");
  const status = S.snap?.setup.families?.minutes;
  const spec = status?.found ? minutesSpec(status.variant) : null;
  const codes = [cur.language, ...(spec?.languages || ["ar", "en"])].filter((c, i, all) => c && all.indexOf(c) === i);
  lang.innerHTML = codes.map((c) => `<option value="${c}">${esc(langName(c))}</option>`).join("");
  lang.value = Minutes.language && codes.includes(Minutes.language) ? Minutes.language : m?.language && codes.includes(m.language) ? m.language : cur.language;
  lang.onchange = () => { Minutes.language = lang.value; };
  lang.disabled = !!running;
  bar.append(write, lang);

  if (m) {
    const wrap = document.createElement("div");
    wrap.className = "menu-wrap";
    const open = document.createElement("button");
    open.className = "btn btn-sm btn-primary";
    open.textContent = t("min.export");
    open.setAttribute("aria-haspopup", "true");
    const menu = document.createElement("div");
    menu.className = "menu";
    menu.id = "minutes-menu";
    menu.hidden = true;
    const scope = document.createElement("div");
    scope.className = "menu-scope";
    scope.setAttribute("role", "radiogroup");
    scope.title = t("min.scopeTip");
    for (const [key, label] of [["alone", "min.alone"], ["both", "min.withLines"]]) {
      const b = document.createElement("button");
      b.type = "button";
      b.setAttribute("role", "radio");
      b.textContent = t(label);
      b.classList.toggle("on", Minutes.scope === key);
      b.setAttribute("aria-checked", String(Minutes.scope === key));
      b.onclick = (ev) => {
        ev.stopPropagation();
        Minutes.scope = key;
        store.set("minutesScope", key);
        for (const x of scope.children) { x.classList.toggle("on", x === b); x.setAttribute("aria-checked", String(x === b)); }
      };
      scope.append(b);
    }
    menu.append(scope);
    for (const fmt of ["docx", "md", "txt"]) {
      const b = document.createElement("button");
      b.textContent = t(`exp.${fmt}`);
      b.onclick = () => exportMinutes(fmt);
      menu.append(b);
    }
    open.onclick = (ev) => { ev.stopPropagation(); menu.hidden = !menu.hidden; };
    wrap.append(open, menu);
    bar.append(wrap);
  }
  return bar;
}

function minutesProgress(job) {
  const box = document.createElement("div");
  box.className = "t-progress minutes-progress";
  const wait = job.status === "queued", loading = job.status === "loading";
  box.innerHTML = `<div class="progress ${wait || loading ? "indeterminate" : ""}"><div class="bar" style="width:${wait || loading ? "" : (job.progress * 100).toFixed(1) + "%"}"></div></div>
    <div class="t-progress-row"><span class="small"></span><button class="btn btn-sm btn-ghost"></button></div>`;
  box.querySelector("span").textContent = wait ? t("min.waiting") : loading ? t("min.loading") : t("min.writing", { pct: Math.floor(job.progress * 100), eta: eta(job.eta_s) });
  const stop = box.querySelector("button");
  stop.textContent = t("min.stop");
  stop.onclick = () => invoke("cancel_job", { id: job.id });
  return box;
}

function minutesStopped(draft) {
  const p = document.createElement("div");
  p.className = "minutes-note";
  const text = document.createElement("span");
  text.textContent = t("min.stopped", { n: draft.notes.length, total: draft.parts.length });
  const go = document.createElement("button");
  go.className = "linkbtn";
  go.textContent = t("min.continue");
  go.onclick = () => startMinutes(draft.language);
  p.append(text, " ", go);
  return p;
}

/** No minutes model yet: the ones to choose from, with their sizes and whether they fit here. */
function minutesOffer() {
  const box = document.createElement("div");
  box.className = "minutes-offer";
  const lead = document.createElement("p");
  lead.textContent = t("min.needModel");
  box.append(lead);
  const d = S.snap.setup.download;
  for (const m of minutesModels()) {
    const row = document.createElement("div");
    row.className = "minutes-model";
    const fit = minutesFit(m);
    row.innerHTML = `<div><div class="name" dir="ltr"></div><div class="small muted fit"></div></div><button class="btn btn-sm"></button>`;
    row.querySelector(".name").textContent = `${modelName(m)} · ${gb(m.size)} GB`;
    const f = row.querySelector(".fit");
    f.textContent = fit ? t({ gpu: "min.fitsGpu", cpu: "min.fitsCpu", big: "min.tooBig" }[fit]) : "";
    f.title = t("min.fitTip");
    row.classList.toggle("fits", fit === "gpu");
    const b = row.querySelector("button");
    const mine = d.active && d.model === m.id;
    b.textContent = mine ? t("min.downloading", { pct: d.total ? Math.floor((d.done / d.total) * 100) : 0 }) : t("min.download");
    b.disabled = d.active;
    b.onclick = async () => {
      if (!(await invoke("download_model", { id: m.id }))) toast(t("toast.exportFailed", { err: "" }));
    };
    box.append(row);
  }
  box.append(privateLine(null));
  return box;
}

function privateLine(m) {
  const p = document.createElement("p");
  p.className = "minutes-private small muted";
  p.textContent = m ? t("min.private", { model: m.model }) : t("min.privateBefore");
  return p;
}

// ------------------------------------------------------------------ the minutes themselves

function minutesBody(m) {
  const cur = S.current;
  const body = document.createElement("div");
  body.className = "minutes-body";
  // the text is in the language of the minutes, the headings in the interface's
  const rtl = S.languages.get(m.language)?.right_to_left ?? S.targets.get(m.language)?.right_to_left ?? false;
  const changed = document.createElement("p");
  changed.className = "minutes-note";
  changed.textContent = t("min.changed");
  changed.hidden = true;
  body.append(changed);
  invoke("minutes_outdated", { id: cur.id }).then((yes) => { changed.hidden = !yes; }).catch(() => {});
  if (m.kind !== "meeting") {
    const note = document.createElement("p");
    note.className = "small muted";
    note.textContent = t("min.lecture");
    body.append(note);
  }
  const section = (title) => {
    const s = document.createElement("section");
    s.className = "minutes-section";
    const h = document.createElement("h2");
    h.textContent = t(title);
    s.append(h);
    body.append(s);
    return s;
  };
  const field = (text, save, cls = "") => {
    const el = document.createElement("div");
    el.className = "minutes-text " + cls;
    el.dir = rtl ? "rtl" : "ltr";
    el.lang = m.language;
    el.contentEditable = "plaintext-only";
    el.spellcheck = false;
    el.textContent = text;
    el.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.shiftKey && !cls.includes("summary")) { ev.preventDefault(); el.blur(); }
      if (ev.key === "Escape") { el.textContent = text; el.blur(); }
    });
    el.addEventListener("blur", () => {
      const now = el.textContent.replace(/\s+/g, " ").trim();
      if (now !== text.trim()) save(now);
    });
    return el;
  };
  const jump = (item) => {
    const b = document.createElement("button");
    b.className = "minutes-time";
    b.textContent = item.time == null ? "–" : ts(item.time);
    b.title = t("min.jumpTip");
    b.disabled = !(item.lines?.length || item.line != null);
    b.onclick = () => jumpToLines(item.lines?.length ? item.lines : [item.line]);
    return b;
  };
  const edit = (f) => saveMinutes((x) => f(x));

  section("min.summary").append(field(m.summary, (v) => edit((x) => { x.summary = v; }), "summary"));
  const list = (title, items, row) => {
    const s = section(title);
    if (!items.length) {
      const p = document.createElement("p");
      p.className = "small muted";
      p.textContent = t("min.none");
      s.append(p);
    }
    items.forEach((it, i) => s.append(row(it, i)));
  };
  const item = (it, parts) => {
    const r = document.createElement("div");
    r.className = "minutes-item" + (it.edited ? " edited" : "");
    r.append(jump(it), ...parts);
    return r;
  };
  if (m.kind === "meeting") {
    list("min.decisions", m.decisions, (d, i) => item(d, [field(d.text, (v) => edit((x) => { x.decisions[i].text = v; }))]));
    list("min.actions", m.actions, (a, i) => {
      const who = field(a.who, (v) => edit((x) => { x.actions[i].who = v; }), "who");
      who.dataset.ph = t("min.who");
      const what = field(a.what, (v) => edit((x) => { x.actions[i].what = v; }), "what");
      const due = field(a.due, (v) => edit((x) => { x.actions[i].due = v; }), "due");
      due.dataset.ph = t("min.due");
      const grid = document.createElement("div");
      grid.className = "minutes-action";
      grid.append(who, what, due);
      return item(a, [grid]);
    });
  }
  if (m.topics.length) list("min.topics", m.topics, (tp, i) => item(tp, [field(tp.title, (v) => edit((x) => { x.topics[i].title = v; }))]));
  if (m.open_questions.length || m.kind === "meeting") list("min.questions", m.open_questions, (q, i) => item(q, [field(q.text, (v) => edit((x) => { x.open_questions[i].text = v; }))]));
  return body;
}

async function saveMinutes(change) {
  const cur = S.current;
  const next = JSON.parse(JSON.stringify(cur.minutes));
  change(next);
  try {
    cur.minutes = await invoke("edit_minutes", { id: cur.id, minutes: next });
  } catch (err) {
    toast(t("min.saveFailed", { err }));
  }
  renderMinutes();
}

/** Show the lines an item came from, on the Transcript tab. */
function jumpToLines(lines) {
  showMinutesTab("lines");
  const box = $("segments");
  const rows = lines.map((i) => box.querySelector(`.seg[data-i="${i}"]`)).filter(Boolean);
  if (!rows.length) return;
  rows[0].scrollIntoView({ block: "center" });
  for (const r of rows) {
    r.classList.add("from-minutes");
    setTimeout(() => r.classList.remove("from-minutes"), 2400);
  }
  const a = $("audio"), s = S.current.segments[lines[0]];
  if (s && a.getAttribute("src") && a.paused) a.currentTime = Math.max(0, s.start - 0.2);
}

// ------------------------------------------------------------------ actions

async function writeMinutes(m) {
  if (m && (m.summary_edited || [m.decisions, m.actions, m.topics, m.open_questions].some((l) => l.some((x) => x.edited))) && !Minutes.armed) {
    Minutes.armed = true;
    toast(t("min.againConfirm"));
    setTimeout(() => { Minutes.armed = false; renderMinutes(); }, 4000);
    renderMinutes();
    return;
  }
  Minutes.armed = false;
  const lang = $("minutes").querySelector("select")?.value || "";
  await startMinutes(lang);
}

async function startMinutes(language) {
  const setup = S.snap.setup;
  if (!setup.families.minutes?.found) { renderMinutes(); return; }
  if (!setup.translator_found) { offerTool("translator", "toast.needTranslatorTool"); return; }
  try {
    await invoke("write_minutes", { id: S.current.id, language: language || null });
  } catch (err) {
    toast(t("min.failed", { err }));
  }
}

async function exportMinutes(fmt) {
  const menu = $("minutes-menu");
  if (menu) menu.hidden = true;
  try {
    const path = await invoke("export_minutes", { id: S.current.id, format: fmt, withTranscript: Minutes.scope === "both" });
    if (path) toast(t("toast.saved", { name: path.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path }) });
  } catch (err) {
    toast(t("toast.exportFailed", { err }));
  }
}

// ------------------------------------------------------------------ where minutes take part in app.js

// the transcript view gains its Minutes tab
{
  const base = renderTranscript;
  renderTranscript = function () { base(); renderMinutes(); };
}

// the queue says what a minutes job is doing
{
  const base = jobLine;
  jobLine = function (j) {
    if (j.kind !== "minutes") return base(j);
    if (j.status === "loading") return t("job.loadingMinutes");
    if (j.status === "writing") return `${t("job.minutes")} · ${Math.floor(j.progress * 100)}% · ${eta(j.eta_s)}`;
    if (j.status === "cancelled") return t("job.cancelledNotes");
    return base(j);
  };
}

// a press on a minutes job opens its transcript on the Minutes tab
{
  const base = renderQueue;
  renderQueue = function () {
    base();
    const jobs = (S.snap?.queue || []).filter((j) => j.status !== "done");
    const rows = $("queue").children;
    jobs.forEach((j, i) => {
      if (j.kind !== "minutes" || !rows[i]) return;
      rows[i].onclick = () => { Minutes.tab = "minutes"; store.set("minutesTab", "minutes"); openTranscript(j.transcript); };
    });
  };
}

// the state: a minutes job's progress, and the minutes once written
{
  const base = onState;
  onState = function (snap) {
    const prev = S.snap;
    base(snap);
    if (S.view !== "transcript" || !S.current || Minutes.tab !== "minutes") return;
    const was = prev?.queue.find((j) => j.kind === "minutes" && j.transcript === S.current.id);
    const now = minutesJob();
    const ended = was && ACTIVE.has(was.status) && (!now || !ACTIVE.has(now.status));
    if (ended && now?.status === "failed") toast(t("min.failed", { err: (now.error || t("job.unknown")).split("\n")[0] }));
    if (ended) reloadCurrent();
    else if (now && ACTIVE.has(now.status)) {
      const box = $("minutes").querySelector(".minutes-progress");
      if (box) box.replaceWith(minutesProgress(now));
      else renderMinutes();
    } else if (prev?.setup.download.active || snap.setup.download.active || !!prev?.setup.families.minutes?.found !== !!snap.setup.families.minutes?.found) renderMinutes();
  };
}

// a step on the start page: the minutes after the transcript
{
  const label = document.createElement("label");
  label.className = "toggle";
  label.innerHTML = `<input type="checkbox" id="step-minutes"> <span data-i18n="steps.minutes"></span>`;
  $("spoken").after(label);
  const baseFor = stepsForJob;
  stepsForJob = function () { return { ...baseFor(), minutes: !!(S.steps.transcribe && S.steps.minutes) }; };
  const baseSet = setStep;
  setStep = function (key, on) {
    if (key === "minutes" && on) S.steps.transcribe = true;
    if (key === "transcribe" && !on) S.steps.minutes = false;
    baseSet(key, on);
  };
  const basePaint = paintSteps;
  paintSteps = function () {
    label.querySelector("span").textContent = t("steps.minutes");
    const box = $("step-minutes");
    box.checked = !!(S.steps.transcribe && S.steps.minutes);
    box.parentElement.title = t("steps.minutesTip");
    basePaint();
  };
  const baseWire = wireSteps;
  wireSteps = function () { baseWire(); $("step-minutes").onchange = () => setStep("minutes", $("step-minutes").checked); };
  const baseDrop = dropText;
  dropText = function () {
    const text = baseDrop();
    if (!stepsForJob().minutes) return text;
    return `${text.replace(/[.،]$/, "")}${t("drop.comma")}${t("drop.minutes")}`;
  };
  const baseReady = stepsReady;
  stepsReady = function (steps) {
    if (!baseReady(steps)) return false;
    if (steps.minutes && !S.snap.setup.families.minutes?.found) {
      toast(t("toast.needMinutes"), { label: t("toast.getMinutes"), run: () => showView("settings") });
      return false;
    }
    if (steps.minutes && !S.snap.setup.translator_found) return offerTool("translator", "toast.needTranslatorTool");
    return true;
  };
}

// Settings: a card of the minutes models, after the translation's
{
  const translation = $("set-translators").closest("section");
  const card = document.createElement("section");
  card.className = "card group";
  card.innerHTML = `<h2 data-i18n="set.minutes"></h2><div id="set-minutes"></div>`;
  translation.after(card);
  const base = renderModelRows;
  renderModelRows = function (st) {
    base(st);
    card.querySelector("h2").textContent = t("set.minutes");
    renderRows($("set-minutes"), st, st.models.filter((m) => m.family === "minutes"));
    card.hidden = !st.models.some((m) => m.family === "minutes");
  };
}

buildMinutesPanel();
document.addEventListener("click", () => { const m = $("minutes-menu"); if (m) m.hidden = true; });
invoke("minutes_info").then((info) => { Minutes.info = info; }).catch(() => {});
listen("minutes-done", (e) => { if (S.current?.id === e.payload) reloadCurrent(); });
