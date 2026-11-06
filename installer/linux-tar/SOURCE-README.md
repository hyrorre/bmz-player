# Rebuild the corresponding Linux source archive

Extract `bmz-player-vX-linux-x64-sources.tar.gz` into a new directory. This is
the workspace root: Cargo.toml, Cargo.lock, crates/, vendor/, .cargo/config.toml
and actual bundled skin files are included. Both archives carry identical
schema 2 build manifests, recording the full commit, submodule commits, tool
versions, image ID, configurations, RPM provenance and content inventories.

## Prepare tools (network required)

Use official `almalinux:8.10` on x86_64. The included
`installer/linux-tar/Dockerfile` tools stage documents the complete setup,
including GCC Toolset 14, Clang/libclang, Python 3.12, Meson 1.7.2,
Ninja 1.11.1.4 and checksum-pinned NASM 2.16.03. Use the Rust/tool versions
recorded in the manifest when comparing builds. All tools are installed before
network access is disabled and are shared by the build and clean rebuild stages.
The x86-64 compiler flags avoid native CPU optimization.

From this extracted source root:

```sh
docker build --target source-verify -t bmz-source-check installer/linux-tar
docker run --rm --network=none -v "$PWD:/archives/sources:ro" bmz-source-check
```

The source-verify image contains tools but no FFmpeg, PipeWire, original
checkout or Cargo dependency/build cache. Only this source directory is mounted.
It validates every SRPM and runs the complete offline release rebuild.

## Build FFmpeg, PipeWire client and BMZ (offline)

With the same tools installed, from the extracted source root:

```sh
export BMZ_REBUILD_DIR="$(mktemp -d /var/tmp/bmz-rebuild.XXXXXX)"
bash installer/linux-tar/rebuild.sh
```

The script uses an empty CARGO_HOME and new target directory, verifies the pinned
FFmpeg 9.0.1 and PipeWire 1.4.9 sources, rebuilds their private shared libraries,
and runs:

```sh
cargo build --offline --locked --release -p bmz-player --no-default-features --features pulseaudio,pipewire,linux-evdev
```

Source hashes and original configure commands are in ffmpeg/ and pipewire/.
Only the installation prefixes change. The executable is
`$BMZ_REBUILD_DIR/target/release/bmz-player`; retain the rebuilt FFmpeg/PipeWire
library paths when running it. No host PipeWire server is built or started.
Byte-for-byte binary reproducibility is not claimed.

## RPM corresponding sources

`resources/licenses/rpm/packages.{json,txt}` in the runtime archive maps each
distribution library's actual file (including /lib64 and /usr/lib64 aliases)
to its owning binary RPM name, epoch, version, release, arch and exact SOURCERPM.
The runtime also contains the original %license and %doc files.
The manifest records these mappings and each SRPM header, URL and SHA256.
`srpms.json` and `srpms/` contain one copy of each exact SRPM.
No newer source version is used if the required version is unavailable.

The checker verifies the archive SHA256, RPM header/payload digests, source
name/epoch/version/release against every binary owner, and payload file sizes
and digests, then extracts the spec, patches and upstream archives.
To inspect an SRPM, in an empty directory:

```sh
rpm -qpi /path/to/package.src.rpm
rpm2cpio /path/to/package.src.rpm | cpio -id
```

The spec describes build dependencies and patch application. Rebuilding all
distribution libraries is separate from the FFmpeg/PipeWire/BMZ offline rebuild.

## Runtime validation and redistribution

Both normal packaging and `scripts/package-linux-tar.sh --verify RUNTIME SOURCES`
test the same runtime SHA256 on independent AlmaLinux 8.10 and Ubuntu 22.04
images, read-only, offline and as an unprivileged user. They check all ELF version
requirements (maximum GLIBC 2.28 and actual GLIBCXX/CXXABI providers), ldd -r,
software Vulkan/Xvfb sample autoplay, PulseAudio null-sink frames, missing
PipeWire-server diagnostics, CLI/relative paths and XDG storage.
The build's validation summary records the actual results; failed or missing
checks cannot produce upload outputs. Containers share the host kernel.
Physical kernels, GPUs, Wayland, audio latency and controllers require separate
hardware testing. Keep both archives, SHA256SUMS.txt, all notices and bundled
assets together when redistributing.
