# Linux tar.gz distribution

GitHub Releases provide runtime and corresponding-source archives for Linux
x86_64, with Ubuntu 22.04 (glibc 2.35) as the minimum runtime baseline.
Other distributions are not guaranteed to work. Flatpak remains available.
Regular users only need the runtime archive; the sources archive is for rebuilding.

The build enables `pulseaudio,pipewire,linux-evdev`. It bundles the pinned
PipeWire 1.4.9 client, native-protocol/client-node/adapter/metadata/rt modules,
SPA support/audioconvert plugins and a private client configuration. It ships no
audio server, services, PAM or udev policy. The Ubuntu 22.04/glibc baseline stays
unchanged. Auto still prefers PulseAudio then ALSA; PipeWire and evdev are opt-in.
See [Linux latency verification](linux-latency.md) for dependencies, permissions,
diagnostics and rollback. The launcher sets package-local PipeWire/SPA/config
search paths only if callers did not already set them; it never forces a quantum.

## Run

Extract `bmz-player-v<version>-linux-x64.tar.gz`, then run its `./bmz-player`
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

### Manual updates and Flatpak migration

Close BMZ and extract the new runtime archive into a new directory, then launch
its top-level `./bmz-player`. Default user data lives outside the package and is
reused. Preserve any explicit `BMZ_*` overrides, especially `BMZ_DATA_DIR`.
Keep additional skins in the user data directory. Linux tar updates are manual;
the Windows updater and macOS Sparkle feeds do not apply to this package.

Flatpak uses sandbox paths under `~/.var/app/net.hyrorre.BMZPlayer/`, so switching
formats does not automatically migrate profiles or scores. Close both versions,
back up their data, and copy the Flatpak data directory's contents to the tar
version's data directory only after checking for existing files. Host chart paths
and access permissions may differ from the Flatpak environment.

### Local packaging

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

Output in `dist/linux-tar/` consists of:

- `bmz-player-v<version>-linux-x64.tar.gz`: executable, libraries, resources and notices.
- `bmz-player-v<version>-linux-x64-sources.tar.gz`: corresponding source workspace,
  Cargo vendor, FFmpeg archive and exact Ubuntu source packages.
- `SHA256SUMS.txt`: SHA256 of both compressed files.

After all checks succeed, `validation.md` is written beside the runtime archive
with its build commit, sizes, checksums and validation summary. This local report
is separate from the three uploaded distribution files.

Each archive must be strictly below 2,147,483,648 bytes, the
[GitHub Releases per-file limit](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases).
The build uses gzip level 9 and fails if either file reaches the limit. It never
removes required sources or notices to meet this limit. A failed check leaves
archives for diagnosis; consider them validated only when the command succeeds.
To revalidate a pair (with `SHA256SUMS.txt` beside the runtime archive):

```sh
scripts/package-linux-tar.sh --verify \
  dist/linux-tar/bmz-player-v<version>-linux-x64.tar.gz \
  dist/linux-tar/bmz-player-v<version>-linux-x64-sources.tar.gz
```

Run the **Optional Linux tar.gz** workflow manually
to download both verified archives and checksums as one Actions artifact. It
uploads only the current run's explicit output paths after all checks succeed,
and records sizes, SHA256, full commit and verification results in the job summary.
GitHub requires
the workflow to exist on the repository's default branch before manual dispatch;
then the run dialog can select another branch. See the
[GitHub instructions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).
This manual workflow does not publish a Release. After verification the package
script also writes a `*-linux-x64-tar-client-manifest.json` sidecar for release
integration; the manual artifact still contains only the two archives and checksums.

### Official release workflow

`release-apps.yml` builds the same pair from the resolved release-tag commit.
It checks the archive version/commit against release metadata and combines the
Linux tar client hash with the Windows, macOS and Flatpak builds. The hash is
from the verified packaged executable after `patchelf`, not the launcher or tarball.
Both archives and the signed unified `release.json` enter the final `SHA256SUMS.txt`.
The release no longer attaches a separate client manifest; rianIR reads the
top-level `builds` in `release.json`. See [Release metadata](release-metadata.md).
Download corresponding sources from the same Release as the runtime; GitHub's
automatic source snapshot does not include vendored dependencies or Ubuntu/FFmpeg sources.

With `upload_to_release=false`, the workflow validates and uploads the combined
files as `verified-release-files` in Actions without publishing Release assets
or update feeds. Before public distribution, verify real GPU/audio/input behavior
and arrange rianIR allowlist registration as described in `docs/rian-ir.md`.

The build stage uses Ubuntu 22.04, records its actual compiler versions, builds
the pinned FFmpeg source without GPL/nonfree or autodetected external codecs,
then runs `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`
before the release build. Tests run with Japanese locale defaults and as an
unprivileged user, matching the existing UI-string and filesystem-permission
tests; test debug symbols are omitted to limit temporary disk/memory usage.
The application uses its existing PulseAudio feature.
No native CPU tuning is enabled.

Validation extracts the actual runtime tar.gz and mounts only it read-only in a separate
Ubuntu 22.04 container as an unprivileged user. This image has no build-stage
filesystem, compiler or FFmpeg. It verifies every packaged ELF's dependency
resolution: non-glibc dependencies must resolve inside the archive even if a
host integration package also provides them. A negative control verifies that
the image cannot resolve FFmpeg for a copy of the executable without its library
directory. It then
exercises help, relative song scanning, SQLite writes, default and overridden
data/cache/log/resource paths, a symlinked launcher, and sample play under Xvfb,
software Vulkan and a null PulseAudio sink. Successful file opens confirm access
to the packaged skin, CJK font, chart and audio sample. Neither the source archive
nor the original build tree is accessible inside this runtime container.

Both archives carry the same `build-manifest.json` (schema 1). It records the
version, commit, submodule commits, target/features/command, lockfile hash,
compiler versions, Ubuntu release, build image ID, FFmpeg URL/hash/configuration,
and Ubuntu binary/source package versions. File inventories cover hashes, sizes,
executable modes and symlink targets. The committed snapshot is inventoried before
building; source and skin files are compared against it after staging and extraction.
The manifest is excluded from its own inventory. Compressed file hashes live only
in the external checksum file.

The source archive is extracted to a separate path. Its inventory and identity
must match the runtime, Cargo workspace version/lockfile, FFmpeg source/configuration,
and exact Ubuntu source versions. Every `.dsc` reference is checked for size and
SHA256 and extracted with `dpkg-source`. A separate tool-only container mounts only
the source archive, uses an empty Cargo cache, resolves vendor dependencies offline,
then rebuilds FFmpeg and BMZ release with networking disabled. This does not rebuild
all Ubuntu libraries or establish byte-for-byte binary reproducibility.

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
verified upstream archive in the matching source archive's `ffmpeg/`. Its recorded configure command and
`installer/linux-tar/build-ffmpeg.sh` describe rebuilding it. This follows the
[FFmpeg distribution checklist](https://ffmpeg.org/legal.html).

Other bundled ELF libraries are taken from Ubuntu packages. Each has its binary
and source versions recorded in `resources/licenses/ubuntu/packages.txt`, its
original copyright notice, and matching `.dsc`/upstream/Debian source archives
downloaded by APT into the source archive's `ubuntu/<source-package>/`. Missing source or notices
fail packaging. Use `dpkg-source -x` on the `.dsc` to unpack the patched source.
ELF RUNPATHs are adjusted by the included packaging script; library code is not
patched. These files accompany the binary, rather than relying on a future source offer.
The host's glibc/loader and GPU drivers are not bundled.

The source archive contains the application workspace directly, including the
actual bundled submodule files, scripts, lockfile and Cargo vendor. Its
`.cargo/config.toml` uses relative `vendor`, preserving existing configuration.
Read `BUILDING.md` in that archive (repository copy:
[`installer/linux-tar/SOURCE-README.md`](../installer/linux-tar/SOURCE-README.md))
for development packages, offline rebuild commands, and Ubuntu source extraction.
Preserve both matching archives and all notices when redistributing. Security
updates to bundled dependencies require rebuilding and revalidating the archive.
To refresh Ubuntu packages and the stable Rust toolchain instead of reusing
cached image layers, first run:

```sh
docker build --pull --no-cache --target build -t bmz-linux-tar-build installer/linux-tar
```

Use the equivalent Podman command when building with Podman.

## Validation records

Historical split-archive and single-archive validation results, artifact hashes,
and workflow evidence are recorded in the
[2026-09-15 work note](../notes/2026/2026-09-15-linux-tar-validation.md).
