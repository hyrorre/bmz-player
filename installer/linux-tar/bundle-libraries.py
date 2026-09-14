"""Bundle the ELF dependency closure, with Ubuntu notices and exact source packages."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


def output(*args):
    return subprocess.check_output(args, text=True)


package = Path(sys.argv[1])
# glibc and its loader must come from the host as a unit. GPU/display drivers
# loaded at runtime also belong to the host; no Mesa/NVIDIA implementation ships.
glibc = re.compile(r"^(ld-linux-x86-64\.so\.2|lib(c|m|pthread|dl|rt|resolv|util)\.so\.[0-9]+)$")
listing = output("ldd", str(package / "bin/bmz-player"))
if "not found" in listing:
    raise RuntimeError(listing)
sources = set()
notices = package / "resources/licenses/ubuntu"
notices.mkdir()
shutil.copytree("/usr/share/common-licenses", notices / "common-licenses")
provenance = []
for line in listing.splitlines():
    match = re.match(r"\s*(\S+) => (/\S+)", line)
    if not match:
        continue
    name, library = match.groups()
    if glibc.fullmatch(name):
        continue
    shutil.copy2(library, package / "lib" / name)
    subprocess.run(["patchelf", "--set-rpath", "$ORIGIN", str(package / "lib" / name)], check=True)
    if library.startswith("/opt/ffmpeg/"):
        provenance.append(f"{name}\tFFmpeg (see ffmpeg-build.txt)")
        continue
    # dpkg may record the pre-usrmerge spelling of a library path.
    candidates = [library, os.path.realpath(library)]
    candidates += [p.removeprefix("/usr") for p in candidates if p.startswith("/usr/")]
    owner = None
    for candidate in candidates:
        result = subprocess.run(["dpkg-query", "-S", candidate], capture_output=True, text=True)
        if result.returncode == 0:
            owner = result.stdout.split(": ", 1)[0]
            break
    if owner is None:
        raise RuntimeError(f"No source package owner for {library}")
    source, version, binary_version = output(
        "dpkg-query", "-W", "-f=${source:Package}\t${source:Version}\t${Version}", owner
    ).split("\t")
    sources.add((source, version))
    copyright_path = Path("/usr/share/doc") / owner.split(":")[0] / "copyright"
    shutil.copy2(copyright_path, notices / (owner.replace(":", "_") + ".txt"))
    provenance.append(f"{name}\t{owner}={binary_version}\t{source}={version}")

for source, version in sorted(sources):
    directory = package / "sources" / source
    directory.mkdir()
    subprocess.run(
        ["apt-get", "source", "--download-only", f"{source}={version}"],
        cwd=directory, check=True,
    )
(notices / "packages.txt").write_text("\n".join(provenance) + "\n")
subprocess.run(["patchelf", "--set-rpath", "$ORIGIN/../lib", str(package / "bin/bmz-player")], check=True)
