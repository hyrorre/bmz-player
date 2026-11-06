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
        self.validation = self.root / "validation"
        self.validation.mkdir()
        from manifest import sha256
        for name in ("almalinux", "ubuntu", "sources"):
            os_release = 'ID=almalinux\nVERSION_ID="8.10"\n' if name != "ubuntu" else 'ID=ubuntu\nVERSION_ID="22.04"\n'
            (self.validation / f"{name}.json").write_text(json.dumps(dict(
                status="passed", commit="a" * 40, runtime_sha256=sha256(self.runtime),
                elf_count=10, max_glibc="2.28", os_release=os_release)))
        self.env = dict(os.environ, BMZ_VERSION="0.4.0", BMZ_RELEASE_COMMIT="a" * 40,
                        GITHUB_OUTPUT=str(self.root / "outputs"),
                        GITHUB_STEP_SUMMARY=str(self.root / "summary"))

    def run_report(self):
        return subprocess.run([
            sys.executable, str(Path(__file__).with_name("report.py")),
            str(self.runtime), str(self.sources), str(self.metadata), str(self.validation),
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

    def test_missing_os_or_rebuild_result_cannot_upload(self):
        for name in ("almalinux", "ubuntu", "sources"):
            path = self.validation / f"{name}.json"
            content = path.read_text()
            path.unlink()
            self.assertNotEqual(self.run_report().returncode, 0)
            self.assertFalse((self.root / "outputs").exists())
            path.write_text(content)

    def test_different_archive_or_new_glibc_cannot_upload(self):
        path = self.validation / "ubuntu.json"
        original = json.loads(path.read_text())
        for key, value in (("runtime_sha256", "0" * 64), ("max_glibc", "2.29"),
                           ("os_release", 'ID=ubuntu\nVERSION_ID="24.04"\n'),
                           ("status", "failed")):
            path.write_text(json.dumps(dict(original, **{key: value})))
            self.assertNotEqual(self.run_report().returncode, 0)
            self.assertFalse((self.root / "outputs").exists())


if __name__ == "__main__":
    unittest.main()
