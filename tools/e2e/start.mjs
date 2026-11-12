// The start page and where files go, in the real app: the common jobs by name tick their steps
// (and show which one the ticked steps are); "Video to sound file" saves a video's sound into the
// folder of one's own chosen in Settings, while a folder run still writes next to each recording;
// a video's transcript offers "The video's sound". Pictures of the start page and Settings, in
// English and Arabic, for a person to look at.
import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { check, ffmpeg, finish, makeHome, MODEL, need, open, OUT, PORT, sleep, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_MODEL", MODEL);
need("DAWWIN_E2E_TOOLS", TOOLS);

const work = join(OUT, "start-files");
const mine = join(OUT, "start-my-folder");
for (const d of [work, mine]) { rmSync(d, { recursive: true, force: true }); mkdirSync(d, { recursive: true }); }
const video = join(work, "lesson.mp4");
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=320x240:rate=15:duration=6", "-f", "lavfi", "-i", "sine=frequency=300:duration=6", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", video]);
const inFolder = join(work, "folder run");
mkdirSync(inFolder);
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=320x240:rate=15:duration=4", "-f", "lavfi", "-i", "sine=frequency=300:duration=4", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", join(inFolder, "clip.mp4")]);

const home = makeHome("start-home", { save_dir: mine });
const a = await open(home, PORT + 10);
await a.ev(`showView('home')`);
const pressed = () => a.ev(`[...document.querySelectorAll('#jobs button[aria-pressed="true"]')].map(b=>b.dataset.job).join()`);
const ticked = () => a.ev(`['step-silence','step-save-audio','step-transcribe','step-translate','step-video'].filter(id=>document.getElementById(id).checked).join()`);

check("five common jobs, named, above the steps", (await a.ev(`[...document.querySelectorAll('#jobs button')].map(b=>b.textContent).join(' | ')`)) === "Transcribe | Transcribe and translate | Video to sound file | Sound without silences | Video with subtitles");
check("as it comes: Transcribe", (await pressed()) === "transcribe" && (await ticked()) === "step-transcribe");
const want = { sound: "step-save-audio", silences: "step-silence,step-save-audio", subtitles: "step-transcribe,step-video", translate: "step-transcribe,step-translate", transcribe: "step-transcribe" };
for (const [job, steps] of Object.entries(want)) {
  await a.ev(`document.querySelector('#jobs [data-job="${job}"]').click()`);
  check(`"${job}" ticks its steps, and shows pressed`, (await ticked()) === steps && (await pressed()) === job, { ticked: await ticked(), pressed: await pressed() });
}
await a.ev(`document.getElementById('step-video').click()`); // transcribe + video, by hand
check("ticking the steps by hand shows the job they make", (await pressed()) === "subtitles");
await a.ev(`document.getElementById('step-translate').click()`);
check("and none when they make no named job", (await pressed()) === "");
check("each says on hover what it does, also to a screen reader", await a.ev(`[...document.querySelectorAll('#jobs button')].every(b=>b.title.length>10 && b.getAttribute('aria-description')===b.title)`));

// ---- Video to sound file, into the folder of one's own
await a.ev(`document.querySelector('#jobs [data-job="sound"]').click()`);
await a.ev(`addFiles([${JSON.stringify(slash(video))}])`);
await a.idle(120);
check("\"Video to sound file\" saves the sound in the folder chosen in Settings", existsSync(join(mine, "lesson.m4a")) && !existsSync(join(work, "lesson.m4a")), { mine: readdirSync(mine), next: readdirSync(work) });
await a.invoke("enqueue_folder", { path: slash(inFolder), language: "ar", steps: { remove_silence: false, save_audio: true, transcribe: false, translate: null, video: false, burn: false } });
await a.idle(120);
check("a folder run still writes next to each recording", existsSync(join(inFolder, "clip.m4a")) && !existsSync(join(mine, "clip.m4a")), readdirSync(inFolder));
await a.ev(`showView('settings')`);
await sleep(300);
check("Settings shows where files go", await a.ev(`document.getElementById('set-save-where').value === 'own' && document.getElementById('set-save-dir').textContent.includes('start-my-folder')`));

// ---- a video's transcript offers its sound by that name
await a.ev(`document.querySelector('#jobs [data-job="transcribe"]').click()`);
await a.invoke("enqueue", { paths: [slash(video)], language: "ar", steps: { remove_silence: false, save_audio: false, transcribe: true, translate: null, video: false, burn: false } });
await a.idle(180);
const tx = (await a.invoke("list_history")).find((h) => h.source_path.endsWith("lesson.mp4"));
if (tx) {
  await a.ev(`openTranscript(${JSON.stringify(tx.id)})`);
  await sleep(400);
  await a.ev(`document.getElementById('btn-export').click()`);
  check("its export menu says \"The video's sound (.m4a)\"", (await a.ev(`document.getElementById('export-audio').textContent`)) === "The video's sound (.m4a)");
  await a.ev(`document.getElementById('export-menu').hidden = true; 1`);
} else check("the video is transcribed", false);

// ---- descriptions on hover, the system's own, and to a screen reader too
const tips = () => a.ev(`({
  step: document.getElementById('step-silence').closest('label').title,
  stepAria: document.getElementById('step-silence').getAttribute('aria-description'),
  status: document.getElementById('chip-status').title,
  docx: document.querySelector('#export-menu [data-fmt="docx"]').title,
  arabic: document.querySelector('#spoken option[value="ar"]').title,
  side: [document.getElementById('btn-side').title, document.getElementById('btn-side').getAttribute('aria-label')],
  play: [document.getElementById('btn-play').title, document.getElementById('btn-play').getAttribute('aria-label')],
  job: document.querySelector('#jobs [data-job="sound"]').title,
  copy: document.getElementById('btn-copy').title,
})`);
await a.ev(`setUiLang('en')`);
let tp = await tips();
check("a step, the status, an export format and \"Arabic\" say on hover what they are", /0\.7 seconds/.test(tp.step) && tp.stepAria === tp.step && /speech model/.test(tp.status) && /Word document/.test(tp.docx) && /dialects/.test(tp.arabic) && /English words/.test(tp.arabic), tp);
check("buttons that are only a picture show their name", tp.side[0] === tp.side[1] && tp.side[0] && tp.play[0] === tp.play[1] && tp.play[0], { side: tp.side, play: tp.play });
check("a button whose name says it all gets nothing", tp.copy === "", tp.copy);
await a.ev(`setUiLang('ar')`);
tp = await tips();
check("in Arabic too", /ثانية/.test(tp.step) && /اللهجات/.test(tp.arabic) && /ملفًا صوتيًا/.test(tp.job) && tp.side[0] === tp.side[1] && /[؀-ۿ]/.test(tp.side[0]), tp);

// ---- the working bar moves in Arabic too; nothing moves with animations off in Windows
const bar = () => a.ev(`(async()=>{
  const p=document.createElement('div'); p.className='progress indeterminate'; p.style.width='300px'; p.innerHTML='<div class="bar"></div>';
  document.getElementById('main').append(p); const b=p.firstChild; const at=[];
  for (let k=0;k<4;k++){ at.push(b.getBoundingClientRect().left); await new Promise(r=>setTimeout(r,150)); }
  const name=getComputedStyle(b).animationName; p.remove(); return {name, moved: Math.max(...at)-Math.min(...at)};
})()`);
const rtl = await bar();
check("the working bar moves in the Arabic interface", rtl.name === "slide-rtl" && rtl.moved > 20, rtl);
await a.call("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
const still = await bar();
check("with animations off in Windows, it stands still", still.moved < 1, still);
await a.call("Emulation.setEmulatedMedia", { features: [] });

// ---- pictures, both languages
for (const lang of ["en", "ar"]) {
  await a.ev(`setUiLang('${lang}')`);
  await a.ev(`showView('home')`);
  await a.ev(`document.querySelector('#jobs [data-job="subtitles"]').click()`);
  await sleep(400);
  await a.shot(join(OUT, `start-${lang}.png`));
  await a.ev(`showView('settings')`);
  await a.ev(`document.getElementById('set-saving').scrollIntoView({block:'center'})`);
  await sleep(400);
  await a.shot(join(OUT, `settings-saving-${lang}.png`));
}
a.close();
finish();
