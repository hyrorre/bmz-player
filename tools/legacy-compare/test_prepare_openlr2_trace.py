import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("prepare", Path(__file__).with_name("prepare_openlr2_trace.py"))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class InstrumentTests(unittest.TestCase):
    def test_adds_both_hooks_and_rejects_reapplication(self):
        source = (b"int ApplyJudgeNote(int judge, game *g, int _player, int lane, Timer *T, char isReplay) { return 0; }\n"
                  b"int ProcNoteOnTiming(game *g, int lane, int keypress, int timing, int player) { return 1; }\n")
        modified = prepare.instrument(source)
        self.assertIn(b"openlr2_parity::judgement(judge, lane);", modified)
        self.assertIn(b"openlr2_parity::Poll parity_poll", modified)
        self.assertIn(b"return 0;", modified)
        with self.assertRaises(ValueError):
            prepare.instrument(modified)

    def test_rejects_an_unrecognized_source_revision(self):
        with self.assertRaises(ValueError):
            prepare.instrument(b"int main() {}")


if __name__ == "__main__":
    unittest.main()
