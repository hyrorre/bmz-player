"""Convert an OpenLR2 .lr2rep into openlr2_oracle input.

The replay's own options (arrangement, seed, gauge) are used; key edges keep
their recorded clock-41 times. This lets the oracle and BMZ receive exactly the
same poll times, which the original recording cannot guarantee.
"""

import argparse
import struct
import sys


def records(data: bytes):
    return [struct.unpack_from("<iB3xi", data, i) for i in range(0, len(data) - 11, 12)]


def oracle_input(chart: str, replay: bytes, keymode: int, gauge: int | None = None) -> str:
    rows = records(replay)
    header = {op: value for ms, op, value in rows if ms == 0 and op >= 40 and op != 210}
    sides = 2 if keymode >= 10 else 1
    lines = [
        f"chart {chart}",
        f"keymode {keymode}",
        f"random {header.get(103, 0)} {header.get(153, 0)}",
        f"seed {header[200]}",
        f"randfix {header.get(105, 0)} {header.get(155, 0)}",
        f"randsc {header.get(106, 0)} {header.get(156, 0)}",
        f"dpflip {header.get(206, 0)}",
        f"gauge {header.get(101, 0) if gauge is None else gauge}",
    ]
    for ms, op, value in rows:
        lane = None
        if op <= 7:
            lane = op
        elif sides == 2 and 20 <= op <= 27:
            lane = op - 10
        if lane is not None and value in (0, 1):
            lines.append(f"e {ms} {lane} {value}")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chart", required=True)
    parser.add_argument("--replay", required=True)
    parser.add_argument("--keymode", type=int, required=True)
    parser.add_argument("--gauge", type=int)
    args = parser.parse_args()
    with open(args.replay, "rb") as handle:
        sys.stdout.write(oracle_input(args.chart, handle.read(), args.keymode, args.gauge))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
