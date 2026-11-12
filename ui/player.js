// The small player of read anything on the screen (1.8.0). Its window never takes the keyboard:
// a click on a button does not move the focus away from the program the user is in.
"use strict";

const TIPS = {
  en: { back: "Back a sentence", pause: "Pause", resume: "Go on", forward: "Forward a sentence", close: "Stop and close" },
  ar: { back: "رجوع جملة", pause: "إيقاف مؤقت", resume: "متابعة", forward: "تقدّم جملة", close: "أوقف وأغلق" },
};

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);
let hideTimer = null;

function show(p) {
  const lang = p.dir === "rtl" ? "ar" : "en";
  document.documentElement.lang = lang;
  document.documentElement.dir = p.dir;
  $("msg").textContent = p.message;
  for (const id of ["back", "pause", "resume", "forward", "close"]) $(id).title = TIPS[lang][id];
  // only a reading has something to pause or skip
  for (const id of ["back", "pause", "forward"]) $(id).hidden = !p.reading;
  $("resume").hidden = true;
  clearTimeout(hideTimer);
  // a message alone goes away by itself
  if (!p.reading) hideTimer = setTimeout(() => invoke("player_action", { action: "hide" }), 4000);
}

document.querySelectorAll("button[data-action]").forEach((b) =>
  b.addEventListener("click", () => {
    const action = b.dataset.action;
    if (action === "pause" || action === "resume") {
      $("pause").hidden = action === "pause";
      $("resume").hidden = action !== "pause";
    }
    invoke("player_action", { action });
  }),
);

window.__TAURI__.event.listen("player-note", (e) => show(e.payload));
// opened by the note it is to show: ask for it
invoke("player_last").then((p) => p && show(p));
