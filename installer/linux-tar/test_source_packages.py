"""RPM ownership, exact acquisition and offline SRPM rejection controls."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from manifest import sha256
from source_packages import check_srpm, download_sources, header, rpm_owner

spec = importlib.util.spec_from_file_location("verify_sources", Path(__file__).with_name("verify-sources.py"))
verify_sources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify_sources)


class SourcePackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.owner = dict(name="systemd-libs", epoch="0", version="239", release="82.el8",
                          arch="x86_64", sourcerpm="systemd-239-82.el8.src.rpm")
        self.one = dict(library="libudev.so.1", rpm=self.owner)
        self.two = dict(library="libsystemd.so.0", rpm=self.owner)
        self.source_header = dict(self.owner, name="systemd", sourcerpm="(none)")
        self.url = "https://repo.example/" + self.owner["sourcerpm"]

    def download(self, packages=None, location=None):
        def fetch(args, **kwargs):
            if args[0] == "curl":
                Path(args[-1]).write_bytes(b"source rpm")
        with patch("source_packages.output", return_value=location or self.url) as query, \
                patch("source_packages.subprocess.run", side_effect=fetch) as run, \
                patch("source_packages.check_srpm", return_value=self.source_header) as check:
            records = download_sources(packages or [self.one, self.two], self.root)
        return records, query, run, check

    def test_one_exact_download_for_shared_source(self):
        records, query, run, check = self.download()
        self.assertEqual(len(records), 1)
        query.assert_called_once_with("dnf", "-q", "--showduplicates", "repoquery", "--enablerepo=*-source",
                                      "--arch=src", "--location", "systemd-239-82.el8")
        self.assertEqual(run.call_count, 1)
        self.assertEqual(records[0]["url"], self.url)
        self.assertEqual(records[0]["sha256"], sha256(self.root / "srpms" / self.owner["sourcerpm"]))
        self.assertEqual(len(check.call_args.args[1]), 2)

    def test_newer_version_is_never_a_fallback(self):
        with self.assertRaisesRegex(RuntimeError, "Exact SRPM unavailable"):
            self.download(location="https://repo.example/systemd-239-99.el8.src.rpm")

    def test_unavailable_source_fails_with_owners(self):
        with patch("source_packages.output", return_value=self.url), \
                patch("source_packages.subprocess.run", side_effect=OSError("offline")):
            with self.assertRaisesRegex(RuntimeError, "systemd-239-82.el8.src.rpm") as caught:
                download_sources([self.one, self.two], self.root)
        self.assertIn("systemd-libs", str(caught.exception))
        self.assertIsInstance(caught.exception.__cause__, OSError)

    def test_owner_queries_real_file_and_lib64_alias(self):
        text = "systemd-libs\t0\t239\t82.el8\tx86_64\tsystemd-239-82.el8.src.rpm\n"
        with patch("source_packages.os.path.realpath", return_value="/usr/lib64/libudev.so.1.6.11"), \
                patch("source_packages.subprocess.run", side_effect=[
                    subprocess.CompletedProcess([], 1, "", ""),
                    subprocess.CompletedProcess([], 0, text, "")]) as run:
            self.assertEqual(rpm_owner("/lib64/libudev.so.1"), self.owner)
        self.assertEqual(run.call_args.args[0][-1], "/lib64/libudev.so.1.6.11")

    def test_unknown_or_ambiguous_owner_fails(self):
        with patch("source_packages.subprocess.run", return_value=subprocess.CompletedProcess([], 1, "", "")):
            with self.assertRaisesRegex(ValueError, "No RPM owner"):
                rpm_owner("/lib64/libmissing.so")
        with self.assertRaises(ValueError):
            header("one\ntwo")

    def test_wrong_header_and_corruption_are_rejected(self):
        path = self.root / self.owner["sourcerpm"]
        path.write_bytes(b"source rpm")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            check_srpm(path, [self.owner], "0" * 64)
        with patch("source_packages.subprocess.run") as run, \
                patch("source_packages.output", return_value="systemd\t0\t239\t99.el8\tsrc\t(none)"):
            with self.assertRaisesRegex(ValueError, "Wrong SRPM"):
                check_srpm(path, [self.owner])
            self.assertEqual(run.call_count, 1)
        with patch("source_packages.subprocess.run", side_effect=subprocess.CalledProcessError(1, ["rpm"])):
            with self.assertRaises(subprocess.CalledProcessError):
                check_srpm(path, [self.owner], sha256(path))

    def test_missing_duplicate_mismatched_and_damaged_sources(self):
        filename = self.owner["sourcerpm"]
        (self.root / "srpms").mkdir()
        path = self.root / "srpms" / filename
        path.write_bytes(b"source rpm")
        record = dict(filename=filename, sha256=sha256(path), url=self.url, header=self.source_header)
        manifest = dict(rpm=[self.one, self.two], srpms=[record])
        (self.root / "srpms.json").write_text(json.dumps([record]))
        with patch.object(verify_sources, "check_srpm", return_value=self.source_header) as check:
            verify_sources.check_sources(self.root, manifest)
            self.assertEqual(check.call_count, 1)
        with self.assertRaisesRegex(ValueError, "Missing/duplicate"):
            verify_sources.check_sources(self.root, dict(manifest, srpms=[record, record]))
        path.unlink()
        with self.assertRaisesRegex(ValueError, "Missing/unexpected"):
            verify_sources.check_sources(self.root, manifest)
        path.write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            verify_sources.check_sources(self.root, manifest)
        path.write_bytes(b"source rpm")
        with patch.object(verify_sources, "check_srpm", return_value=dict(self.source_header, epoch="1")):
            with self.assertRaisesRegex(ValueError, "header manifest mismatch"):
                verify_sources.check_sources(self.root, manifest)


if __name__ == "__main__":
    unittest.main()
