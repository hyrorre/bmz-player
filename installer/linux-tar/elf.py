"""Inspect every shipped ELF, its version needs and actual ldd providers."""

from pathlib import Path
import re
import subprocess

HOST_GLIBC = re.compile(r"^(ld-linux-x86-64\.so\.2|lib(c|m|pthread|dl|rt|resolv|util)\.so\.[0-9]+)$")


def version_needs(text):
    needs = {}
    library = None
    active = False
    for line in text.splitlines():
        if line.startswith("Version "):
            active = line.startswith("Version needs section")
        if not active:
            continue
        match = re.search(r"File: (\S+)", line)
        if match:
            library = match[1]
            needs.setdefault(library, set())
        match = re.search(r"Name: (\S+)", line)
        if match and library:
            needs[library].add(match[1])
    return needs


def glibc_max(needs):
    versions = [(0,)]
    for names in needs.values():
        for name in names:
            if name.startswith("GLIBC_"):
                value = name.removeprefix("GLIBC_")
                if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", value):
                    raise ValueError(f"Unsupported GLIBC requirement: {name}")
                version = tuple(map(int, value.split(".")))
                if version > (2, 28):
                    raise ValueError(f"GLIBC_2.28 baseline exceeded: {name}")
                versions.append(version)
    return max(versions)


def definitions(text):
    section = text.split("Version definition section", 1)
    if len(section) == 1:
        return set()
    return set(re.findall(r"Name: (\S+)", section[1].split("Version needs section", 1)[0]))


def inspect(package):
    maximum = (0,)
    count = 0
    checked = set()
    for binary in sorted(package.rglob("*")):
        if not binary.is_file() or binary.resolve() in checked:
            continue
        with binary.open("rb") as stream:
            if stream.read(4) != b"\x7fELF":
                continue
        checked.add(binary.resolve())
        count += 1
        def readelf(path):
            return subprocess.check_output(["readelf", "--version-info", "--wide", str(path)], text=True)
        needs = version_needs(readelf(binary))
        maximum = max(maximum, glibc_max(needs))
        result = subprocess.run(["ldd", "-r", str(binary)], capture_output=True, text=True, check=True)
        text = result.stdout + result.stderr
        print(f"ELF {binary.relative_to(package)}\n{text}", flush=True)
        if any(error in text for error in ("not found", "undefined symbol", "version lookup error")):
            raise ValueError(f"Unresolved ELF: {binary}: {text}")
        providers = dict(re.findall(r"\s*(\S+) => (.+?) \(", text))
        # ldd prints the loader as an absolute path without an arrow.
        for path in re.findall(r"^\s*(/\S+) \(", text, re.M):
            providers[Path(path).name] = path
        for name, path in providers.items():
            if not HOST_GLIBC.fullmatch(name) and not Path(path).resolve().is_relative_to(package / "lib"):
                raise ValueError(f"Dependency escaped bundle: {binary}: {name}: {path}")
        for library, symbols in needs.items():
            if library not in providers:
                raise ValueError(f"Missing version provider: {binary}: {library}")
            provided = definitions(readelf(providers[library]))
            missing = symbols - provided
            if missing:
                raise ValueError(f"Missing symbol versions in {providers[library]} for {binary}: {missing}")
    if not count:
        raise ValueError("No ELF files inspected")
    return dict(elf_count=count, max_glibc=".".join(map(str, maximum)))
