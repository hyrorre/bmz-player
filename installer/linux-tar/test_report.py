"""Release identity and final ELF hash exported after archive verification."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.runtime = self.root / "bmz-player-v0.4.0-linux-x64.tar.gz"
        self.sources = self.root / "bmz-player-v0.4.0-linux-x64-sources.tar.gz"
        self.runtime.write_bytes(b"runtime archive, not the executable")
        self.sources.write_bytes(b"source archive")
        self.metadata = self.root / "verified.json"
        self.metadata.write_text(json.dumps({
            "version": "0.4.0", "commit": "a" * 40,
            "files": {"runtime": {"bin/bmz-player": {"sha256": "b" * 64}}},
        }))
        self.env = dict(os.environ, BMZ_VERSION="0.4.0", BMZ_RELEASE_COMMIT="a" * 40,
                        GITHUB_OUTPUT=str(self.root / "outputs"),
                        GITHUB_STEP_SUMMARY=str(self.root / "summary"))

    def run_report(self):
        return subprocess.run([
            sys.executable, str(Path(__file__).with_name("report.py")),
            str(self.runtime), str(self.sources), str(self.metadata),
        ], env=self.env, capture_output=True, text=True)

    def test_exports_verified_executable_hash_and_identity(self):
        result = self.run_report()
        self.assertEqual(result.returncode, 0, result.stderr)
        paths = dict(line.split("=", 1) for line in (self.root / "outputs").read_text().splitlines())
        manifest = json.loads(Path(paths["client_manifest"]).read_text())
        self.assertEqual(manifest["client_hash"], "b" * 64)
        self.assertEqual(manifest["git_commit"], "a" * 40)
        self.assertEqual(manifest["target"], "linux-x64-tar")
        self.assertEqual(manifest["executable"], "bmz-player")
        self.assertEqual(Path(paths["runtime"]), self.runtime)
        self.assertEqual(Path(paths["sources"]), self.sources)

    def test_wrong_release_identity_produces_no_upload_outputs(self):
        for key, value in (("BMZ_VERSION", "0.5.0"), ("BMZ_RELEASE_COMMIT", "c" * 40)):
            with self.subTest(key=key):
                previous = self.env[key]
                self.env[key] = value
                result = self.run_report()
                self.env[key] = previous
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(key, result.stderr)
                self.assertFalse((self.root / "outputs").exists())
                self.assertEqual(list(self.root.glob("*-client-manifest.json")), [])

    def test_manual_workflow_needs_no_release_environment(self):
        del self.env["BMZ_VERSION"]
        del self.env["BMZ_RELEASE_COMMIT"]
        result = self.run_report()
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
