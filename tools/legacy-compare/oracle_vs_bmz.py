"""Differential check: OpenLR2 code (oracle) vs BMZ LEGACY on identical inputs.

For every play compared by compare.py, the recorded key edges are passed to
both the headless OpenLR2 oracle and the BMZ runner (legacy_compare with
`oracle_output_path`). Both see the same poll times, so any difference is a
behaviour difference of the two programs, not of the recording clock.

Compared per play:
- judgements: (poll ms, OpenLR2 lane, judge) in order
- mine hits: (poll ms, lane, damage) in order
- state after each poll with an event: GROOVE/HARD/DEATH/EASY gauge values,
  EX, combo, max combo, judge counts and processed notes
- closing: whether and at which poll the selected gauge stops play
The recorded OpenLR2 judgements are reported alongside.
"""

import argparse
import json
import sqlite3
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from replay_to_oracle import oracle_input, records  # noqa: E402

GAUGES = ("groove", "hard", "death", "easy")


def keymodes(openlr2: Path) -> dict[str, int]:
    uri = (openlr2 / "LR2files/Database/song.db").resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as db:
        return {h.lower(): int(m) for h, m in db.execute("SELECT hash, mode FROM song")}


def parse(lines: list[str]) -> dict:
    out = {"J": [], "M": [], "S": [], "E": None}
    for line in lines:
        f = line.strip().split(",")
        if f[0] == "J":
            out["J"].append((int(f[1]), int(f[2]), int(f[3])))
        elif f[0] == "M":
            out["M"].append((int(f[1]), int(f[2]), round(float(f[3]))))
        elif f[0] == "S":
            out["S"].append((int(f[1]), [float(x) for x in f[2:6]], [int(x) for x in f[8:]]))
        elif f[0] == "E":
            out["E"] = (int(f[1]), f[2])
    return out


def first_diff(a: list, b: list):
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return {"index": i, "oracle": x, "bmz": y}
    if len(a) != len(b):
        i = min(len(a), len(b))
        return {"index": i, "oracle": a[i] if i < len(a) else None, "bmz": b[i] if i < len(b) else None}
    return None


def compare(oracle: dict, bmz: dict, tolerance: float) -> dict:
    result = {
        "judgements": first_diff(oracle["J"], bmz["J"]),
        "mines": first_diff(oracle["M"], bmz["M"]),
        "closing": None,
        "state": None,
        "gauge_max_abs_diff": [0.0] * 4,
    }
    if oracle["E"] and bmz["E"] and (oracle["E"][1] == "closed" or bmz["E"][1] == "closed") and oracle["E"] != bmz["E"]:
        result["closing"] = {"oracle": oracle["E"], "bmz": bmz["E"]}
    o_states = {ms: (hp, rest) for ms, hp, rest in oracle["S"]}
    b_states = {ms: (hp, rest) for ms, hp, rest in bmz["S"]}
    if set(o_states) != set(b_states):
        only = sorted(set(o_states) ^ set(b_states))
        result["state"] = {"poll_set_differs_at": only[:5]}
    for ms in sorted(set(o_states) & set(b_states)):
        (ohp, orest), (bhp, brest) = o_states[ms], b_states[ms]
        for i in range(4):
            result["gauge_max_abs_diff"][i] = max(result["gauge_max_abs_diff"][i], abs(ohp[i] - bhp[i]))
        if result["state"] is None and (orest != brest or any(abs(o - b) > tolerance for o, b in zip(ohp, bhp))):
            result["state"] = {"ms": ms, "oracle": [ohp, orest], "bmz": [bhp, brest]}
    result["equal"] = all(result[k] is None for k in ("judgements", "mines", "closing", "state"))
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results", required=True, help="compare.py output directory")
    parser.add_argument("--oracle", required=True)
    parser.add_argument("--runner", required=True, help="legacy_compare example executable")
    parser.add_argument("--openlr2", default=r"D:\OpenLR2")
    parser.add_argument("--output", required=True)
    parser.add_argument("--tolerance", type=float, default=1e-3, help="gauge value tolerance (BMZ uses f32)")
    args = parser.parse_args()
    modes = keymodes(Path(args.openlr2))
    oracle = Path(args.oracle).resolve()
    out_dir = Path(args.output).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    plays = []
    for line in (Path(args.results) / "results.jsonl").open(encoding="utf-8"):
        r = json.loads(line)
        if r["status"] in ("match", "judgement_sequence_mismatch", "mismatch"):
            plays.append(r)
    # BMZ side: one runner process for every play.
    requests = []
    for r in plays:
        r["bmz_path"] = out_dir / f"{r['profile']}-{r['md5']}.bmz.csv"
        requests.append(json.dumps({"chart_path": r["chart_path"], "replay_path": r["replay_path"],
                                    "md5": r["md5"], "oracle_output_path": str(r["bmz_path"])}))
    runner = subprocess.run([args.runner], input=("\n".join(requests) + "\n").encode("utf-8"),
                            capture_output=True, timeout=7200)
    runner_results = runner.stdout.decode("utf-8", "replace").splitlines()
    rows = []
    for r, runner_line in zip(plays, runner_results):
        replay = Path(r["replay_path"]).read_bytes()
        keymode = modes.get(r["md5"].lower())
        proc = subprocess.run([str(oracle)], input=oracle_input(r["chart_path"], replay, keymode).encode("utf-8"),
                              capture_output=True, cwd=oracle.parent, timeout=600)
        oracle_lines = proc.stdout.decode("utf-8", "replace").splitlines()
        (out_dir / f"{r['profile']}-{r['md5']}.oracle.csv").write_text("\n".join(oracle_lines), encoding="utf-8")
        runner_result = json.loads(runner_line)
        row = {"md5": r["md5"], "title": r.get("title"), "keymode": keymode, "notes": r.get("chart_notes")}
        if runner_result.get("status") == "unsupported_or_error" or not r["bmz_path"].is_file():
            row.update(status="unsupported", reason=runner_result.get("reason"))
        else:
            o, b = parse(oracle_lines), parse(r["bmz_path"].read_text(encoding="utf-8").splitlines())
            diff = compare(o, b, args.tolerance)
            recorded = [v for _, op, v in records(replay) if op in (210, 211)]
            row.update(status="equal" if diff["equal"] else "differs", oracle_end=o["E"], bmz_end=b["E"],
                       oracle_vs_recorded=[j for _, _, j in o["J"]] == recorded, **diff)
        rows.append(row)
        print(json.dumps(row, ensure_ascii=False), flush=True)
    summary = {
        "plays": len(rows),
        "equal": sum(r["status"] == "equal" for r in rows),
        "differs": sum(r["status"] == "differs" for r in rows),
        "unsupported": sum(r["status"] == "unsupported" for r in rows),
        "judgements_equal": sum(r.get("judgements", 0) is None for r in rows),
        "mines_equal": sum(r.get("mines", 0) is None for r in rows),
        "state_equal": sum(r.get("state", 0) is None for r in rows),
        "closing_equal": sum(r.get("closing", 0) is None for r in rows),
        "closed_plays": sum(bool(r.get("oracle_end")) and r["oracle_end"][1] == "closed" for r in rows),
        "max_gauge_abs_diff": [max((r.get("gauge_max_abs_diff") or [0] * 4)[i] for r in rows) for i in range(4)],
        "tolerance": args.tolerance,
        "oracle_equals_recorded": sum(bool(r.get("oracle_vs_recorded")) for r in rows),
    }
    (out_dir / "results.jsonl").write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows), encoding="utf-8")
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
