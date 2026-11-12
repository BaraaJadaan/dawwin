// What came with Dawwin 1.4: who said what, the language heard, words and names, how burned-in
// subtitles look. Loaded before app.js, whose functions and state it uses once the page runs; the
// shared files only call into it at a few places (whoWire, whoDecorateRow, whoInSearch, …).
"use strict";

Object.assign(I18N.en.s, {
  "steps.speakers": "Tell speakers apart",
  "tip.speakers": "Writes who speaks where the speaker changes, for up to four people. Rename them in the transcript.",
  "lang.auto": "Detect the language",
  "tip.spoken": "Detect the language: Dawwin listens to the first half minute of speech of each recording.",
  "job.detecting": "Finding the language…",
  "job.speakers": "Telling the speakers apart…",
  "job.findSpeakers": "Finding the speakers",
  "drop.speakers": "tell the speakers apart",
  "toast.needSpeakersModel": "Telling speakers apart needs its model: {size}, once.",
  "toast.needLanguageModel": "Detecting the language needs its model: {size}, once.",
  "toast.modelDownloading": "Downloading it. You can carry on meanwhile.",
  "toast.noModelFor": "This recording is in {lang}. Dawwin has no model for it yet: {size}.",
  "toast.noModelAtAll": "This recording is in {lang}, which Dawwin does not transcribe.",
  "model.speakers.name": "Who said what",
  "model.speakers.long": "Who said what",
  "model.speakers.detail": "Tells up to four speakers apart, on the processor, in a small part of the recording's length. NVIDIA Open Model License.",
  "model.language.name": "Detect the language",
  "model.language.long": "Detect the language",
  "model.language.detail": "Hears which of 107 languages a recording is in, from its first half minute of speech.",
  "who.speaker": "Speaker {n}",
  "who.rename": "Rename…",
  "who.merge": "Same person as {name}",
  "who.search": "Search their lines",
  "who.everyone": "Everyone",
  "who.searchWho": "Search the lines of",
  "who.saidBy": "Said by {name}",
  "who.someoneNew": "Said by someone new",
  "who.find": "Find the speakers",
  "who.findAgain": "Find the speakers again",
  "who.finding": "Finding the speakers. The names you gave are kept only where the speakers stay the same.",
  "who.found": { one: "Found one speaker.", other: "Found {n} speakers." },
  "who.foundMax": "Found four speakers. Dawwin tells up to four apart: if more people speak, some of them share a name.",
  "who.name": "Name",
  "t.more": "More ▾",
  "t.heardAs": "heard as {lang}",
  "t.again": "Transcribe again in…",
  "t.againStarted": "Transcribing again in {lang}.",
  "words.apply": "Apply my words",
  "words.applied": { zero: "None of your words is in it.", one: "One line changed.", other: "{n} lines changed." },
  "words.always": "Always write it like this…",
  "words.as": "Write “{heard}” as",
  "set.words": "Words and names",
  "tip.words": "Every new transcript writes these words this way; an “All languages” model also hears them better. Applied as whole words, whatever the hamza or the case.",
  "words.write": "How to write it",
  "words.heard": "What it comes out as, separated by commas",
  "words.add": "Add a word",
  "words.save": "Save to a file…",
  "words.load": "Load from a file…",
  "words.remove": "Remove",
  "words.saved": "Saved {name}.",
  "set.subtitles": "Subtitles",
  "set.namesSrt": "The speaker's name where the speaker changes, in SRT and burned in",
  "set.namesVtt": "The speaker in WebVTT's voice tag",
  "set.look": "Subtitle look",
  "tip.look": "How subtitles burned into a video look.",
  "look.size": "Size",
  "look.small": "Small",
  "look.medium": "Medium",
  "look.large": "Large",
  "look.very-large": "Very large",
  "look.font": "Font",
  "tip.font": "The fonts of this PC that draw Arabic correctly.",
  "look.color": "Colour",
  "look.white": "White",
  "look.yellow": "Yellow",
  "look.back": "Behind the text",
  "look.outline": "Outline",
  "look.box": "Dark box",
  "look.place": "Place",
  "look.bottom": "Bottom",
  "look.top": "Top",
  "look.sharp": "Sharp text on small videos",
  "tip.sharp": "Enlarges a video under 1080p before drawing the text, so the text stays sharp. The video file gets about three times larger.",
  "look.preview": "Show it on my video",
  "look.making": "Burning one second of your video…",
  "set.toolUpdate": "Update",
  "set.toolOutdated": "A newer build is available",
});

Object.assign(I18N.ar.s, {
  "steps.speakers": "تمييز المتحدثين",
  "tip.speakers": "يكتب مَن يتكلم حيث يتغير المتحدث، حتى أربعة أشخاص. غيّر أسماءهم في النص.",
  "lang.auto": "التعرّف على اللغة",
  "tip.spoken": "التعرّف على اللغة: يستمع دَوِّن إلى أول نصف دقيقة من الكلام في كل تسجيل.",
  "job.detecting": "جارٍ التعرّف على اللغة…",
  "job.speakers": "جارٍ تمييز المتحدثين…",
  "job.findSpeakers": "البحث عن المتحدثين",
  "drop.speakers": "تمييز المتحدثين",
  "toast.needSpeakersModel": "يحتاج تمييز المتحدثين إلى نموذجه: {size}، مرة واحدة.",
  "toast.needLanguageModel": "يحتاج التعرّف على اللغة إلى نموذجه: {size}، مرة واحدة.",
  "toast.modelDownloading": "جارٍ تنزيله. يمكنك المتابعة في أثناء ذلك.",
  "toast.noModelFor": "هذا التسجيل باللغة {lang}، ولا نموذج لها في دَوِّن بعد: {size}.",
  "toast.noModelAtAll": "هذا التسجيل باللغة {lang}، وهي لغة لا يفرّغها دَوِّن.",
  "model.speakers.name": "من قال ماذا",
  "model.speakers.long": "من قال ماذا",
  "model.speakers.detail": "يميّز حتى أربعة متحدثين، على المعالج، في جزء صغير من مدة التسجيل. رخصة NVIDIA Open Model.",
  "model.language.name": "التعرّف على اللغة",
  "model.language.long": "التعرّف على اللغة",
  "model.language.detail": "يعرف بأي لغة من 107 لغات يتكلم التسجيل، من أول نصف دقيقة من كلامه.",
  "who.speaker": "المتحدث {n}",
  "who.rename": "إعادة التسمية…",
  "who.merge": "الشخص نفسه مثل {name}",
  "who.search": "البحث في كلامه",
  "who.everyone": "الجميع",
  "who.searchWho": "البحث في كلام",
  "who.saidBy": "قالها {name}",
  "who.someoneNew": "قالها شخص آخر",
  "who.find": "البحث عن المتحدثين",
  "who.findAgain": "البحث عن المتحدثين من جديد",
  "who.finding": "جارٍ البحث عن المتحدثين. لا تبقى الأسماء التي أعطيتها إلا حيث يبقى المتحدثون كما هم.",
  "who.found": { zero: "لم يُعثر على متحدثين.", one: "عُثر على متحدث واحد.", two: "عُثر على متحدثَين.", few: "عُثر على {n} متحدثين.", many: "عُثر على {n} متحدثًا.", other: "عُثر على {n} متحدث." },
  "who.foundMax": "عُثر على أربعة متحدثين. يميّز دَوِّن حتى أربعة: إن تكلم أكثر منهم فبعضهم يشترك في اسم.",
  "who.name": "الاسم",
  "t.more": "المزيد ▾",
  "t.heardAs": "سُمعت بـ{lang}",
  "t.again": "إعادة التفريغ بلغة…",
  "t.againStarted": "جارٍ التفريغ من جديد بـ{lang}.",
  "words.apply": "تطبيق كلماتي",
  "words.applied": { zero: "لا توجد فيه أي من كلماتك.", one: "تغيّر سطر واحد.", two: "تغيّر سطران.", few: "تغيّرت {n} أسطر.", many: "تغيّر {n} سطرًا.", other: "تغيّر {n} سطر." },
  "words.always": "اكتبها دائمًا هكذا…",
  "words.as": "اكتب «{heard}» هكذا",
  "set.words": "الكلمات والأسماء",
  "tip.words": "يكتب كل نص جديد هذه الكلمات هكذا، ويسمعها نموذج «كل اللغات» أفضل أيضًا. تُطبَّق على الكلمات كاملة، أيًا كانت الهمزة أو حالة الأحرف.",
  "words.write": "كيف تُكتب",
  "words.heard": "كيف تخرج خطأً، تفصل بينها فواصل",
  "words.add": "إضافة كلمة",
  "words.save": "حفظ في ملف…",
  "words.load": "تحميل من ملف…",
  "words.remove": "إزالة",
  "words.saved": "حُفظ {name}.",
  "set.subtitles": "النص على الفيديو",
  "set.namesSrt": "اسم المتحدث حيث يتغير، في ملفات SRT والنص المدمج",
  "set.namesVtt": "المتحدث في وسم الصوت في WebVTT",
  "set.look": "شكل النص على الفيديو",
  "tip.look": "كيف يبدو النص المدمج في صورة الفيديو.",
  "look.size": "الحجم",
  "look.small": "صغير",
  "look.medium": "متوسط",
  "look.large": "كبير",
  "look.very-large": "كبير جدًا",
  "look.font": "الخط",
  "tip.font": "خطوط هذا الحاسوب التي ترسم العربية رسمًا صحيحًا.",
  "look.color": "اللون",
  "look.white": "أبيض",
  "look.yellow": "أصفر",
  "look.back": "خلف النص",
  "look.outline": "حدّ",
  "look.box": "مربع داكن",
  "look.place": "المكان",
  "look.bottom": "في الأسفل",
  "look.top": "في الأعلى",
  "look.sharp": "نص حاد على الفيديوهات الصغيرة",
  "tip.sharp": "يكبّر الفيديو الأقل من 1080p قبل رسم النص، فيبقى النص حادًّا. يصبح ملف الفيديو أكبر بنحو ثلاث مرات.",
  "look.preview": "عرضه على الفيديو الخاص بي",
  "look.making": "جارٍ دمج ثانية من الفيديو…",
  "set.toolUpdate": "تحديث",
  "set.toolOutdated": "يتوفّر إصدار أحدث",
});

const WHO = {
  filter: 0,      // the speaker the search is kept to (0: everyone)
  extras: null,   // the word list, names in subtitles, the look (get_extras)
  fonts: null,    // the fonts the look may use
  pop: null,      // the little menu open now
};
const SPEECH_FAMILIES = ["cohere", "canary", "whisper"];

// ------------------------------------------------------------------ helpers

/** What a speaker of the transcript on screen is called. */
function speakerName(tx, id) {
  const s = (tx.speakers || []).find((x) => x.id === id);
  return s && s.name.trim() ? s.name.trim() : t("who.speaker", { n: id });
}

/** The speakers who say something in the transcript, in their order. */
function speakersOf(tx) {
  const ids = [...new Set(tx.segments.filter((s) => s.speaker && s.text.trim()).map((s) => s.speaker))].sort((a, b) => a - b);
  return ids.map((id) => ({ id, name: speakerName(tx, id) }));
}

/** The model of a family that is on the disk, or offered with its size; false after offering it. */
function haveModel(id, message) {
  const st = S.snap.setup;
  const m = st.models.find((x) => x.id === id);
  if (st.families[m?.family]?.found) return true;
  toast(t(message, { size: sizeText(m?.size || 0) }), {
    label: t("dl.download"),
    run: async () => toast(t((await invoke("download_model", { id })) ? "toast.modelDownloading" : "toast.downloadBusy")),
  });
  return false;
}

/** The speech model for "Detect the language": any of the speech families on the disk. */
function anySpeechModel(st) {
  for (const f of SPEECH_FAMILIES) if (st.families[f]?.found) return st.families[f];
  return null;
}

// ------------------------------------------------------------------ the little menu

function closePop() {
  WHO.pop?.remove();
  WHO.pop = null;
}

/** A menu by `at` (an element, or {x, y}): items {label, run} or {input: placeholder, value, run(text)}. */
function popMenu(at, items) {
  closePop();
  const m = document.createElement("div");
  m.className = "menu ctx-menu who-pop";
  m.setAttribute("role", "menu");
  for (const it of items) {
    if (it.input !== undefined) {
      const box = document.createElement("form");
      box.className = "who-input";
      if (it.title) {
        const l = document.createElement("label");
        l.textContent = it.title;
        l.dir = "auto";
        box.append(l);
      }
      const inp = document.createElement("input");
      inp.className = "input input-sm";
      inp.dir = "auto";
      inp.placeholder = it.input;
      inp.value = it.value || "";
      box.append(inp);
      box.onsubmit = (ev) => { ev.preventDefault(); closePop(); it.run(inp.value.trim()); };
      inp.addEventListener("keydown", (ev) => { if (ev.key === "Escape") closePop(); });
      m.append(box);
      setTimeout(() => { inp.focus(); inp.select(); }, 0);
    } else {
      const b = document.createElement("button");
      b.setAttribute("role", "menuitem");
      b.textContent = it.label;
      b.dir = "auto";
      b.onclick = () => { closePop(); it.run(); };
      m.append(b);
    }
  }
  document.body.append(m);
  const r = at instanceof Element ? at.getBoundingClientRect() : { left: at.x, bottom: at.y, right: at.x };
  const w = m.offsetWidth, h = m.offsetHeight;
  const x = I18N[UI_LANG].dir === "rtl" && at instanceof Element ? r.right - w : r.left;
  m.style.left = `${Math.max(8, Math.min(x, innerWidth - w - 8))}px`;
  m.style.top = `${Math.max(8, Math.min(r.bottom + 4, innerHeight - h - 8))}px`;
  WHO.pop = m;
}

// ------------------------------------------------------------------ the transcript

/** The name over a line where a turn starts: where the speaker changes, not on every line. */
function whoDecorateRow(row, s, i) {
  const segs = S.current?.segments || [];
  if (!s.speaker || !s.text.trim()) return;
  let last = null;
  for (let k = i - 1; k >= 0; k--) {
    if (segs[k].text.trim() && segs[k].speaker) { last = segs[k].speaker; break; }
  }
  if (last === s.speaker) return;
  const b = document.createElement("button");
  b.className = "who";
  b.dir = "auto";
  b.textContent = speakerName(S.current, s.speaker);
  b.dataset.speaker = s.speaker;
  b.onclick = (ev) => { ev.stopPropagation(); speakerMenu(s.speaker, b); };
  row.prepend(b);
}

/** Rename a speaker, merge them into another, or search their lines. */
function speakerMenu(id, el) {
  const cur = S.current;
  if (isLive(cur.id)) return;
  const items = [{ label: t("who.rename"), run: () => renameSpeaker(id, el) }];
  for (const o of speakersOf(cur).filter((x) => x.id !== id)) {
    items.push({ label: t("who.merge", { name: o.name }), run: () => editSpeakers("merge_speakers", { from: id, into: o.id }) });
  }
  items.push({ label: t("who.search"), run: () => { WHO.filter = id; paintWhoFilter(); runSearch(false); $("search").focus(); } });
  popMenu(el, items);
}

function renameSpeaker(id, el) {
  const s = (S.current.speakers || []).find((x) => x.id === id);
  popMenu(el, [{ input: t("who.name"), value: s?.name || "", run: (name) => editSpeakers("rename_speaker", { speaker: id, name }) }]);
}

/** A change to the speakers: the transcript comes back as it is now, and is drawn again. */
async function editSpeakers(cmd, args) {
  try {
    S.current = await invoke(cmd, { id: S.current.id, ...args });
    renderTranscript();
  } catch (err) {
    toast(String(err));
  }
}

/** Whether line `s` is one the search looks in: everyone's, or the chosen speaker's. */
function whoInSearch(s) {
  return !WHO.filter || s.speaker === WHO.filter;
}

/** The "search the lines of" menu: shown when the transcript has speakers. */
function paintWhoFilter() {
  const sel = $("search-who"), list = S.current ? speakersOf(S.current) : [];
  if (!list.some((x) => x.id === WHO.filter)) WHO.filter = 0;
  sel.hidden = list.length < 2;
  sel.innerHTML = `<option value="0">${esc(t("who.everyone"))}</option>` + list.map((x) => `<option value="${x.id}">${esc(x.name)}</option>`).join("");
  sel.value = String(WHO.filter);
}

/** Right-click in the lines: "Always write it like this…" for a selection, and who said the line. */
function whoContext(ev, selected) {
  const cur = S.current, box = $("ctx-who");
  box.innerHTML = "";
  const live = !cur || isLive(cur.id);
  $("ctx-always").hidden = !selected || live || !document.getSelection().toString().trim();
  const row = ev.target.closest(".seg");
  if (!live && row && cur.speakers?.length) {
    const i = Number(row.dataset.i), own = cur.segments[i]?.speaker;
    for (const o of speakersOf(cur).filter((x) => x.id !== own)) {
      const b = document.createElement("button");
      b.setAttribute("role", "menuitem");
      b.dir = "auto";
      b.textContent = t("who.saidBy", { name: o.name });
      b.onclick = () => { hideCtx(); editSpeakers("give_line", { index: i, speaker: o.id }); };
      box.append(b);
    }
    const b = document.createElement("button");
    b.setAttribute("role", "menuitem");
    b.textContent = t("who.someoneNew");
    b.onclick = () => { hideCtx(); editSpeakers("give_line", { index: i, speaker: null }); };
    box.append(b);
  }
  return !$("ctx-always").hidden || box.children.length > 0;
}

/** "Always write it like this…": the words selected, and how to write them from now on. */
function alwaysWrite() {
  const heard = document.getSelection().toString().replace(/\s+/g, " ").trim();
  const r = $("ctx-menu").getBoundingClientRect();
  hideCtx();
  if (!heard) return;
  popMenu({ x: r.left, y: r.top }, [{
    title: t("words.as", { heard }),
    input: t("words.write"),
    value: heard,
    run: async (write) => {
      if (!write || write === heard) return;
      try {
        const before = S.current.segments.map((s) => s.text);
        S.current = await invoke("always_write", { id: S.current.id, heard, write });
        WHO.extras = null;
        renderTranscript();
        toast(t("words.applied", { n: S.current.segments.filter((s, k) => s.text !== before[k]).length }));
      } catch (err) { toast(String(err)); }
    },
  }]);
}

async function applyWords() {
  try {
    const before = S.current.segments.map((s) => s.text);
    S.current = await invoke("apply_words", { id: S.current.id });
    renderTranscript();
    toast(t("words.applied", { n: S.current.segments.filter((s, k) => s.text !== before[k]).length }));
  } catch (err) { toast(String(err)); }
}

async function findSpeakers() {
  if (!haveModel("speakers", "toast.needSpeakersModel")) return;
  try {
    await invoke("find_speakers", { id: S.current.id });
    onState(await invoke("get_state"));
    toast(t("who.finding"));
  } catch (err) { toast(String(err)); }
}

/** The languages a recording can be transcribed in now, Arabic and English first. */
function languagesWithModel() {
  const st = S.snap.setup;
  const all = [...S.languages.keys()].filter((c) => spokenModel(st, c));
  const first = ["ar", "en"].filter((c) => all.includes(c));
  return [...first, ...all.filter((c) => !first.includes(c)).sort((a, b) => langName(a).localeCompare(langName(b), I18N[UI_LANG].locale))];
}

/** "Transcribe again in…": the same recording, in the language chosen. */
function transcribeAgain(el) {
  const cur = S.current;
  popMenu(el, languagesWithModel().filter((c) => c !== cur.language).slice(0, 30).map((c) => ({
    label: langName(c),
    run: async () => {
      const steps = { ...DEFAULT_STEPS, translate: null, speakers: (cur.speakers || []).length > 0 };
      const ids = await invoke("enqueue", { paths: [cur.source_path], language: c, steps });
      if (!ids.length) { toast(t("toast.noRecording")); return; }
      toast(t("t.againStarted", { lang: langName(c) }));
      onState(await invoke("get_state"));
      openTranscript(ids[0]);
    },
  })));
}

/** The "More" menu of the transcript: speakers, words, the language. */
function moreMenu(el) {
  const cur = S.current;
  if (!cur || isLive(cur.id)) return;
  const items = [
    { label: t(cur.segments.some((s) => s.speaker) ? "who.findAgain" : "who.find"), run: findSpeakers },
    { label: t("words.apply"), run: applyWords },
  ];
  if (cur.source_path) items.push({ label: t("t.again"), run: () => transcribeAgain(el) });
  popMenu(el, items);
}

/** The job finding the speakers of `id`, while it waits or runs. */
function whoJob(id) {
  return S.snap?.queue.find((j) => j.kind === "speakers" && j.transcript === id && ACTIVE.has(j.status)) || null;
}

/** What the state says that is ours: a speakers job of the transcript on screen ended. */
function whoOnState(prev, snap) {
  if (S.view !== "transcript" || !S.current) return;
  const ended = (prev?.queue || []).filter((x) => x.kind === "speakers" && x.transcript === S.current.id && ACTIVE.has(x.status))
    .map((x) => snap.queue.find((y) => y.id === x.id)).filter((y) => y && !ACTIVE.has(y.status));
  for (const y of ended) {
    if (y.status === "failed") { toast(String(y.error || "").split("\n")[0]); continue; }
    if (y.status !== "done") continue;
    invoke("get_transcript", { id: S.current.id }).then((tx) => {
      if (S.current?.id !== tx.id) return;
      S.current = tx;
      renderTranscript();
      const n = speakersOf(tx).length;
      toast(n >= (WHO.extras?.max_speakers || 4) ? t("who.foundMax") : t("who.found", { n }));
    }).catch(() => {});
  }
  // a transcription that told the speakers apart, finished on screen: four is as many as it tells
  const done = (prev?.queue || []).find((x) => x.id === S.current.id && ACTIVE.has(x.status) && x.steps?.speakers);
  const now = done && snap.queue.find((y) => y.id === done.id);
  if (now && now.status === "done") {
    invoke("get_transcript", { id: now.id }).then((tx) => {
      if (speakersOf(tx).length >= 4) toast(t("who.foundMax"));
    }).catch(() => {});
  }
}

/** A recording in a language Dawwin has no model for: said, with the model a press away. */
function whoNeedsModel({ language, name, model }) {
  const lang = S.languages.has(language) ? langName(language) : name;
  const m = S.snap.setup.models.find((x) => x.id === model);
  if (!m) { toast(t("toast.noModelAtAll", { lang })); return; }
  toast(t("toast.noModelFor", { lang, size: sizeText(m.size) }), {
    label: t("dl.download"),
    run: async () => toast(t((await invoke("download_model", { id: m.id })) ? "toast.modelDownloading" : "toast.downloadBusy")),
  });
}

// ------------------------------------------------------------------ settings

async function whoSettings() {
  if (!WHO.extras) {
    try { WHO.extras = await invoke("get_extras"); } catch { return; }
  }
  const x = WHO.extras;
  $("set-names-srt").checked = x.names_in_srt;
  $("set-names-vtt").checked = x.names_in_vtt;
  paintWords();
  paintLook();
  if (!WHO.fonts) {
    WHO.fonts = [];
    invoke("subtitle_fonts").then((f) => { WHO.fonts = f; paintLook(); }).catch(() => {});
  }
}

function paintWords() {
  const box = $("set-words-list"), list = WHO.extras.words;
  if (box.dataset.lang === UI_LANG && box.children.length === list.length && document.activeElement?.closest("#set-words-list")) return;
  box.dataset.lang = UI_LANG;
  box.innerHTML = list.map((w, k) => `
    <div class="word-row" data-k="${k}">
      <input class="input input-sm write" dir="auto" value="${esc(w.write)}" placeholder="${esc(t("words.write"))}" aria-label="${esc(t("words.write"))}">
      <input class="input input-sm heard" dir="auto" value="${esc(w.heard.join(UI_LANG === "ar" ? "، " : ", "))}" placeholder="${esc(t("words.heard"))}" aria-label="${esc(t("words.heard"))}">
      <button class="icon-btn" aria-label="${esc(t("words.remove"))}" title="${esc(t("words.remove"))}">×</button>
    </div>`).join("");
  for (const row of box.children) {
    const k = Number(row.dataset.k);
    for (const inp of row.querySelectorAll("input")) inp.onchange = () => saveWords();
    row.querySelector("button").onclick = () => { WHO.extras.words.splice(k, 1); saveWords(true); };
  }
}

/** The list as the page shows it, saved. */
async function saveWords(redraw) {
  const rows = [...$("set-words-list").children].map((row) => ({
    write: row.querySelector(".write").value.trim(),
    heard: row.querySelector(".heard").value.split(/[,،]/).map((h) => h.trim()).filter(Boolean),
  }));
  const words = redraw ? WHO.extras.words : rows;
  try {
    WHO.extras = await invoke("set_words", { words: words.filter((w) => w.write) });
    if (redraw) paintWords();
  } catch (err) { toast(String(err)); }
}

function paintLook() {
  const look = WHO.extras.look;
  const opts = (id, values) => {
    const sel = $(id);
    if (sel.dataset.lang !== UI_LANG) {
      sel.dataset.lang = UI_LANG;
      sel.innerHTML = values.map((v) => `<option value="${v}">${esc(t("look." + v))}</option>`).join("");
    }
  };
  opts("look-size", ["small", "medium", "large", "very-large"]);
  opts("look-color", ["white", "yellow"]);
  opts("look-back", ["outline", "box"]);
  opts("look-place", ["bottom", "top"]);
  $("look-size").value = look.size;
  $("look-color").value = look.color;
  $("look-back").value = look.back;
  $("look-place").value = look.place;
  $("look-sharp").checked = look.sharp;
  const fonts = WHO.fonts || [];
  const font = $("look-font"), key = `${fonts.length}:${look.font}`;
  if (font.dataset.key !== key) {
    font.dataset.key = key;
    const names = fonts.includes("Segoe UI") ? fonts : ["Segoe UI", ...fonts];
    font.innerHTML = names.map((f) => `<option value="${esc(f === "Segoe UI" ? "" : f)}" style="font-family:'${esc(f)}'">${esc(f)}</option>`).join("");
  }
  font.value = names_has(fonts, look.font) ? look.font : "";
}

function names_has(fonts, f) {
  return !!f && fonts.includes(f);
}

/** The look as the page shows it, saved; the preview, when one is shown, made again. */
async function saveLook() {
  const look = {
    size: $("look-size").value, font: $("look-font").value, color: $("look-color").value,
    back: $("look-back").value, place: $("look-place").value, sharp: $("look-sharp").checked,
  };
  try { WHO.extras = await invoke("set_look", { look }); } catch (err) { toast(String(err)); return; }
  if (!$("look-video").hidden) previewLook();
}

/** One second of the user's own video with a line of its subtitles in the look chosen. */
async function previewLook() {
  const v = $("look-video"), b = $("look-preview");
  b.disabled = true;
  b.textContent = t("look.making");
  try {
    const id = S.current && MEDIA.video.includes(extOf(S.current.source_path || "")) ? S.current.id : null;
    const p = await invoke("preview_look", { id, look: WHO.extras.look });
    v.src = fileSrc(p);
    v.hidden = false;
    v.addEventListener("loadeddata", () => { v.currentTime = 0.5; }, { once: true });
  } catch (err) {
    toast(String(err));
  }
  b.disabled = false;
  b.textContent = t("look.preview");
}

// ------------------------------------------------------------------ wiring (from app.js's wire)

function whoWire() {
  $("btn-more").onclick = (ev) => { ev.stopPropagation(); moreMenu($("btn-more")); };
  $("search-who").onchange = () => { WHO.filter = Number($("search-who").value); S.hit = 0; runSearch(true); };
  $("ctx-always").onclick = alwaysWrite;
  document.addEventListener("mousedown", (ev) => { if (WHO.pop && !ev.target.closest(".who-pop")) closePop(); });
  document.addEventListener("keydown", (ev) => { if (ev.key === "Escape") closePop(); });
  $("segments").addEventListener("scroll", closePop);
  listen("language-needs-model", (e) => whoNeedsModel(e.payload));

  $("set-names-srt").onchange = $("set-names-vtt").onchange = async () => {
    try {
      onState(await invoke("set_subtitle_names", { srt: $("set-names-srt").checked, vtt: $("set-names-vtt").checked }));
      WHO.extras = { ...WHO.extras, names_in_srt: $("set-names-srt").checked, names_in_vtt: $("set-names-vtt").checked };
    } catch (err) { toast(String(err)); }
  };
  $("words-add").onclick = () => {
    WHO.extras.words.push({ write: "", heard: [] });
    paintWords();
    $("set-words-list").lastElementChild?.querySelector(".write").focus();
  };
  $("words-save").onclick = async () => {
    try {
      const p = await invoke("save_words");
      if (p) toast(t("words.saved", { name: p.split(/[\\/]/).pop() }), { label: t("toast.showFolder"), run: () => invoke("reveal", { path: p }) });
    } catch (err) { toast(String(err)); }
  };
  $("words-load").onclick = async () => {
    try {
      const x = await invoke("load_words");
      if (x) { WHO.extras = x; paintWords(); }
    } catch (err) { toast(String(err)); }
  };
  for (const id of ["look-size", "look-font", "look-color", "look-back", "look-place", "look-sharp"]) $(id).onchange = saveLook;
  $("look-preview").onclick = previewLook;
}
