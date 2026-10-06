"""Exercise the real player build script in a small, offline Cargo workspace."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest


REPO = Path(__file__).resolve().parents[1]


@unittest.skipUnless(sys.platform == "linux", "fixture omits Windows/macOS native resources")
class BuildIdentityTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bmz-build-identity-")
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.root = self.directory / "source"
        self.env = os.environ.copy()
        self.env.pop("BMZ_BUILD_COMMIT_OVERRIDE", None)
        self.env.pop("BMZ_SPARKLE_DIR", None)
        self.env.update({
            "CARGO_TARGET_DIR": str(self.directory / "target"),
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "Build identity test",
            "GIT_AUTHOR_EMAIL": "test@example.invalid",
            "GIT_COMMITTER_NAME": "Build identity test",
            "GIT_COMMITTER_EMAIL": "test@example.invalid",
        })
        crate = self.root / "crates/bmz-player"
        (crate / "src").mkdir(parents=True)
        (crate / "native").mkdir()
        (crate / "native/sparkle.m").touch()
        (self.root / "assets/app-icon").mkdir(parents=True)
        (self.root / "assets/app-icon/bmz-player.ico").touch()
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/bmz-player"]\nresolver = "3"\n'
        )
        dependencies = tomllib.loads(
            (REPO / "crates/bmz-player/Cargo.toml").read_text()
        )["build-dependencies"]
        (crate / "Cargo.toml").write_text(
            '[package]\nname = "build-identity-fixture"\nversion = "0.1.0"\n'
            'edition = "2024"\n[build-dependencies]\n'
            + "\n".join(f"{key} = {json.dumps(value)}" for key, value in dependencies.items())
            + "\n"
        )
        shutil.copyfile(REPO / "crates/bmz-player/build.rs", crate / "build.rs")
        (crate / "src/main.rs").write_text(
            'fn main() { println!("{}", env!("BMZ_BUILD_COMMIT")); }\n'
        )
        # Keep the build dependencies at the repository's locked versions.
        shutil.copyfile(REPO / "Cargo.lock", self.root / "Cargo.lock")
        self.run_command(["cargo", "metadata", "--offline", "--format-version=1"])

    def run_command(self, args, root=None, env=None):
        result = subprocess.run(
            args, cwd=root or self.root, env=env or self.env,
            text=True, capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout.strip()

    def git(self, *args, root=None):
        return self.run_command(["git", *args], root=root)

    def init_git(self):
        self.git("init", "-b", "main")
        self.git("add", ".")
        return self.commit()

    def commit(self, root=None):
        self.git("commit", "--allow-empty", "--no-gpg-sign", "-m", "fixture", root=root)
        return self.git("rev-parse", "HEAD", root=root)

    def assert_build(self, expected, *, fresh, root=None, override=None):
        env = self.env.copy()
        if override is not None:
            env["BMZ_BUILD_COMMIT_OVERRIDE"] = override
        output = self.run_command(
            ["cargo", "build", "--offline", "--locked", "--message-format=json"],
            root=root, env=env,
        )
        artifacts = [
            entry for line in output.splitlines()
            if (entry := json.loads(line)).get("reason") == "compiler-artifact"
            and entry.get("executable")
        ]
        self.assertEqual(len(artifacts), 1, output)
        artifact = artifacts[0]
        self.assertEqual(self.run_command([artifact["executable"]]), expected)
        self.assertEqual(artifact["fresh"], fresh, output)

    def test_checkout_tracks_loose_packed_detached_and_dirty_revisions(self):
        initial = self.init_git()
        self.assert_build(initial, fresh=False)
        self.assert_build(initial, fresh=True)
        next_commit = self.commit()
        self.assert_build(next_commit, fresh=False)
        self.assert_build(next_commit, fresh=True)

        # A packed branch has no loose ref file until its next commit.
        self.git("pack-refs", "--all", "--prune")
        self.assert_build(next_commit, fresh=False)
        self.assert_build(next_commit, fresh=True)
        next_commit = self.commit()
        self.assert_build(next_commit, fresh=False)
        self.assert_build(next_commit, fresh=True)

        self.git("checkout", "--detach", initial)
        self.assert_build(initial, fresh=False)
        self.assert_build(initial, fresh=True)
        source = self.root / "crates/bmz-player/src/main.rs"
        source.write_text(source.read_text() + "// local edit\n")
        self.assert_build(initial + "-dirty", fresh=False)
        self.assert_build(initial + "-dirty", fresh=True)

    def test_linked_worktree_tracks_its_own_head(self):
        initial = self.init_git()
        worktree = self.directory / "worktree"
        self.git("worktree", "add", "-b", "feature/test", str(worktree))
        self.assertTrue((worktree / ".git").is_file())
        self.assert_build(initial, fresh=False, root=worktree)
        self.assert_build(initial, fresh=True, root=worktree)
        next_commit = self.commit(root=worktree)
        self.assert_build(next_commit, fresh=False, root=worktree)
        self.assert_build(next_commit, fresh=True, root=worktree)
        self.assertEqual(self.git("rev-parse", "HEAD"), initial)
        self.git("checkout", "--detach", initial, root=worktree)
        self.assert_build(initial, fresh=False, root=worktree)
        self.assert_build(initial, fresh=True, root=worktree)
        detached_commit = self.commit(root=worktree)
        self.assert_build(detached_commit, fresh=False, root=worktree)
        self.assert_build(detached_commit, fresh=True, root=worktree)

    def test_archive_tracks_manifest_and_override(self):
        manifest = self.root / "BUILD-COMMIT"
        manifest.write_text("archive-one\n")
        self.assert_build("archive-one", fresh=False)
        self.assert_build("archive-one", fresh=True)
        manifest.write_text("archive-two\n")
        self.assert_build("archive-two", fresh=False)
        self.assert_build("archive-two", fresh=True)
        self.assert_build("override", fresh=False, override="override")
        self.assert_build("override", fresh=True, override="override")
        self.assert_build("archive-two", fresh=False)

    def test_archive_without_identity_is_reused(self):
        self.assert_build("unknown", fresh=False)
        self.assert_build("unknown", fresh=True)


if __name__ == "__main__":
    unittest.main()
