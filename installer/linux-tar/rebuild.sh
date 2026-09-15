#!/usr/bin/env bash
# Run from any freshly extracted source archive; no Git checkout is needed.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export CARGO_HOME="${BMZ_REBUILD_DIR:?Set BMZ_REBUILD_DIR to a new, empty absolute directory}/cargo-home"
export CARGO_TARGET_DIR="$BMZ_REBUILD_DIR/target"
mkdir -p "$CARGO_HOME" "$BMZ_REBUILD_DIR/ffmpeg-source"
cp ffmpeg/*.tar.xz "$BMZ_REBUILD_DIR/ffmpeg-source/"
bash installer/linux-tar/build-ffmpeg.sh "$BMZ_REBUILD_DIR/ffmpeg-source" "$BMZ_REBUILD_DIR/ffmpeg"
export PKG_CONFIG_PATH="$BMZ_REBUILD_DIR/ffmpeg/lib/pkgconfig"
export LD_LIBRARY_PATH="$BMZ_REBUILD_DIR/ffmpeg/lib"
cargo build --offline --locked --release -p bmz-player --no-default-features --features pulseaudio
"$CARGO_TARGET_DIR/release/bmz-player" --help
