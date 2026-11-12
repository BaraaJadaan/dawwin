// Recording a meeting: the Record bar on the start page, the levels of "me" and "them" while it
// runs, the choice of microphone, and the recordings found after a crash. Loaded after i18n.js and
// before app.js, whose helpers it uses once the page is wired.
"use strict";

Object.assign(I18N.en.s, {
  "rec.record": "Record",
  "rec.recordTip": "Records your microphone and the sound the PC plays, such as a Zoom, Meet or Teams call, and writes the text while people speak",
  "rec.stop": "Stop",
  "rec.both": "Microphone and PC sound",
  "rec.mic": "Microphone only",
  "rec.pc": "PC sound only",
  "rec.sourcesTip": "Both sides of a call; your microphone alone, for a lecture in a room; or the PC's sound alone, for a webinar or a video",
  "rec.defaultMic": "Windows' microphone ({name})",
  "rec.defaultMicNone": "Windows' microphone",
  "rec.micTip": "The microphone to record. Windows' own is followed when it changes",
  "rec.me": "Me",
  "rec.them": "Them",
  "rec.meTip": "What your microphone hears",
  "rec.themTip": "What the PC plays: the others in the call",
  "rec.showText": "Show the text",
  "rec.micSilent": "No sound comes from “{name}”: it may be muted, or not connected. Choose another microphone:",
  "rec.micBlocked": "Windows does not let Dawwin use the microphone.",
  "rec.openPrivacy": "Allow it in Windows",
  "rec.noDevice": "There is no microphone.",
  "rec.pcFailed": "The PC's sound cannot be recorded.",
  "rec.diskLow": "Less than 2 GB is free on this disk. The recording stops by itself under 500 MB.",
  "rec.diskStopped": "The recording stopped: the disk is nearly full.",
  "rec.behind": "This computer cannot write the text as fast as people speak. The recording goes on, and the rest of the text comes after it.",
  "rec.interrupted": "A recording was cut off: “{title}”, {dur}.",
  "rec.recover": "Transcribe it",
  "rec.discard": "Delete it",
  "rec.discardSure": "Delete it for good?",
  "rec.synced": "Recordings are saved in {folder}, which is in OneDrive: OneDrive uploads them to the internet.",
  "rec.failed": "The recording stopped: it could not be written to the disk ({err}).",
  "rec.needFfmpeg": "The recording is kept as a WAV, five times larger than an M4A. Download ffmpeg ({size}) to keep recordings small.",
  "job.recording": "Recording · {dur}",
  "job.waitRecording": "Waiting for the recording to end",
  "guide.recordTitle": "Record a meeting",
  "guide.recordText": "Press Record on the start page to record your microphone and everything the PC plays, such as a Zoom, Meet or Teams call or a video, with nothing joining the call. Choose the microphone alone for a lecture in a room, or the PC's sound alone for a webinar. The text appears while people speak, each line marked Me or Them, and the recording is kept in Documents\\Dawwin (or in Dawwin in your user folder when Documents is in OneDrive, so that it is not uploaded). Tell the others before you record a call.",
});

Object.assign(I18N.ar.s, {
  "rec.record": "سجّل",
  "rec.recordTip": "يسجّل الميكروفون والصوت الذي يشغّله الحاسوب، مثل مكالمة Zoom أو Meet أو Teams، ويكتب النص أثناء الكلام",
  "rec.stop": "أوقف",
  "rec.both": "الميكروفون وصوت الحاسوب",
  "rec.mic": "الميكروفون وحده",
  "rec.pc": "صوت الحاسوب وحده",
  "rec.sourcesTip": "طرفا المكالمة، أو الميكروفون وحده لمحاضرة في قاعة، أو صوت الحاسوب وحده لندوة عبر الإنترنت أو فيديو",
  "rec.defaultMic": "ميكروفون Windows ({name})",
  "rec.defaultMicNone": "ميكروفون Windows",
  "rec.micTip": "الميكروفون الذي يُسجَّل منه. إن تغيّر ميكروفون Windows تبعه التسجيل",
  "rec.me": "أنا",
  "rec.them": "هم",
  "rec.meTip": "ما يسمعه ميكروفونك",
  "rec.themTip": "ما يشغّله الحاسوب: الآخرون في المكالمة",
  "rec.showText": "اعرض النص",
  "rec.micSilent": "لا يصل أي صوت من «{name}»: ربما هو مكتوم أو غير موصول. اختر ميكروفونًا آخر:",
  "rec.micBlocked": "لا يسمح Windows لدَوِّن باستخدام الميكروفون.",
  "rec.openPrivacy": "اسمح به في Windows",
  "rec.noDevice": "لا يوجد ميكروفون.",
  "rec.pcFailed": "تعذّر تسجيل صوت الحاسوب.",
  "rec.diskLow": "المساحة الحرة على هذا القرص أقل من 2 غيغابايت. يتوقف التسجيل من تلقاء نفسه تحت 500 ميغابايت.",
  "rec.diskStopped": "توقف التسجيل: القرص شبه ممتلئ.",
  "rec.behind": "لا يستطيع هذا الحاسوب كتابة النص بسرعة الكلام. يستمر التسجيل، ويأتي باقي النص بعد انتهائه.",
  "rec.interrupted": "انقطع تسجيل قبل أن ينتهي: «{title}»، {dur}.",
  "rec.recover": "فرّغه",
  "rec.discard": "احذفه",
  "rec.discardSure": "أتحذفه نهائيًا؟",
  "rec.synced": "تُحفظ التسجيلات في {folder}، وهو داخل OneDrive، فيرفعها OneDrive إلى الإنترنت.",
  "rec.failed": "توقف التسجيل: تعذّرت كتابته على القرص ({err}).",
  "rec.needFfmpeg": "حُفظ التسجيل بصيغة WAV، وهي أكبر بخمس مرات من M4A. نزّل ffmpeg ‏({size}) لتبقى التسجيلات صغيرة.",
  "job.recording": "جارٍ التسجيل · {dur}",
  "job.waitRecording": "في انتظار انتهاء التسجيل",
  "guide.recordTitle": "سجّل اجتماعًا",
  "guide.recordText": "اضغط «سجّل» في صفحة البدء لتسجيل ميكروفونك وكل ما يشغّله الحاسوب، مثل مكالمة Zoom أو Meet أو Teams أو فيديو، دون أن ينضم أي شيء إلى المكالمة. اختر الميكروفون وحده لمحاضرة في قاعة، أو صوت الحاسوب وحده لندوة عبر الإنترنت. يظهر النص أثناء الكلام وكل سطر موسوم بـ«أنا» أو «هم»، ويُحفظ التسجيل في المستندات داخل مجلد Dawwin (أو في مجلد Dawwin داخل مجلد المستخدم إن كانت المستندات داخل OneDrive، كي لا يُرفع). أخبر الآخرين قبل أن تسجّل مكالمة.",
});

const REC = {
  live: null,       // the recording being made: {id, status, behind}
  setup: null,      // {settings: {sources, mic}, microphones: [...], folder}
  cutOff: [],       // recordings found after a crash
  armed: null,      // the one whose Delete was pressed once
};

/** The job of the recording being made, if one is. */
function recordJob() {
  return S.snap?.queue.find((j) => j.kind === "record" && j.status === "recording");
}

/** The queue's line for a recording. */
function recordJobLine(j) {
  const s = REC.live?.id === j.id ? REC.live.status : null;
  return t("job.recording", { dur: ts(s?.seconds || j.duration_s || 0) });
}

/** Why a job waits: a recording holds the speech engine. */
function recordWaiting() {
  return S.snap?.queue.some((j) => j.kind === "record" && ACTIVE.has(j.status) && j.status !== "queued") ? t("job.waitRecording") : "";
}

/** "Me" or "Them" before a line of a recorded meeting. */
function whoLabel(s) {
  if (!s.track) return null;
  const el = document.createElement("span");
  el.className = "who who-" + s.track;
  el.textContent = t(s.track === "me" ? "rec.me" : "rec.them");
  return el;
}

async function loadRecordSetup() {
  try { REC.setup = await invoke("record_setup"); } catch { /* recording is not there */ }
}

function micName(id) {
  return REC.setup?.microphones.find((m) => m.id === id)?.name || id;
}

/** The microphone menu: Windows' own first, then each one there is. */
function micOptions(selected) {
  const mics = REC.setup?.microphones || [];
  const def = mics.find((m) => m.default);
  const first = `<option value="">${esc(def ? t("rec.defaultMic", { name: def.name }) : t("rec.defaultMicNone"))}</option>`;
  return first + mics.map((m) => `<option value="${esc(m.id)}"${m.id === selected ? " selected" : ""}>${esc(m.name)}</option>`).join("");
}

function meter(track, level) {
  // -60 dBFS to 0 across the bar
  const w = level ? Math.max(0, Math.min(100, ((level.db + 60) / 60) * 100)) : 0;
  return `<div class="rec-meter" title="${esc(t(track === "me" ? "rec.meTip" : "rec.themTip"))}">
    <span class="rec-meter-name">${esc(t(track === "me" ? "rec.me" : "rec.them"))}</span>
    <span class="rec-meter-bar"><span style="width:${w.toFixed(0)}%"></span></span></div>`;
}

/** What is wrong while it records, said plainly, with the way out. */
function recordProblems(st) {
  const out = [];
  if (st.me_problem === "mic-blocked") out.push(`<p class="rec-warn">${esc(t("rec.micBlocked"))} <button class="linkbtn" data-rec="privacy">${esc(t("rec.openPrivacy"))}</button></p>`);
  else if (st.me_problem === "no-device") out.push(`<p class="rec-warn">${esc(t("rec.noDevice"))}</p>`);
  if (st.mic_silent) {
    out.push(`<p class="rec-warn">${esc(t("rec.micSilent", { name: st.mic || "" }))}
      <select class="input select select-sm" data-rec="mic-now" title="${esc(t("rec.micTip"))}">${micOptions(REC.setup?.settings.mic || "")}</select></p>`);
  }
  if (st.them_problem) out.push(`<p class="rec-warn">${esc(t("rec.pcFailed"))}</p>`);
  if (st.error) out.push(`<p class="rec-warn">${esc(t("rec.failed", { err: st.error }))}</p>`);
  if (st.disk_stopped) out.push(`<p class="rec-warn">${esc(t("rec.diskStopped"))}</p>`);
  else if (st.disk_low) out.push(`<p class="rec-warn">${esc(t("rec.diskLow"))}</p>`);
  if (REC.live?.behind) out.push(`<p class="rec-note">${esc(t("rec.behind"))}</p>`);
  return out.join("");
}

/** The panel of a recording that runs: its time, the two levels, Stop, and what is wrong. */
function recordPanel(withText) {
  const live = REC.live, st = live?.status || {};
  const sources = live?.sources || REC.setup?.settings.sources || "both";
  const meters = (sources !== "pc" ? meter("me", st.me) : "") + (sources !== "mic" ? meter("them", st.them) : "");
  return `<div class="rec-live">
      <span class="rec-dot on" aria-hidden="true"></span>
      <span class="rec-time" dir="ltr">${esc(ts(st.seconds || 0))}</span>
      <div class="rec-meters">${meters}</div>
      ${withText ? `<button class="btn btn-sm btn-ghost" data-rec="open">${esc(t("rec.showText"))}</button>` : ""}
      <button class="btn btn-sm btn-primary" data-rec="stop">${esc(t("rec.stop"))}</button>
    </div>${recordProblems(st)}`;
}

/** The start page's Record bar: Record and what to record, or the recording that runs. */
/** A menu of the panel is open: drawing it again would close the menu under the pointer. */
function choosing(box) {
  const el = document.activeElement;
  return !!el && el.tagName === "SELECT" && box.contains(el);
}

function renderRecordBar() {
  const bar = $("record-bar");
  if (!bar || choosing(bar)) return;
  const job = recordJob();
  let html = "";
  if (job) html = recordPanel(true);
  else {
    const s = REC.setup?.settings || { sources: "both", mic: null };
    html = `<div class="rec-idle">
      <button class="btn btn-record" data-rec="start" title="${esc(t("rec.recordTip"))}"><span class="rec-dot" aria-hidden="true"></span>${esc(t("rec.record"))}</button>
      <select class="input select" data-rec="sources" aria-label="${esc(t("rec.sourcesTip"))}" title="${esc(t("rec.sourcesTip"))}">
        ${["both", "mic", "pc"].map((k) => `<option value="${k}"${s.sources === k ? " selected" : ""}>${esc(t("rec." + k))}</option>`).join("")}
      </select>
      ${s.sources !== "pc" ? `<select class="input select" data-rec="mic" aria-label="${esc(t("rec.micTip"))}" title="${esc(t("rec.micTip"))}">${micOptions(s.mic || "")}</select>` : ""}
    </div>`;
    // why the last recording stopped by itself, until the next one
    if (REC.diskStopped) html += `<p class="rec-warn">${esc(t("rec.diskStopped"))}</p>`;
    if (REC.failed) html += `<p class="rec-warn">${esc(t("rec.failed", { err: REC.failed }))}</p>`;
    // a folder OneDrive uploads: said where Record is
    if (REC.setup?.synced) html += `<p class="rec-warn" dir="auto">${tHtml("rec.synced", { folder: ltr(REC.setup.folder) })}</p>`;
  }
  for (const c of REC.cutOff) {
    const sure = REC.armed === c.journal;
    html += `<div class="rec-cutoff" data-journal="${esc(c.journal)}">
      <span dir="auto">${esc(t("rec.interrupted", { title: c.title, dur: dur(c.seconds) }))}</span>
      <button class="btn btn-sm btn-primary" data-rec="recover">${esc(t("rec.recover"))}</button>
      <button class="btn btn-sm ${sure ? "btn-danger" : "btn-ghost"}" data-rec="discard">${esc(t(sure ? "rec.discardSure" : "rec.discard"))}</button>
    </div>`;
  }
  bar.innerHTML = html;
}

/** The levels and Stop above the text of the recording on screen. */
function recordProgress(j) {
  const box = $("t-rec");
  if (!box || choosing(box)) return;
  const on = j?.kind === "record" && j.status === "recording";
  box.hidden = !on;
  if (on) box.innerHTML = recordPanel(false);
  $("t-progress").classList.toggle("recording", on);
}

async function startRecording() {
  const steps = { ...stepsForJob(), transcribe: true, remove_silence: false, save_audio: false, video: false, burn: false };
  if (!stepsReady(steps)) return;
  // once, before the first recording: consent
  if (!(await invoke("recording_notice"))) return;
  try {
    REC.diskStopped = null;
    REC.failed = null;
    const id = await invoke("start_recording", { language: S.spoken, steps });
    onState(await invoke("get_state"));
    openTranscript(id);
  } catch (err) {
    toast(String(err));
  }
}

async function onRecordClick(ev) {
  const el = ev.target.closest("[data-rec]");
  if (!el || el.tagName === "SELECT") return;
  const what = el.dataset.rec;
  if (what === "start") return startRecording();
  if (what === "stop") return invoke("stop_recording");
  if (what === "open") { const j = recordJob(); if (j) openTranscript(j.id); return; }
  if (what === "privacy") return invoke("open_microphone_privacy");
  const card = el.closest(".rec-cutoff");
  if (!card) return;
  const journal = card.dataset.journal;
  if (what === "discard" && REC.armed !== journal) { REC.armed = journal; renderRecordBar(); return; }
  try {
    const id = await invoke("recover_recording", { journal, keep: what === "recover" });
    REC.cutOff = REC.cutOff.filter((c) => c.journal !== journal);
    REC.armed = null;
    renderRecordBar();
    if (id) { onState(await invoke("get_state")); openTranscript(id); }
    else loadHistory();
  } catch (err) {
    toast(String(err));
  }
}

async function onRecordChange(ev) {
  const el = ev.target.closest("select[data-rec]");
  if (!el) return;
  el.blur();
  try {
    if (el.dataset.rec === "sources") REC.setup.settings = await invoke("set_record_sources", { sources: el.value });
    else REC.setup.settings = await invoke("choose_microphone", { id: el.value || null });
  } catch (err) {
    toast(String(err));
  }
  renderRecordBar();
}

/** Called by app.js when the page is wired. */
async function recordWire() {
  for (const id of ["record-bar", "t-rec"]) {
    const box = $(id);
    if (!box) continue;
    box.addEventListener("click", onRecordClick);
    box.addEventListener("change", onRecordChange);
    // the list of microphones as it is when it is opened
    box.addEventListener("pointerdown", async (ev) => {
      if (ev.target.closest('select[data-rec="mic"], select[data-rec="mic-now"]')) await loadRecordSetup();
    });
  }
  listen("recording", (e) => {
    const { id, status } = e.payload;
    REC.live = { ...(REC.live?.id === id ? REC.live : {}), id, status, sources: REC.live?.sources };
    // it stopped by itself: said once, whatever page is on screen
    if (status.ended && status.disk_stopped && !REC.diskStopped) { REC.diskStopped = id; toast(t("rec.diskStopped")); }
    if (status.ended && status.error && !REC.failed) { REC.failed = status.error; toast(t("rec.failed", { err: status.error })); }
    paintRecording();
  });
  await loadRecordSetup();
  try { REC.live = await invoke("recording_state"); } catch { /* none */ }
  try { REC.cutOff = await invoke("interrupted_recordings"); } catch { /* none */ }
  renderRecordBar();
}

let recPaint = 0;
/** Ten times a second at most: the levels and the time. */
function paintRecording() {
  if (recPaint) return;
  recPaint = requestAnimationFrame(async () => {
    recPaint = 0;
    if (REC.live && REC.live.sources === undefined) {
      try { const v = await invoke("recording_state"); if (v?.id === REC.live.id) { REC.live.sources = v.sources; REC.live.behind = v.behind; } } catch { /* ignore */ }
    }
    if (S.view === "home") renderRecordBar();
    const j = S.current && jobById(S.current.id);
    if (S.view === "transcript" && j?.kind === "record") recordProgress(j);
  });
}

/** Called by app.js with each new state of the app. */
function recordOnState(snap) {
  const j = snap.queue.find((x) => x.kind === "record" && ACTIVE.has(x.status));
  if (j && REC.live?.id === j.id) {
    // behind or not comes with the state of the job's recording
    invoke("recording_state").then((v) => { if (v?.id === j.id) { REC.live.behind = v.behind; REC.live.sources = v.sources; } }).catch(() => {});
  }
  if (S.view === "home") renderRecordBar();
  // a recording whose text was not written (the engine failed, or it was stopped): offered again
  const ended = (snap.queue || []).filter((x) => x.kind === "record" && ["failed", "cancelled"].includes(x.status) && !(REC.offered ||= new Set()).has(x.id));
  if (ended.length) {
    for (const x of ended) REC.offered.add(x.id);
    invoke("interrupted_recordings").then((list) => { REC.cutOff = list; if (S.view === "home") renderRecordBar(); }).catch(() => {});
  }
  // a recording kept as a WAV: ffmpeg would keep it five times smaller
  for (const x of snap.queue) {
    if (x.kind === "record" && x.status === "done" && x.outputs?.some((o) => o.toLowerCase().endsWith(".wav")) && !snap.setup.ffmpeg && !REC.told?.has(x.id)) {
      (REC.told ||= new Set()).add(x.id);
      setTimeout(() => offerTool("ffmpeg-audio", "rec.needFfmpeg"), 4000);
    }
  }
}
