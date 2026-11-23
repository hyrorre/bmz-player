"""Compare what BMZ LEGACY and OpenLR2 load from each chart in the library.

For every SP/DP chart in OpenLR2's song.db (or a random sample), the OpenLR2
oracle (`end 0`: load only) and the BMZ runner (`dump_path`) load the chart with
the NORMAL arrangement and the same LR2 random seed. Compared:
- total notes, judge widths (PGREAT..EMPTY POOR) and the GROOVE / HARD / DEATH /
  EASY damage tables (exact doubles; they also depend on the int TOTAL)
- every note, LN and mine per lane: time floored to µs, LN end, mine damage
"""

import argparse
import json
import math
import random
import sqlite3
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

MODES = (5, 7, 10, 14)


def charts(openlr2: Path) -> list[dict]:
    uri = (openlr2 / "LR2files/Database/song.db").resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as db:
        rows = db.execute("SELECT hash, path, mode, title FROM song").fetchall()
    return [{"md5": h.lower(), "path": p, "keymode": int(m), "title": t}
            for h, p, m, t in rows if int(m) in MODES and len(h) == 32]


def run_oracle(oracle: Path, chart: dict, seed: int) -> list[str]:
    text = f"chart {chart['path']}\nkeymode {chart['keymode']}\nrandom 0 0\nseed {seed}\ngauge 0\nend 0\n"
    proc = subprocess.run([str(oracle)], input=text.encode("utf-8"), capture_output=True,
                          cwd=oracle.parent, timeout=300)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.decode("utf-8", "replace")[:300])
    return proc.stdout.decode("utf-8", "replace").splitlines()


def oracle_view(lines: list[str]) -> dict:
    header, notes = {}, {}
    for line in lines:
        f = line.split(",")
        if f[0] == "H":
            header[f[1]] = f[2]
        elif f[0] == "N":
            lane, rt, ln, mine = int(f[1]), float(f[3]), float(f[4]), int(f[5])
            end = math.floor(ln * 1000) if ln > rt else -1
            notes.setdefault(lane, []).append((math.floor(rt * 1000), end, mine if mine > 0 else -1))
    return {"header": header, "notes": {k: sorted(v) for k, v in notes.items()}}


def bmz_view(path: Path) -> dict:
    header, notes = {}, {}
    for line in path.read_text(encoding="utf-8").splitlines():
        f = line.split(",")
        if f[0] == "H":
            header[f[1]] = f[2]
        elif f[0] == "N":
            notes.setdefault(int(f[1]), []).append((int(f[2]), int(f[3]), int(f[4])))
    return {"header": header, "notes": {k: sorted(v) for k, v in notes.items()}}


def compare(o: dict, b: dict) -> list[str]:
    diffs = []
    if o["header"].get("totalnotes") == b["header"].get("totalnotes") == "0" and not o["notes"]:
        # LR2 sets judge widths and damage only for a chart with notes.
        return [] if not b["notes"] else ["notes: oracle has none"]
    if o["header"].get("totalnotes") != b["header"].get("totalnotes"):
        diffs.append(f"totalnotes {o['header'].get('totalnotes')} != {b['header'].get('totalnotes')}")
    for j in range(1, 6):
        key = f"judgetime{j}"
        if o["header"].get(key) != b["header"].get(key):
            diffs.append(f"{key} {o['header'].get(key)} != {b['header'].get(key)}")
    for g in range(4):
        for j in range(6):
            key = f"damage{g}_{j}"
            ov, bv = o["header"].get(key), b["header"].get(key)
            if ov is None and bv is None:
                continue
            if ov is None or bv is None or float(ov) != float(bv):
                diffs.append(f"{key} {ov} != {bv}")
                break
    for lane in sorted(set(o["notes"]) | set(b["notes"])):
        on, bn = o["notes"].get(lane, []), b["notes"].get(lane, [])
        if on != bn:
            first = next((i for i, (x, y) in enumerate(zip(on, bn)) if x != y), min(len(on), len(bn)))
            diffs.append(f"lane {lane}: {len(on)} vs {len(bn)} objects, first diff #{first}: "
                         f"{on[first] if first < len(on) else None} != {bn[first] if first < len(bn) else None}")
    return diffs


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", required=True)
    parser.add_argument("--runner", required=True)
    parser.add_argument("--openlr2", default=r"D:\OpenLR2")
    parser.add_argument("--output", required=True)
    parser.add_argument("--sample", type=int, help="random sample size (default: all charts)")
    parser.add_argument("--sample-seed", type=int, default=1)
    parser.add_argument("--lr2-seed", type=int, default=1, help="LR2 random seed for #RANDOM")
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--only", help="file of chart MD5s to compare (whitespace separated)")
    args = parser.parse_args()
    out_dir = Path(args.output).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    items = [c for c in charts(Path(args.openlr2)) if Path(c["path"]).is_file()]
    if args.only:
        wanted = set(Path(args.only).read_text(encoding="utf-8").split())
        items = [c for c in items if c["md5"] in wanted]
    if args.sample:
        items = random.Random(args.sample_seed).sample(items, min(args.sample, len(items)))
    oracle = Path(args.oracle).resolve()
    work = Path(tempfile.mkdtemp(prefix="bmz-loader-"))

    # BMZ side: one runner process per job, each over a chunk of charts.
    chunks = [items[i::args.jobs] for i in range(args.jobs)]

    def run_runner(chunk: list[dict]) -> list[dict]:
        lines = []
        for c in chunk:
            c["dump"] = str(work / f"{c['md5']}.bmz.csv")
            lines.append(json.dumps({"chart_path": c["path"], "replay_path": "", "md5": c["md5"],
                                     "seed": args.lr2_seed, "dump_path": c["dump"]}))
        proc = subprocess.run([args.runner], input=("\n".join(lines) + "\n").encode("utf-8"),
                              capture_output=True, timeout=36000)
        return [json.loads(x) for x in proc.stdout.decode("utf-8", "replace").splitlines()]

    with ThreadPoolExecutor(args.jobs) as pool:
        for chunk, results in zip(chunks, pool.map(run_runner, chunks)):
            for c, r in zip(chunk, results):
                c["bmz_status"] = r.get("status")
                c["bmz_reason"] = r.get("reason")

    def check(c: dict) -> dict:
        row = {"md5": c["md5"], "keymode": c["keymode"], "title": c["title"], "path": c["path"]}
        try:
            o = oracle_view(run_oracle(oracle, c, args.lr2_seed))
        except Exception as error:  # noqa: BLE001
            return {**row, "status": "oracle_error", "reason": str(error)}
        if c.get("bmz_status") != "dumped":
            return {**row, "status": "bmz_error", "reason": c.get("bmz_reason")}
        diffs = compare(o, bmz_view(Path(c["dump"])))
        return {**row, "status": "equal" if not diffs else "differs", "diffs": diffs[:8]}

    rows = []
    with ThreadPoolExecutor(args.jobs) as pool:
        for row in pool.map(check, items):
            rows.append(row)
    counts: dict = {}
    for r in rows:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    kinds: dict = {}
    for r in rows:
        for d in r.get("diffs", []):
            k = d.split(" ")[0] if not d.startswith("lane") else "notes"
            kinds[k] = kinds.get(k, 0) + 1
    summary = {"charts": len(rows), "statuses": counts, "difference_kinds": kinds,
               "lr2_seed": args.lr2_seed, "sample": args.sample}
    (out_dir / "results.jsonl").write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows), encoding="utf-8")
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
