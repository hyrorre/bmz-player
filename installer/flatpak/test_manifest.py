"""Distribution guardrails; runtime socket access is verified separately."""
from pathlib import Path
import unittest


class ManifestTests(unittest.TestCase):
    def test_native_pipewire_keeps_pulse_and_narrow_permissions(self):
        manifest = Path(__file__).with_name("net.hyrorre.BMZPlayer.yml").read_text()
        for value in ("--features pulseaudio,pipewire", "--locked", "--socket=pulseaudio",
                      "--filesystem=xdg-run/pipewire-0", "--device=input", "--require-version=1.15.6"):
            self.assertIn(value, manifest)
        for value in ("--socket=pipewire", "--device=all", "--filesystem=host",
                      "--socket=system-bus", "--socket=session-bus", "PIPEWIRE_QUANTUM",
                      "pipewire-0-manager"):
            self.assertNotIn(value, manifest)

    def test_launcher_does_not_force_backend_or_quantum(self):
        launcher = Path(__file__).with_name("bmz-player-flatpak").read_text()
        self.assertNotIn("PIPEWIRE_QUANTUM", launcher)
        self.assertNotIn("PIPEWIRE_LATENCY", launcher)
        self.assertIn('exec /app/bin/bmz-player "$@"', launcher)


if __name__ == "__main__":
    unittest.main()
