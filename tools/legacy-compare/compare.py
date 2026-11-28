"""Read-only OpenLR2 score/replay inventory and BMZ LEGACY engine comparison."""
import argparse
import collections
from contextlib import contextmanager
import csv
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import sys


@contextmanager
def open_db(path):
    con = sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA query_only=ON")
    try:
        yield con
    finally:
        con.close()


def inventory(root):
    songs = collections.defaultdict(list)
    with open_db(root / "LR2files/Database/song.db") as db:
        for row in db.execute("SELECT hash,title,subtitle,path FROM song ORDER BY path"):
            songs[row["hash"].lower()].append(dict(row))
    for score_db in sorted((root / "LR2files/Database/Score").glob("*.db")):
        with open_db(score_db) as db:
            tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            if "score" not in tables:
                continue
            # Include imported rows, but do not silently duplicate a normal score row.
            scores = {r["hash"].lower(): dict(r) for r in db.execute("SELECT * FROM score")}
            if "imported_score" in tables:
                for row in db.execute("SELECT * FROM imported_score"):
                    scores.setdefault(row["hash"].lower(), dict(row))
            replay_dir = root / "LR2files/Replay" / score_db.stem
            # Replay-only charts are useful too; best score DB need not contain them.
            hashes = set(scores) | {p.stem.lower() for p in replay_dir.glob("*.lr2rep") if len(p.stem) == 32}
            for md5 in sorted(hashes):
                score = scores.get(md5)
                candidates = songs.get(md5, [])
                chart = next((c for c in candidates if chart_path(root, c["path"]).is_file()), None)
                info = chart or (candidates[0] if candidates else {})
                replay = replay_dir / (md5 + ".lr2rep")
                yield {
                    "profile": score_db.stem, "md5": md5,
                    "title": info.get("title", "") + info.get("subtitle", ""),
                    "chart_path": str(chart_path(root, chart["path"])) if chart else "",
                    "replay_path": str(replay) if replay.is_file() else "",
                    "database_best_ex": score["perfect"] * 2 + score["great"] if score else None,
                    "database_total_notes": score["totalnotes"] if score else None,
                    "has_score": score is not None,
                    "status": "ready" if chart and replay.is_file() else "missing_chart" if not chart else "missing_replay",
                }


def chart_path(root, value):
    path = Path(value)
    return path if path.is_absolute() else root / path


def captured_chart_row(root, md5, profile=None):
    # A new captured play need not replace the PB or save a best-score replay.
    with open_db(root / "LR2files/Database/song.db") as db:
        candidates = db.execute(
            "SELECT title,subtitle,path FROM song WHERE lower(hash)=? ORDER BY path", (md5.lower(),)
        )
        for song in candidates:
            path = chart_path(root, song["path"])
            if path.is_file():
                return {"profile": profile or "captured", "md5": md5.lower(),
                        "title": song["title"] + song["subtitle"], "chart_path": str(path),
                        "replay_path": "", "database_best_ex": None,
                        "database_total_notes": None, "has_score": False, "status": "ready"}
    raise FileNotFoundError(f"Captured chart is missing from song.db or disk: {md5}")


def run(args):
    rows = list(inventory(args.openlr2.resolve()))
    if args.profile:
        rows = [r for r in rows if r["profile"] == args.profile]
    if args.md5:
        rows = [r for r in rows if r["md5"] == args.md5.lower()]
    if args.poll_trace:
        row = next((r for r in rows if r["chart_path"]), None)
        if row is None:
            row = captured_chart_row(args.openlr2.resolve(), args.md5, args.profile)
        row["status"] = "ready"
        rows = [row]
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "traces").mkdir(exist_ok=True)
    if not args.inventory_only and not args.runner.is_file():
        raise FileNotFoundError(f"Build the runner first: {args.runner}")
    worker = None
    runner_sha256 = None
    completed = 0
    try:
        if not args.inventory_only:
            runner_sha256 = hashlib.sha256(args.runner.read_bytes()).hexdigest()
            worker = subprocess.Popen([str(args.runner.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      text=True, encoding="utf-8", bufsize=1)
        with (args.output / "results.jsonl").open("w", encoding="utf-8") as out:
            for row in rows:
                if row["status"] == "ready" and worker and (args.limit is None or completed < args.limit):
                    request = {k: row[k] for k in ("chart_path", "replay_path", "md5")}
                    trace_path = (args.output / "traces" / f"{row['profile']}-{row['md5']}.csv").resolve()
                    request["trace_path"] = str(trace_path)
                    if args.poll_trace:
                        request["poll_trace"] = str(args.poll_trace.resolve())
                    worker.stdin.write(json.dumps(request, ensure_ascii=True) + "\n")
                    worker.stdin.flush()
                    line = worker.stdout.readline()
                    if not line:
                        code = worker.wait(timeout=10)
                        raise RuntimeError(f"Runner stopped unexpectedly (exit {code}); check FFmpeg DLLs on PATH")
                    row.update(json.loads(line))
                    if trace_path.is_file():
                        row["trace_path"] = str(trace_path)
                    # Keep the aggregate PB separate: it may belong to another play.
                    if args.poll_trace:
                        row["poll_trace_sha256"] = hashlib.sha256(args.poll_trace.read_bytes()).hexdigest()
                    else:
                        row["replay_sha256"] = hashlib.sha256(Path(row["replay_path"]).read_bytes()).hexdigest()
                    completed += 1
                    print(f"[{completed}] {row['profile']} {row['md5']} {row['status']}", flush=True)
                elif row["status"] == "ready":
                    row["status"] = "not_run"
                out.write(json.dumps(row, ensure_ascii=False) + "\n")
                out.flush()
    finally:
        if worker:
            worker.stdin.close()
            worker.wait(timeout=30)
    write_reports(rows, args.output, completed, runner_sha256)


def write_reports(rows, output, completed, runner_sha256=None):
    # Old reports counted equal totals as a match. Preserve strict semantics when
    # regenerating their reports as well.
    for row in rows:
        if row["status"] == "match" and row.get("sequence_equal") is False:
            row["status"] = "judgement_sequence_mismatch"
    with (output / "results.jsonl").open("w", encoding="utf-8") as out:
        for row in rows:
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
    statuses = dict(collections.Counter(r["status"] for r in rows))
    summary = {
        "rows": len(rows), "unique_charts": len({r["md5"] for r in rows}), "compared": completed,
        "statuses": statuses,
        "scope": ("Captured OpenLR2 judgement calls vs BMZ LEGACY judge engine; not full GameSession"
                  if any(r.get("model") == "captured-openlr2-procnotetiming-arguments" for r in rows)
                  else "Recorded OpenLR2 P1/P2 judgements vs BMZ LEGACY judge engine; not full GameSession or newly executed OpenLR2"),
        "count_order": ["empty_poor", "poor", "bad", "good", "great", "pgreat"],
        "database_best_is_same_play": False,
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "runner_sha256": runner_sha256,
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    fields = ["profile", "md5", "title", "status", "database_best_ex", "reference_ex", "bmz_ex", "ex_delta",
              "sequence_equal", "chart_notes", "reference_notes", "reason", "chart_path", "replay_path", "trace_path"]
    with (output / "results.csv").open("w", newline="", encoding="utf-8-sig") as out:
        writer = csv.DictWriter(out, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)
    report = ["# LEGACY比較結果", "", f"対象: {len(rows)}件 / {summary['unique_charts']}譜面", "",
              "| 結果 | 件数 |", "|---|---:|"]
    report.extend(f"| {status} | {count} |" for status, count in sorted(statuses.items()))
    report.extend(["", summary["scope"],
                   "ゲージによる途中終了を含むGameSession全体は検証対象外。DBの自己ベストは別プレイの場合があるため合否判定に使用しない。",
                   "", "詳細はresults.csv / results.jsonl、対応範囲はtools/legacy-compare/README.mdを参照。", ""])
    (output / "report.md").write_text("\n".join(report), encoding="utf-8")
    print(json.dumps(summary, indent=2, ensure_ascii=True))


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--openlr2", type=Path, default=Path("D:/OpenLR2"))
    parser.add_argument("--runner", type=Path, default=repo / "target/debug/examples/legacy_compare.exe")
    parser.add_argument("--output", type=Path, default=repo / ".local/legacy-compare")
    parser.add_argument("--inventory-only", action="store_true")
    parser.add_argument("--profile")
    parser.add_argument("--md5")
    parser.add_argument("--poll-trace", type=Path, help="CSV recorded by the instrumented OpenLR2 client; MD5 is read from the capture")
    parser.add_argument("--limit", type=int)
    args = parser.parse_args()
    if args.limit is not None and args.limit < 0:
        parser.error("--limit must be non-negative")
    if args.poll_trace and not args.md5:
        with args.poll_trace.open(encoding="utf-8", newline="") as stream:
            hashes = {row["md5"].lower() for row in csv.DictReader(stream) if row["kind"] == "P"}
        if len(hashes) != 1:
            parser.error("capture must contain one chart; close OpenLR2 normally after one play")
        args.md5 = next(iter(hashes))
        if len(args.md5) != 32 or any(c not in '0123456789abcdef' for c in args.md5):
            parser.error("capture has no valid chart MD5; specify --md5 from an independently verified chart")
    run(args)


if __name__ == "__main__":
    main()
