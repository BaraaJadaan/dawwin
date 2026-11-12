// The common jobs, named on the start page above the steps: each ticks the steps it takes, which
// can still be changed one by one. The one the ticked steps make is shown pressed.
"use strict";

/** Each job: its name's key, and the steps it turns on (every other one goes off). */
const NAMED_JOBS = [
  { id: "transcribe", steps: { transcribe: true } },
  { id: "translate", steps: { transcribe: true, translate: true } },
  { id: "sound", steps: { save_audio: true } },
  { id: "silences", steps: { remove_silence: true, save_audio: true } },
  { id: "subtitles", steps: { transcribe: true, video: true } },
];
const JOB_STEPS = ["remove_silence", "save_audio", "transcribe", "translate", "video"];

/** The named job the ticked steps are, if they are one. */
function currentJob() {
  const s = stepsForJob();
  const on = { remove_silence: s.remove_silence, save_audio: s.save_audio, transcribe: s.transcribe, translate: !!s.translate, video: s.video };
  return NAMED_JOBS.find((j) => JOB_STEPS.every((k) => !!j.steps[k] === on[k]))?.id || "";
}

function pickJob(id) {
  const job = NAMED_JOBS.find((j) => j.id === id);
  if (!job) return;
  for (const k of JOB_STEPS) S.steps[k] = !!job.steps[k];
  store.set("steps", JSON.stringify(S.steps));
  paintSteps();
}

function paintJobs() {
  const box = $("jobs");
  if (!box) return;
  if (box.dataset.lang !== UI_LANG) {
    box.dataset.lang = UI_LANG;
    box.innerHTML = NAMED_JOBS.map((j) => `<button type="button" class="btn btn-sm job-choice" data-job="${j.id}" title="${esc(t(`job.${j.id}.tip`))}" aria-description="${esc(t(`job.${j.id}.tip`))}">${esc(t(`job.${j.id}`))}</button>`).join("");
    for (const b of box.querySelectorAll("button")) b.onclick = () => pickJob(b.dataset.job);
  }
  const now = currentJob();
  for (const b of box.querySelectorAll("button")) {
    const on = b.dataset.job === now;
    b.classList.toggle("on", on);
    b.setAttribute("aria-pressed", on ? "true" : "false");
  }
}
