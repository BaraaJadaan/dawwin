// The small player of read anything on the screen (1.8.0): it reads the selection sentence by
// sentence in the voices of Read aloud (reading/mod.rs makes each sentence's sound), and says
// what happened when there is nothing to read. Its window never takes the keyboard: a click on a
// button does not move the focus away from the program the user is in.
"use strict";

const TIPS = {
  en: { back: "Back a sentence", pause: "Pause", resume: "Go on", forward: "Forward a sentence", close: "Stop and close" },
  ar: { back: "رجوع جملة", pause: "إيقاف مؤقت", resume: "متابعة", forward: "تقدّم جملة", close: "أوقف وأغلق" },
};

const core = window.__TAURI__.core;
const invoke = (cmd, args) => core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);
const P = { doc: null, i: 0, token: 0, playing: false, audio: new Audio() };
let hideTimer = null;

function speed() {
  // the speed chosen in Read aloud (the same page origin, the same storage)
  try { return Number(localStorage.getItem("readSpeed")) || 1; } catch { return 1; }
}

function paint() {
  const reading = !!P.doc;
  $("back").hidden = $("forward").hidden = !reading;
  $("pause").hidden = !reading || !P.playing;
  $("resume").hidden = !reading || P.playing;
  if (reading) {
    $("back").disabled = P.i === 0 && P.audio.currentTime < 1.5;
    $("forward").disabled = P.i + 1 >= P.doc.sentences.length;
  }
}

function hideLater(ms) {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => invoke("player_action", { action: "hide" }), ms);
}

function show(p) {
  const lang = p.dir === "rtl" ? "ar" : "en";
  document.documentElement.lang = lang;
  document.documentElement.dir = p.dir;
  for (const id of ["back", "pause", "resume", "forward", "close"]) $(id).title = TIPS[lang][id];
  clearTimeout(hideTimer);
  if (p.doc) {
    P.doc = p.doc;
    readFrom(0);
    return;
  }
  $("msg").textContent = p.message;
  paint();
  // a message alone goes away by itself
  if (!P.doc) hideLater(4000);
}

async function readFrom(i) {
  const d = P.doc;
  if (!d || i < 0 || i >= d.sentences.length) return;
  const my = ++P.token;
  P.i = i;
  P.playing = true;
  P.audio.pause();
  $("msg").textContent = d.sentences[i].text;
  paint();
  try {
    const ready = await invoke("reading_sentence", { doc: d.id, index: i });
    if (my !== P.token) return;
    P.audio.src = core.convertFileSrc(ready.path);
    P.audio.playbackRate = P.audio.defaultPlaybackRate = speed();
    P.audio.preservesPitch = true;
    await P.audio.play();
  } catch (err) {
    if (my !== P.token) return;
    // no voice yet, or the voice failed: said here, and the reading ends
    $("msg").textContent = String(err);
    finish(6000);
  }
  paint();
}

/** The reading is over (or failed): Dawwin forgets it, and the player goes. */
function finish(after) {
  P.token++;
  P.playing = false;
  P.audio.pause();
  P.doc = null;
  invoke("player_action", { action: "ended" });
  paint();
  hideLater(after);
}

/** Stopped from elsewhere (the shortcut pressed again, the notification area's menu). */
function stopped() {
  P.token++;
  P.playing = false;
  P.audio.pause();
  P.doc = null;
  paint();
  hideLater(1500);
}

function act(action) {
  if (!P.doc && action !== "close") return;
  if (action === "pause") {
    P.audio.pause();
    P.playing = false;
  } else if (action === "resume") {
    P.playing = true;
    if (P.audio.src && !P.audio.ended) P.audio.play().catch(() => readFrom(P.i));
    else readFrom(P.i);
  } else if (action === "back") {
    // back from the middle of a sentence starts it again
    readFrom(P.audio.currentTime > 1.5 ? P.i : Math.max(0, P.i - 1));
  } else if (action === "forward") {
    readFrom(Math.min(P.doc.sentences.length - 1, P.i + 1));
  } else if (action === "close") {
    P.token++;
    P.audio.pause();
    P.doc = null;
    P.playing = false;
    invoke("player_action", { action: "close" });
  }
  paint();
}

P.audio.addEventListener("ended", () => {
  if (!P.playing || !P.doc) return;
  if (P.i + 1 < P.doc.sentences.length) readFrom(P.i + 1);
  else finish(1500);
});

document.querySelectorAll("button[data-action]").forEach((b) => b.addEventListener("click", () => act(b.dataset.action)));

window.__TAURI__.event.listen("player-note", (e) => show(e.payload));
window.__TAURI__.event.listen("player-control", (e) => (e.payload === "stop" ? stopped() : act(e.payload)));
// opened by what it is to show: ask for it
invoke("player_last").then((p) => p && show(p));
// what the tests read
window.__player = P;
