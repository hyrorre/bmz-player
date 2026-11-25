#!/usr/bin/env bash
set -euo pipefail

readonly DEFAULT_FFMPEG_VERSION="9.1"
readonly DEFAULT_FFMPEG_SHA256="1a4d394ae63d190a526cd052eb4415c156b5c5e2df37efab41a7de0b8fde2433"
readonly DEFAULT_MACOS_DEPLOYMENT_TARGET="10.13"
cleanup_dir=""

usage() {
  cat <<'USAGE'
Usage: scripts/build-ffmpeg-macos.sh --target TARGET --prefix DIR

Build the shared FFmpeg libraries used by BMZ Player for macOS.

Required options:
  --target TARGET         Rust target triple (aarch64-apple-darwin or x86_64-apple-darwin).
  --prefix DIR            Empty installation prefix for headers, pkg-config files, and dylibs.

Environment:
  BMZ_FFMPEG_VERSION              FFmpeg release version (default: 9.1).
  BMZ_FFMPEG_SHA256               SHA-256 of the official source archive.
  BMZ_FFMPEG_SOURCE_ARCHIVE       Use an existing source archive instead of downloading.
  BMZ_MACOS_DEPLOYMENT_TARGET     Minimum macOS version (default: 10.13).

The build intentionally excludes libavdevice, libavfilter, external codec
libraries, network protocols, and FFmpeg command-line programs. BMZ Player uses
local-file demuxing/decoding plus software scaling/resampling only. Apple
Silicon targets require macOS 11.0 or newer.
USAGE
}

die() {
  echo "error: $*" >&2
  exit 1
}

need_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

cleanup() {
  if [[ -n "${cleanup_dir}" && -d "${cleanup_dir}" ]]; then
    rm -rf -- "${cleanup_dir}"
  fi
}

version_lte() {
  local actual="$1"
  local limit="$2"
  awk -v actual="${actual}" -v limit="${limit}" '
    BEGIN {
      actual_count = split(actual, actual_parts, ".")
      limit_count = split(limit, limit_parts, ".")
      count = actual_count > limit_count ? actual_count : limit_count
      for (i = 1; i <= count; i++) {
        actual_part = i <= actual_count ? actual_parts[i] + 0 : 0
        limit_part = i <= limit_count ? limit_parts[i] + 0 : 0
        if (actual_part < limit_part) exit 0
        if (actual_part > limit_part) exit 1
      }
      exit 0
    }
  '
}

macho_minimum_version() {
  local binary="$1"
  vtool -show-build "${binary}" | awk '
    $1 == "minos" { print $2; exit }
    $1 == "version" { print $2; exit }
  '
}

main() {
  local target=""
  local prefix=""

  while (($# > 0)); do
    case "$1" in
      --target)
        shift
        [[ $# -gt 0 ]] || die "--target requires a value"
        target="$1"
        ;;
      --prefix)
        shift
        [[ $# -gt 0 ]] || die "--prefix requires a value"
        prefix="$1"
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        die "unknown option: $1"
        ;;
    esac
    shift
  done

  [[ -n "${target}" ]] || die "--target is required"
  [[ -n "${prefix}" ]] || die "--prefix is required"
  [[ "${prefix}" != "/" ]] || die "--prefix must not be /"

  local ffmpeg_arch clang_arch
  case "${target}" in
    aarch64-apple-darwin)
      ffmpeg_arch="aarch64"
      clang_arch="arm64"
      ;;
    x86_64-apple-darwin)
      ffmpeg_arch="x86_64"
      clang_arch="x86_64"
      ;;
    *)
      die "unsupported macOS target: ${target}"
      ;;
  esac

  local ffmpeg_version="${BMZ_FFMPEG_VERSION:-${DEFAULT_FFMPEG_VERSION}}"
  local ffmpeg_sha256="${BMZ_FFMPEG_SHA256:-${DEFAULT_FFMPEG_SHA256}}"
  local deployment_target="${BMZ_MACOS_DEPLOYMENT_TARGET:-${DEFAULT_MACOS_DEPLOYMENT_TARGET}}"
  [[ "${ffmpeg_version}" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] || \
    die "invalid FFmpeg version: ${ffmpeg_version}"
  [[ "${ffmpeg_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "invalid FFmpeg SHA-256"
  [[ "${deployment_target}" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] || \
    die "invalid macOS deployment target: ${deployment_target}"
  if [[ "${target}" == "aarch64-apple-darwin" ]] && ! version_lte "11.0" "${deployment_target}"; then
    die "Apple Silicon requires a macOS deployment target of 11.0 or newer"
  fi

  need_command awk
  need_command make
  need_command shasum
  need_command tar
  need_command vtool
  need_command xcrun

  mkdir -p "$(dirname "${prefix}")"
  local prefix_parent prefix_name
  prefix_parent="$(cd "$(dirname "${prefix}")" && pwd)"
  prefix_name="$(basename "${prefix}")"
  prefix="${prefix_parent}/${prefix_name}"
  if [[ -e "${prefix}" ]]; then
    [[ -d "${prefix}" && -z "$(find "${prefix}" -mindepth 1 -print -quit)" ]] || \
      die "installation prefix already exists and is not empty: ${prefix}"
    rmdir "${prefix}"
  fi

  local work_dir
  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/bmz-ffmpeg-macos.XXXXXX")"
  cleanup_dir="${work_dir}"
  trap cleanup EXIT

  local archive="${work_dir}/ffmpeg-${ffmpeg_version}.tar.xz"
  local source_url="https://ffmpeg.org/releases/ffmpeg-${ffmpeg_version}.tar.xz"
  local source_archive="${BMZ_FFMPEG_SOURCE_ARCHIVE:-}"
  if [[ -n "${source_archive}" ]]; then
    [[ -f "${source_archive}" ]] || die "FFmpeg source archive not found: ${source_archive}"
    echo "==> Using FFmpeg source archive ${source_archive}"
    cp "${source_archive}" "${archive}"
  else
    need_command curl
    echo "==> Downloading ${source_url}"
    curl --fail --location --retry 3 --retry-all-errors \
      --output "${archive}" \
      "${source_url}"
  fi
  printf '%s  %s\n' "${ffmpeg_sha256}" "${archive}" | shasum -a 256 --check

  tar -xf "${archive}" -C "${work_dir}"
  local source_dir="${work_dir}/ffmpeg-${ffmpeg_version}"
  [[ -x "${source_dir}/configure" ]] || die "FFmpeg source archive has no configure script"

  local clang sdk_path
  clang="$(xcrun --sdk macosx --find clang)"
  sdk_path="$(xcrun --sdk macosx --show-sdk-path)"
  local common_target_flags="-arch ${clang_arch} -mmacosx-version-min=${deployment_target}"
  local configure_args=(
    "--prefix=${prefix}"
    "--arch=${ffmpeg_arch}"
    "--target-os=darwin"
    "--cc=${clang}"
    "--objcc=${clang}"
    "--host-cc=${clang}"
    "--host-ld=${clang}"
    "--host-cflags=-isysroot ${sdk_path}"
    "--host-ldflags=-isysroot ${sdk_path}"
    "--sysroot=${sdk_path}"
    "--enable-shared"
    "--disable-static"
    "--disable-autodetect"
    "--disable-programs"
    "--disable-doc"
    "--disable-network"
    "--disable-avdevice"
    "--disable-avfilter"
    "--disable-encoders"
    "--disable-muxers"
    "--enable-pthreads"
    "--enable-audiotoolbox"
    "--enable-videotoolbox"
    "--enable-zlib"
    "--enable-bzlib"
    "--disable-iconv"
    "--disable-debug"
    "--enable-stripping"
    "--extra-cflags=${common_target_flags}"
    "--extra-ldflags=${common_target_flags}"
  )
  if [[ "${ffmpeg_arch}" == "x86_64" ]] && ! command -v nasm >/dev/null 2>&1; then
    echo "==> nasm is unavailable; disabling x86 assembly optimizations"
    configure_args+=("--disable-x86asm")
  fi

  echo "==> Configuring FFmpeg ${ffmpeg_version} for ${target} (macOS ${deployment_target}+)."
  (
    cd "${source_dir}"
    MACOSX_DEPLOYMENT_TARGET="${deployment_target}" ./configure "${configure_args[@]}"
  )

  local jobs
  jobs="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 2)"
  echo "==> Building FFmpeg with ${jobs} jobs"
  make -C "${source_dir}" -j "${jobs}"
  make -C "${source_dir}" install

  [[ ! -e "${prefix}/lib/libavdevice.dylib" ]] || die "unexpected libavdevice build"
  [[ ! -e "${prefix}/lib/libavfilter.dylib" ]] || die "unexpected libavfilter build"

  local dylib minos
  while IFS= read -r -d '' dylib; do
    minos="$(macho_minimum_version "${dylib}")"
    [[ -n "${minos}" ]] || die "missing Mach-O minimum version: ${dylib}"
    version_lte "${minos}" "${deployment_target}" || \
      die "${dylib} requires macOS ${minos}, expected ${deployment_target} or older"
    echo "==> Verified $(basename "${dylib}"): macOS ${minos}+"
  done < <(find "${prefix}/lib" -type f -name '*.dylib' -print0)

  local provenance_dir="${prefix}/share/bmz-player"
  mkdir -p "${provenance_dir}"
  {
    echo "BMZ Player macOS FFmpeg build provenance"
    echo "ffmpeg_version=${ffmpeg_version}"
    echo "source_url=${source_url}"
    echo "source_sha256=${ffmpeg_sha256}"
    echo "target=${target}"
    echo "macos_deployment_target=${deployment_target}"
    echo "configure_flags:"
    printf '  %s\n' "${configure_args[@]}"
  } > "${provenance_dir}/ffmpeg-build.txt"

  echo "==> Installed FFmpeg into ${prefix}"
}

main "$@"
