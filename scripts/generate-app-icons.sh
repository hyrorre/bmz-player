#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/generate-app-icons.sh [APPLE_SOURCE_SVG]

Generate BMZ Player desktop app icons.
Apple PNG/window/ICNS icons use the optional argument, BMZ_APPLE_ICON_SVG when
set, or the checked-in Apple-specific icon source. Windows ICO/window icons use
BMZ_WINDOWS_ICON_SVG when set, otherwise the checked-in Windows-specific icon
source.

Default Apple source:
  assets/app-icon/bmz-player-apple.svg
Default Windows source:
  assets/app-icon/bmz-player-windows.svg

Outputs:
  assets/app-icon/bmz-player.png
  assets/app-icon/bmz-player-window.png
  assets/app-icon/bmz-player-window-windows.png
  assets/app-icon/bmz-player.ico
  assets/app-icon/bmz-player.icns
USAGE
}

die() {
  echo "error: $*" >&2
  exit 1
}

need_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

repo_root() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  cd "${script_dir}/.." && pwd
}

write_icns() {
  local tmp_dir="$1"
  local output="$2"

  node - "${tmp_dir}" "${output}" <<'NODE'
const fs = require('fs');
const path = require('path');

const inputDir = process.argv[2];
const outputPath = process.argv[3];
const chunkTypes = ['icp4', 'icp5', 'icp6', 'ic07', 'ic08', 'ic09', 'ic10'];

const chunks = chunkTypes.map((type) => {
  const png = fs.readFileSync(path.join(inputDir, `${type}.png`));
  const header = Buffer.alloc(8);
  header.write(type, 0, 4, 'ascii');
  header.writeUInt32BE(png.length + 8, 4);
  return Buffer.concat([header, png]);
});

const body = Buffer.concat(chunks);
const header = Buffer.alloc(8);
header.write('icns', 0, 4, 'ascii');
header.writeUInt32BE(body.length + 8, 4);
fs.writeFileSync(outputPath, Buffer.concat([header, body]));
NODE
}

main() {
  if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
  fi

  local root
  root="$(repo_root)"
  cd "${root}"

  need_command magick
  need_command node

  local apple_source_svg="${1:-${BMZ_APPLE_ICON_SVG:-assets/app-icon/bmz-player-apple.svg}}"
  [[ -f "${apple_source_svg}" ]] || die "missing Apple source icon: ${apple_source_svg}"
  local windows_source_svg="${BMZ_WINDOWS_ICON_SVG:-assets/app-icon/bmz-player-windows.svg}"
  [[ -f "${windows_source_svg}" ]] || die "missing Windows source icon: ${windows_source_svg}"

  local out_dir="${root}/assets/app-icon"
  mkdir -p "${out_dir}"

  echo "==> Generating PNG icons"
  magick "${apple_source_svg}" -resize 1024x1024 -depth 8 "PNG32:${out_dir}/bmz-player.png"
  magick "${apple_source_svg}" -resize 256x256 -depth 8 "PNG32:${out_dir}/bmz-player-window.png"
  magick -background none "${windows_source_svg}" -resize 256x256 -depth 8 "PNG32:${out_dir}/bmz-player-window-windows.png"

  echo "==> Generating Windows ICO"
  magick -background none "${windows_source_svg}" \
    -define icon:auto-resize=256,128,64,48,32,16 \
    "${out_dir}/bmz-player.ico"

  echo "==> Generating macOS ICNS"
  local tmp_dir
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bmz-player-icns.XXXXXX")"
  trap "rm -rf '${tmp_dir}'" EXIT

  local spec type size
  for spec in icp4:16 icp5:32 icp6:64 ic07:128 ic08:256 ic09:512 ic10:1024; do
    type="${spec%%:*}"
    size="${spec##*:}"
    magick "${apple_source_svg}" -resize "${size}x${size}" -depth 8 "PNG32:${tmp_dir}/${type}.png"
  done
  write_icns "${tmp_dir}" "${out_dir}/bmz-player.icns"

  echo "==> Done"
  echo "${out_dir}"
}

main "$@"
