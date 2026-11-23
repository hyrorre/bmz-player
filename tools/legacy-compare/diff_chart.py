"""Show where BMZ LEGACY and OpenLR2 first load a chart differently.

Prints the first bar line whose time differs, the timing-related lines of that
measure and the one before, and the first differing note per lane.
"""

import argparse
import json
import math
import os
import re
import sqlite3
import subprocess
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("md5")
    parser.add_argument("--oracle", default=r".local\openlr2-oracle\openlr2_oracle.exe")
    parser.add_argument("--runner", default=r"target\debug\examples\legacy_compare.exe")
    parser.add_argument("--openlr2", default=r"D:\OpenLR2")
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    uri = (Path(args.openlr2) / "LR2files/Database/song.db").resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as db:
        path, mode = db.execute("SELECT path, mode FROM song WHERE hash=?", (args.md5,)).fetchone()
    oracle = Path(args.oracle).resolve()
    o = subprocess.run([str(oracle)], input=f"chart {path}\nkeymode {mode}\nrandom 0 0\nseed {args.seed}\ngauge 0\nend 0\n".encode(),
                       capture_output=True, cwd=oracle.parent).stdout.decode("utf-8", "replace").splitlines()
    dump = Path(".local") / f"diff-{args.md5}.bmz.csv"
    env = dict(os.environ)
    subprocess.run([args.runner], input=(json.dumps({"chart_path": path, "replay_path": "", "md5": args.md5,
                                                      "seed": args.seed, "dump_path": str(dump)}) + "\n").encode(),
                   capture_output=True, env=env)
    b = dump.read_text(encoding="utf-8").splitlines()
    ol = [math.floor(float(x.split(",")[2]) * 1000) for x in o if x.startswith("L,")]
    bl = [int(x.split(",")[2]) for x in b if x.startswith("L,")]
    print(f"bars: oracle {len(ol)} bmz {len(bl)}")
    first = next((i for i, (x, y) in enumerate(zip(ol, bl)) if x != y), None)
    text = Path(path).read_bytes().decode("cp932", "replace").splitlines()
    if first is not None:
        print(f"first differing bar #{first}: oracle {ol[first]} bmz {bl[first]} (prev {ol[first-1] if first else None})")
        for measure in (first - 2, first - 1, first):
            for line in text:
                m = re.match(r"#(\d{3})(02|03|08|09):", line)
                if m and int(m.group(1)) == measure:
                    print("  ", line[:120])
    for line in text:
        if re.match(r"(?i)#(BPM\w\w|STOP\w\w|EXBPM\w\w)[ \t]", line) and len(line) < 40:
            pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
