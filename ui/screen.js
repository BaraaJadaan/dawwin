// Read anything on the screen (1.8.0), in Settings: the shortcut, copying where a program gives no
// selection, Start with Windows, and "say it like this". Its strings join I18N here.
"use strict";

Object.assign(I18N.en.s, {
  "scr.title": "Read anything on the screen",
  "scr.on": "Read the selected text with a shortcut",
  "scr.onTip": "Select text in any program and press the shortcut. Press it again to stop. The text is read on this computer and kept nowhere.",
  "scr.keys": "Shortcut",
  "scr.keysTip": "Click here, then press the keys you want.",
  "scr.pressKeys": "Press the keys…",
  "scr.use": "Use {keys}",
  "scr.copy": "Copy the selection when a program does not give it",
  "scr.copyTip": "Some programs do not show Dawwin what is selected. Dawwin then presses Ctrl+C for you and puts back what you had copied. Never in a terminal.",
  "scr.start": "Start with Windows",
  "scr.startTip": "Dawwin starts in the notification area when you sign in, ready for the shortcut.",
  "scr.say": "Say it like this",
  "scr.sayTip": "Words the voice says wrongly, and how to say them. Used in sentences of the language chosen.",
  "scr.written": "Written",
  "scr.said": "Said",
  "scr.add": "Add a word",
  "scr.remove": "Remove",
  "scr.lang.ar": "Arabic",
  "scr.lang.en": "English",
  "scr.saveSound": "Save as sound",
  "scr.saveSoundTip": "A sound file of the reading, to listen to away from the computer: M4A, or M4B with its chapters for a book. It is made in the list on the left, and can be stopped there.",
  "scr.saving": "Saving as sound: it goes on in the list on the left.",
  "scr.jobSaving": "Saving the reading as sound",
  "scr.chapters": "Chapters",
  "scr.chaptersTip": "Go to a chapter",
  "scr.pdfiumAsk": "Reading PDFs needs PDFium, a download of about 4 MB, once. Download it now?",
  "scr.pdfiumGetting": "Downloading PDFium…",
  "guide.screenTitle": "Read anything on the screen",
  "guide.screenText": "In Settings, under Read anything on the screen, turn on the shortcut. Then select text in any program, a web page, a Word document, a PDF, an email or a chat, and press Ctrl+Shift+Space: Dawwin reads it in the voices of Read aloud, in a small player at the corner of the screen that leaves the keyboard where it was, with pause, back and forward a sentence. Press the shortcut again to stop. If another program already uses those keys, Dawwin says so and offers free ones. While the shortcut is on, closing Dawwin's window keeps it in the notification area, and it can start with Windows. Where a program does not show Dawwin what is selected, Dawwin presses Ctrl+C for you and puts back what you had copied, never in a terminal (this can be turned off). What you select is read on this computer and kept nowhere. Words the voice gets wrong go in Say it like this.",
});

Object.assign(I18N.ar.s, {
  "scr.title": "قراءة أي نص على الشاشة",
  "scr.on": "اقرأ النص المحدَّد باختصار لوحة المفاتيح",
  "scr.onTip": "حدِّد نصًّا في أي برنامج واضغط الاختصار، واضغطه مرة أخرى للإيقاف. يُقرأ النص على هذا الحاسوب ولا يُحفظ في أي مكان.",
  "scr.keys": "الاختصار",
  "scr.keysTip": "انقر هنا ثم اضغط المفاتيح التي تريدها.",
  "scr.pressKeys": "اضغط المفاتيح…",
  "scr.use": "استخدم {keys}",
  "scr.copy": "انسخ المحدَّد حين لا يعطيه البرنامج",
  "scr.copyTip": "بعض البرامج لا تُطلع دَوِّن على ما حُدِّد فيها، فيضغط دَوِّن Ctrl+C عنك ثم يعيد ما كنت نسخته. ولا يفعل ذلك في الطرفية أبدًا.",
  "scr.start": "التشغيل مع ويندوز",
  "scr.startTip": "يبدأ دَوِّن في منطقة الإشعارات عند تسجيل دخولك، جاهزًا للاختصار.",
  "scr.say": "انطقها هكذا",
  "scr.sayTip": "كلمات ينطقها الصوت خطأً، وكيف تُنطق. تُطبَّق في جمل اللغة المختارة.",
  "scr.written": "تُكتب",
  "scr.said": "تُنطق",
  "scr.add": "أضف كلمة",
  "scr.remove": "احذف",
  "scr.lang.ar": "العربية",
  "scr.lang.en": "الإنجليزية",
  "scr.saveSound": "احفظها صوتًا",
  "scr.saveSoundTip": "ملف صوتي للقراءة تسمعه بعيدًا عن الحاسوب: M4A، أو M4B بفصوله إن كان كتابًا. يُصنع في القائمة على الجانب، ويمكن إيقافه منها.",
  "scr.saving": "يُحفَظ صوتًا: تابعه في القائمة على الجانب.",
  "scr.jobSaving": "حفظ القراءة صوتًا",
  "scr.chapters": "الفصول",
  "scr.chaptersTip": "انتقل إلى فصل",
  "scr.pdfiumAsk": "تحتاج قراءة ملفات PDF إلى PDFium، تنزيل بحجم 4 ميغابايت تقريبًا مرة واحدة. أتنزّله الآن؟",
  "scr.pdfiumGetting": "يُنزَّل PDFium…",
  "guide.screenTitle": "اقرأ أي نص على الشاشة",
  "guide.screenText": "من الإعدادات، تحت «قراءة أي نص على الشاشة»، شغّل الاختصار. ثم حدِّد نصًا في أي برنامج، صفحة ويب أو مستند Word أو ملف PDF أو بريد أو محادثة، واضغط Ctrl+Shift+Space: يقرؤه دَوِّن بأصوات «القراءة بصوت عالٍ» في مشغّل صغير في زاوية الشاشة يترك لوحة المفاتيح حيث كانت، فيه إيقاف مؤقت ورجوع جملة وتقدّم جملة. اضغط الاختصار مرة أخرى للإيقاف. وإن كان برنامج آخر يستخدم تلك المفاتيح قال لك دَوِّن ذلك واقترح مفاتيح خالية. وما دام الاختصار مشغّلًا فإغلاق نافذة دَوِّن يُبقيه في منطقة الإشعارات، ويمكن أن يبدأ مع ويندوز. وحيث لا يُطلع البرنامج دَوِّن على ما حُدِّد فيه، يضغط دَوِّن Ctrl+C عنك ثم يعيد ما كنت نسخته، ولا يفعل ذلك في الطرفية أبدًا (ويمكن إطفاء ذلك). ما تحدِّده يُقرأ على هذا الحاسوب ولا يُحفظ في أي مكان. والكلمات التي ينطقها الصوت خطأً تضعها في «انطقها هكذا».",
});



(function () {
  const call = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
  let state = null;

  const box = () => document.getElementById("set-screen");

  function build() {
    const el = box();
    if (!el || el.dataset.built) return;
    el.dataset.built = "1";
    el.innerHTML = `
      <h2 data-i18n="scr.title"></h2>
      <div class="setting">
        <label class="toggle" id="scr-on-label"><input type="checkbox" id="scr-on"> <span data-i18n="scr.on"></span></label>
      </div>
      <div class="setting" id="scr-keys-row">
        <div class="setting-text">
          <h3 data-i18n="scr.keys"></h3>
          <p class="error setting-error" id="scr-error" dir="auto" hidden></p>
          <div class="setting actions" id="scr-free" hidden></div>
        </div>
        <input class="input" id="scr-keys" readonly dir="ltr" data-i18n-aria="scr.keys">
      </div>
      <div class="setting">
        <label class="toggle" id="scr-copy-label"><input type="checkbox" id="scr-copy"> <span data-i18n="scr.copy"></span></label>
      </div>
      <div class="setting">
        <label class="toggle" id="scr-start-label"><input type="checkbox" id="scr-start"> <span data-i18n="scr.start"></span></label>
      </div>
      <div class="setting">
        <div class="setting-text">
          <h3 id="scr-say-title" data-i18n="scr.say"></h3>
          <div id="scr-say"></div>
          <button class="btn btn-sm btn-ghost" id="scr-add" data-i18n="scr.add"></button>
        </div>
      </div>`;
    applyI18n();
    wire();
  }

  function tips() {
    const tip = (id, key) => {
      const e = document.getElementById(id);
      if (e) e.title = t(key);
    };
    tip("scr-on-label", "scr.onTip");
    tip("scr-keys", "scr.keysTip");
    tip("scr-copy-label", "scr.copyTip");
    tip("scr-start-label", "scr.startTip");
    tip("scr-say-title", "scr.sayTip");
  }

  function render(s) {
    if (!s) return;
    state = s;
    build();
    const st = s.settings;
    document.getElementById("scr-on").checked = st.on;
    const keys = document.getElementById("scr-keys");
    if (document.activeElement !== keys) keys.value = st.shortcut;
    document.getElementById("scr-copy").checked = st.copy;
    const start = document.getElementById("scr-start");
    start.checked = st.on && st.start_with_windows;
    start.disabled = !st.on;
    for (const id of ["scr-keys", "scr-copy"]) document.getElementById(id).disabled = !st.on;
    const err = document.getElementById("scr-error");
    err.hidden = !s.error;
    err.textContent = s.error || "";
    const free = document.getElementById("scr-free");
    free.hidden = !s.free.length;
    free.innerHTML = "";
    for (const k of s.free) {
      const b = document.createElement("button");
      b.className = "btn btn-sm";
      b.textContent = t("scr.use", { keys: k });
      b.onclick = async () => render(await call("screen_set_shortcut", { keys: k }));
      free.append(b);
    }
    renderSay(st.say);
    tips();
  }

  function renderSay(list) {
    const host = document.getElementById("scr-say");
    host.innerHTML = "";
    list.forEach((item, i) => {
      const row = document.createElement("div");
      row.className = "setting actions scr-say-row";
      const lang = document.createElement("select");
      lang.className = "input select";
      for (const l of ["ar", "en"]) {
        const o = document.createElement("option");
        o.value = l;
        o.textContent = t("scr.lang." + l);
        lang.append(o);
      }
      lang.value = item.lang;
      const written = Object.assign(document.createElement("input"), { className: "input", value: item.written, dir: "auto", placeholder: t("scr.written") });
      const said = Object.assign(document.createElement("input"), { className: "input", value: item.said, dir: "auto", placeholder: t("scr.said") });
      written.setAttribute("aria-label", t("scr.written"));
      said.setAttribute("aria-label", t("scr.said"));
      const remove = Object.assign(document.createElement("button"), { className: "btn btn-sm btn-ghost", textContent: "✕", title: t("scr.remove") });
      remove.setAttribute("aria-label", t("scr.remove"));
      const save = () => {
        const next = state.settings.say.slice();
        next[i] = { lang: lang.value, written: written.value, said: said.value };
        saveSay(next);
      };
      lang.onchange = save;
      written.onchange = save;
      said.onchange = save;
      remove.onclick = () => saveSay(state.settings.say.filter((_, j) => j !== i));
      row.append(lang, written, said, remove);
      host.append(row);
    });
  }

  async function saveSay(list) {
    // an empty row being typed in stays on the page until it has a word
    const kept = await call("screen_set_say", { list });
    render(kept);
  }

  /** The keys pressed, as the shortcut is written: by the key's place, so any layout works. */
  function keysOf(e) {
    const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : /^F\d+$/.test(e.code) ? e.code : e.code === "Space" ? "Space" : null;
    if (!key) return null;
    const parts = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");
    return parts.length ? [...parts, key].join("+") : null;
  }

  function wire() {
    document.getElementById("scr-on").onchange = async (e) => render(await call("screen_set_on", { on: e.target.checked }));
    document.getElementById("scr-copy").onchange = async (e) => render(await call("screen_set_copy", { on: e.target.checked }));
    document.getElementById("scr-start").onchange = async (e) => {
      try {
        render(await call("screen_set_start_with_windows", { on: e.target.checked }));
      } catch (err) {
        render({ ...state, error: String(err) });
      }
    };
    const keys = document.getElementById("scr-keys");
    keys.onfocus = () => (keys.value = t("scr.pressKeys"));
    keys.onblur = () => (keys.value = state ? state.settings.shortcut : "");
    keys.onkeydown = async (e) => {
      if (e.key === "Tab") return;
      e.preventDefault();
      if (e.key === "Escape") return keys.blur();
      const k = keysOf(e);
      if (!k) return;
      const s = await call("screen_set_shortcut", { keys: k });
      keys.blur();
      render(s);
    };
    document.getElementById("scr-add").onclick = () => {
      const next = state.settings.say.concat([{ lang: UI_LANG === "ar" ? "ar" : "en", written: "", said: "" }]);
      state.settings.say = next;
      renderSay(next);
      const inputs = document.querySelectorAll("#scr-say .scr-say-row input");
      if (inputs.length) inputs[inputs.length - 2].focus();
    };
  }

  async function refresh() {
    try {
      render(await call("screen_state"));
    } catch (e) {
      // an app without this part (a test build of another version): nothing to show
    }
  }

  window.addEventListener("DOMContentLoaded", () => {
    // How to use: a part of its own, after Read aloud (app.js, which lists the parts, comes after this file)
    if (typeof GUIDE !== "undefined" && !GUIDE.includes("screen")) GUIDE.splice(GUIDE.indexOf("read") + 1, 0, "screen");
    refresh();
  });
  // what the pictures of How to use show (tools/e2e/pictures.mjs)
  window.__screen = { render };
  // the interface language changed: the tooltips and the lists follow
  new MutationObserver(() => state && render(state)).observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });
})();
