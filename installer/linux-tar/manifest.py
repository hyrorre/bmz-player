"""Build identity and content inventories shared by the two Linux archives."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def inventory(root, exclude=()):
    result = {}
    for path in sorted(root.rglob("*")):
        name = path.relative_to(root).as_posix()
        if name in exclude:
            continue
        if path.is_symlink():
            result[name] = {"symlink": os.readlink(path)}
        elif path.is_dir():
            continue
        else:
            result[name] = {"sha256": sha256(path), "size": path.stat().st_size,
                            "executable": bool(path.stat().st_mode & 0o111)}
    return result


def check_inventory(root, expected, exact=True):
    actual = inventory(root, ("build-manifest.json",))
    for name, entry in expected.items():
        if actual.get(name) != entry:
            raise ValueError(f"Content mismatch: {root / name}")
    if exact and actual.keys() != expected.keys():
        raise ValueError(f"Unexpected files in {root}: {actual.keys() - expected.keys()}")


def check_snapshot(root, snapshot):
    check_inventory(root, {k: v for k, v in snapshot.items()
                           if k != ".cargo/config.toml"}, exact=False)
    if ".cargo/config.toml" in snapshot:
        original = snapshot[".cargo/config.toml"]
        content = (root / ".cargo/config.toml").read_bytes()[:original["size"]]
        if hashlib.sha256(content).hexdigest() != original["sha256"]:
            raise ValueError("Original Cargo configuration was not preserved")


def version(root):
    section = (root / "Cargo.toml").read_text().split("[workspace.package]", 1)[1].split("[", 1)[0]
    return re.search(r'^version\s*=\s*"([^"]+)"', section, re.M)[1]


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def create(runtime, source):
    snapshot = json.loads((source / "BUILD-SNAPSHOT.json").read_text())
    # The only committed file allowed to change is Cargo config, which gains
    # vendor replacement tables. Its original content remains in the snapshot.
    check_snapshot(source, snapshot)
    ffmpeg = (source / "ffmpeg/configure.txt").read_text()
    manifest = {
        "schema": 1, "version": version(source),
        "commit": (source / "BUILD-COMMIT").read_text().strip(),
        "submodules": [dict(commit=line.split()[0], path=line.split()[1])
                       for line in (source / "BUILD-SUBMODULES").read_text().splitlines()],
        "target": "x86_64-unknown-linux-gnu", "default_features": False,
        "features": ["pulseaudio"],
        "build_command": "cargo build --locked --release -p bmz-player --no-default-features --features pulseaudio",
        "rebuild_command": "cargo build --offline --locked --release -p bmz-player --no-default-features --features pulseaudio",
        "cargo_lock_sha256": sha256(source / "Cargo.lock"),
        "toolchain": {tool: command(*args) for tool, args in {
            "rust": ["rustc", "-Vv"], "cargo": ["cargo", "-V"],
            "cc": ["cc", "--version"], "cxx": ["c++", "--version"],
            "glibc": ["ldd", "--version"]}.items()},
        "os_release": Path("/etc/os-release").read_text(),
        "build_image": os.environ["BMZ_BUILD_IMAGE_ID"],
        "ffmpeg": {"version": re.search(r"ffmpeg-([0-9.]+)\.tar", ffmpeg)[1],
                   "url": ffmpeg.splitlines()[0].split("=", 1)[1],
                   "sha256": ffmpeg.splitlines()[1].split("=", 1)[1],
                   "configure": ffmpeg.splitlines()[2]},
        "ubuntu": json.loads((runtime / "resources/licenses/ubuntu/packages.json").read_text()),
        "snapshot": snapshot,
        "files": {"runtime": inventory(runtime), "sources": inventory(source)},
    }
    encoded = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    for root in (runtime, source):
        (root / "build-manifest.json").write_text(encoded)


def check_pair(runtime, source):
    manifest = json.loads((source / "build-manifest.json").read_text())
    if (runtime / "build-manifest.json").read_bytes() != (source / "build-manifest.json").read_bytes():
        raise ValueError("Runtime/source manifests do not match")
    if manifest["schema"] != 1 or not re.fullmatch(r"[0-9a-f]{40}", manifest["commit"]):
        raise ValueError("Invalid build identity")
    for kind, root in (("runtime", runtime), ("sources", source)):
        check_inventory(root, manifest["files"][kind])
    if version(source) != manifest["version"] or sha256(source / "Cargo.lock") != manifest["cargo_lock_sha256"]:
        raise ValueError("Cargo version/lockfile mismatch")
    check_snapshot(source, manifest["snapshot"])
    if (source / "BUILD-COMMIT").read_text().strip() != manifest["commit"]:
        raise ValueError("Source commit mismatch")
    ffmpeg = manifest["ffmpeg"]
    if sha256(source / "ffmpeg" / f"ffmpeg-{ffmpeg['version']}.tar.xz") != ffmpeg["sha256"]:
        raise ValueError("FFmpeg source mismatch")
    if (source / "ffmpeg/configure.txt").read_bytes() != (runtime / "resources/licenses/ffmpeg-build.txt").read_bytes():
        raise ValueError("FFmpeg configuration mismatch")
    if (source / "ffmpeg/configure.txt").read_text().splitlines()[2] != ffmpeg["configure"]:
        raise ValueError("FFmpeg manifest configuration mismatch")
    if json.loads((runtime / "resources/licenses/ubuntu/packages.json").read_text()) != manifest["ubuntu"]:
        raise ValueError("Ubuntu manifest mismatch")
    return manifest


if __name__ == "__main__":
    if sys.argv[1] == "snapshot":
        root = Path(sys.argv[2])
        (root / "BUILD-SNAPSHOT.json").write_text(json.dumps(inventory(root), sort_keys=True) + "\n")
    elif sys.argv[1] == "create":
        create(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        raise SystemExit("Usage: manifest.py snapshot SOURCE | create RUNTIME SOURCE")
