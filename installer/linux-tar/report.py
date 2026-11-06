"""Emit upload paths only after all checks and the offline rebuild succeed."""

import json
import os
from pathlib import Path
import sys

from manifest import sha256

runtime, sources, metadata, validation = map(Path, sys.argv[1:])
manifest = json.loads(metadata.read_text())
for variable, field in (("BMZ_VERSION", "version"), ("BMZ_RELEASE_COMMIT", "commit")):
    if os.environ.get(variable) and os.environ[variable] != manifest[field]:
        raise ValueError(f"{variable} does not match verified archives")
results = {name: json.loads((validation / f"{name}.json").read_text())
           for name in ("almalinux", "ubuntu", "sources")}
runtime_sha = sha256(runtime)
for name, result in results.items():
    if result["status"] != "passed" or result["commit"] != manifest["commit"]:
        raise ValueError(f"Missing/failed validation: {name}")
    if name != "sources":
        if result["runtime_sha256"] != runtime_sha or result["elf_count"] <= 0:
            raise ValueError(f"Wrong runtime validated: {name}")
        if tuple(map(int, result["max_glibc"].split("."))) > (2, 28):
            raise ValueError(f"GLIBC baseline exceeded: {name}")
    expected = ("ubuntu", "22.04") if name == "ubuntu" else ("almalinux", "8.10")
    os_fields = dict(line.split("=", 1) for line in result["os_release"].splitlines() if "=" in line)
    if tuple(os_fields[key].strip('"') for key in ("ID", "VERSION_ID")) != expected:
        raise ValueError(f"Wrong validation OS: {name}")
# The inventory has been checked against the extracted runtime, after patchelf.
client_manifest = runtime.parent / f"bmz-player-v{manifest['version']}-linux-x64-tar-client-manifest.json"
client_manifest.write_text(json.dumps({
    "schema_version": 1, "client": "bmz-player", "version": manifest["version"],
    "git_commit": manifest["commit"], "target": "linux-x64-tar",
    "executable": "bmz-player",
    "client_hash": manifest["files"]["runtime"]["bin/bmz-player"]["sha256"],
}, indent=2) + "\n")
summary = f"## Verified Linux archives\n\nVersion: {manifest['version']}\n\nCommit: `{manifest['commit']}`\n\n"
summary += "| File | Bytes | SHA256 |\n| --- | ---: | --- |\n"
for archive in (runtime, sources):
    summary += f"| {archive.name} | {archive.stat().st_size} | `{sha256(archive)}` |\n"
summary += "\n| Runtime OS | Result | ELF count | Maximum required GLIBC |\n| --- | --- | ---: | --- |\n"
for name in ("almalinux", "ubuntu"):
    result = results[name]
    summary += f"| {name} | {result['status']} | {result['elf_count']} | {result['max_glibc']} |\n"
summary += "\nPassed: same runtime SHA256 on AlmaLinux 8.10 and Ubuntu 22.04; all ELF version providers and ldd -r; read-only unprivileged offline runtime checks, PulseAudio null sink, missing PipeWire server, software Vulkan sample autoplay, CLI/paths/XDG; exact SRPM headers/digests/payload extraction; empty-cache Cargo metadata; offline FFmpeg/PipeWire/BMZ release rebuild on clean AlmaLinux 8.10.\n\n"
summary += "Not tested: real hardware kernel/GPU/Wayland/audio latency/controllers; rebuilding all RPM libraries; byte-identical binary reproducibility. Containers share the host kernel.\n"
(runtime.parent / "validation.md").write_text(summary)
print(summary)
if os.environ.get("GITHUB_STEP_SUMMARY"):
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as stream:
        stream.write(summary)
if os.environ.get("GITHUB_OUTPUT"):
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        stream.write(f"runtime={runtime}\nsources={sources}\nchecksums={runtime.parent / 'SHA256SUMS.txt'}\n")
        stream.write(f"client_manifest={client_manifest}\n")
