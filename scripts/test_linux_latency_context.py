import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("context", Path(__file__).with_name("linux-latency-context.py"))
context = importlib.util.module_from_spec(spec)
spec.loader.exec_module(context)


class ContextTests(unittest.TestCase):
    def test_flatpak_host_pid_selects_client_and_its_nodes_without_guessing_quantum(self):
        def obj(id, **props):
            return {"id": id, "info": {"props": props}}
        objects = [obj(9, **{"application.process.id": 2, "pipewire.sec.pid": 4321}),
                   obj(10, **{"client.id": 9, "node.latency": "128/48000"}),
                   obj(11, **{"client.id": 8}), obj(12, **{"application.process.id": 888})]
        nodes = context.select_nodes(objects, 4321)
        self.assertEqual([o["id"] for o in nodes], [9, 10])
        self.assertEqual(nodes[1]["props"]["node.latency"], "128/48000")
        self.assertNotIn("driver_quantum_frames", nodes[1])

    def test_missing_tool_is_unknown_instead_of_zero(self):
        self.assertIsNone(context.query(["/bmz-nonexistent-command"])["value"])

    def test_cpal_pulse_process_name_is_matched_exactly(self):
        inputs = [{"properties": {"application.name": "cpal-pulseaudio-123"}},
                  {"properties": {"application.name": "cpal-pulseaudio-1234"}},
                  {"properties": {"application.process.id": "123"}}]
        self.assertEqual(context.select_pulse_inputs(inputs, 123), [inputs[0], inputs[2]])


if __name__ == "__main__":
    unittest.main()
