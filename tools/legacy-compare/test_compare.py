import importlib.util
import json
from contextlib import closing
from pathlib import Path
import sqlite3
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("compare", Path(__file__).with_name("compare.py"))
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)


class InventoryTests(unittest.TestCase):
    def test_report_does_not_hide_a_judgement_order_difference(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)
            rows = [{"profile": "test", "md5": "1" * 32, "status": "match", "sequence_equal": False}]
            compare.write_reports(rows, output, 1)
            summary = json.loads((output / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["statuses"], {"judgement_sequence_mismatch": 1})
            saved = json.loads((output / "results.jsonl").read_text(encoding="utf-8"))
            self.assertEqual(saved["status"], "judgement_sequence_mismatch")

    def test_keeps_missing_rows_imported_scores_and_replay_only_charts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            dbdir = root / "LR2files/Database"
            (dbdir / "Score").mkdir(parents=True)
            replaydir = root / "LR2files/Replay/test"
            replaydir.mkdir(parents=True)
            chart = root / "chart.bms"
            chart.write_bytes(b"#TITLE test")
            hashes = [str(i) * 32 for i in range(1, 6)]
            with closing(sqlite3.connect(dbdir / "song.db")) as db:
                db.execute("CREATE TABLE song(hash,title,subtitle,path)")
                db.executemany("INSERT INTO song VALUES(?,?,?,?)",
                               [(h, "test", "", str(chart)) for h in hashes if h != hashes[2]])
                db.commit()
            with closing(sqlite3.connect(dbdir / "Score/test.db")) as db:
                db.execute("CREATE TABLE score(hash,perfect,great,totalnotes)")
                db.execute("CREATE TABLE imported_score(hash,perfect,great,totalnotes)")
                db.executemany("INSERT INTO score VALUES(?,?,?,?)", [(h, 10, 2, 12) for h in hashes[:3]])
                db.executemany("INSERT INTO imported_score VALUES(?,?,?,?)",
                               [(hashes[0], 99, 0, 99), (hashes[3], 2, 1, 3)])
                db.commit()
            for h in [hashes[0], hashes[4]]:
                (replaydir / (h + ".lr2rep")).write_bytes(b"dummy")
            rows = {r["md5"]: r for r in compare.inventory(root)}
            self.assertEqual(len(rows), 5)
            self.assertEqual(rows[hashes[0]]["database_best_ex"], 22)
            self.assertEqual(rows[hashes[1]]["status"], "missing_replay")
            self.assertEqual(rows[hashes[2]]["status"], "missing_chart")
            self.assertEqual(rows[hashes[3]]["database_best_ex"], 5)
            self.assertFalse(rows[hashes[4]]["has_score"])
            captured = compare.captured_chart_row(root, hashes[1])
            self.assertEqual(captured["status"], "ready")
            self.assertEqual(captured["chart_path"], str(chart))
            self.assertEqual(captured["replay_path"], "")
            with self.assertRaises(FileNotFoundError):
                compare.captured_chart_row(root, hashes[2])
            with compare.open_db(dbdir / "song.db") as db:
                with self.assertRaises(sqlite3.OperationalError):
                    db.execute("DELETE FROM song")


if __name__ == "__main__":
    unittest.main()
