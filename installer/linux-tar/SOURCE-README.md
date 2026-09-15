# Rebuild the corresponding Linux source archive

Extract `bmz-player-vX-linux-x64-sources.tar.gz` into any new directory.
This directory is the BMZ workspace root: `Cargo.toml`, `Cargo.lock`, `crates/`,
`vendor/`, `.cargo/config.toml`, and the actual bundled skin files are here.
`build-manifest.json` identifies the version, full Git commit, submodule commits,
toolchain, build image ID, FFmpeg configuration, and exact Ubuntu package versions.
The runtime archive carries an identical manifest with both file inventories.

## Prepare tools (network required)

Use Ubuntu 22.04 x86_64. Install the toolchain versions recorded in the manifest
when comparing builds. The packaging Dockerfile's `tools` stage documents setup:

```sh
sudo apt-get update
sudo apt-get install build-essential ca-certificates curl git pkg-config clang \
  libclang-dev nasm xz-utils python3 dpkg-dev libasound2-dev libpulse-dev \
  libudev-dev libfontconfig1-dev libdbus-1-dev libxkbcommon-dev zlib1g-dev libbz2-dev
```

Install Rust/Cargo with rustup (see the included Dockerfile); use the Rust release
recorded by `toolchain.rust` in the manifest. APT and toolchain installation need
network access. The next steps use the included FFmpeg archive and Cargo vendor,
and run without network access. No `.git`, original checkout, `/source` mount,
or `/tmp/bmz-source` path is required.

## Build FFmpeg and BMZ (offline)

From the extracted source root:

```sh
export BMZ_REBUILD_DIR="$(mktemp -d /var/tmp/bmz-rebuild.XXXXXX)"
bash installer/linux-tar/rebuild.sh
```

The script verifies the pinned FFmpeg SHA256, extracts `ffmpeg/*.tar.xz`, builds
the shared libraries with `installer/linux-tar/build-ffmpeg.sh`, and sets
`PKG_CONFIG_PATH` and `LD_LIBRARY_PATH` to the new prefix. Only the install prefix
differs from `ffmpeg/configure.txt`. It then runs:

```sh
cargo build --offline --locked --release -p bmz-player --no-default-features --features pulseaudio
```

The script uses a new Cargo cache and target directory under `BMZ_REBUILD_DIR`.
The executable is `$BMZ_REBUILD_DIR/target/release/bmz-player`; retain the FFmpeg
library path when running it. Rebuilding with the same sources and settings is
tested; byte-for-byte binary reproducibility is not claimed.

## Ubuntu corresponding sources

`ubuntu/<source-package>/` contains the exact `.dsc` and its upstream/Debian
archives. `build-manifest.json` and the runtime's
`resources/licenses/ubuntu/packages.{json,txt}` map each bundled ELF library to
its binary package/version and source package/version. To extract one:

```sh
dpkg-source -x ubuntu/<source-package>/<file>.dsc /path/to/new/source-directory
```

The extracted `debian/control`, `debian/rules`, and `debian/patches/` describe
package build dependencies, commands, and patches. The checker validates SHA256,
sizes, package versions and successful extraction for every `.dsc`. It does not
rebuild all Ubuntu libraries; preparing their build dependencies is separate.

## Container verification

With Docker (or Podman), from this source root:

```sh
docker build --target source-verify -t bmz-source-check installer/linux-tar
docker run --rm --network=none -v "$PWD:/archives/sources:ro" bmz-source-check
```

Only this source directory is mounted. The image contains development tools but
no prebuilt FFmpeg or Cargo dependency/build cache. Allow several GB of temporary
disk space. Both archives and `SHA256SUMS.txt` must accompany redistribution;
retain all notices and unmodified bundled assets.
