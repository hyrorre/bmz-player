"""Generate an OpenLR2 source overlay; never change the original source tree."""
import argparse
import difflib
from pathlib import Path
import re


def instrument(source):
    if b"openlr2_parity::" in source:
        raise ValueError("source is already instrumented")
    poll = re.compile(rb"int ProcNoteOnTiming\(game \*g, int lane, int keypress, int timing, int player\)\s*\{")
    judge = re.compile(rb"int ApplyJudgeNote\(int judge, game \*g, int _player, int lane, Timer \*T, char isReplay\)\s*\{")
    if len(poll.findall(source)) != 1 or len(judge.findall(source)) != 1:
        raise ValueError("expected exactly one ProcNoteOnTiming and ApplyJudgeNote definition")
    result = judge.sub(lambda m: b'#include "openlr2_parity_trace.hpp"\n\n' + m.group(0)
                       + b"\n\topenlr2_parity::judgement(judge, lane);", source)
    return poll.sub(lambda m: m.group(0)
                    + b"\n\topenlr2_parity::Poll parity_poll(g, lane, keypress, timing, player);", result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True, help="original LR2/Scene04_Play.cpp")
    parser.add_argument("--output", type=Path, required=True, help="separate overlay directory")
    args = parser.parse_args()
    original = args.source.read_bytes()
    modified = instrument(original)
    if args.source.resolve() == (args.output / args.source.name).resolve():
        parser.error("output must be separate from the original source")
    args.output.mkdir(parents=True, exist_ok=True)
    header = Path(__file__).with_name("openlr2_parity_trace.hpp").read_bytes()
    (args.output / args.source.name).write_bytes(modified)
    (args.output / "openlr2_parity_trace.hpp").write_bytes(header)
    # UTF-8 is used only for the reviewable diff; the source overlay preserves bytes.
    diff = "".join(difflib.unified_diff(original.decode("utf-8", errors="replace").splitlines(True),
                                     modified.decode("utf-8", errors="replace").splitlines(True),
                                     fromfile="a/LR2/Scene04_Play.cpp", tofile="b/LR2/Scene04_Play.cpp"))
    (args.output / "source-change.diff").write_text(diff, encoding="utf-8")
    print(args.output.resolve())


if __name__ == "__main__":
    main()
