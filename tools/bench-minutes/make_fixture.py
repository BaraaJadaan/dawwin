"""Write same_as_app.json: what this benchmark sends for a small made-up meeting. A test in the app
(minutes.rs, the_app_asks_as_the_benchmark_measured) builds the same requests and compares them."""
import json
import os

import minutes

LINES = [
    (2.0, "سارة", "طيب يا جماعة نبدأ، أول شي الـ release."),
    (9.0, "عمر", "أنا بقترح نأجل الـ launch لـ 15 نيسان."),
    (16.0, "سارة", "تمام، متفقين. ورامي بيصلّح الـ login bug قبل الخميس."),
    (24.0, "", "مين رح يدفع للـ venue؟ ما حدا بيعرف لسا."),
]
NOTES = [
    {"kind": "meeting", "topics": [{"title": "الإطلاق", "line": 1}], "decisions": [], "actions": [], "open_questions": [], "summary": "بدأ الاجتماع."},
    {"kind": "meeting", "topics": [], "decisions": [{"text": "تأجيل الإطلاق إلى 15 نيسان.", "lines": [2, 3]}],
     "actions": [{"who": "رامي", "what": "إصلاح الـ login bug", "due": "الخميس", "lines": [3]}], "open_questions": [], "summary": "اتفقوا على الموعد."},
]


def main():
    whole_m, whole_s = minutes.messages_whole(LINES, "ar", "ar")
    part_m, part_s = minutes.messages_part(LINES, 1, 3, 2, 3, "ar", "en")
    comb_m, _ = minutes.messages_combine(NOTES, [(0, 1), (1, 3)], LINES, "ar", "ar")
    out = {
        "lines": [list(l) for l in LINES],
        "whole": {"messages": whole_m, "schema": whole_s},
        "part": {"messages": part_m, "schema": part_s},
        "notes": NOTES,
        "combine": {"messages": comb_m},
    }
    path = os.path.join(minutes.HERE, "same_as_app.json")
    open(path, "w", encoding="utf-8").write(json.dumps(out, ensure_ascii=False, indent=1) + "\n")


if __name__ == "__main__":
    main()
