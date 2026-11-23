"""Differential check: OpenLR2 code (oracle) vs BMZ LEGACY on identical inputs.

For every play compared by compare.py, the recorded key edges are passed to
both the headless OpenLR2 oracle and (already, via compare.py) BMZ. Both see the
same poll times, so any difference is a behaviour difference of the two
programs, not of the original recording clock. The recorded OpenLR2 judgements
are reported alongside to measure how faithfully a replay reproduces a play.
"""

import argparse
import csv
import json
import sqlite3
import subprocess
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from replay_to_oracle import oracle_input, records  # noqa: E402


def keymodes(openlr2: Path) -> dict[str, int]:
    uri = (openlr2 / "LR2files/Database/song.db").resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as db:
        return {h.lower(): int(m) for h, m in db.execute("SELECT hash, mode FROM song")}


def bmz_sequence(trace: Path) -> list[int]:
    with trace.open(encoding="utf-8") as handle:
        return [int(r["bmz_judge"]) for r in csv.DictReader(handle) if r["bmz_judge"] != ""]


def first_diff(a: list, b: list):
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return i
    return None if len(a) == len(b) else min(len(a), len(b))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results", required=True, help="compare.py output directory")
    parser.add_argument("--oracle", required=True)
    parser.add_argument("--openlr2", default=r"D:\OpenLR2")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    modes = keymodes(Path(args.openlr2))
    oracle = Path(args.oracle).resolve()
    out_dir = Path(args.output)
    out_dir.mkdir(parents=True, exist_ok=True)
    rows = []
    for line in (Path(args.results) / "results.jsonl").open(encoding="utf-8"):
        r = json.loads(line)
        if r["status"] not in ("match", "judgement_sequence_mismatch", "mismatch") or not r.get("trace_path"):
            continue
        keymode = modes.get(r["md5"].lower())
        replay = Path(r["replay_path"]).read_bytes()
        proc = subprocess.run([str(oracle)], input=oracle_input(r["chart_path"], replay, keymode).encode("utf-8"),
                              capture_output=True, cwd=oracle.parent, timeout=600)
        out = proc.stdout.decode("utf-8", "replace").splitlines()
        (out_dir / f"{r['profile']}-{r['md5']}.oracle.csv").write_text("\n".join(out), encoding="utf-8")
        oracle_j = [int(x.split(",")[3]) for x in out if x.startswith("J,")]
        oracle_m = [x for x in out if x.startswith("M,")]
        recorded = [v for _, op, v in records(replay) if op in (210, 211)]
        bmz = bmz_sequence(Path(r["trace_path"]))
        row = {
            "md5": r["md5"], "title": r.get("title"), "keymode": keymode, "notes": r.get("chart_notes"),
            "oracle_rc": proc.returncode,
            "bmz_vs_oracle": "equal" if bmz == oracle_j else "differs",
            "bmz_vs_oracle_first": first_diff(bmz, oracle_j),
            "oracle_vs_recorded": "equal" if oracle_j == recorded else "differs",
            "bmz_vs_recorded": r["status"],
            "oracle_counts": [Counter(oracle_j)[i] for i in range(6)],
            "bmz_counts": r.get("bmz_counts_ep_p_b_g_gr_pg"),
            "oracle_mine_hits": len(oracle_m), "bmz_mine_hits": r.get("bmz_mine_hits"),
        }
        rows.append(row)
        print(json.dumps(row, ensure_ascii=False), flush=True)
    summary = {
        "plays": len(rows),
        "bmz_equals_oracle": sum(r["bmz_vs_oracle"] == "equal" for r in rows),
        "oracle_equals_recorded": sum(r["oracle_vs_recorded"] == "equal" for r in rows),
        "bmz_equals_recorded": sum(r["bmz_vs_recorded"] == "match" for r in rows),
        "mine_hits_equal": sum(r["oracle_mine_hits"] == r["bmz_mine_hits"] for r in rows),
    }
    (out_dir / "results.jsonl").write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows), encoding="utf-8")
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
