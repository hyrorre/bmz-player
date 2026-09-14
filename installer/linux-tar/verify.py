"""Exercise the extracted, read-only package as an unprivileged desktop user."""

import os
import json
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess


def run(*args, **kwargs):
    print("+", *map(str, args), flush=True)
    return subprocess.run(args, check=True, text=True, **kwargs)


package = Path("/opt/BMZ Player")
launcher = package / "bmz-player"
assert not os.access(package, os.W_OK), "Package must be mounted read-only"
assert os.getuid() != 0, "Verify as an unprivileged user"
glibc = re.compile(r"^lib(c|m|pthread|dl|rt|resolv|util)\.so\.[0-9]+$")
for binary in [package / "bin/bmz-player", *sorted((package / "lib").iterdir())]:
    result = run("ldd", "-r", str(binary), capture_output=True).stdout
    print(result, flush=True)
    assert "not found" not in result
    assert "undefined symbol" not in result
    for name, path in re.findall(r"\s*(\S+) => (.+?) \(", result):
        assert Path(path).resolve().parent == package / "lib" or glibc.fullmatch(name), result

# Negative control: the runtime image must not supply a missing FFmpeg library.
isolated = Path.home() / "without-libraries/bin"
isolated.mkdir(parents=True)
shutil.copy2(package / "bin/bmz-player", isolated / "bmz-player")
result = run("ldd", str(isolated / "bmz-player"), capture_output=True).stdout
assert re.search(r"libavcodec\.so\.\d+ => not found", result), result

home = Path.home()
cwd = home / "caller with spaces"
cwd.mkdir()
os.chdir(cwd)
(cwd / "data").mkdir()  # Must not capture packaged user state via cwd/data.
run(str(launcher), "--help")
assert not (home / ".local/share/bmz-player").exists()
sample = package / "resources/songs/sample-playable"
shutil.copytree(sample, cwd / "relative songs")
run(str(launcher), "songs", "add", "./relative songs")
run(str(launcher), "songs", "load", "./relative songs")
data = home / ".local/share/bmz-player"
assert (data / "config.toml").is_file()
assert (home / ".cache/bmz-player").is_dir()
assert (home / ".local/state/bmz-player/logs").is_dir()
assert not list((cwd / "data").iterdir())
with sqlite3.connect(data / "library.db") as db:
    assert db.execute("SELECT COUNT(*) FROM charts").fetchone()[0] > 0

# Real startup decodes the packaged skin/font/sample on software Vulkan and a
# null PulseAudio sink. This tests packaging, not hardware performance or sound.
run("pulseaudio", "--start", "--exit-idle-time=-1")
run("pactl", "load-module", "module-null-sink")
trace = home / "resource-access.trace"
run("timeout", "120", "xvfb-run", "-a", "strace", "-f", "-e", "trace=openat",
    "-o", str(trace), str(launcher), "--boot-play-sample", "--autoplay-on-start",
    "--renderer", "vulkan", "--smoke-exit-after-play-frames", "3")
opened = [line for line in trace.read_text().splitlines() if re.search(r"= [0-9]+$", line)]
for resource in ("skins/default/play7.json", "NotoSansCJK-Regular.ttc", "sample-playable.bms", "key.wav"):
    assert any(str(package / "resources") in line and resource in line for line in opened), resource
assert (data / "profiles/default/profile.toml").is_file()
assert (data / "profiles/default/score.db").is_file()

effective = run(str(launcher), "./relative songs/sample-playable.bms", "--print-effective-options",
                capture_output=True).stdout
assert json.loads(effective)["chart"] == "./relative songs/sample-playable.bms"

run(str(launcher), "songs", "add", "./relative songs",
    env=dict(os.environ, BMZ_DATA_DIR="./data only"))
assert (cwd / "data only/config.toml").is_file()
assert (cwd / "data only/cache").is_dir()
assert (cwd / "data only/logs").is_dir()

# Explicit relative BMZ overrides must stay relative to the caller, including resources.
os.symlink(package / "resources", cwd / "alternate resources")
overrides = dict(os.environ, BMZ_RESOURCE_DIR="./alternate resources",
                 BMZ_DATA_DIR="./custom data", BMZ_CACHE_DIR="./custom cache",
                 BMZ_LOGS_DIR="./custom logs")
run(str(launcher), "songs", "add", "./relative songs", env=overrides)
assert (cwd / "custom data/config.toml").is_file()
assert (cwd / "custom cache").is_dir()
assert (cwd / "custom logs").is_dir()
shutil.copytree(data / "profiles", cwd / "custom data/profiles")
effective = run(str(launcher), "--boot-play-sample", "--print-effective-options",
                env=overrides, capture_output=True).stdout
assert "alternate resources" in effective, effective

# A symlinked launcher and XDG defaults are also part of the public entry point.
link = cwd / "launch link"
link.symlink_to(launcher)
xdg = dict(os.environ, XDG_DATA_HOME=str(home / "xdg-data"),
           XDG_CACHE_HOME=str(home / "xdg-cache"), XDG_STATE_HOME=str(home / "xdg-state"))
run(str(link), "songs", "add", "./relative songs", env=xdg)
assert (home / "xdg-data/bmz-player/config.toml").is_file()
assert (home / "xdg-cache/bmz-player").is_dir()
assert (home / "xdg-state/bmz-player/logs").is_dir()

print("PASS: ELF closure, CLI, cwd/overrides, read-only resources, user storage, packaged play")
