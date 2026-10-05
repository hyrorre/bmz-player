#!/usr/bin/env bash
# Build a private client library on the existing Ubuntu 22.04 baseline.
# Never install it into the host, start a server, or ship service/udev/PAM files.
set -euo pipefail
version=1.4.9
sha256=e606aa3f6d53ec4c56fe35034d35cadfe0bbea1a5275e4e006dd7d1abaec6b92
source_dir=${1:-/opt/pipewire-source}
prefix=${2:-/opt/pipewire}
mkdir -p "$source_dir"
cd "$source_dir"
if [[ $# == 0 ]]; then
  curl --fail --location --retry 3 \
    "https://deb.debian.org/debian/pool/main/p/pipewire/pipewire_${version}.orig.tar.bz2" \
    -o "pipewire-${version}.tar.bz2"
fi
printf '%s  %s\n' "$sha256" "pipewire-${version}.tar.bz2" | sha256sum --check
mkdir source
tar -xf "pipewire-${version}.tar.bz2" -C source --strip-components=1
args=(--prefix="$prefix" --libdir=lib --buildtype=release -Dauto_features=disabled
      -Ddbus=enabled -Dspa-plugins=enabled -Dsupport=enabled -Daudioconvert=enabled
      -Dpipewire-jack=disabled -Dpipewire-v4l2=disabled -Dsession-managers=[]
      -Dexamples=disabled -Dtests=disabled -Dsystemd-user-service=disabled
      -Drlimits-install=false -Dpam-defaults-install=false)
meson setup build source "${args[@]}"
meson compile -C build -j 2
meson install -C build
{
  printf 'source_url=https://deb.debian.org/debian/pool/main/p/pipewire/pipewire_%s.orig.tar.bz2\n' "$version"
  printf 'source_sha256=%s\n' "$sha256"
  printf '%q ' meson setup build source "${args[@]}"
  printf '\n'
} > configure.txt
cp source/COPYING LICENSE
