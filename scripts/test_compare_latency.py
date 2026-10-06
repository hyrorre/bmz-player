import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("compare", Path(__file__).with_name("compare-latency.py"))
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)


class CompareTests(unittest.TestCase):
    def test_streams_epochs_and_input_generations_are_not_combined(self):
        import json
        samples = [
            {"kind": "audio", "stream": {"id": 1}, "epoch": 0, "count": 2},
            {"kind": "audio", "stream": {"id": 1}, "epoch": 0, "count": 3},
            {"kind": "audio", "stream": {"id": 1}, "epoch": 1, "count": 0},
            {"kind": "audio", "stream": {"id": 2}, "epoch": 0, "count": 1},
            {"kind": "input_queue", "generation": 1},
            {"kind": "input_queue", "generation": 2},
        ]
        result = compare.read_samples(["prefix BMZ_LATENCY_JSON " + json.dumps(s) for s in samples])
        self.assertEqual(len(result["segments"]), 5)
        self.assertEqual(result["segments"][0]["count"], 3)
        self.assertEqual(result["segments"][1]["count"], 0)

    def test_malformed_data_is_counted(self):
        self.assertEqual(compare.read_samples(["BMZ_LATENCY_JSON {}", "normal log"])["rejected_lines"], 1)

    def test_synthetic_window_probe_keeps_epochs_and_physical_unknowns(self):
        import json
        samples = [
            {"kind": "window_event_loop_probe", "epoch": 1, "synthetic": True,
             "enqueue_to_dispatch_ns": {"count": 5}, "physical_press_to_output_ns": None},
            {"kind": "window_event_loop_probe", "epoch": 1, "synthetic": True,
             "enqueue_to_dispatch_ns": {"count": 10}, "physical_press_to_output_ns": None},
            {"kind": "window_event_loop_probe", "epoch": 2, "synthetic": True,
             "enqueue_to_dispatch_ns": {"count": 0}, "physical_press_to_output_ns": None},
        ]
        result = compare.read_samples(["BMZ_LATENCY_JSON " + json.dumps(s) for s in samples])
        self.assertEqual([s["enqueue_to_dispatch_ns"]["count"] for s in result["segments"]], [10, 0])
        self.assertIsNone(result["segments"][0]["physical_press_to_output_ns"])


if __name__ == "__main__":
    unittest.main()
