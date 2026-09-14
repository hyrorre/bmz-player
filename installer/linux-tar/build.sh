#!/usr/bin/env bash
set -euo pipefail
cp -a /source/. /work/
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
# Keep test linking and temporary artifacts within the hosted runner's budget.
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --check
cargo check --locked
cargo clippy --locked
# Existing UI tests expect Japanese defaults; permission tests need a non-root
# process. Keep Cargo/toolchain access as root and run the test binaries as nobody.
LANG=ja_JP.UTF-8 CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='runuser -u nobody --' \
  cargo test --locked
cargo clean --profile dev
cargo build --locked --release -p bmz-player --no-default-features --features pulseaudio
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml)
name="bmz-player-${version}-linux-x86_64"
package="/tmp/$name"
mkdir -p "$package/bin" "$package/lib" "$package/resources/licenses" "$package/sources"
install -m755 target/release/bmz-player "$package/bin/"
install -m755 installer/linux-tar/bmz-player "$package/"
cp -a /source/data/skins /source/data/fonts /source/data/songs "$package/resources/"
cp LICENSE "$package/resources/licenses/BMZ-GPL-3.0-only.txt"
cp THIRD-PARTY-NOTICES.txt "$package/resources/licenses/third-party-notices.txt"
cp docs/licenses.md "$package/resources/licenses/license-notes.md"
cp data/fonts/noto-cjk/LICENSE "$package/resources/licenses/NotoSansCJK-OFL-1.1.txt"
cp docs/linux-tar.md "$package/README.md"
cargo-about generate --workspace --locked --fail --target x86_64-unknown-linux-gnu \
  --output-file "$package/resources/licenses/rust-dependency-licenses.txt" about.hbs
rust_docs="$(rustc --print sysroot)/share/doc/rust"
mkdir "$package/resources/licenses/rust-runtime"
cp -a "$rust_docs/COPYRIGHT-library.html" "$rust_docs/licenses" \
  "$package/resources/licenses/rust-runtime/"
python3 installer/linux-tar/bundle-libraries.py "$package"
cp /opt/ffmpeg-source/*.tar.xz "$package/sources/"
cp /opt/ffmpeg-source/configure.txt "$package/resources/licenses/ffmpeg-build.txt"
cp /opt/ffmpeg-source/COPYING* /opt/ffmpeg-source/LICENSE.md "$package/resources/licenses/"
{
  cat BUILD-COMMIT BUILD-SUBMODULES
  rustc -Vv
  cargo -V
  cc --version
  ldd --version
  printf '\nBuild command: cargo build --locked --release -p bmz-player --no-default-features --features pulseaudio\n'
  printf '\nFFmpeg source SHA256:\n'
  sha256sum /opt/ffmpeg-source/*.tar.xz
} > "$package/resources/licenses/build.txt"
# Ship the actual application source and Cargo dependencies, including build scripts.
# This avoids an expiring external source offer for an Actions artifact.
mkdir -p /tmp/bmz-source
cp -a /source/. /tmp/bmz-source/
mkdir -p /tmp/bmz-source/.cargo
cargo vendor --locked /tmp/bmz-source/vendor > /tmp/bmz-source/.cargo/config.toml
tar -czf "$package/sources/bmz-player.tar.gz" -C /tmp/bmz-source .
rm -rf /tmp/bmz-source
tar -czf "/output/$name.tar.gz" -C /tmp "$name"
