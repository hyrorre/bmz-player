"""Check distribution file limits/checksums, extract safely, and match content."""

import argparse
import json
from pathlib import Path
import tarfile

from manifest import check_pair, sha256

LIMIT = 2_147_483_648


def checksums(archives):
    rows = []
    for archive in archives:
        size = archive.stat().st_size
        if not 0 < size < LIMIT:
            raise ValueError(f"Archive must be below 2 GiB: {archive} ({size} bytes)")
        digest = sha256(archive)
        print(f"{archive.name}: {size} bytes, SHA256 {digest}", flush=True)
        rows.append(f"{digest}  {archive.name}\n")
    return "".join(rows)


def extract(archive, destination):
    destination.mkdir()
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            target = destination / member.name
            if not target.resolve().is_relative_to(destination.resolve()):
                raise ValueError(f"Unsafe archive path: {member.name}")
            if member.issym():
                if not (target.parent / member.linkname).resolve().is_relative_to(destination.resolve()):
                    raise ValueError(f"Unsafe archive link: {member.name}")
            elif not (member.isfile() or member.isdir()):
                raise ValueError(f"Unsupported archive entry: {member.name}")
            # Python 3.10 on Ubuntu 22.04 has no extraction filter. The explicit
            # checks above constrain paths, symlinks and entry types.
            tar.extract(member, destination)
    roots = list(destination.iterdir())
    if len(roots) != 1 or not roots[0].is_dir() or roots[0].is_symlink():
        raise ValueError("Expected a single archive root")
    return roots[0]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("runtime", type=Path)
    parser.add_argument("sources", type=Path)
    parser.add_argument("--write-checksums", action="store_true")
    parser.add_argument("--extract-to", type=Path)
    args = parser.parse_args()
    sums = checksums([args.runtime, args.sources])
    path = args.runtime.parent / "SHA256SUMS.txt"
    if args.write_checksums:
        path.write_text(sums)
    elif path.read_text() != sums:
        raise ValueError("SHA256SUMS.txt does not match the supplied archive pair")
    if args.extract_to:
        runtime = extract(args.runtime, args.extract_to / "runtime")
        sources = extract(args.sources, args.extract_to / "sources")
        manifest = check_pair(runtime, sources)
        name = f"bmz-player-v{manifest['version']}-linux-x64"
        if args.runtime.name != name + ".tar.gz" or args.sources.name != name + "-sources.tar.gz":
            raise ValueError("Archive filenames do not match manifest version")
        if runtime.name != name or sources.name != name + "-sources":
            raise ValueError("Archive root names do not match manifest version")
        print(f"PASS: both content inventories and build identity {manifest['commit']}", flush=True)
        (args.extract_to / "verified.json").write_text(json.dumps(manifest))


if __name__ == "__main__":
    main()
