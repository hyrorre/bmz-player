#!/usr/bin/env bash
set -euo pipefail

# Same upstream release as the macOS builder; Linux has its own configure line.
version=9.0.1
sha256=cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635
source_dir=${1:-/opt/ffmpeg-source}
prefix=${2:-/opt/ffmpeg}
mkdir -p "$source_dir"
cd "$source_dir"
# Supplying a source directory is the offline rebuild interface: never download.
if [[ $# == 0 ]]; then
  curl --fail --location --retry 3 "https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz" -O
fi
printf '%s  %s\n' "$sha256" "ffmpeg-${version}.tar.xz" | sha256sum --check
tar -xf "ffmpeg-${version}.tar.xz"
cd "ffmpeg-${version}"
args=(--prefix="$prefix" --arch=x86_64 --cpu=x86-64 --enable-shared
      --disable-static --disable-autodetect --disable-programs --disable-doc
      --disable-network --disable-avdevice --disable-avfilter --disable-encoders
      --disable-muxers --disable-debug --enable-pthreads --enable-zlib --enable-bzlib)
./configure "${args[@]}"
{
  printf 'source_url=https://ffmpeg.org/releases/ffmpeg-%s.tar.xz\n' "$version"
  printf 'source_sha256=%s\n' "$sha256"
  printf '%q ' ./configure "${args[@]}"
  printf '\n'
} > ../configure.txt
make -j 2
make install
cp COPYING* LICENSE.md ../
