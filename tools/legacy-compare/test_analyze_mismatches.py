import importlib.util
from pathlib import Path
import unittest
import csv
import tempfile

spec = importlib.util.spec_from_file_location('analyze', Path(__file__).with_name('analyze_mismatches.py'))
analyze = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyze)


class AlignmentTests(unittest.TestCase):
    def test_global_assignment_avoids_greedy_trap(self):
        self.assertEqual(analyze.assignment([[1, 2], [2, 100]]), {0: 1, 1: 0})

    def test_same_frame_order_is_not_mistaken_for_grade_changes(self):
        left = [{'ms': 100, 'grade': 5}, {'ms': 100, 'grade': 4}]
        right = [{'ms': 101, 'grade': 4}, {'ms': 101, 'grade': 5}]
        aligned = list(analyze.align(left, right))
        self.assertTrue(all(a['grade'] == b['grade'] for a, b, _ in aligned))
        self.assertTrue(all(ambiguous for _, _, ambiguous in aligned))

    def test_distant_judgements_stay_unpaired(self):
        pairs = list(analyze.align([{'ms': 100, 'grade': 5}], [{'ms': 200, 'grade': 2}]))
        self.assertEqual(len(pairs), 2)
        self.assertTrue(all(a is None or b is None for a, b, _ in pairs))

    def test_bad_recursion_into_ln_has_direct_trace_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'trace.csv'
            with path.open('w', newline='') as stream:
                writer = csv.writer(stream)
                writer.writerow(['index', 'bmz_ms', 'reference_ms', 'bmz_judge', 'reference_judge',
                                 'bmz_lane', 'bmz_note_id', 'delta_us'])
                writer.writerow([0, 100, 100, 2, 2, 7, 0, 150000])
                writer.writerow([1, 1001, 100, 4, 4, 7, 1, -37000])
            windows = dict(pgreat_us=21000, great_us=60000, good_us=120000,
                           bad_slow_us=200000, bad_fast_us=200000, empty_poor_fast_us=1000000)
            diagnostic = {'windows': windows, 'notes': [
                dict(id=0, lane=7, kind='Tap', time_us=-50000, tail_us=None),
                dict(id=1, lane=7, kind='LongStart', time_us=137000, tail_us=1000000)]}
            row = dict(md5='test', title='test', ex_delta=0, rank_percent=100, seed=1, trace_path=str(path))
            result = analyze.analyze(row, diagnostic)
            self.assertEqual(result['assessment'], 'implementation_path_difference_confirmed')
            self.assertEqual(result['categories']['bad_recursion_into_ln_confirmed'], 2)


if __name__ == '__main__':
    unittest.main()
