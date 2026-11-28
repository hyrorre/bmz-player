"""Differential fuzz test: OpenLR2 (oracle) vs BMZ LEGACY on synthetic input.

Each case is a random BMS chart and a random play on it. The chart mixes the
constructs the LR2 loader treats specially (BPM / extended BPM / STOP changes,
measure lengths, large divisions, LN channels, #LNOBJ, mines, invisible notes,
overlapping objects, #RANDOM, objects at position 0, odd characters). The play
mixes accurate, early, late and missing presses, LN holds and early releases,
mine hits and stray presses, with a random arrangement and gauge.

Per case:
1. load: the oracle (`end 0`) and the BMZ runner (`dump_path`) load the chart
   with the NORMAL arrangement; compared as in loader_vs_oracle.py.
2. play: the key edges are written as an .lr2rep (options in its header) and
   replayed through the oracle and the runner (`oracle_output_path`); compared
   as in oracle_vs_bmz.py with tolerance 0.
Failing cases keep their chart, replay and both outputs in the output folder.
"""

import argparse
import hashlib
import json
import math
import random
import shutil
import struct
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from loader_vs_oracle import bmz_view, oracle_view  # noqa: E402
from loader_vs_oracle import compare as compare_load  # noqa: E402
from oracle_vs_bmz import compare as compare_play  # noqa: E402
from oracle_vs_bmz import parse  # noqa: E402
from replay_to_oracle import oracle_input  # noqa: E402

B36 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
# Visible note channels (second digit) per side for each key mode.
KEYS = {5: "123456", 7: "12345689", 10: "123456", 14: "12345689"}
DIVISIONS = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 192, 7, 9, 13, 384, 1000]


def b36(n: int) -> str:
    return B36[n // 36] + B36[n % 36]


def data(rng: random.Random, division: int, density: float, value) -> str:
    return "".join(value() if rng.random() < density else "00" for _ in range(division))


def gen_chart(rng: random.Random, keymode: int) -> str:
    sides = 2 if keymode >= 10 else 1
    lines = [f"#PLAYER {3 if sides == 2 else 1}", "#TITLE fuzz", "#ARTIST fuzz"]
    if rng.random() < 0.98:
        lines.append(f"#BPM {rng.choice([60, 120, 150, 155.5, 180, 222, 300, 999, 33.3])}")
    if rng.random() < 0.9:
        lines.append(f"#RANK {rng.choice([0, 1, 2, 3, 3, 4, -1])}")
    if rng.random() < 0.5:
        lines.append(f"#TOTAL {rng.choice([100, 160, 200, 300.5, 1000, 0, 20])}")
    if rng.random() < 0.3:
        lines.append(f"#DEFEXRANK {rng.choice([50, 100, 120])}")
    lnobj = None
    if rng.random() < 0.3:
        lnobj = rng.choice(["ZZ", "0Z", "1A"])
        lines.append(f"#LNOBJ {lnobj}")
    if rng.random() < 0.5:
        lines.append("#LNTYPE 1")
    wavs = [b36(i) for i in range(1, 30)]
    for w in wavs:
        lines.append(f"#WAV{w} s{w}.wav")
    bpm_ids = [b36(i) for i in range(1, 6)]
    for b in bpm_ids:
        lines.append(f"#BPM{b} {rng.choice([90, 140.25, 200, 400, 75, 0.5, -120] * 4 + [0])}")
    stop_ids = [b36(i) for i in range(1, 6)]
    for s in stop_ids:
        lines.append(f"#STOP{s} {rng.choice([48, 96, 192, 1, 500, -48, 0])}")

    def wav() -> str:
        return rng.choice(wavs)

    measures = rng.randint(3, 12)
    body: list[str] = []
    for m in range(measures):
        def add(ch: str, division: int, density: float, value) -> None:
            body.append(f"#{m:03d}{ch}:{data(rng, division, density, value)}")

        if rng.random() < 0.2:
            body.append(f"#{m:03d}02:{rng.choice(['0.5', '0.75', '1.5', '0.125', '2', '0.001', '1.0'])}")
        if rng.random() < 0.25:
            add("03", rng.choice([1, 2, 4, 8]), 0.5, lambda: f"{rng.randint(1, 255):02X}")
        if rng.random() < 0.2:
            add("08", rng.choice([1, 2, 4]), 0.5, lambda: rng.choice(bpm_ids))
        if rng.random() < 0.15:
            add("09", rng.choice([1, 2, 4, 8]), 0.4, lambda: rng.choice(stop_ids))
        if rng.random() < 0.5:
            add("01", rng.choice(DIVISIONS), 0.3, wav)
        if rng.random() < 0.1:
            add(rng.choice(["04", "07"]), 4, 0.5, wav)
        for side in range(sides):
            for key in KEYS[keymode]:
                ch = str(side + 1) + key
                has_note = rng.random() < 0.45
                if has_note:
                    division = rng.choice(DIVISIONS)
                    density = 0.25 if division <= 64 else 0.02
                    if lnobj and rng.random() < 0.2:
                        add(ch, division, density, lambda: lnobj if rng.random() < 0.4 else wav())
                    else:
                        add(ch, division, density, wav)
                if rng.random() < 0.12:  # LN channel
                    add(str(side + 5) + key, rng.choice([2, 4, 8, 16]), 0.3, wav)
                if rng.random() < 0.06:  # invisible
                    add(str(side + 3) + key, rng.choice([4, 8]), 0.3, wav)
                if rng.random() < (0.08 if has_note else 0.005):  # mine: value is the damage
                    add(("D" if side == 0 else "E") + key, rng.choice([4, 8, 16]), 0.3,
                        lambda: rng.choice(["01", "05", "0A", "ZZ", "1Z", "10"]))
    # Guarantee the channels that fix the key mode (LR2 takes it from song.db).
    marks = (["18"] if keymode in (7, 14) else []) + (["21"] if sides == 2 else [])
    for ch in marks:
        body.append(f"#{rng.randint(1, measures - 1):03d}{ch}:00{wav()}")
    if rng.random() < 0.3:  # an object exactly at the start shifts every position by 1 measure
        body.append(f"#000{rng.choice(['11', '16', '13'] + (['21'] if sides == 2 else []))}:{wav()}")
    if rng.random() < 0.2:
        # Lines whose objects are in a non-transitive order under LR2's comparator.
        n = rng.choice([9781, 9789, 4999, 20011])
        body.append(f"#{rng.randint(0, measures - 1):03d}01:" + wav() * n)
    if rng.random() < 0.15:
        body.append(f"#{rng.randint(0, measures - 1):03d}{rng.choice(['11', '12'])}:{wav()}0{wav()}")  # odd length
    if rng.random() < 0.15:
        body.append(f"#{rng.randint(0, measures - 1):03d}11:{wav()}!!{wav()}zz")  # invalid characters
    if rng.random() < 0.2:
        key = rng.choice(KEYS[keymode])
        body.append("#RANDOM 2")
        body.append("#IF 1")
        body.append(f"#{rng.randint(0, measures - 1):03d}1{key}:{wav()}00{wav()}00")
        body.append("#ENDIF")
        body.append("#IF 2")
        body.append(f"#{rng.randint(0, measures - 1):03d}1{key}:00{wav()}00{wav()}")
        body.append("#ENDIF")
        body.append("#ENDRANDOM")
    rng.shuffle(body)
    if rng.random() < 0.1:
        body = [b.lower() if rng.random() < 0.3 else b for b in body]
    return "\r\n".join(lines + body) + "\r\n"


def lr2_scan_keymode(data: bytes) -> int:
    """LR2's song.db key mode (ParseBMSMETA in LR2_songmanage.cpp), ported separately
    from BMZ so the two can be compared."""
    mode, skip = 5, False
    for raw in data.split(b"\n"):
        for line in (raw[i:i + 102399] for i in range(0, max(len(raw), 1), 102399)):
            if line.startswith(b"#IF") and not line.startswith(b"#IF 1"):
                skip = True
            elif line.startswith(b"#ENDIF"):
                skip = False
            elif line[:1] != b"#" or skip:
                pass
            elif len(line) > 5 and all(48 <= c <= 57 for c in line[1:6]):
                ch = (line[4] - 48) * 10 + line[5] - 48
                if 51 <= ch <= 69:
                    ch -= 40
                if 31 <= ch <= 49:
                    ch -= 20
                if ch in (18, 19, 38, 39, 58, 59):
                    mode = {5: 7, 10: 14}.get(mode, mode)
                elif ch in (28, 29, 48, 49, 68, 69):
                    mode = 14
                elif 20 < ch <= 29 or 40 < ch <= 49 or 61 <= ch <= 69:
                    mode = {5: 10, 7: 14}.get(mode, mode)
    return mode


def gen_edges(rng: random.Random, notes: list[tuple], lanes: list[int]) -> list[tuple[int, int, int]]:
    """notes: (lane, realTiming, realTiming_ln, mine) from the oracle."""
    style = rng.random()  # overall accuracy of this play
    spans: dict[int, list[tuple[int, int]]] = {}
    for lane, rt, ln, mine in notes:
        if lane not in lanes:
            continue  # a key the key mode has no input for (e.g. 6/7 in a 10-key chart)
        if mine > 0:
            if rng.random() < 0.3:
                start = max(int(rt) + rng.randint(-30, 30), 0)
                spans.setdefault(lane, []).append((start, start + rng.randint(1, 120)))
            continue
        r = rng.random()
        if r < 0.1 * (1 + style):
            continue  # miss
        if r < 0.75:
            offset = round(rng.gauss(0, 8 + 40 * style))
        else:
            offset = rng.randint(-400, 400)
        start = int(rt) + offset
        if ln > rt:
            end = int(ln) + (round(rng.gauss(0, 20)) if rng.random() < 0.8 else -rng.randint(0, int(ln - rt) + 50))
        else:
            end = start + rng.randint(1, 150)
        start = max(start, 0)
        spans.setdefault(lane, []).append((start, max(end, start + 1)))
    last = int(max((max(rt, ln) for _, rt, ln, _ in notes), default=2000))
    for _ in range(rng.randint(0, 40)):  # stray presses
        start = rng.randint(0, last + 500)
        spans.setdefault(rng.choice(lanes), []).append((start, start + rng.randint(1, 300)))
    edges = []
    for lane, items in spans.items():
        items.sort()
        merged: list[list[int]] = []
        for start, end in items:
            start = max(start, 0)
            if merged and start <= merged[-1][1]:
                # Same-poll release and press is allowed and exercised.
                if start == merged[-1][1] and rng.random() < 0.5:
                    merged.append([start, end])
                else:
                    merged[-1][1] = max(merged[-1][1], end)
            else:
                merged.append([start, end])
        for start, end in merged:
            edges.append((start, lane, 1))
            edges.append((end, lane, 0))
    edges.sort(key=lambda e: (e[0], e[2]))  # release before press at the same ms
    return edges


def replay_bytes(edges, arrange: int, seed: int, gauge: int, fixed: int) -> bytes:
    rows = [(0, op, 0) for op in (106, 107, 201, 202, 204, 205, 206, 90, 156, 157)]
    rows += [(0, 103, arrange), (0, 153, arrange), (0, 105, fixed), (0, 155, fixed),
             (0, 200, seed), (0, 101, gauge)]
    for ms, lane, press in edges:
        rows.append((ms, lane if lane < 10 else lane + 10, press))
    return b"".join(struct.pack("<iB3xi", *r) for r in rows)


def finite(text: str) -> bool:
    """MSVC prints NaN as `-nan(ind)`."""
    try:
        return math.isfinite(float(text))
    except ValueError:
        return False


def run_oracle(oracle: Path, text: str) -> list[str]:
    proc = subprocess.run([str(oracle)], input=text.encode("utf-8"), capture_output=True,
                          cwd=oracle.parent, timeout=600)
    if proc.returncode != 0:
        raise RuntimeError(f"oracle exit {proc.returncode:#x}: {proc.stderr.decode('utf-8', 'replace')[:200]}")
    return proc.stdout.decode("utf-8", "replace").splitlines()


def run_runner(runner: str, requests: list[dict]) -> list[dict]:
    proc = subprocess.run([runner], input=("\n".join(json.dumps(r) for r in requests) + "\n").encode("utf-8"),
                          capture_output=True, timeout=36000)
    return [json.loads(x) for x in proc.stdout.decode("utf-8", "replace").splitlines()]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", required=True)
    parser.add_argument("--runner", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--jobs", type=int, default=8)
    args = parser.parse_args()
    out = Path(args.output).resolve()
    if out.exists():
        shutil.rmtree(out)
    work = out / "cases"
    work.mkdir(parents=True)
    oracle = Path(args.oracle).resolve()
    runner = str(Path(args.runner).resolve())

    cases = []
    for i in range(args.count):
        rng = random.Random(args.seed * 1_000_003 + i)
        keymode = rng.choice([5, 7, 7, 7, 10, 14, 14])
        text = gen_chart(rng, keymode).encode("ascii")
        path = work / f"{i:05d}.bms"
        path.write_bytes(text)
        # The generator aims at `keymode`; LR2 decides it from the text.
        keymode = lr2_scan_keymode(text)
        cases.append({"id": i, "rng": rng, "keymode": keymode, "path": str(path),
                      "md5": hashlib.md5(text).hexdigest(), "lr2_seed": rng.randint(1, 0x7ffe)})

    # 1. Load with the NORMAL arrangement.
    chunks = [cases[j::args.jobs] for j in range(args.jobs)]

    def dump(chunk):
        reqs = []
        for c in chunk:
            c["dump"] = c["path"][:-4] + ".bmz-load.csv"
            reqs.append({"chart_path": c["path"], "replay_path": "", "md5": c["md5"],
                         "seed": c["lr2_seed"], "dump_path": c["dump"]})
        for c, r in zip(chunk, run_runner(runner, reqs)):
            c["dump_status"] = r

    with ThreadPoolExecutor(args.jobs) as pool:
        list(pool.map(dump, chunks))

    def load(c):
        text = f"chart {c['path']}\nkeymode {c['keymode']}\nrandom 0 0\nseed {c['lr2_seed']}\ngauge 0\nend 0\n"
        try:
            lines = run_oracle(oracle, text)
        except Exception as error:  # noqa: BLE001
            c["status"], c["reason"] = "oracle_error", str(error)
            return
        Path(c["path"][:-4] + ".oracle-load.csv").write_text("\n".join(lines), encoding="utf-8")
        if c["dump_status"].get("status") != "dumped":
            c["status"], c["reason"] = "bmz_error", c["dump_status"].get("reason")
            return
        if any(x.startswith("N,") and not all(map(finite, x.split(",")[3:5])) for x in lines):
            # LR2 itself cannot play it (BPM 0): times are infinite or NaN.
            c["status"] = "oracle_unplayable"
            return
        b = bmz_view(Path(c["dump"]))
        expected = {5: "K5", 7: "K7", 10: "K10", 14: "K14"}[c["keymode"]]
        diffs = compare_load(oracle_view(lines), b)
        if b["header"].get("keymode") != expected:
            diffs.insert(0, f"keymode {expected} != {b['header'].get('keymode')}")
        c["load_diffs"] = diffs
        c["notes"] = [(int(f[1]), float(f[3]), float(f[4]), int(f[5]))
                      for f in (x.split(",") for x in lines) if f[0] == "N"]
        c["totalnotes"] = b["header"].get("totalnotes")
        c["status"] = "load_differs" if diffs else "loaded"

    with ThreadPoolExecutor(args.jobs) as pool:
        list(pool.map(load, cases))

    # 2. Play with a random arrangement, gauge and key edges.
    # A chart with no notes is not played: OpenLR2 leaves its judge widths at 0,
    # so only its mines would be judged, against a width it never set.
    playable = [c for c in cases if c["status"] == "loaded" and c["notes"] and c["totalnotes"] != "0"]
    for c in playable:
        rng = c["rng"]
        keys = 7 if c["keymode"] in (7, 14) else 5
        lanes = list(range(0, keys + 1)) + (list(range(10, 11 + keys)) if c["keymode"] >= 10 else [])
        c["gauge"] = rng.choice([0, 1, 2, 3])
        c["arrange"] = rng.choice([0, 1, 2])
        fixed = rng.choice([0] * 3 + list(range(1, keys + 1))) if c["arrange"] == 2 else 0
        # Edges are generated against the arranged lanes, so ask the oracle where the notes are.
        replay = replay_bytes([], c["arrange"], c["lr2_seed"], c["gauge"], fixed)
        try:
            lines = run_oracle(oracle, oracle_input(c["path"], replay, c["keymode"]) + "end 0\n")
        except Exception as error:  # noqa: BLE001
            c["status"], c["reason"] = "oracle_error", str(error)
            continue
        notes = [(int(f[1]), float(f[3]), float(f[4]), int(f[5]))
                 for f in (x.split(",") for x in lines) if f[0] == "N"]
        by_lane: dict = {}
        for lane, _, _, mine in notes:
            by_lane.setdefault(lane, []).append(mine > 0)
        # OpenLR2 judges the empty buffer slots after a lane of only mines (see README).
        c["mine_only_lanes"] = sorted(k for k, v in by_lane.items() if all(v))
        edges = gen_edges(rng, notes, lanes)
        c["replay"] = c["path"][:-4] + ".lr2rep"
        Path(c["replay"]).write_bytes(replay_bytes(edges, c["arrange"], c["lr2_seed"], c["gauge"], fixed))
        c["edges"] = len(edges)
    playable = [c for c in playable if "replay" in c]
    chunks = [playable[j::args.jobs] for j in range(args.jobs)]

    def play_bmz(chunk):
        reqs = []
        for c in chunk:
            c["bmz_play"] = c["path"][:-4] + ".bmz-play.csv"
            reqs.append({"chart_path": c["path"], "replay_path": c["replay"], "md5": c["md5"],
                         "oracle_output_path": c["bmz_play"]})
        for c, r in zip(chunk, run_runner(runner, reqs)):
            c["play_status"] = r

    with ThreadPoolExecutor(args.jobs) as pool:
        list(pool.map(play_bmz, chunks))

    def play(c):
        try:
            text = oracle_input(c["path"], Path(c["replay"]).read_bytes(), c["keymode"])
            lines = run_oracle(oracle, text)
        except Exception as error:  # noqa: BLE001
            # Reading past a lane buffer can also crash OpenLR2 (heap corruption).
            undefined = c["play_status"].get("lr2_undefined")
            c["status"], c["reason"] = "oracle_undefined" if undefined else "oracle_error", str(error)
            return
        Path(c["path"][:-4] + ".oracle-play.csv").write_text("\n".join(lines), encoding="utf-8")
        if c["play_status"].get("status") == "unsupported_or_error":
            reason = c["play_status"].get("reason") or ""
            # Synthetic tempo extremes; the runner caps plays at one hour.
            c["status"] = "skipped_over_one_hour" if "one-hour" in reason else "bmz_play_error"
            c["reason"] = reason
            return
        try:
            o = parse(lines)
        except ValueError as error:
            # OpenLR2 wrote past a lane buffer and corrupted its own output.
            undefined = c["play_status"].get("lr2_undefined")
            c["status"] = "oracle_undefined" if undefined else "oracle_error"
            c["reason"] = f"unreadable oracle output: {error}"
            return
        b = parse(Path(c["bmz_play"]).read_text(encoding="utf-8").splitlines())
        diff = compare_play(o, b, 0.0)
        c["judgements"] = len(o["J"])
        c["closed"] = bool(o["E"]) and o["E"][1] == "closed"
        if not diff["equal"] and c["play_status"].get("lr2_undefined"):
            # OpenLR2 read past a lane buffer (see legacy_cap.rs): its result is undefined.
            c["status"], c["play_diff"] = "oracle_undefined", {k: v for k, v in diff.items() if v}
        elif not diff["equal"]:
            c["status"] = "play_differs_mine_only_lane" if c["mine_only_lanes"] else "play_differs"
            c["play_diff"] = {k: v for k, v in diff.items() if v}
        else:
            c["status"] = "equal"

    with ThreadPoolExecutor(args.jobs) as pool:
        list(pool.map(play, playable))

    rows, counts = [], {}
    for c in cases:
        if c["status"] == "loaded":
            c["status"] = "equal_no_notes"
        counts[c["status"]] = counts.get(c["status"], 0) + 1
        row = {k: c.get(k) for k in ("id", "keymode", "md5", "lr2_seed", "status", "reason", "load_diffs",
                                     "gauge", "arrange", "mine_only_lanes", "edges", "judgements", "closed", "play_diff")}
        rows.append({k: v for k, v in row.items() if v not in (None, [])})
    for c in cases:  # keep only failing cases' files
        if c["status"] in ("equal", "equal_no_notes"):
            for f in work.glob(f"{c['id']:05d}.*"):
                f.unlink()
    summary = {"cases": len(cases), "seed": args.seed, "statuses": counts,
               "judgements": sum(c.get("judgements", 0) for c in cases),
               "closed_plays": sum(bool(c.get("closed")) for c in cases),
               "mines_loaded": sum(sum(1 for n in c.get("notes", []) if n[3] > 0) for c in cases),
               "lns_loaded": sum(sum(1 for n in c.get("notes", []) if n[2] > n[1]) for c in cases)}
    (out / "results.jsonl").write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows), encoding="utf-8")
    (out / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
