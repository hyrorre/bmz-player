"""Negative controls for corresponding-source packaging, without a full build."""

import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

from archives import LIMIT, checksums, extract, verify_checksums
from manifest import check_pair, inventory, sha256

spec = importlib.util.spec_from_file_location("verify_sources", Path(__file__).with_name("verify-sources.py"))
verify_sources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify_sources)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.runtime = self.root / "runtime"
        self.source = self.root / "sources"
        for root in (self.runtime, self.source):
            root.mkdir()
        self.write(self.source, "Cargo.toml", '[workspace.package]\nversion = "0.4.0"\n')
        self.write(self.source, "Cargo.lock", "lockfile")
        self.write(self.source, "BUILD-COMMIT", "a" * 40)
        self.write(self.source, "crates/example.rs", "original source")
        self.write(self.source, "data/skins/skin/file", "original skin")
        self.snapshot = inventory(self.source)
        self.write(self.source, "vendor/dependency/Cargo.toml", "dependency")
        self.write(self.source, "ffmpeg/ffmpeg-9.0.1.tar.xz", "ffmpeg source")
        self.write(self.source, "ffmpeg/configure.txt", "url=example\nsha256=example\n./configure --enable-shared\n")
        self.write(self.runtime, "resources/licenses/ffmpeg-build.txt",
                   (self.source / "ffmpeg/configure.txt").read_text())
        self.write(self.runtime, "resources/licenses/ubuntu/packages.json", "[]")
        self.write(self.runtime, "bin/bmz-player", "binary")
        self.manifest = {
            "schema": 1, "commit": "a" * 40, "version": "0.4.0",
            "cargo_lock_sha256": sha256(self.source / "Cargo.lock"),
            "snapshot": self.snapshot, "ubuntu": [],
            "ffmpeg": {"version": "9.0.1", "sha256": sha256(self.source / "ffmpeg/ffmpeg-9.0.1.tar.xz"),
                       "configure": "./configure --enable-shared"},
            "files": {"runtime": inventory(self.runtime), "sources": inventory(self.source)},
        }
        self.save_manifest()

    def write(self, root, name, content):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def save_manifest(self):
        for root in (self.runtime, self.source):
            (root / "build-manifest.json").write_text(json.dumps(self.manifest))

    def test_valid_pair(self):
        check_pair(self.runtime, self.source)

    def test_different_commit_pair(self):
        other = dict(self.manifest, commit="b" * 40)
        (self.source / "build-manifest.json").write_text(json.dumps(other))
        with self.assertRaisesRegex(ValueError, "manifests do not match"):
            check_pair(self.runtime, self.source)

    def test_missing_vendor(self):
        (self.source / "vendor/dependency/Cargo.toml").unlink()
        with self.assertRaisesRegex(ValueError, "vendor/dependency"):
            check_pair(self.runtime, self.source)

    def test_source_tamper_even_if_inventory_regenerated(self):
        self.write(self.source, "crates/example.rs", "changed source")
        self.manifest["files"]["sources"] = inventory(self.source, ("build-manifest.json",))
        self.save_manifest()
        with self.assertRaisesRegex(ValueError, "crates/example.rs"):
            check_pair(self.runtime, self.source)

    def test_skin_tamper(self):
        self.write(self.source, "data/skins/skin/file", "changed skin")
        with self.assertRaisesRegex(ValueError, "data/skins"):
            check_pair(self.runtime, self.source)

    def test_runtime_tamper(self):
        self.write(self.runtime, "bin/bmz-player", "changed binary")
        with self.assertRaisesRegex(ValueError, "bin/bmz-player"):
            check_pair(self.runtime, self.source)

    def test_directory_symlink_is_inventoried(self):
        (self.source / "skin-link").symlink_to("data/skins", target_is_directory=True)
        self.assertEqual(inventory(self.source)["skin-link"], {"symlink": "data/skins"})

    def test_dsc_references(self):
        upstream = self.write(self.source, "ubuntu/example/example.tar.xz", "upstream")
        dsc = self.write(self.source, "ubuntu/example/example.dsc",
                         f"Source: example\nVersion: 1:2.3-4\nChecksums-Sha256:\n {sha256(upstream)} 8 example.tar.xz\n")
        verify_sources.check_dsc(dsc, "example", "1:2.3-4")
        with self.assertRaisesRegex(ValueError, "Wrong source"):
            verify_sources.check_dsc(dsc, "example", "2.3-5")
        upstream.write_text("tampered")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            verify_sources.check_dsc(dsc, "example", "1:2.3-4")
        upstream.unlink()
        with self.assertRaises(FileNotFoundError):
            verify_sources.check_dsc(dsc, "example", "1:2.3-4")

    def test_size_limit(self):
        archive = self.root / "oversize.tar.gz"
        with archive.open("wb") as stream:
            stream.truncate(LIMIT)
        with self.assertRaisesRegex(ValueError, "below 2 GiB"):
            checksums([archive])

    def test_release_checksums_include_other_platforms(self):
        pair = "aaa  runtime.tar.gz\nbbb  sources.tar.gz\n"
        verify_checksums(pair, "ccc  windows.zip\n" + pair + "ddd  client-manifest.json\n")
        for invalid in ("aaa  runtime.tar.gz\n", pair.replace("bbb", "bad"),
                        pair + "bbb  sources.tar.gz\n"):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                verify_checksums(pair, invalid)

    def test_extract_rejects_traversal(self):
        archive = self.root / "unsafe.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            member = tarfile.TarInfo("../outside")
            member.size = 1
            tar.addfile(member, io.BytesIO(b"x"))
        with self.assertRaisesRegex(ValueError, "Unsafe archive path"):
            extract(archive, self.root / "extracted")


if __name__ == "__main__":
    unittest.main()
