"""How Dawwin asks for minutes, the same way src-tauri/src/minutes.rs does: the prompts and limits
come from src-tauri/prompts/minutes.json, the schema and the cutting into parts are written here
again, and tests in the app check that the two agree on the example in test_same_as_app.json."""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
PROMPTS = json.load(open(os.path.join(HERE, "..", "..", "src-tauri", "prompts", "minutes.json"), encoding="utf-8"))
NAMES = {"ar": "Arabic", "en": "English"}


def load_meeting(path):
    """A .txt of `MM:SS<TAB>speaker<TAB>text` lines: [(start seconds, speaker, text)]."""
    lines = []
    for raw in open(path, encoding="utf-8"):
        raw = raw.rstrip("\r\n")
        if not raw.strip():
            continue
        time, speaker, text = raw.split("\t", 2)
        parts = [int(p) for p in time.split(":")]
        seconds = parts[-1] + 60 * parts[-2] + (3600 * parts[-3] if len(parts) > 2 else 0)
        lines.append((float(seconds), speaker.strip(), text.strip()))
    return lines


def ts(seconds):
    s = int(max(seconds, 0))
    return f"{s // 3600}:{s // 60 % 60:02}:{s % 60:02}" if s >= 3600 else f"{s // 60:02}:{s % 60:02}"


def render(lines, first=0, last=None):
    """Lines `first..last` (0-based, end exclusive) as the model reads them, numbered from 1."""
    last = len(lines) if last is None else last
    out = []
    for i in range(first, last):
        start, speaker, text = lines[i]
        who = f" {speaker}:" if speaker else ""
        out.append(f"[{i + 1}] {ts(start)}{who} {text}")
    return "\n".join(out)


SCHEMA = open(os.path.join(HERE, "..", "..", "src-tauri", "prompts", "minutes-schema.json"), encoding="utf-8").read()


def schema(lowest, highest):
    """The JSON the answer must be, with line numbers held to the lines given."""
    return json.loads(SCHEMA.replace('"LOWEST"', str(lowest)).replace('"HIGHEST"', str(highest)))


def language_note(source, target):
    if source == target:
        return PROMPTS["same_language"]
    return PROMPTS["other_language"].replace("{language}", NAMES[target])


def messages_whole(lines, source, target):
    user = "\n\n".join([
        PROMPTS["transcript"],
        render(lines),
        PROMPTS["task"].replace("{language}", NAMES[target]) + " " + language_note(source, target),
        PROMPTS["fields"],
    ])
    return [{"role": "system", "content": PROMPTS["system"]}, {"role": "user", "content": user}], schema(1, len(lines))


def messages_part(lines, first, last, part, parts, source, target):
    intro = (PROMPTS["part"].replace("{part}", str(part)).replace("{parts}", str(parts))
             .replace("{from}", ts(lines[first][0])).replace("{to}", ts(lines[last - 1][0])))
    user = "\n\n".join([
        PROMPTS["transcript"] + " " + intro,
        render(lines, first, last),
        PROMPTS["task_part"].replace("{language}", NAMES[target]) + " " + language_note(source, target),
        PROMPTS["fields"],
    ])
    return [{"role": "system", "content": PROMPTS["system"]}, {"role": "user", "content": user}], schema(first + 1, last)


def render_notes(notes, spans, lines):
    out = []
    for n, (note, (first, last)) in enumerate(zip(notes, spans), 1):
        block = [f"Part {n} ({ts(lines[first][0])} to {ts(lines[last - 1][0])}), {note.get('kind', 'other')}:",
                 f"Summary: {note.get('summary', '').strip()}"]
        for t in note.get("topics", []):
            block.append(f"Topic [{t['line']}]: {t['title']}")
        for d in note.get("decisions", []):
            block.append(f"Decision {d['lines']}: {d['text']}")
        for a in note.get("actions", []):
            block.append(f"Action {a['lines']}: who: {a['who'] or '-'}; what: {a['what']}; due: {a['due'] or '-'}")
        for q in note.get("open_questions", []):
            block.append(f"Open question {q['lines']}: {q['text']}")
        out.append("\n".join(block))
    return "\n\n".join(out)


def messages_combine(notes, spans, lines, source, target):
    user = "\n\n".join([
        PROMPTS["notes"],
        render_notes(notes, spans, lines),
        PROMPTS["task_combine"].replace("{language}", NAMES[target]) + " " + language_note(source, target),
        PROMPTS["fields"],
    ])
    return [{"role": "system", "content": PROMPTS["system"]}, {"role": "user", "content": user}], schema(1, len(lines))


def cut(lines, part_seconds, part_chars=12000):
    """Parts of at most `part_seconds` and `part_chars`, cut where the speaker changes or pauses,
    in the last quarter of a part: [(first, last)], 0-based, end exclusive."""
    spans, first = [], 0
    n = len(lines)
    while first < n:
        start, size, last = lines[first][0], 0, first
        while last < n and (last == first or (lines[last][0] - start < part_seconds and size + len(lines[last][2]) <= part_chars)):
            size += len(lines[last][2])
            last += 1
        if last < n:
            best, best_score = last, -1.0
            for i in range(first + 1, last + 1):
                if lines[i][0] - start < 0.75 * part_seconds and i != last:
                    continue
                gap = lines[i][0] - lines[i - 1][0]
                score = gap + (2.0 if lines[i][1] != lines[i - 1][1] else 0.0)
                if score >= best_score:
                    best, best_score = i, score
            last = best
        spans.append((first, last))
        first = last
    return spans
