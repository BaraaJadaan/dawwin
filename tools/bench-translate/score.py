"""Score every model's translations: COMET-22 (Unbabel/wmt22-comet-da) and chrF++.

    score.py <tests.jsonl> <dir with <key>.jsonl and <key>.meta.json> <results.md>
"""
import glob
import json
import os
import sys
from collections import defaultdict

import sacrebleu
from comet import download_model, load_from_checkpoint


def main(tests_path, out_dir, results_path):
    tests = {t["id"]: t for t in (json.loads(l) for l in open(tests_path, encoding="utf-8"))}
    directions = list(dict.fromkeys(f"{t['src']}-{t['tgt']}" for t in tests.values()))
    runs = {}
    for meta_path in sorted(glob.glob(os.path.join(out_dir, "**", "*.meta.json"), recursive=True)):
        meta = json.load(open(meta_path))
        hyps = [json.loads(l) for l in open(meta_path.replace(".meta.json", ".jsonl"), encoding="utf-8")]
        runs[meta["key"]] = (meta, hyps)

    model = load_from_checkpoint(download_model("Unbabel/wmt22-comet-da"))
    rows = []
    for key, (meta, hyps) in runs.items():
        data = [{"src": tests[h["id"]]["source"], "mt": h["hyp"], "ref": tests[h["id"]]["reference"]} for h in hyps]
        scores = model.predict(data, batch_size=16, gpus=0, progress_bar=False).scores
        comet, chrf = defaultdict(list), {}
        by_dir = defaultdict(lambda: ([], []))
        for h, s in zip(hyps, scores):
            t = tests[h["id"]]
            d = f"{t['src']}-{t['tgt']}"
            comet[d].append(s)
            by_dir[d][0].append(h["hyp"])
            by_dir[d][1].append(t["reference"])
        for d, (hy, ref) in by_dir.items():
            chrf[d] = sacrebleu.corpus_chrf(hy, [ref], word_order=2).score
        row = {
            "key": key,
            "gb": round(meta["bytes"] / 1e9, 2),
            "comet": {d: round(100 * sum(v) / len(v), 1) for d, v in comet.items()},
            "chrf": {d: round(v, 1) for d, v in chrf.items()},
            "tokens_per_second": round(meta["tokens"] / meta["wall_seconds"], 1),
            "truncated": meta["truncated"],
        }
        row["comet_avg"] = round(sum(row["comet"].values()) / len(row["comet"]), 1)
        row["chrf_avg"] = round(sum(row["chrf"].values()) / len(row["chrf"]), 1)
        rows.append(row)
        print(json.dumps(row))
    rows.sort(key=lambda r: -r["comet_avg"])

    per = len(tests) // max(1, len(directions))
    lines = [f"COMET-22 (higher is better), per direction; {per} sentences each; Q4_K_M on a 4-core CPU runner.", ""]
    lines.append("| Model | GB | " + " | ".join(directions) + " | COMET avg | chrF++ avg | tokens/s | cut off |")
    lines.append("|" + "---|" * (len(directions) + 6))
    for r in rows:
        cells = [str(r["comet"].get(d, "")) for d in directions]
        lines.append(
            f"| {r['key']} | {r['gb']} | " + " | ".join(cells)
            + f" | **{r['comet_avg']}** | {r['chrf_avg']} | {r['tokens_per_second']} | {r['truncated']} |"
        )
    lines += ["", "chrF++ per direction:", "", "| Model | " + " | ".join(directions) + " |", "|" + "---|" * (len(directions) + 1)]
    for r in rows:
        lines.append(f"| {r['key']} | " + " | ".join(str(r["chrf"].get(d, "")) for d in directions) + " |")
    text = "\n".join(lines) + "\n"
    open(results_path, "w", encoding="utf-8").write(text)
    json.dump(rows, open(results_path.replace(".md", ".json"), "w"), indent=1)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        open(summary, "a", encoding="utf-8").write(text)
    print(text)


if __name__ == "__main__":
    main(*sys.argv[1:4])
