"""Validate exact SRPM sets and rebuild using only extracted sources."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from manifest import check_inventory
from source_packages import check_srpm


def check_sources(source, manifest):
    records = manifest["srpms"]
    expected = {p["rpm"]["sourcerpm"] for p in manifest["rpm"]}
    if len(records) != len(expected) or {r["filename"] for r in records} != expected:
        raise ValueError("Missing/duplicate SRPM mapping")
    if json.loads((source / "srpms.json").read_text()) != records:
        raise ValueError("SRPM manifest mismatch")
    if {p.name for p in (source / "srpms").glob("*")} != expected:
        raise ValueError("Missing/unexpected SRPM files")
    for record in records:
        if Path(record["filename"]).name != record["filename"]:
            raise ValueError("Unsafe SRPM filename")
        owners = [p["rpm"] for p in manifest["rpm"]
                  if p["rpm"]["sourcerpm"] == record["filename"]]
        actual = check_srpm(source / "srpms" / record["filename"], owners, record["sha256"])
        if actual != record["header"]:
            raise ValueError("SRPM header manifest mismatch")


def verify(source, rebuild, result):
    assert not Path("/source").exists() and not Path("/opt/ffmpeg").exists()
    assert not Path("/opt/pipewire").exists()
    manifest = json.loads((source / "build-manifest.json").read_text())
    check_inventory(source, manifest["files"]["sources"])
    check_sources(source, manifest)
    with tempfile.TemporaryDirectory(prefix="bmz-source-check-") as temporary:
        work = Path(temporary)
        env = dict(os.environ, CARGO_HOME=str(work / "empty-cargo-home"))
        subprocess.run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1"],
                       cwd=source, env=env, stdout=subprocess.DEVNULL, check=True)
        print("PASS: exact SRPM headers, digests, payload extraction; empty-cache offline Cargo metadata", flush=True)
        if rebuild:
            env["BMZ_REBUILD_DIR"] = str(work / "rebuild")
            subprocess.run(["bash", str(source / "installer/linux-tar/rebuild.sh")], env=env, check=True)
            print("PASS: FFmpeg, PipeWire client and BMZ release rebuilt offline from source archive", flush=True)
        if result:
            if not rebuild:
                raise ValueError("A validation result requires the full rebuild")
            result.write_text(json.dumps(dict(status="passed", commit=manifest["commit"],
                                             os_release=Path("/etc/os-release").read_text())))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--rebuild", action="store_true")
    parser.add_argument("--result", type=Path)
    args = parser.parse_args()
    verify(args.source.resolve(), args.rebuild, args.result)
