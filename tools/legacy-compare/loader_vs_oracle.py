"""Compare what BMZ LEGACY and OpenLR2 load from each chart in the library.

For every SP/DP chart in OpenLR2's song.db (or a random sample), the OpenLR2
oracle (`end 0`: load only) and the BMZ runner (`dump_path`) load the chart with
the NORMAL arrangement and the same LR2 random seed. Compared:
- key mode (song.db), total notes, judge widths (PGREAT..EMPTY POOR) and the GROOVE / HARD / DEATH /
  EASY damage tables (exact doubles; they also depend on the int TOTAL)
- every note, LN and mine per lane: time floored to µs, LN end, mine damage
- the time of every BGM, BGA, BPM and STOP event: each must be the time of an
  OpenLR2 object of the same kind (BMZ may drop an object, e.g. one without a
  #WAV, but must not move one). BMZ's #BMP00 POOR layer at 0 has no object.
"""

import argparse
import json
import math
import random
import sqlite3
import subprocess
import tempfile
from collections import Counter
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
    text = f"chart {chart['path']}\nkeymode {chart['keymode']}\nrandom 0 0\nseed {seed}\ngauge 0\nobjects 1\nend 0\n"
    proc = subprocess.run([str(oracle)], input=text.encode("utf-8"), capture_output=True,
                          cwd=oracle.parent, timeout=300)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.decode("utf-8", "replace")[:300])
    return proc.stdout.decode("utf-8", "replace").splitlines()


# OpenLR2 op -> event kind; extended BPM (8) is a BPM change like 3, and
# OpenLR2 keeps a POOR image object (6) as 106 after loading.
EVENT_OPS = {1: 1, 3: 3, 4: 4, 6: 6, 106: 6, 7: 7, 8: 3, 9: 9}


def oracle_view(lines: list[str]) -> dict:
    header, notes, events = {}, {}, {}
    for line in lines:
        f = line.split(",")
        if f[0] == "H":
            header[f[1]] = f[2]
        elif f[0] == "O":
            kind = EVENT_OPS.get(int(f[2]))
            if kind is not None:
                events.setdefault(kind, []).append(math.floor(float(f[3]) * 1000))
        elif f[0] == "N":
            lane, rt, ln, mine = int(f[1]), float(f[3]), float(f[4]), int(f[5])
            end = math.floor(ln * 1000) if ln > rt else -1
            # ProcNoteOnTiming plays realTiming < realTiming_ln as a long note,
            # even a mine (one before -1 ms), so its damage is never used.
            mine = mine if mine > 0 and end == -1 else -1
            notes.setdefault(lane, []).append((math.floor(rt * 1000), end, mine))
    return {"header": header, "notes": {k: sorted(v) for k, v in notes.items()}, "events": events}


def bmz_view(path: Path) -> dict:
    header, notes, events = {}, {}, {}
    for line in path.read_text(encoding="utf-8").splitlines():
        f = line.split(",")
        if f[0] == "H":
            header[f[1]] = f[2]
        elif f[0] == "N":
            notes.setdefault(int(f[1]), []).append((int(f[2]), int(f[3]), int(f[4])))
        elif f[0] == "E":
            events.setdefault(int(f[1]), []).append(int(f[2]))
    return {"header": header, "notes": {k: sorted(v) for k, v in notes.items()}, "events": events}


EVENT_NAMES = {1: "bgm", 3: "bpm", 4: "bga_base", 6: "bga_poor", 7: "bga_layer", 9: "stop"}


def compare_events(o: dict, b: dict) -> list[str]:
    """Every BMZ event time must be an unused OpenLR2 object time of its kind."""
    diffs = []
    for kind, times in sorted(b.get("events", {}).items()):
        left = Counter(o.get("events", {}).get(kind, []))
        moved = []
        for t in sorted(times):
            if left[t] > 0:
                left[t] -= 1
            elif kind == 6 and t == 0:
                # BMZ's #BMP00 POOR image from the start; LR2 shows it without an object.
                continue
            else:
                moved.append(t)
        if moved:
            diffs.append(f"events {EVENT_NAMES[kind]}: {len(moved)} of {len(times)} at no OpenLR2 time, "
                         f"first {moved[0]}")
    return diffs


def compare(o: dict, b: dict) -> list[str]:
    diffs = []
    no_notes = o["header"].get("totalnotes") == b["header"].get("totalnotes") == "0"
    if o["header"].get("totalnotes") != b["header"].get("totalnotes"):
        diffs.append(f"totalnotes {o['header'].get('totalnotes')} != {b['header'].get('totalnotes')}")
    # LR2 sets judge widths and damage only for a chart with notes; a chart of
    # only mines or BGM still has its objects compared.
    for j in range(1, 6) if not no_notes else ():
        key = f"judgetime{j}"
        if o["header"].get(key) != b["header"].get(key):
            diffs.append(f"{key} {o['header'].get(key)} != {b['header'].get(key)}")
    for g in range(4) if not no_notes else ():
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
        b = bmz_view(Path(c["dump"]))
        diffs = compare(o, b) + compare_events(o, b)
        # The key mode LR2 stored in song.db, which LEGACY must reproduce.
        expected = {5: "K5", 7: "K7", 10: "K10", 14: "K14"}[c["keymode"]]
        if b["header"].get("keymode") != expected:
            diffs.insert(0, f"keymode {expected} != {b['header'].get('keymode')}")
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
