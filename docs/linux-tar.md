# Optional Linux tar.gz

This is an opt-in build and validation path for Linux x86_64, with Ubuntu 22.04
(glibc 2.35) as the minimum runtime baseline. It does not designate tar.gz as an
official distribution format. Other distributions are not guaranteed to work.
The existing Flatpak and release workflows are unchanged.

## Run

Extract `bmz-player-<version>-linux-x86_64.tar.gz`, then run its `./bmz-player`
launcher. Use the launcher rather than `bin/bmz-player` directly. The directory
may be read-only; keep its `bin`, `lib`, and `resources` directories together.
No Flatpak installation, root installation, or system FFmpeg is needed.

The executable and shared libraries use relative ELF RUNPATHs. The launcher
does not export a package-wide `LD_LIBRARY_PATH` to external programs such as
the video-export encoder or the desktop browser.

The host must provide an x86_64 glibc runtime, a desktop session (X11 or Wayland),
a working Vulkan driver (or OpenGL with `--renderer gl`), and an audio device
through ALSA or a PulseAudio-compatible server such as PipeWire-Pulse. GPU
drivers, their loader/ICDs, display servers, audio servers and device permissions
belong to the host. A minimal Ubuntu 22.04 installation can install the integration
packages with:

```sh
sudo apt-get install libvulkan1 libgl1 libegl1 libx11-6 libxcursor1 libxi6 \
  libxrandr2 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0 libwayland-egl1 \
  libasound2 libasound2-plugins libpulse0 libfontconfig1 libudev1 ca-certificates
```

Install the appropriate GPU driver separately (for example Mesa Vulkan drivers
or the distribution's NVIDIA driver) and configure the desktop's sound server.
OS credential storage additionally needs a session D-Bus and Secret Service.
Opening URLs needs the desktop's `xdg-open`. Video export needs an external
FFmpeg executable with the required encoders; the bundled FFmpeg is decode-only.

Default writable paths are:

| Content | Location |
| --- | --- |
| Configuration, library DB, profiles, scores, replay, user skins | `${XDG_DATA_HOME:-$HOME/.local/share}/bmz-player` |
| Cache | `${XDG_CACHE_HOME:-$HOME/.cache}/bmz-player` |
| Logs | `${XDG_STATE_HOME:-$HOME/.local/state}/bmz-player/logs` |

Nonempty `BMZ_RESOURCE_DIR`, `BMZ_DATA_DIR`, `BMZ_CACHE_DIR`, and `BMZ_LOGS_DIR`
override these defaults. When `BMZ_DATA_DIR` is explicitly set, the app's existing
default of `BMZ_DATA_DIR/cache` and `BMZ_DATA_DIR/logs` remains in effect unless
overridden. Empty BMZ values mean unset, as in the app. The wrapper does not
change the caller's working directory; relative arguments and relative BMZ
overrides retain their meaning. A caller's `data/` directory does not override
the package defaults. XDG values should be absolute paths.

## Build and validate

On a Linux x86_64 host with Docker, Git and initialized bundled-skin submodules:

```sh
git submodule update --init --recursive
scripts/package-linux-tar.sh
# Or use rootless Podman:
CONTAINER_ENGINE=podman scripts/package-linux-tar.sh
```

Commit tracked changes first. The script builds only committed source and the
recorded skin submodule commits, excluding untracked files, local skin edits,
credentials, configuration, databases, external skins and additional songs.
No host runtime data is copied. All Git-managed fonts, skins and sample songs
are included unchanged. Docker/Podman caches the build image; allow several GB
of disk space, network access for dependencies, and time for Rust compilation.

Output is `dist/linux-tar/bmz-player-<version>-linux-x86_64.tar.gz` and
`SHA256SUMS.txt`. The archive includes `sources/`, so it is deliberately larger
than a binary-only archive. A failed runtime check leaves the completed archive
for diagnosis; consider it validated only when the command exits successfully.
To validate an existing archive without compiling again:

```sh
scripts/package-linux-tar.sh --verify dist/linux-tar/bmz-player-<version>-linux-x86_64.tar.gz
```

Run the **Optional Linux tar.gz** workflow manually
to download the same verified archive as an Actions artifact. GitHub requires
the workflow to exist on the repository's default branch before manual dispatch;
then the run dialog can select another branch. See the
[GitHub instructions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).
It has no release upload, IR manifest, signing or updater integration.

The build stage uses Ubuntu 22.04, records its actual compiler versions, builds
the pinned FFmpeg source without GPL/nonfree or autodetected external codecs,
then runs `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`
before the release build. Tests run with Japanese locale defaults and as an
unprivileged user, matching the existing UI-string and filesystem-permission
tests; test debug symbols are omitted to limit temporary disk/memory usage.
The application uses its existing PulseAudio feature.
No native CPU tuning is enabled.

Validation extracts the actual tar.gz and mounts it read-only in a separate
Ubuntu 22.04 container as an unprivileged user. This image has no build-stage
filesystem, compiler or FFmpeg. It verifies every packaged ELF's dependency
resolution: non-glibc dependencies must resolve inside the archive even if a
host integration package also provides them. A negative control verifies that
the image cannot resolve FFmpeg for a copy of the executable without its library
directory. It then
exercises help, relative song scanning, SQLite writes, default and overridden
data/cache/log/resource paths, a symlinked launcher, and sample play under Xvfb,
software Vulkan and a null PulseAudio sink. Successful file opens confirm access
to the packaged skin, CJK font, chart and audio sample.

This smoke check does not establish real GPU compatibility, Wayland behavior,
audible output, timing/latency, or physical controller access. Check those on a
real Ubuntu 22.04 desktop before proposing a public binary release.

## Notices and corresponding source

`resources/licenses/` contains the existing BMZ and third-party notices, the
mandatory cargo-about report, Rust runtime notices, the Noto OFL, FFmpeg licenses/configuration and
build provenance. Skin-specific notices stay beside the unmodified assets.
The existing skin restrictions, including noncommercial/NoDerivatives terms,
still apply; see `resources/licenses/license-notes.md` in the archive
(`docs/licenses.md` in the source tree).

FFmpeg is dynamically linked, retains its library names, and ships with the exact
verified upstream archive in `sources/`. Its recorded configure command and
`installer/linux-tar/build-ffmpeg.sh` describe rebuilding it. This follows the
[FFmpeg distribution checklist](https://ffmpeg.org/legal.html).

Other bundled ELF libraries are taken from Ubuntu packages. Each has its binary
and source versions recorded in `resources/licenses/ubuntu/packages.txt`, its
original copyright notice, and matching `.dsc`/upstream/Debian source archives
downloaded by APT into `sources/<source-package>/`. Missing source or notices
fail packaging. Use `dpkg-source -x` on the `.dsc` to unpack the patched source.
ELF RUNPATHs are adjusted by the included packaging script; library code is not
patched. These files accompany the binary, rather than relying on a future source offer.
The host's glibc/loader and GPU drivers are not bundled.

`sources/bmz-player.tar.gz` includes the exact application source, bundled
submodules, build scripts, lockfile and vendored Cargo dependencies. Extract it
and build with its `.cargo/config.toml` to use that vendor directory. The Ubuntu
build dependencies and FFmpeg prefix are described in `installer/linux-tar/`.
Preserve `sources/` and all notices when redistributing the artifact. Security
updates to bundled dependencies require rebuilding and revalidating the archive.
To refresh Ubuntu packages and the stable Rust toolchain instead of reusing
cached image layers, first run:

```sh
docker build --pull --no-cache --target build -t bmz-linux-tar-build installer/linux-tar
```

Use the equivalent Podman command when building with Podman.

## Local validation record (2026-09-15)

The archive built from `308ca195` was verified with the final `--verify` checker
using rootless Podman, Ubuntu 22.04/glibc 2.35, Rust 1.98.1 and GCC 11.4.0.
Later changes only adjust the verification fixtures and documentation; the Rust
sources, launcher and packaged libraries/resources are unchanged.

- `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked`: passed.
- `cargo test --locked`: 1,941 passed, 0 failed, 3 existing ignored tests.
- Extracted archive: all 25 bundled libraries and their relocations resolved;
  the missing-library negative control, read-only sample play, resource opens,
  relative CLI arguments, BMZ/XDG overrides and user DB/profile writes passed.
- All 3,483 packaged skin/font/sample files matched their committed Git blobs,
  including executable modes. Notices, corresponding sources for 18 Ubuntu
  packages, FFmpeg source and vendored application source were present.
- The archive is 2,172,396,883 bytes (about 2.02 GiB), including sources;
  `sha256sum --check SHA256SUMS.txt` passed.
- The user extracted this archive on a CachyOS desktop and launched
  `--boot-play-sample` with a separate `BMZ_DATA_DIR`, confirming the displayed
  game, audible output and keyboard input. This is an additional manual check;
  it does not extend the supported baseline to CachyOS.

GitHub-hosted Actions/Docker execution has not been run; local validation used
the same entry point with Podman. An Ubuntu 22.04 physical desktop, specific GPU
drivers, Wayland, latency and physical game controllers remain unverified.
The ignored Rust tests are
two manual skin profiling helpers and a GPU/external-FFmpeg video export test.
