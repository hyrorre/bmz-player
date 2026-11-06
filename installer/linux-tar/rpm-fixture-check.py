"""Integration check using real rpmbuild/rpm/rpm2cpio/cpio, without downloads."""

from pathlib import Path
import subprocess
import tempfile

from manifest import sha256
from source_packages import check_srpm


with tempfile.TemporaryDirectory(prefix="bmz-rpm-fixture-") as directory:
    root = Path(directory)
    for name in ("SPECS", "SOURCES", "SRPMS", "BUILD", "RPMS"):
        (root / name).mkdir()
    (root / "SOURCES/example.txt").write_text("BMZ source verification fixture\n")
    spec = root / "SPECS/example.spec"
    spec.write_text("""Name: bmz-source-fixture
Epoch: 1
Version: 1.2
Release: 3
Summary: BMZ SRPM verification fixture
License: GPL-3.0-only
Source0: example.txt
%description
Self-contained source verification fixture.
""")
    subprocess.run(["rpmbuild", "-bs", "--define", f"_topdir {root}", str(spec)], check=True)
    path = root / "SRPMS/bmz-source-fixture-1.2-3.src.rpm"
    owner = dict(name="bmz-source-fixture-libs", epoch="1", version="1.2",
                 release="3", arch="x86_64", sourcerpm=path.name)
    check_srpm(path, [owner, owner], sha256(path))
    for key, value in (("epoch", "0"), ("version", "1.3"), ("release", "4"),
                       ("sourcerpm", "other-1.2-3.src.rpm")):
        try:
            check_srpm(path, [dict(owner, **{key: value})], sha256(path))
        except ValueError:
            pass
        else:
            raise AssertionError(f"Wrong {key} was accepted")
    data = bytearray(path.read_bytes())
    data[-20] ^= 255
    path.write_bytes(data)
    try:
        check_srpm(path, [owner], sha256(path))
    except (ValueError, subprocess.CalledProcessError):
        pass
    else:
        raise AssertionError("Corrupt RPM payload accepted with a regenerated archive SHA256")
print("PASS: real SRPM header/payload extraction, wrong epoch/version/release/source rejection, corrupt payload rejection")
