"""Bundle the ELF dependency closure with RPM notices and exact SRPMs."""

import os
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

from source_packages import copy_notices, download_sources, nevra, rpm_owner


def output(*args):
    return subprocess.check_output(args, text=True)


package = Path(sys.argv[1])
source_root = Path(sys.argv[2])
# glibc and its loader must come from the host as a unit. GPU/display drivers
# loaded at runtime also belong to the host; no Mesa/NVIDIA implementation ships.
glibc = re.compile(r"^(ld-linux-x86-64\.so\.2|lib(c|m|pthread|dl|rt|resolv|util)\.so\.[0-9]+)$")
listing = output("ldd", str(package / "bin/bmz-player"))
plugins = list((package / "lib/pipewire-0.3").glob("*.so")) + list((package / "lib/spa-0.2").rglob("*.so"))
for plugin in plugins:
    listing += output("ldd", str(plugin))
    relative = os.path.relpath(package / "lib", plugin.parent)
    subprocess.run(["patchelf", "--set-rpath", "$ORIGIN/" + relative, str(plugin)], check=True)
if "not found" in listing:
    raise RuntimeError(listing)
notices = package / "resources/licenses/rpm"
notices.mkdir()
provenance = []
packages = []
custom = {}
for plugin in plugins:
    custom[plugin.relative_to(package).as_posix()] = "pipewire"
seen = set()
for line in listing.splitlines():
    match = re.match(r"\s*(\S+) => (/\S+)", line)
    if not match:
        continue
    name, library = match.groups()
    if glibc.fullmatch(name):
        continue
    if name in seen:
        continue
    seen.add(name)
    shutil.copy2(library, package / "lib" / name)
    subprocess.run(["patchelf", "--set-rpath", "$ORIGIN", str(package / "lib" / name)], check=True)
    if library.startswith("/opt/ffmpeg/"):
        provenance.append(f"{name}\tFFmpeg (see ffmpeg-build.txt)")
        custom["lib/" + name] = "ffmpeg"
        continue
    if library.startswith("/opt/pipewire/"):
        provenance.append(f"{name}\tPipeWire client (see pipewire-build.txt)")
        custom["lib/" + name] = "pipewire"
        continue
    owner = rpm_owner(library)
    copy_notices(owner, notices)
    provenance.append(f"{name}\t{nevra(owner)}\t{owner['sourcerpm']}")
    packages.append(dict(library=name, path=library, realpath=os.path.realpath(library), rpm=owner))

download_sources(packages, source_root)
(notices / "packages.txt").write_text("\n".join(provenance) + "\n")
(notices / "packages.json").write_text(json.dumps(packages, indent=2) + "\n")
(notices / "custom-libraries.json").write_text(json.dumps(custom, indent=2) + "\n")
subprocess.run(["patchelf", "--set-rpath", "$ORIGIN/../lib", str(package / "bin/bmz-player")], check=True)
