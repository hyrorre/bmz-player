"""Exact RPM ownership and SRPM acquisition; never substitute newer sources."""

from collections import defaultdict
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
from urllib.parse import urlsplit, unquote

from manifest import sha256

RPM_FORMAT = "%{NAME}\t%{EPOCHNUM}\t%{VERSION}\t%{RELEASE}\t%{ARCH}\t%{SOURCERPM}\n"


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def header(text):
    rows = text.splitlines()
    if len(rows) != 1 or len(rows[0].split("\t")) != 6:
        raise ValueError(f"Expected one RPM owner/header: {text}")
    return dict(zip(("name", "epoch", "version", "release", "arch", "sourcerpm"),
                    rows[0].split("\t")))


def rpm_owner(library):
    # Query the real file, including both EL8 lib64 spellings. A SONAME
    # symlink alone can incorrectly identify a devel RPM as the owner.
    real = os.path.realpath(library)
    candidates = list(dict.fromkeys([real, real.removeprefix("/usr"),
                                    "/usr" + real if real.startswith("/lib64/") else real]))
    for candidate in candidates:
        result = subprocess.run(["rpm", "-qf", "--qf", RPM_FORMAT, candidate],
                                capture_output=True, text=True)
        if result.returncode == 0:
            owner = header(result.stdout)
            if owner["arch"] != "x86_64" or not owner["sourcerpm"].endswith(".src.rpm"):
                raise ValueError(f"Invalid binary RPM owner for {library}: {owner}")
            return owner
    raise ValueError(f"No RPM owner for {library} (resolved to {real})")


def nevra(owner):
    return f"{owner['name']}-{owner['epoch']}:{owner['version']}-{owner['release']}.{owner['arch']}"


def copy_notices(owner, destination):
    package = nevra(owner)
    # Some EL8 copyright notices are %doc instead of %license; retain both.
    licenses = output("rpm", "-q", "--licensefiles", package).splitlines()
    docs = output("rpm", "-q", "--docfiles", package).splitlines()
    if not licenses and not any(Path(name).name.partition(".")[0].lower() in
                                {"license", "licence", "copying", "copyright"} for name in docs):
        raise ValueError(f"No RPM license files for {package}")
    files = set(licenses + docs)
    # Minimal base images can have tsflags=nodocs. Restore notices from the
    # exact installed NEVRA, never from a newer binary package.
    if any(not Path(name).exists() for name in files):
        subprocess.run(["dnf", "-y", "--setopt=tsflags=", "reinstall", package], check=True)
    for name in sorted(files):
        path = Path(name)
        if not path.is_absolute() or not path.exists():
            raise ValueError(f"Missing RPM notice: {package}: {name}")
        target = destination / package / path.relative_to("/")
        target.parent.mkdir(parents=True, exist_ok=True)
        if path.is_dir():
            shutil.copytree(path, target, dirs_exist_ok=True)
        else:
            shutil.copy2(path, target)


def check_srpm(path, owners, digest=None):
    if digest is not None and sha256(path) != digest:
        raise ValueError(f"SRPM checksum mismatch: {path}")
    # Check header and compressed payload digests. Signature trust is separate.
    subprocess.run(["rpm", "--checksig", "--nosignature", str(path)], check=True)
    source = header(output("rpm", "-qp", "--qf", RPM_FORMAT, str(path)))
    expected = f"{source['name']}-{source['version']}-{source['release']}.src.rpm"
    # Source RPMs retain the build machine's ARCH (usually x86_64), not "src".
    # SOURCEPACKAGE distinguishes an SRPM from a renamed binary RPM.
    is_source = output("rpm", "-qp", "--qf", "%{SOURCEPACKAGE}", str(path))
    if is_source != "1" or path.name != expected:
        raise ValueError(f"Wrong SRPM header/name: {path}: {source}")
    for owner in owners:
        if (owner["sourcerpm"] != expected or
                any(owner[key] != source[key] for key in ("epoch", "version", "release"))):
            raise ValueError(f"Wrong SRPM version for {nevra(owner)}: {source}")
    algorithm = output("rpm", "-qp", "--qf", "%{FILEDIGESTALGO}", str(path))
    digest_algorithm = {"1": "md5", "2": "sha1", "8": "sha256", "9": "sha384", "10": "sha512"}.get(algorithm)
    if digest_algorithm is None:
        raise ValueError(f"Unsupported RPM file digest: {algorithm}")
    listing = output("rpm", "-qp", "--qf",
                     "[%{FILENAMES}\t%{FILEMODES}\t%{FILESIZES}\t%{FILEDIGESTS}\n]", str(path))
    files = {}
    for line in listing.splitlines():
        name, mode, size, digest = line.split("\t")
        if Path(name).name != name or name in (".", "..") or name in files or not stat.S_ISREG(int(mode)):
            raise ValueError(f"Unsafe SRPM file header: {name}")
        files[name] = (int(size), digest)
    # Extract each payload independently, including spec, patches and upstream
    # archives. SRPM payloads must consist of flat regular files.
    with tempfile.TemporaryDirectory() as temporary:
        payload = Path(temporary) / "payload.cpio"
        with payload.open("wb") as stream:
            subprocess.run(["rpm2cpio", str(path)], stdout=stream, check=True)
        with payload.open("rb") as stream:
            names = subprocess.check_output(["cpio", "-it", "--quiet"], stdin=stream,
                                            text=True).splitlines()
        if not names or not any(name.endswith(".spec") for name in names):
            raise ValueError(f"Missing SRPM spec/payload: {path}")
        if len(names) != len(files) or set(names) != files.keys():
            raise ValueError(f"Unsafe SRPM payload: {path}")
        target = Path(temporary) / "extracted"
        target.mkdir()
        with payload.open("rb") as stream:
            subprocess.run(["cpio", "-idm", "--quiet", "--no-absolute-filenames"],
                           stdin=stream, cwd=target, check=True)
        if any(not (target / name).is_file() or (target / name).is_symlink() for name in names):
            raise ValueError(f"Invalid SRPM payload files: {path}")
        for name, (size, digest) in files.items():
            file = target / name
            hasher = hashlib.new(digest_algorithm)
            with file.open("rb") as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b""):
                    hasher.update(block)
            if file.stat().st_size != size or hasher.hexdigest() != digest:
                raise ValueError(f"SRPM payload/header mismatch: {name}")
    return source


def download_sources(packages, source_root):
    groups = defaultdict(list)
    for package in packages:
        owner = package["rpm"]
        source = owner["sourcerpm"]
        if Path(source).name != source or not source.endswith(".src.rpm"):
            raise ValueError(f"Invalid SOURCERPM: {source}")
        groups[source].append(owner)
        print(f"RPM library {package['library']}: {nevra(owner)} -> {source}", flush=True)
    records = []
    for filename, owners in sorted(groups.items()):
        directory = source_root / "srpms"
        directory.mkdir(exist_ok=True)
        try:
            locations = output("dnf", "-q", "--showduplicates", "repoquery", "--enablerepo=*-source",
                               "--arch=src", "--location", filename[:-8]).splitlines()
            urls = sorted({url for url in locations if
                           unquote(urlsplit(url).path).rsplit("/", 1)[-1] == filename
                           and urlsplit(url).scheme == "https"})
            if not urls:
                raise ValueError(f"Exact SRPM unavailable: {filename}")
            path = directory / filename
            subprocess.run(["curl", "--fail", "--location", "--retry", "3",
                            urls[0], "-o", str(path)], check=True)
            source = check_srpm(path, owners)
            records.append(dict(filename=filename, url=urls[0], sha256=sha256(path), header=source))
        except (subprocess.CalledProcessError, OSError, ValueError) as error:
            raise RuntimeError(f"Failed exact SRPM {filename} for {owners}: {error}") from error
    (source_root / "srpms.json").write_text(json.dumps(records, indent=2) + "\n")
    return records
