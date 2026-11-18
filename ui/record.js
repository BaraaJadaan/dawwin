// Recording a meeting: the Record bar on the start page, the levels of "me" and "them" while it
// runs, the choice of microphone, what to do with a recording once it stopped (transcribe it with
// the steps ticked, just keep it, or delete it), and the recordings found after a crash. Loaded
// after i18n.js and before app.js, whose helpers it uses once the page is wired.
"use strict";

Object.assign(I18N.en.s, {
  "rec.record": "Record",
  "rec.recordTip": "Records your microphone and the sound the PC plays, such as a Zoom, Meet or Teams call. When you stop, you choose what to do with it",
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
  "rec.micSilent": "No sound comes from “{name}”: it may be muted, or not connected. Choose another microphone:",
  "rec.micBlocked": "Windows does not let Dawwin use the microphone.",
  "rec.openPrivacy": "Allow it in Windows",
  "rec.noDevice": "There is no microphone.",
  "rec.pcFailed": "The PC's sound cannot be recorded.",
  "rec.diskLow": "Less than 2 GB is free on this disk. The recording stops by itself under 500 MB.",
  "rec.diskStopped": "The recording stopped: the disk is nearly full.",
  "rec.interrupted": "A recording was cut off: “{title}”, {dur}.",
  "rec.recorded": "Recorded: “{title}”, {dur}.",
  "rec.recover": "Transcribe it",
  "rec.choose": "What to do with it",
  "rec.chooseTitle": "What should Dawwin do with the recording?",
  "rec.go": "Start",
  "rec.justSave": "Just save it",
  "rec.justSaveTip": "Keeps the recording in the history without its text. Open it there to transcribe it later",
  "rec.later": "Decide later",
  "rec.silences": "Save the sound without silences",
  "rec.savedNote": "This recording has no text yet.",
  "rec.discard": "Delete it",
  "rec.discardSure": "Delete it for good?",
  "rec.synced": "Recordings are saved in {folder}, which is in OneDrive: OneDrive uploads them to the internet.",
  "rec.failed": "The recording stopped: it could not be written to the disk ({err}).",
  "rec.needFfmpeg": "The recording is kept as a WAV, five times larger than an M4A. Download ffmpeg ({size}) to keep recordings small.",
  "job.recording": "Recording · {dur}",
  "rec.stopWriting": "Stop writing",
  "rec.stopWritingSure": "Stop writing the text?",
  "rec.stopWritingTip": "The recording is kept, and its whole text can be written later from the start page",
  // recordings go to the folder of one's own too
  "set.saveWhere": "Where the sound, the videos and the recordings are saved",
  "tip.saveWhere": "Where the steps save the sound and the videos, where meetings are recorded, and where save dialogs open. A folder's recordings keep their files next to them.",
  "guide.recordTitle": "Record a meeting",
  "guide.recordText": "Press Record on the start page to record your microphone and everything the PC plays, such as a Zoom, Meet or Teams call or a video, with nothing joining the call. Choose the microphone alone for a lecture in a room, or the PC's sound alone for a webinar. It only records. When you press Stop, Dawwin asks what to do with the recording: transcribe it in the language you choose, each line marked Me or Them, and tell the speakers apart, write the minutes, translate it, or save the sound without its silences; or just save it, and transcribe it later from the history. The recording is kept in the folder chosen in Settings, or in Documents\\Dawwin (in Dawwin in your user folder when Documents is in OneDrive, so that it is not uploaded). It keeps the sound as the PC played it, in stereo, with your microphone in the middle of it. Tell the others before you record a call.",
});

Object.assign(I18N.ar.s, {
  "rec.record": "سجّل",
  "rec.recordTip": "يسجّل الميكروفون والصوت الذي يشغّله الحاسوب، مثل مكالمة Zoom أو Meet أو Teams. وحين توقفه تختار ما تفعله به",
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
  "rec.micSilent": "لا يصل أي صوت من «{name}»: ربما هو مكتوم أو غير موصول. اختر ميكروفونًا آخر:",
  "rec.micBlocked": "لا يسمح Windows لدَوِّن باستخدام الميكروفون.",
  "rec.openPrivacy": "اسمح به في Windows",
  "rec.noDevice": "لا يوجد ميكروفون.",
  "rec.pcFailed": "تعذّر تسجيل صوت الحاسوب.",
  "rec.diskLow": "المساحة الحرة على هذا القرص أقل من 2 غيغابايت. يتوقف التسجيل من تلقاء نفسه تحت 500 ميغابايت.",
  "rec.diskStopped": "توقف التسجيل: القرص شبه ممتلئ.",
  "rec.interrupted": "انقطع تسجيل قبل أن ينتهي: «{title}»، {dur}.",
  "rec.recorded": "سُجّل: «{title}»، {dur}.",
  "rec.recover": "فرّغه",
  "rec.choose": "اختر ما تفعله به",
  "rec.chooseTitle": "ماذا يفعل دَوِّن بالتسجيل؟",
  "rec.go": "ابدأ",
  "rec.justSave": "احفظه فقط",
  "rec.justSaveTip": "يبقى التسجيل في السجل دون نص. افتحه من هناك لتفرّغه لاحقًا",
  "rec.later": "قرّر لاحقًا",
  "rec.silences": "احفظ الصوت دون فترات الصمت",
  "rec.savedNote": "لا نص لهذا التسجيل بعد.",
  "rec.discard": "احذفه",
  "rec.discardSure": "أتحذفه نهائيًا؟",
  "rec.synced": "تُحفظ التسجيلات في {folder}، وهو داخل OneDrive، فيرفعها OneDrive إلى الإنترنت.",
  "rec.failed": "توقف التسجيل: تعذّرت كتابته على القرص ({err}).",
  "rec.needFfmpeg": "حُفظ التسجيل بصيغة WAV، وهي أكبر بخمس مرات من M4A. نزّل ffmpeg ‏({size}) لتبقى التسجيلات صغيرة.",
  "job.recording": "جارٍ التسجيل · {dur}",
  "rec.stopWriting": "أوقف الكتابة",
  "rec.stopWritingSure": "أتوقف كتابة النص؟",
  "rec.stopWritingTip": "يبقى التسجيل محفوظًا، ويمكن كتابة نصه كاملًا لاحقًا من صفحة البدء",
  "set.saveWhere": "مكان حفظ الصوت والفيديوهات والتسجيلات",
  "tip.saveWhere": "أين تحفظ الخطوات الصوت والفيديوهات، وأين تُسجَّل الاجتماعات، وأين تُفتح نوافذ الحفظ. تسجيلات المجلد تبقى ملفاتها بجانبها.",
  "guide.recordTitle": "سجّل اجتماعًا",
  "guide.recordText": "اضغط «سجّل» في صفحة البدء لتسجيل ميكروفونك وكل ما يشغّله الحاسوب، مثل مكالمة Zoom أو Meet أو Teams أو فيديو، دون أن ينضم أي شيء إلى المكالمة. اختر الميكروفون وحده لمحاضرة في قاعة، أو صوت الحاسوب وحده لندوة عبر الإنترنت. هو يسجّل فقط، وحين تضغط «أوقف» يسألك دَوِّن ماذا يفعل بالتسجيل: يفرّغه باللغة التي تختارها وكل سطر موسوم بـ«أنا» أو «هم»، ويميّز المتحدثين، ويكتب المحضر، ويترجمه، أو يحفظ الصوت دون فترات الصمت؛ أو يحفظه فقط لتفرّغه لاحقًا من السجل. يُحفظ التسجيل في المجلد المختار في الإعدادات، أو في المستندات داخل مجلد Dawwin (أو في مجلد Dawwin داخل مجلد المستخدم إن كانت المستندات داخل OneDrive، كي لا يُرفع). ويحفظ الصوت كما شغّله الحاسوب، بصوت مجسَّم، وميكروفونك في وسطه. أخبر الآخرين قبل أن تسجّل مكالمة.",
});

const REC = {
  live: null,       // the recording being made: {id, status, sources}
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

/** How full a level bar is: -60 dBFS to 0 across it. */
function meterWidth(level) {
  return (level ? Math.max(0, Math.min(100, ((level.db + 60) / 60) * 100)) : 0).toFixed(0) + "%";
}

function meter(track) {
  return `<div class="rec-meter" data-track="${track}" title="${esc(t(track === "me" ? "rec.meTip" : "rec.themTip"))}">
    <span class="rec-meter-name">${esc(t(track === "me" ? "rec.me" : "rec.them"))}</span>
    <span class="rec-meter-bar"><span style="width:0%"></span></span></div>`;
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
  return out.join("");
}

/** The panel of a recording that runs, in `box`: its time, the two levels, Stop, and what is
 *  wrong. It is drawn once and then brought up to date in place, ten times a second: Stop stays
 *  the same button, so that a click on it is never lost to a redraw. */
function paintRecordPanel(box, job) {
  // the levels of this recording only: never those of the one before it
  const live = REC.live && (!job || REC.live.id === job.id) ? REC.live : null, st = live?.status || {};
  if (job && !live && !REC.asking) {
    REC.asking = true;
    invoke("recording_state").then((v) => { if (v?.id === job.id) { REC.live = v; paintRecording(); } }).catch(() => {}).finally(() => { REC.asking = false; });
  }
  const sources = live?.sources || REC.setup?.settings.sources || "both";
  const key = `${sources}|${UI_LANG}`;
  let panel = box.querySelector(":scope > .rec-live");
  if (!panel || box.dataset.panel !== key) {
    box.dataset.panel = key;
    const meters = (sources !== "pc" ? meter("me") : "") + (sources !== "mic" ? meter("them") : "");
    box.innerHTML = `<div class="rec-live">
        <span class="rec-dot on" aria-hidden="true"></span>
        <span class="rec-time" dir="ltr"></span>
        <div class="rec-meters">${meters}</div>
        <button class="btn btn-sm btn-primary" data-rec="stop">${esc(t("rec.stop"))}</button>
      </div><div class="rec-problems"></div><div class="rec-cutoffs"></div>`;
    panel = box.querySelector(":scope > .rec-live");
  }
  setText(panel.querySelector(".rec-time"), ts(st.seconds || 0));
  for (const m of panel.querySelectorAll(".rec-meter")) {
    const bar = m.querySelector(".rec-meter-bar > span");
    const w = meterWidth(st[m.dataset.track]);
    if (bar.style.width !== w) bar.style.width = w;
  }
  const problems = box.querySelector(":scope > .rec-problems");
  const html = recordProblems(st);
  if (problems.dataset.html !== html && !choosing(problems)) {
    problems.dataset.html = html;
    problems.innerHTML = html;
  }
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
  if (job) {
    paintRecordPanel(bar, job);
    const cards = bar.querySelector(":scope > .rec-cutoffs");
    const html = cutOffCards();
    if (cards.dataset.html !== html) {
      cards.dataset.html = html;
      cards.innerHTML = html;
    }
    return;
  }
  let html = "";
  {
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
  html += cutOffCards();
  // drawn again only when something in it changed: a button pressed while it is drawn keeps its click
  if (bar.dataset.html === html && !bar.dataset.panel) return;
  delete bar.dataset.panel;
  bar.dataset.html = html;
  bar.innerHTML = html;
}

/** The recordings waiting beside their journals, each with What to do with it and Delete it: one
 *  stopped while the question was put off, or one cut off by a crash. */
function cutOffCards() {
  let html = "";
  for (const c of REC.cutOff) {
    const sure = REC.armed === c.journal;
    html += `<div class="rec-cutoff${c.stopped ? " rec-waiting" : ""}" data-journal="${esc(c.journal)}">
      <span dir="auto">${esc(t(c.stopped ? "rec.recorded" : "rec.interrupted", { title: c.title, dur: dur(c.seconds) }))}</span>
      <button class="btn btn-sm btn-primary" data-rec="choose">${esc(t("rec.choose"))}</button>
      <button class="btn btn-sm ${sure ? "btn-danger" : "btn-ghost"}" data-rec="discard">${esc(t(sure ? "rec.discardSure" : "rec.discard"))}</button>
    </div>`;
  }
  return html;
}

/** The levels and Stop above the text of the recording on screen. */
function recordProgress(j) {
  const box = $("t-rec");
  if (!box || choosing(box)) return;
  const on = j?.kind === "record" && j.status === "recording";
  box.hidden = !on;
  if (on) paintRecordPanel(box, j);
  $("t-progress").classList.toggle("recording", on);
}

/** Cancelling a job. Once a recording is being transcribed, cancelling stops the writing of the
 *  rest of its text: that takes a second click, the first one asking on the button itself, and no
 *  click in the moment after Stop counts, since it was meant for Stop. */
const CANCEL = { armed: null, timer: 0, recording: new Set(), ended: new Map() };
function cancelJob(j, repaint) {
  if (j.kind === "record" && j.status !== "recording") {
    if (Date.now() - (CANCEL.ended.get(j.id) || 0) < 1500) return;
    if (CANCEL.armed !== j.id) {
      CANCEL.armed = j.id;
      clearTimeout(CANCEL.timer);
      CANCEL.timer = setTimeout(() => { CANCEL.armed = null; repaint?.(); }, 4000);
      repaint?.();
      return;
    }
    CANCEL.armed = null;
  }
  invoke("cancel_job", { id: j.id });
}

/** What cancelling a job is called on its button. */
function cancelLabel(j) {
  if (j.status === "recording") return t("rec.stop");
  // writing its text: stopped by Stop writing; only kept, it has no text to stop
  if (j.kind === "record" && ACTIVE.has(j.status) && j.steps?.transcribe && j.status !== "saving_audio") return t(CANCEL.armed === j.id ? "rec.stopWritingSure" : "rec.stopWriting");
  return "";
}

/** Record: it only records, on the start page, until Stop. */
async function startRecording() {
  // once, before the first recording: consent
  if (!(await invoke("recording_notice"))) return;
  try {
    REC.diskStopped = null;
    REC.failed = null;
    await invoke("start_recording", { language: S.spoken });
    onState(await invoke("get_state"));
    if (S.view !== "home") showView("home");
  } catch (err) {
    toast(String(err));
  }
}

// ------------------------------------------------------------------ what to do with it

/** The question put once a recording stopped. `CHOOSE.rec` is the recording it is about: one
 *  beside its journal ({journal, id, title, seconds, sources}) or one kept without its text
 *  ({saved: its transcript's id, title, seconds, sources}). */
const CHOOSE = { rec: null, armed: false, timer: 0, busy: false, openedAt: 0, held: null, leaving: 0 };

/** What was chosen the last time, else the steps ticked on the start page. */
function choiceDefaults() {
  let last = {};
  try { last = JSON.parse(store.get("recChoice", "{}")) || {}; } catch { /* none */ }
  const s = S.steps;
  return { transcribe: true, speakers: !!s.speakers, minutes: !!s.minutes, translate: !!s.translate, target: stepTarget(), silences: false, ...last };
}

function chooserBox() {
  let back = $("rec-choose");
  if (back) return back;
  back = document.createElement("div");
  back.id = "rec-choose";
  back.className = "modal-back";
  back.hidden = true;
  back.innerHTML = `<div class="modal" role="dialog" aria-modal="true" aria-labelledby="rc-title">
      <div class="modal-head">
        <h2 id="rc-title"></h2>
        <button class="icon-btn modal-x" data-c="later" aria-label="">
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M6 6l12 12M18 6 6 18"/></svg>
        </button>
      </div>
      <p class="rc-what" dir="auto"></p>
      <div class="rc-opts">
        <div class="rc-row"><label class="toggle"><input type="checkbox" data-c="transcribe"> <span data-t="steps.transcribe"></span></label><select class="input select" data-c="lang"></select></div>
        <div class="rc-row rc-sub" data-for="speakers"><label class="toggle"><input type="checkbox" data-c="speakers"> <span data-t="steps.speakers"></span></label></div>
        <div class="rc-row rc-sub"><label class="toggle"><input type="checkbox" data-c="minutes"> <span data-t="steps.minutes"></span></label></div>
        <div class="rc-row rc-sub"><label class="toggle"><input type="checkbox" data-c="translate"> <span data-t="steps.translate"></span></label><select class="input select" data-c="target"></select></div>
        <div class="rc-row"><label class="toggle"><input type="checkbox" data-c="silences"> <span data-t="rec.silences"></span></label></div>
      </div>
      <div class="modal-actions">
        <button class="btn btn-primary" data-c="go" data-t="rec.go"></button>
        <button class="btn btn-ghost" data-c="save" data-t="rec.justSave"></button>
        <span class="grow"></span>
        <button class="btn btn-ghost rc-delete" data-c="delete"></button>
      </div>
    </div>`;
  document.body.append(back);
  // a press outside the question, or Escape: decided later, the recording waits on the start page
  // (not in the moment it opens: a second click meant for Stop would put it off unseen)
  back.addEventListener("pointerdown", (ev) => { if (ev.target === back && Date.now() - CHOOSE.openedAt > 700) closeChooser(); });
  back.addEventListener("click", onChooserClick);
  back.addEventListener("change", onChooserChange);
  return back;
}

/** Ask what to do with `rec`. One question at a time: another waits on the start page. */
function openChooser(rec) {
  if (CHOOSE.rec) return;
  const back = chooserBox();
  // one that was closing a moment ago opens again rather than vanish
  clearTimeout(CHOOSE.leaving);
  back.classList.remove("leaving");
  CHOOSE.rec = rec;
  CHOOSE.armed = false;
  CHOOSE.openedAt = Date.now();
  const c = choiceDefaults();
  for (const el of back.querySelectorAll("[data-t]")) el.textContent = t(el.dataset.t);
  $("rc-title").textContent = t("rec.chooseTitle");
  const x = back.querySelector(".modal-x");
  x.title = t("rec.later");
  x.setAttribute("aria-label", t("rec.later"));
  back.querySelector('[data-c="save"]').title = t("rec.justSaveTip");
  back.querySelector(".rc-what").textContent = `${rec.title} · ${ts(rec.seconds || 0)}`;
  const lang = back.querySelector('[data-c="lang"]');
  lang.innerHTML = $("spoken").innerHTML;
  lang.value = S.spoken;
  lang.setAttribute("aria-label", t("home.spoken"));
  const target = back.querySelector('[data-c="target"]');
  target.innerHTML = $("step-target").innerHTML;
  target.value = S.targets.has(c.target) ? c.target : stepTarget();
  otherTarget();
  target.setAttribute("aria-label", t("steps.translate"));
  for (const k of ["transcribe", "speakers", "minutes", "translate", "silences"]) back.querySelector(`[data-c="${k}"]`).checked = !!c[k];
  // telling the others apart is for what the PC played: not there with the microphone alone
  back.querySelector('[data-for="speakers"]').hidden = rec.sources === "mic";
  // one kept without its text is deleted from its own page, and is saved already
  back.querySelector('[data-c="save"]').hidden = !!rec.saved;
  back.querySelector('[data-c="delete"]').hidden = !!rec.saved;
  paintChooser();
  back.hidden = false;
  // the rest of the window waits; a message that comes meanwhile can still be pressed
  CHOOSE.held = [...document.body.children].filter((el) => el !== back && el.id !== "toast" && el.tagName !== "SCRIPT" && !el.inert);
  for (const el of CHOOSE.held) el.inert = true;
  back.querySelector('[data-c="go"]').focus({ preventScroll: true });
}

function closeChooser() {
  const back = $("rec-choose");
  if (!back || back.hidden) return;
  CHOOSE.rec = null;
  CHOOSE.busy = false;
  clearTimeout(CHOOSE.timer);
  for (const el of CHOOSE.held || []) el.inert = false;
  CHOOSE.held = null;
  back.classList.add("leaving");
  CHOOSE.leaving = setTimeout(() => { back.hidden = true; back.classList.remove("leaving"); }, 160);
  if (S.view === "home") renderRecordBar();
}

/** What depends on what: the steps after the text need the text; Start needs something to do. */
function paintChooser() {
  const back = $("rec-choose"), on = (k) => back.querySelector(`[data-c="${k}"]`).checked;
  const text = on("transcribe");
  for (const el of back.querySelectorAll(".rc-sub")) el.classList.toggle("off", !text);
  back.querySelector('[data-c="lang"]').disabled = !text;
  back.querySelector('[data-c="target"]').disabled = !text || !on("translate");
  back.querySelector('[data-c="go"]').disabled = CHOOSE.busy || (!text && !on("silences"));
  back.querySelector('[data-c="save"]').disabled = CHOOSE.busy;
  const del = back.querySelector('[data-c="delete"]');
  del.textContent = t(CHOOSE.armed ? "rec.discardSure" : "rec.discard");
  del.classList.toggle("btn-danger", CHOOSE.armed);
  del.classList.toggle("btn-ghost", !CHOOSE.armed);
}

/** Never into the language it is in: then English, or Arabic for a recording in English. */
function otherTarget() {
  const back = $("rec-choose"), lang = back.querySelector('[data-c="lang"]').value, target = back.querySelector('[data-c="target"]');
  if (target.value === lang) target.value = lang === "en" ? "ar" : "en";
}

function onChooserChange(ev) {
  const k = ev.target.dataset.c, back = $("rec-choose");
  if (k === "lang") otherTarget();
  const box = (n) => back.querySelector(`[data-c="${n}"]`);
  // a step after the text ticks the text; no text, none of them
  if (["speakers", "minutes", "translate"].includes(k) && ev.target.checked) box("transcribe").checked = true;
  if (k === "transcribe" && !ev.target.checked) for (const n of ["speakers", "minutes", "translate"]) box(n).checked = false;
  paintChooser();
}

async function onChooserClick(ev) {
  const el = ev.target.closest("[data-c]");
  if (!el || el.tagName === "SELECT" || el.tagName === "INPUT" || !CHOOSE.rec || CHOOSE.busy) return;
  const what = el.dataset.c, rec = CHOOSE.rec, back = $("rec-choose");
  if (what === "later") return closeChooser();
  if (what === "delete") {
    if (!CHOOSE.armed) {
      CHOOSE.armed = true;
      clearTimeout(CHOOSE.timer);
      CHOOSE.timer = setTimeout(() => { CHOOSE.armed = false; if (CHOOSE.rec) paintChooser(); }, 4000);
      return paintChooser();
    }
    return finishChoice(() => invoke("recover_recording", { journal: rec.journal, keep: false }), rec, false);
  }
  if (what === "save") return finishChoice(() => invoke("finish_recording", { journal: rec.journal, language: null, steps: null }), rec, false);
  if (what !== "go") return;
  const on = (k) => back.querySelector(`[data-c="${k}"]`).checked;
  const language = back.querySelector('[data-c="lang"]').value;
  const target = back.querySelector('[data-c="target"]').value;
  const steps = {
    remove_silence: on("silences"), save_audio: false, transcribe: on("transcribe"),
    speakers: on("transcribe") && on("speakers") && rec.sources !== "mic",
    translate: on("transcribe") && on("translate") ? target : null,
    video: false, burn: false, minutes: on("transcribe") && on("minutes"),
  };
  // what the steps need, as the start page checks it, for the language chosen here
  if (steps.transcribe) {
    const was = S.spoken;
    S.spoken = language;
    const ready = stepsReady(steps);
    S.spoken = was;
    if (!ready) return;
  }
  try { store.set("recChoice", JSON.stringify({ transcribe: on("transcribe"), speakers: on("speakers"), minutes: on("minutes"), translate: on("translate"), target, silences: on("silences") })); } catch { /* not kept */ }
  const run = rec.saved
    ? () => invoke("transcribe_saved", { id: rec.saved, language, steps })
    : () => invoke("finish_recording", { journal: rec.journal, language, steps });
  return finishChoice(run, rec, steps.transcribe);
}

/** The choice made: the question goes, and the text being written is shown when there is one. */
async function finishChoice(run, rec, open) {
  CHOOSE.busy = true;
  paintChooser();
  let id = null;
  try {
    id = await run();
  } catch (err) {
    CHOOSE.busy = false;
    paintChooser();
    toast(String(err));
    return;
  }
  REC.cutOff = REC.cutOff.filter((c) => c.journal !== rec.journal || !rec.journal);
  REC.armed = null;
  closeChooser();
  try { onState(await invoke("get_state")); } catch { /* the next state comes by itself */ }
  if (!id) loadHistory();
  if (open && id) openTranscript(id);
}

async function onRecordClick(ev) {
  const el = ev.target.closest("[data-rec]");
  if (!el || el.tagName === "SELECT") return;
  const what = el.dataset.rec;
  if (what === "start") return startRecording();
  // the recording on screen, by its id: never another one that is still finishing
  if (what === "stop") return invoke("stop_recording", { id: recordJob()?.id || REC.live?.id || null });
  if (what === "privacy") return invoke("open_microphone_privacy");
  const card = el.closest(".rec-cutoff");
  if (!card) return;
  const journal = card.dataset.journal;
  if (what === "choose") { const c = REC.cutOff.find((x) => x.journal === journal); if (c) openChooser(c); return; }
  if (what === "discard" && REC.armed !== journal) { REC.armed = journal; renderRecordBar(); return; }
  try {
    await invoke("recover_recording", { journal, keep: false });
    REC.cutOff = REC.cutOff.filter((c) => c.journal !== journal);
    REC.armed = null;
    renderRecordBar();
    loadHistory();
  } catch (err) {
    toast(String(err));
  }
}

/** The page of a recording kept without its text: what to do with it, from there. */
function savedRecordingBox(cur) {
  const d = document.createElement("div");
  d.className = "empty-live rec-saved";
  d.innerHTML = `<p>${esc(t("rec.savedNote"))}</p><button class="btn btn-primary" data-rec="saved">${esc(t("rec.recover"))}</button>`;
  d.querySelector("button").onclick = () => chooseForSaved(cur);
  return d;
}

function chooseForSaved(cur) {
  openChooser({ saved: cur.id, title: cur.title, seconds: cur.duration_s, sources: cur.sources });
}

/** A recording that stopped, its file whole: the question, at once. */
async function onRecorded(journal) {
  try { REC.cutOff = await invoke("interrupted_recordings"); } catch { /* none */ }
  if (S.view === "home") renderRecordBar();
  const c = REC.cutOff.find((x) => x.journal === journal);
  if (c) openChooser(c);
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
  listen("recorded", (e) => onRecorded(e.payload.journal));
  // going elsewhere (a message's button, the side panel) puts the question off
  const baseShow = showView;
  // eslint-disable-next-line no-global-assign
  showView = function (...args) { if (CHOOSE.rec) closeChooser(); return baseShow.apply(this, args); };
  $("t-transcribe").addEventListener("click", () => { if (S.current) chooseForSaved(S.current); });
  // Escape puts the question off, before anything else hears it (Settings would close)
  document.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape" && CHOOSE.rec) { ev.stopImmediatePropagation(); ev.preventDefault(); closeChooser(); }
  }, true);
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
      try { const v = await invoke("recording_state"); if (v?.id === REC.live.id) REC.live.sources = v.sources; } catch { /* ignore */ }
    }
    if (S.view === "home") renderRecordBar();
    const j = S.current && jobById(S.current.id);
    if (S.view === "transcript" && j?.kind === "record") recordProgress(j);
  });
}

/** Called by app.js with each new state of the app. */
function recordOnState(snap) {
  // when each recording stopped: a click right after it was meant for Stop
  for (const x of snap.queue) {
    if (x.kind !== "record") continue;
    if (x.status === "recording") CANCEL.recording.add(x.id);
    else if (CANCEL.recording.delete(x.id)) CANCEL.ended.set(x.id, Date.now());
  }
  if (S.view === "home") renderRecordBar();
  // a recording whose text was not written (the engine failed, or it was stopped): asked about again
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
