"""Checks for exact Ubuntu source selection and failure diagnostics."""

import contextlib
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from source_packages import download_sources


def library(name, binary, binary_version, source, source_version):
    return dict(library=name, binary_package=binary, binary_version=binary_version,
                source_package=source, source_version=source_version)


class SourcePackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.one = library("libudev.so.1", "libudev1:amd64", "1.2", "systemd", "1.2")
        self.two = library("libsystemd.so.0", "libsystemd0:amd64", "1.2", "systemd", "1.2")

    @patch("source_packages.subprocess.run")
    def test_one_exact_download_for_shared_source(self, run):
        log = io.StringIO()
        with contextlib.redirect_stdout(log):
            download_sources([self.one, self.two], self.root)
        run.assert_called_once_with(
            ["apt-get", "source", "--download-only", "systemd=1.2"],
            cwd=self.root / "ubuntu" / "systemd", check=True,
        )
        self.assertIn("libudev.so.1: libudev1:amd64=1.2 -> systemd=1.2", log.getvalue())
        self.assertIn("libsystemd.so.0: libsystemd0:amd64=1.2 -> systemd=1.2", log.getvalue())

    @patch("source_packages.subprocess.run")
    def test_unavailable_source_fails_with_all_owners_and_no_fallback(self, run):
        run.side_effect = subprocess.CalledProcessError(100, ["apt-get"])
        with self.assertRaises(RuntimeError) as caught:
            download_sources([self.one, self.two], self.root)
        message = str(caught.exception)
        for expected in ("systemd=1.2", "libudev.so.1", "libudev1:amd64=1.2",
                         "libsystemd.so.0", "libsystemd0:amd64=1.2", "100"):
            self.assertIn(expected, message)
        self.assertEqual(run.call_count, 1)
        self.assertIsInstance(caught.exception.__cause__, subprocess.CalledProcessError)

    @patch("source_packages.subprocess.run")
    def test_spawn_error_reports_source_and_preserves_cause(self, run):
        run.side_effect = OSError("network helper unavailable")
        with self.assertRaises(RuntimeError) as caught:
            download_sources([self.one], self.root)
        self.assertIn("systemd=1.2", str(caught.exception))
        self.assertIn("network helper unavailable", str(caught.exception))
        self.assertIsInstance(caught.exception.__cause__, OSError)

    @patch("source_packages.subprocess.run")
    def test_multiple_versions_fail_before_any_download(self, run):
        older = library("libsystemd.so.0", "libsystemd0:amd64", "1.1", "systemd", "1.1")
        unrelated = library("libother.so", "other:amd64", "2", "other", "2")
        with self.assertRaises(ValueError) as caught:
            download_sources([unrelated, self.one, older], self.root)
        message = str(caught.exception)
        for expected in ("Multiple versions of Ubuntu source systemd", "libudev.so.1",
                         "libudev1:amd64=1.2", "libsystemd.so.0", "libsystemd0:amd64=1.1"):
            self.assertIn(expected, message)
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
