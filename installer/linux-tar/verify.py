"""Exercise the extracted, read-only package as an unprivileged desktop user."""

import os
import json
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import select
import sys

from elf import inspect


def run(*args, **kwargs):
    print("+", *map(str, args), flush=True)
    try:
        return subprocess.run(args, check=True, text=True, **kwargs)
    except subprocess.CalledProcessError as error:
        # Captured output is otherwise lost from the CI log on failure.
        print(error.stdout or "", error.stderr or "", sep="\n", flush=True)
        raise


package = Path("/opt/BMZ Player")
launcher = package / "bmz-player"
assert not os.access(package, os.W_OK), "Package must be mounted read-only"
assert os.getuid() != 0, "Verify as an unprivileged user"
assert not (package / "sources").exists()
assert not Path("/source").exists() and not Path("/archives/sources").exists()
for required in ("build-manifest.json", "README.md", "resources/licenses/BMZ-GPL-3.0-only.txt",
                 "resources/licenses/third-party-notices.txt", "resources/licenses/rust-dependency-licenses.txt",
                 "resources/licenses/ffmpeg-build.txt", "resources/licenses/NotoSansCJK-OFL-1.1.txt",
                 "resources/licenses/pipewire-build.txt", "resources/licenses/PipeWire-MIT.txt",
                 "resources/pipewire/client.conf"):
    assert (package / required).is_file(), required
assert {p.name for p in package.iterdir()} == {"bmz-player", "bin", "lib", "resources", "README.md", "build-manifest.json"}
elf_result = inspect(package)

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
# Client library/modules must load even without a PipeWire server.
missing = subprocess.run([str(launcher), "audio-probe", "pipewire", "128", "48000", "3"],
                         env=dict(os.environ, PIPEWIRE_REMOTE="bmz-test-missing-server"),
                         capture_output=True, text=True, timeout=15)
assert missing.returncode != 0 and '"kind":"audio_open_failure"' in missing.stdout, missing
failures = [json.loads(line.split("BMZ_LATENCY_JSON ", 1)[1]) for line in missing.stdout.splitlines()
            if '"kind":"audio_open_failure"' in line]
assert failures[-1]["phase"] == "server_or_host_initialization", missing
assert "error while loading shared libraries" not in missing.stderr, missing
sample = package / "resources/songs/sample-playable"
data = home / ".local/share/bmz-player"

# Real startup decodes the packaged skin/font/sample on software Vulkan and a
# null PulseAudio sink. This tests packaging, not hardware performance or sound.
# cpal's PulseAudio client finds the socket only via PULSE_SERVER/PULSE_RUNTIME_PATH/
# XDG_RUNTIME_DIR, unlike libpulse; a container has no login session to set it.
runtime_dir = home / "runtime"
runtime_dir.mkdir(mode=0o700)
os.environ["XDG_RUNTIME_DIR"] = str(runtime_dir)
run("pulseaudio", "--start", "--exit-idle-time=-1")
run("pactl", "load-module", "module-null-sink")
# A null sink can stall the first callbacks; a >1 s gap restarts the 2 s timing warmup.
probe = run(str(launcher), "audio-probe", "pulse", "256", "48000", "8",
            env=dict(os.environ, BMZ_LATENCY_DIAGNOSTICS="1"), capture_output=True).stdout
print(probe, flush=True)
samples = [json.loads(line.split("BMZ_LATENCY_JSON ", 1)[1]) for line in probe.splitlines()
           if "BMZ_LATENCY_JSON " in line]
assert samples[-1]["stream"]["actual_host"] == "PulseAudio", samples[-1]
assert samples[-1]["rendered_frames"] > 0, samples[-1]
assert samples[-1]["frames"]["count"] > 0, samples[-1]
trace = home / "resource-access.trace"
# Xvfb's displayfd works on both distributions; EL8 does not ship xvfb-run.
# Select lavapipe explicitly so the test cannot depend on a physical GPU.
icds = list(Path("/usr/share/vulkan/icd.d").glob("lvp_icd*.json"))
assert len(icds) == 1, icds
os.environ["VK_ICD_FILENAMES"] = str(icds[0])
with subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1280x720x24",
                       "-nolisten", "tcp", "-ac"], stdout=subprocess.PIPE, text=True) as xvfb:
    try:
        assert select.select([xvfb.stdout], [], [], 15)[0], "Xvfb startup timeout"
        display = xvfb.stdout.readline().strip()
        assert display.isdigit(), f"Xvfb did not start: {display}"
        run("timeout", "120", "strace", "-f", "-e", "trace=openat",
            "-e", "status=successful", "-o", str(trace), str(launcher),
            "--boot-play-sample", "--autoplay-on-start", "--renderer", "vulkan",
            "--smoke-exit-after-play-frames", "3",
            env=dict(os.environ, DISPLAY=":" + display))
    finally:
        xvfb.terminate()
        xvfb.wait(timeout=10)
opened = [line for line in trace.read_text().splitlines() if re.search(r"= [0-9]+$", line)]
for resource in ("skins/default/play7.json", "NotoSansCJK-Regular.ttc", "sample-playable.bms", "key.wav"):
    assert any(str(package / "resources") in line and resource in line for line in opened), resource
assert (data / "profiles/default/profile.toml").is_file()
assert (data / "profiles/default/score.db").is_file()
assert (data / "config.toml").is_file()
assert (home / ".cache/bmz-player").is_dir()
assert (home / ".local/state/bmz-player/logs").is_dir()
assert not list((cwd / "data").iterdir())

# Use a different chart hash so library deduplication cannot redirect packaged
# sample playback to the caller's copy (or hide a failed relative-path scan).
shutil.copytree(sample, cwd / "relative songs")
with (cwd / "relative songs/sample-playable.bms").open("a") as chart:
    chart.write("\n#TITLE Relative path smoke\n")
run(str(launcher), "songs", "add", "./relative songs")
run(str(launcher), "songs", "load", "./relative songs")
with sqlite3.connect(data / "library.db") as db:
    assert db.execute("SELECT COUNT(*) FROM charts").fetchone()[0] == 2

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
shutil.copytree(data / "profiles", cwd / "custom data/profiles", dirs_exist_ok=True)
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
if len(sys.argv) == 2:
    manifest = json.loads((package / "build-manifest.json").read_text())
    Path(sys.argv[1]).write_text(json.dumps(dict(
        status="passed", commit=manifest["commit"], runtime_sha256=os.environ["BMZ_RUNTIME_SHA256"],
        os_release=Path("/etc/os-release").read_text(), **elf_result)))
