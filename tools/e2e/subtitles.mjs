// Subtitles in every script, through the real app: made-up transcripts in Chinese, Japanese,
// Korean, Thai and Khmer (each one line of 28 seconds, which 1.3.0 made one giant subtitle),
// exported; a Chinese video given its subtitle track and a Thai one burned in, with Dawwin's own
// ffmpeg; an Arabic transcript with English words and its translation under it; and the exports
// that players and spreadsheets would read as something else.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { check, ffmpeg, FFMPEG, finish, makeHome, need, open, OUT, PORT, slash, TOOLS } from "./lib.mjs";

need("DAWWIN_E2E_TOOLS", TOOLS);

const home = makeHome("subtitles-home");
const work = join(OUT, "subtitles-files");
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const video = (name, seconds) => {
  const file = join(work, name);
  ffmpeg(["-f", "lavfi", "-i", `testsrc=size=854x480:rate=24:duration=${seconds}`, "-f", "lavfi", "-i", `sine=frequency=300:duration=${seconds}`, "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", file]);
  return file;
};

const talks = {
  zh: ["今天我们开会讨论新产品的发布计划。市场部希望在下个月底之前完成所有的准备工作，包括宣传视频和网站更新。技术部表示时间有点紧张，但是会尽量配合。大家还有什么问题吗？", 16],
  ja: ["今日は新しいプロジェクトについて話し合います。まず、スケジュールを確認しましょう。来月の終わりまでに、デザインとテストを終わらせる必要があります。何か質問はありますか？", 13],
  ko: ["오늘 회의에서는 새로운 앱의 출시 계획을 논의하겠습니다. 마케팅팀은 다음 달 말까지 모든 준비를 마치기를 원합니다. 개발팀은 일정이 조금 빠듯하지만 최대한 맞추겠다고 했습니다.", 16],
  th: ["เมื่อวานเราไปตลาดด้วยกัน แล้วก็ซื้อผลไม้มาเยอะมาก วันนี้เราจะประชุมเรื่องแผนงานใหม่ ฝ่ายการตลาดอยากให้เสร็จก่อนสิ้นเดือนหน้า ฝ่ายเทคนิคบอกว่าเวลาค่อนข้างกระชั้นชิด แต่จะพยายามทำให้ทัน", 35],
  km: ["ថ្ងៃនេះយើងនឹងពិភាក្សាអំពីផែនការថ្មី។ ក្រុមទីផ្សារចង់ឱ្យរួចរាល់មុនចុងខែក្រោយ។ ក្រុមបច្ចេកទេសនិយាយថាពេលវេលាតឹងតែងបន្តិច ប៉ុន្តែនឹងព្យាយាមឱ្យអស់ពីសមត្ថភាព។", 35],
};
const zhVideo = video("会议.mp4", 30);
const thVideo = video("ประชุม.mp4", 30);
const save = (id, language, segments, source, translations) =>
  writeFileSync(join(home, "history", `${id}.json`), JSON.stringify({ id, title: id, source_path: source || "C:/made-up/x.m4a", created_ms: Date.now(), duration_s: 30, language, elapsed_s: 1, device: "CPU", model: "made up", partial: false, segments, translations: translations || [] }));
for (const [lang, [text]] of Object.entries(talks)) {
  save(`talk-${lang}`, lang, [{ start: 1, end: 29, text, edited: false, voice: [1, 29] }], lang === "zh" ? zhVideo : lang === "th" ? thVideo : undefined);
}
// Arabic with English words, punctuation, and a translation
const ar = "تمام. التقارير الشهرية بتطلع ناقصة، والأرقام ما بتطابق الـ invoices أبدًا. رح نراجعها مع المحاسبة يوم الاثنين، ونرجعلكم بالتفاصيل يوم الثلاثاء.";
const en = "Fine. The monthly reports come out incomplete, and the numbers never match the invoices. We will go through them with accounting on Monday, and come back to you with the details on Tuesday.";
save("talk-ar", "ar", [{ start: 0.5, end: 18, text: ar, edited: false, voice: [0.5, 18], pauses: [[0.05, 1.2, 2.0]] }], undefined, [{ language: "en", model: "made up", lines: [{ text: en, from: ar, edited: false }] }]);
save("talk-odd", "en", [{ start: 0.5, end: 6, text: "=SUM(A1:A9) said <b>Sam</b> --> & left", edited: false }, { start: 7, end: 9, text: "-5 degrees today", edited: false }]);

const a = await open(home, PORT + 4);
const width = (lang, line) => [...line].reduce((n, c) => {
  if (["zh", "ja", "ko"].includes(lang)) return n + (/[\u1100-\u11ff\u2e80-\u9fff\uac00-\ud7af\uf900-\ufaff\uff01-\uff60]/u.test(c) ? 1 : 0.5);
  if (["th", "km"].includes(lang)) return n + (/[\u0e31\u0e34-\u0e3a\u0e47-\u0e4e\u17b4-\u17d3]/u.test(c) ? 0 : 1);
  return n + 1;
}, 0);
const readSrt = (file) => readFileSync(file, "utf8").replace(/^\uFEFF/, "").trim().split(/\r?\n\r?\n/).map((b) => { const l = b.split(/\r?\n/); return { time: l[1], lines: l.slice(2) }; });

for (const [lang, [text, max]] of Object.entries(talks)) {
  const file = join(work, `talk-${lang}.srt`);
  await a.invoke("export_to_path", { id: `talk-${lang}`, path: slash(file), stamps: false });
  const cues = readSrt(file);
  const widest = Math.max(...cues.flatMap((c) => c.lines.map((l) => width(lang, l))));
  const squeeze = (s) => s.replace(/\s+/g, "");
  check(`${lang}: one 28-second line becomes ${cues.length} subtitles, no line over ${max}`, cues.length >= 4 && widest <= max && cues.every((c) => c.lines.length <= 2), { cues: cues.length, widest });
  check(`${lang}: no character lost`, squeeze(cues.map((c) => c.lines.join("")).join("")) === squeeze(text));
  if (lang === "zh" || lang === "ja") check(`${lang}: no line starts with a full stop or a comma`, cues.every((c) => c.lines.every((l) => !/^[。、，？！」]/u.test(l))), cues.map((c) => c.lines.join(" / ")));
}

// ---- a Chinese video gets its track, with Dawwin's own small ffmpeg
const st = (await a.state()).setup;
check("the app uses Dawwin's own ffmpeg", (st.ffmpeg || "").includes("tools"), st.ffmpeg);
const zhOut = join(work, "会议 (Chinese).mp4");
await a.invoke("export_video_to_path", { id: "talk-zh", path: slash(zhOut) });
const inside = join(work, "inside-zh.srt");
if (existsSync(zhOut)) execFileSync(FFMPEG, ["-nostdin", "-loglevel", "error", "-y", "-i", zhOut, "-map", "0:s:0", inside]);
check("the Chinese video carries the same subtitles as a track", existsSync(inside) && readSrt(inside).length === readSrt(join(work, "talk-zh.srt")).length, existsSync(inside) ? readSrt(inside).length : "no track");

// ---- a Thai video with its subtitles burned in, with Dawwin's own video ffmpeg
const thOut = join(work, "ประชุม (Thai, burned in).mp4");
const job = await a.invoke("export_burned_to_path", { id: "talk-th", path: slash(thOut) });
await a.idle(300);
const burned = (await a.state()).queue.find((j) => j.id === job);
check("a Thai video is burned in", existsSync(thOut) && (!burned || burned.status === "done"), burned?.error);
if (existsSync(thOut)) {
  const frame = join(OUT, "subtitles-thai-frame.png");
  execFileSync(FFMPEG, ["-nostdin", "-loglevel", "error", "-y", "-ss", "6", "-i", thOut, "-frames:v", "1", frame]);
  console.log("      a frame of it, to look at:", frame);
}

// ---- both languages: each sentence of the translation whole, under its subtitle
const both = join(work, "talk-ar-both.srt");
await a.invoke("export_to_path", { id: "talk-ar", path: slash(both), stamps: false, view: "both", language: "en" });
const bc = readSrt(both);
const english = bc.map((c) => c.lines.filter((l) => /^[\x20-\x7e]+$/.test(l)).join(" ")).filter(Boolean);
// a sentence too long for two lines is cut where a clause ends, never in the middle of one
check("the translation under each subtitle is whole sentences, or clauses of a long one", english.length >= 2 && english.every((e) => /[.?!,]$/.test(e)) && english.filter((e) => /[.?!]$/.test(e)).length >= 2, english);
check("its lines are wrapped to 42 characters", bc.every((c) => c.lines.every((l) => [...l].length <= 42)), bc.map((c) => c.lines));
check("the Arabic keeps its English word and its punctuation", bc.some((c) => c.lines.some((l) => l.includes("invoices"))) && bc.some((c) => c.lines.some((l) => l.endsWith("."))));

// ---- what a player or a spreadsheet would read as something else
const out = (ext) => { const f = join(work, `odd.${ext}`); return a.invoke("export_to_path", { id: "talk-odd", path: slash(f), stamps: false }).then(() => readFileSync(f, "utf8")); };
const vtt = await out("vtt"), csv = await out("csv"), tsv = await out("tsv");
check("WebVTT shows <b> and --> as text", vtt.includes("&lt;b&gt;Sam&lt;/b&gt;") && vtt.includes("--&gt; &amp; left"), vtt);
check("a spreadsheet takes =SUM and -5 as text", csv.includes(`"'=SUM(A1:A9)`) && csv.includes(`"'-5 degrees today"`) && tsv.includes("\t'=SUM(A1:A9)") && tsv.includes("\t'-5 degrees"), { csv, tsv });

a.close();
finish();
