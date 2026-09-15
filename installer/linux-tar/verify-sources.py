"""Validate exact Ubuntu source sets and rebuild using only extracted sources."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from manifest import check_inventory, sha256


def dsc_fields(path):
    # A .dsc can be clearsigned. Only parse its Debian control paragraph.
    fields = {}
    key = None
    for line in path.read_text().splitlines():
        if line.startswith("-----BEGIN PGP SIGNATURE"):
            break
        if line.startswith(" ") and key:
            fields[key] += "\n" + line.strip()
        elif re.match(r"^[A-Za-z0-9-]+:", line):
            key, value = line.split(":", 1)
            fields[key] = value.strip()
        else:
            key = None
    return fields


def check_dsc(path, package, version):
    fields = dsc_fields(path)
    if fields.get("Source") != package or fields.get("Version") != version:
        raise ValueError(f"Wrong source package/version: {path}")
    entries = fields["Checksums-Sha256"].strip().splitlines()
    if not entries:
        raise ValueError(f"Empty source checksums: {path}")
    for entry in entries:
        digest, size, name = entry.split()
        if Path(name).name != name:
            raise ValueError(f"Unsafe .dsc filename: {name}")
        target = path.parent / name
        if target.stat().st_size != int(size) or sha256(target) != digest:
            raise ValueError(f"Source checksum mismatch: {target}")


def verify(source, rebuild):
    manifest = json.loads((source / "build-manifest.json").read_text())
    check_inventory(source, manifest["files"]["sources"])
    with tempfile.TemporaryDirectory(prefix="bmz-source-check-") as temporary:
        work = Path(temporary)
        for package, version in sorted({(p["source_package"], p["source_version"])
                                        for p in manifest["ubuntu"]}):
            descriptors = list((source / "ubuntu" / package).glob("*.dsc"))
            if len(descriptors) != 1:
                raise ValueError(f"Expected one .dsc for {package}={version}")
            check_dsc(descriptors[0], package, version)
            # Extract separately to keep peak disk usage bounded. dpkg-source
            # validates all reference checksums and applies the Debian patches.
            with tempfile.TemporaryDirectory(dir=work) as unpack:
                subprocess.run(["dpkg-source", "--require-strong-checksums", "-x", str(descriptors[0]),
                                str(Path(unpack) / "source")], check=True)
        env = dict(os.environ, CARGO_HOME=str(work / "empty-cargo-home"))
        subprocess.run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1"],
                       cwd=source, env=env, stdout=subprocess.DEVNULL, check=True)
        print("PASS: exact Ubuntu sources, .dsc extraction, empty-cache offline Cargo metadata", flush=True)
        if rebuild:
            env["BMZ_REBUILD_DIR"] = str(work / "rebuild")
            subprocess.run(["bash", str(source / "installer/linux-tar/rebuild.sh")], env=env, check=True)
            print("PASS: FFmpeg and BMZ release rebuilt offline from the source archive", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--rebuild", action="store_true")
    args = parser.parse_args()
    verify(args.source.resolve(), args.rebuild)
