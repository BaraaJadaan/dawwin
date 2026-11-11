"""Score the minutes against the hand-made answers.

    score.py <artifacts dir> <results.md>

An item the model gives (a decision, an action, an open question) is the answer's item when the
lines it names fall within two lines of the answer's: the app links every item to its lines, so an
item that names the wrong lines is wrong there too. Each answer item counts once. Of a found action,
the owner is right when one of the person's spellings is in it, and the deadline when one of its
spellings is (or both are empty). A topic is found when it starts within three lines of the
answer's. The summary is scored on the key facts it mentions. Text is compared after folding Arabic
letter forms, diacritics and digits, and case.
"""
import glob
import json
import os
import re
import sys
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
DIACRITICS = re.compile("[ؐ-ًؚ-ٰٟۖ-ۭـ]")
FOLD = str.maketrans({"أ": "ا", "إ": "ا", "آ": "ا", "ٱ": "ا", "ى": "ي", "ة": "ه", "ؤ": "و", "ئ": "ي",
                      **{chr(0x660 + d): str(d) for d in range(10)}, **{chr(0x6f0 + d): str(d) for d in range(10)}})
EMPTY = {"", "-", "none", "n/a", "na", "not specified", "not given", "unspecified", "tbd", "لا يوجد", "غير محدد",
         "لم يحدد", "بدون", "لا"}


def norm(s):
    s = unicodedata.normalize("NFKC", s or "").lower()
    s = DIACRITICS.sub("", s).translate(FOLD)
    s = re.sub(r"[^\w/]+", " ", s)
    return " ".join(s.split())


def has_any(text, alternatives):
    t = norm(text)
    return any(norm(a) and norm(a) in t for a in alternatives)


def near(pred_lines, gold_lines, slack):
    lo, hi = min(gold_lines) - slack, max(gold_lines) + slack
    return any(isinstance(p, int) and lo <= p <= hi for p in pred_lines or [])


def pair(pred, gold, key, slack):
    """One-to-one: each gold item takes the first unused predicted item near it."""
    used, pairs = set(), []
    for g in gold:
        for i, p in enumerate(pred):
            if i not in used and near(p.get(key) if isinstance(p.get(key), list) else [p.get(key)], g[key] if isinstance(g[key], list) else [g[key]], slack):
                used.add(i)
                pairs.append((g, p))
                break
    return pairs


def f1(found, n_pred, n_gold):
    if n_pred == 0 and n_gold == 0:
        return None
    p = found / n_pred if n_pred else 0.0
    r = found / n_gold if n_gold else 0.0
    return 0.0 if p + r == 0 else 2 * p * r / (p + r)


def arabic_share(text):
    letters = [c for c in text if c.isalpha()]
    return sum("؀" <= c <= "ۿ" for c in letters) / len(letters) if letters else 0.0


def score_one(answer, gold, target):
    s = {"valid": answer is not None}
    if answer is None:
        return s
    people = gold["people"]
    dec, act, oq, top = (answer.get(k) or [] for k in ("decisions", "actions", "open_questions", "topics"))
    s["kind"] = answer.get("kind") == gold["kind"]
    if gold["kind"] == "meeting":
        dp = pair(dec, gold["decisions"], "lines", 2)
        ap = pair(act, gold["actions"], "lines", 2)
        s["dec_f1"] = f1(len(dp), len(dec), len(gold["decisions"]))
        s["dec_recall"] = len(dp) / max(len(gold["decisions"]), 1)
        s["dec_invented"] = len(dec) - len(dp)
        s["act_f1"] = f1(len(ap), len(act), len(gold["actions"]))
        s["act_recall"] = len(ap) / max(len(gold["actions"]), 1)
        s["act_invented"] = len(act) - len(ap)
        who_ok = due_ok = 0
        for g, p in ap:
            who = p.get("who") or ""
            if g["who"]:
                who_ok += has_any(who, g["who"])
            else:
                who_ok += not any(has_any(who, alts) for alts in people)
            due = p.get("due") or ""
            if g["due"]:
                due_ok += has_any(due, g["due"])
            else:
                due_ok += norm(due) in EMPTY
        s["who"] = who_ok / len(ap) if ap else None
        s["due"] = due_ok / len(ap) if ap else None
        s["who_unknown"] = sum(1 for p in act if norm(p.get("who")) not in EMPTY and not any(has_any(p.get("who"), alts) for alts in people))
    else:
        s["no_meeting_items"] = not dec and not act
    qp = pair(oq, gold["open_questions"], "lines", 2)
    s["oq_f1"] = f1(len(qp), len(oq), len(gold["open_questions"]))
    tp = pair(top, gold["topics"], "line", 3)
    s["topic_f1"] = f1(len(tp), len(top), len(gold["topics"]))
    summary = answer.get("summary") or ""
    s["facts"] = sum(has_any(summary, alts) for alts in gold["facts"]) / max(len(gold["facts"]), 1)
    every = " ".join([summary] + [d.get("text", "") for d in dec] + [a.get("what", "") for a in act] + [t.get("title", "") for t in top])
    share = arabic_share(every)
    s["language_ok"] = share >= 0.5 if target == "ar" else share <= 0.05
    s["summary_words"] = len(summary.split())
    return s


def overall(s, kind):
    """One number per run, 0 to 100: what matters most in minutes weighs most."""
    if not s.get("valid"):
        return 0.0
    v = lambda k: s.get(k) if s.get(k) is not None else 1.0
    if kind == "meeting":
        total = (0.25 * v("dec_f1") + 0.25 * v("act_f1") + 0.1 * v("who") + 0.1 * v("due") + 0.15 * v("facts")
                 + 0.1 * v("topic_f1") + 0.05 * float(s["kind"]))
    else:
        total = 0.4 * v("facts") + 0.3 * v("topic_f1") + 0.15 * float(s["kind"]) + 0.15 * float(s["no_meeting_items"])
    return 100 * total * (1.0 if s["language_ok"] else 0.5)


def mean(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else None


def fmt(x, pct=True):
    if x is None:
        return "–"
    return f"{100 * x:.0f}" if pct else f"{x:.1f}"


def main(art, out_md):
    golds = {os.path.basename(p)[:-5]: json.load(open(p, encoding="utf-8")) for p in glob.glob(os.path.join(HERE, "meetings", "*.json"))}
    runs = []
    for path in sorted(glob.glob(os.path.join(art, "out-*", "*.json"))):
        if path.endswith(".meta.json"):
            continue
        runs.append(json.load(open(path, encoding="utf-8")))
    rows, detail = [], {}
    for run in runs:
        per = []
        for r in run["results"]:
            gold = golds[r["meeting"]]
            s = score_one(r.get("answer"), gold, r["target"]) if "error" not in r else {"valid": False}
            s["overall"] = overall(s, gold["kind"])
            s.update({k: r.get(k) for k in ("task", "meeting", "target", "parts", "seconds", "prompt_tokens", "tokens", "prompt_per_s", "tokens_per_s", "finish")})
            s["mode"] = "parts" if r.get("parts") else ("own" if r["target"] == gold["language"] else "other")
            s["kind_gold"] = gold["kind"]
            per.append(s)
        detail[run["key"]] = per
        own = [s for s in per if s["mode"] == "own"]
        meet = [s for s in own if s["kind_gold"] == "meeting"]
        whole = [s for s in per if s["mode"] != "parts" and s.get("tokens_per_s")]
        rows.append({
            "key": run["key"],
            "gb": run["bytes"] / 1e9,
            "score": mean([s["overall"] for s in own]),
            "ar": mean([s["overall"] for s in own if golds[s["meeting"]]["language"] == "ar"]),
            "en": mean([s["overall"] for s in own if golds[s["meeting"]]["language"] == "en"]),
            "other": mean([s["overall"] for s in per if s["mode"] == "other"]),
            "parts": mean([s["overall"] for s in per if s["mode"] == "parts"]),
            "dec_recall": mean([s.get("dec_recall") for s in meet]),
            "dec_invented": sum(s.get("dec_invented", 0) for s in meet),
            "act_recall": mean([s.get("act_recall") for s in meet]),
            "act_invented": sum(s.get("act_invented", 0) for s in meet),
            "who": mean([s.get("who") for s in meet]),
            "due": mean([s.get("due") for s in meet]),
            "facts": mean([s.get("facts") for s in own]),
            "topics": mean([s.get("topic_f1") for s in own]),
            "kind": mean([float(s.get("kind", False)) for s in own]),
            "lang": mean([float(s.get("language_ok", False)) for s in per]),
            "invalid": sum(not s["valid"] for s in per),
            "pp": mean([s["prompt_per_s"] for s in whole]),
            "tg": mean([s["tokens_per_s"] for s in whole]),
            "sec": mean([s["seconds"] for s in whole]),
        })
    rows.sort(key=lambda r: -(r["score"] or 0))
    n_meet = sum(1 for g in golds.values() if g["kind"] == "meeting")
    md = [
        f"Minutes of {len(golds)} made-up recordings ({n_meet} meetings, the rest a lecture and an interview), "
        "Q4_K_M unless named, greedy, on a 4-core CPU runner. Score 0–100 (see score.py). "
        "Decisions and actions: share of the answer's items found, and items given that match none (invented or misplaced).",
        "",
        "| Model | GB | Score | Arabic | English | Other language | In parts | Decisions found | invented | Actions found | invented | Owner right | Deadline right | Summary facts | Topics | Kind | Language right | Broken | Prompt tok/s | Output tok/s | s per run |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        md.append(
            f"| {r['key']} | {r['gb']:.2f} | **{fmt(r['score'], False)}** | {fmt(r['ar'], False)} | {fmt(r['en'], False)} | {fmt(r['other'], False)} | "
            f"{fmt(r['parts'], False)} | {fmt(r['dec_recall'])} | {r['dec_invented']} | {fmt(r['act_recall'])} | {r['act_invented']} | "
            f"{fmt(r['who'])} | {fmt(r['due'])} | {fmt(r['facts'])} | {fmt(r['topics'])} | {fmt(r['kind'])} | {fmt(r['lang'])} | {r['invalid']} | "
            f"{fmt(r['pp'], False)} | {fmt(r['tg'], False)} | {fmt(r['sec'], False)} |"
        )
    md += ["", "Per recording (score):", "", "| Model | " + " | ".join(sorted({s["task"] for per in detail.values() for s in per})) + " |"]
    tasks = sorted({s["task"] for per in detail.values() for s in per})
    md.append("|---|" + "---|" * len(tasks))
    for r in rows:
        by = {s["task"]: s for s in detail[r["key"]]}
        md.append(f"| {r['key']} | " + " | ".join(fmt(by[t]["overall"], False) if t in by else "–" for t in tasks) + " |")
    open(out_md, "w", encoding="utf-8").write("\n".join(md) + "\n")
    json.dump({"rows": rows, "detail": detail}, open(out_md[:-3] + ".json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("\n".join(md))


if __name__ == "__main__":
    main(*sys.argv[1:3])
