"""Emit upload paths only after all checks and the offline rebuild succeed."""

import json
import os
from pathlib import Path
import sys

from manifest import sha256

runtime, sources, metadata = map(Path, sys.argv[1:])
manifest = json.loads(metadata.read_text())
for variable, field in (("BMZ_VERSION", "version"), ("BMZ_RELEASE_COMMIT", "commit")):
    if os.environ.get(variable) and os.environ[variable] != manifest[field]:
        raise ValueError(f"{variable} does not match verified archives")
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
summary += "\nPassed: size/checksum and content matching; isolated runtime smoke; exact Ubuntu sources and extraction; empty-cache Cargo metadata; offline FFmpeg/BMZ release rebuild.\n\n"
summary += "Not tested: real GPU/Wayland/audio latency/controllers; rebuilding all Ubuntu libraries; byte-identical binary reproducibility.\n"
(runtime.parent / "validation.md").write_text(summary)
print(summary)
if os.environ.get("GITHUB_STEP_SUMMARY"):
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as stream:
        stream.write(summary)
if os.environ.get("GITHUB_OUTPUT"):
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        stream.write(f"runtime={runtime}\nsources={sources}\nchecksums={runtime.parent / 'SHA256SUMS.txt'}\n")
        stream.write(f"client_manifest={client_manifest}\n")
