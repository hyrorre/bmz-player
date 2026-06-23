#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/package-macos-app.sh [OPTIONS]

Build a macOS .app bundle for BMZ Player.

Options:
  --debug                 Use target/debug/bmz-player instead of release.
  --release               Use target/release/bmz-player (default).
  --target TRIPLE         Pass --target TRIPLE to cargo and read the binary from target/TRIPLE.
  --skip-build            Do not run cargo build; use an existing binary.
  --out-dir DIR           Output directory (default: dist/macos).
  --app-name NAME         App bundle name (default: BMZ Player).
  --bundle-id ID          CFBundleIdentifier (default: dev.hyrorre.bmz-player).
  --bundle-dylibs         Copy non-system dylib dependencies into Contents/Frameworks.
                          Automatically ad-hoc signs unless --sign/--ad-hoc-sign is set.
  --sign IDENTITY         codesign with the given identity.
  --ad-hoc-sign           codesign with ad-hoc identity (-).
  --smoke                 Run a short packaged smoke launch after bundling.
  -h, --help              Show this help.

Environment:
  BMZ_MACOS_APP_NAME      Default app name override.
  BMZ_MACOS_BUNDLE_ID     Default bundle id override.
  BMZ_MACOS_OUT_DIR       Default output directory override.
  BMZ_CODESIGN_IDENTITY   Default signing identity override.
  BMZ_BUNDLE_DYLIBS=1     Same as --bundle-dylibs.
  BMZ_PACKAGE_SMOKE=1     Same as --smoke.

Notes:
  --bundle-dylibs can copy FFmpeg and codec libraries. Check docs/licenses.md
  before publishing any redistributable artifact created with that option.
  Bundling dylibs rewrites Mach-O load commands, so the bundle must be re-signed
  before Finder/Dock launch on macOS.
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

cargo_version() {
  sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1
}

copy_dir() {
  local src="$1"
  local dst="$2"
  [[ -d "${src}" ]] || die "missing directory: ${src}"
  mkdir -p "${dst}"
  rsync -a --delete \
    --exclude '.git' \
    --exclude '.DS_Store' \
    "${src}/" "${dst}/"
}

copy_file() {
  local src="$1"
  local dst="$2"
  [[ -f "${src}" ]] || die "missing file: ${src}"
  mkdir -p "$(dirname "${dst}")"
  cp -p "${src}" "${dst}"
}

is_system_dylib() {
  local dep="$1"
  [[ "${dep}" == /usr/lib/* || "${dep}" == /System/Library/* ]]
}

is_rewriteable_dylib_ref() {
  local dep="$1"
  [[ "${dep}" == /* && -f "${dep}" ]] && ! is_system_dylib "${dep}"
}

dylib_deps() {
  local binary="$1"
  otool -L "${binary}" | awk 'NR > 1 { print $1 }'
}

bundle_dylib_dependencies() {
  local executable="$1"
  local frameworks_dir="$2"
  need_command otool
  need_command install_name_tool
  mkdir -p "${frameworks_dir}"

  local queue=("${executable}")
  local processed=""
  local item dep dep_name copied

  while ((${#queue[@]} > 0)); do
    item="${queue[0]}"
    queue=("${queue[@]:1}")
    case "${processed}" in
      *"|${item}|"*) continue ;;
    esac
    processed="${processed}|${item}|"

    while IFS= read -r dep; do
      [[ -n "${dep}" ]] || continue
      is_rewriteable_dylib_ref "${dep}" || continue

      dep_name="$(basename "${dep}")"
      copied="${frameworks_dir}/${dep_name}"
      if [[ ! -f "${copied}" ]]; then
        cp -L "${dep}" "${copied}"
        chmod u+w "${copied}"
        install_name_tool -id "@executable_path/../Frameworks/${dep_name}" "${copied}" || true
        queue+=("${copied}")
      fi

      install_name_tool -change "${dep}" "@executable_path/../Frameworks/${dep_name}" "${item}" || true
    done < <(dylib_deps "${item}")
  done
}

write_info_plist() {
  local plist="$1"
  local executable="$2"
  local icon_file="$3"
  local app_name="$4"
  local bundle_id="$5"
  local version="$6"

  cat > "${plist}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${app_name}</string>
  <key>CFBundleExecutable</key>
  <string>${executable}</string>
  <key>CFBundleIconFile</key>
  <string>${icon_file}</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${app_name}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.games</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.13</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
EOF
}

sign_bundle() {
  local app_dir="$1"
  local identity="$2"

  need_command codesign
  need_command file

  local codesign_args=(--force --timestamp=none --sign "${identity}")
  if [[ "${identity}" != "-" ]]; then
    codesign_args=(--force --options runtime --timestamp=none --sign "${identity}")
  fi

  if [[ -d "${app_dir}/Contents/Frameworks" ]]; then
    while IFS= read -r -d '' file_path; do
      if file "${file_path}" | grep -q 'Mach-O'; then
        codesign "${codesign_args[@]}" "${file_path}"
      fi
    done < <(find "${app_dir}/Contents/Frameworks" -type f -print0)
  fi

  codesign "${codesign_args[@]}" "${app_dir}"
  codesign --verify --deep --strict --verbose=4 "${app_dir}"
}

main() {
  local root
  root="$(repo_root)"
  cd "${root}"

  need_command cargo
  need_command rsync
  need_command plutil

  local profile="release"
  local skip_build=0
  local target_triple=""
  local out_dir="${BMZ_MACOS_OUT_DIR:-${root}/dist/macos}"
  local app_name="${BMZ_MACOS_APP_NAME:-BMZ Player}"
  local bundle_id="${BMZ_MACOS_BUNDLE_ID:-dev.hyrorre.bmz-player}"
  local bundle_dylibs="${BMZ_BUNDLE_DYLIBS:-0}"
  local sign_identity="${BMZ_CODESIGN_IDENTITY:-}"
  local smoke="${BMZ_PACKAGE_SMOKE:-0}"

  while (($# > 0)); do
    case "$1" in
      --debug)
        profile="debug"
        ;;
      --release)
        profile="release"
        ;;
      --target)
        shift
        [[ $# -gt 0 ]] || die "--target requires a value"
        target_triple="$1"
        ;;
      --skip-build)
        skip_build=1
        ;;
      --out-dir)
        shift
        [[ $# -gt 0 ]] || die "--out-dir requires a value"
        out_dir="$1"
        ;;
      --app-name)
        shift
        [[ $# -gt 0 ]] || die "--app-name requires a value"
        app_name="$1"
        ;;
      --bundle-id)
        shift
        [[ $# -gt 0 ]] || die "--bundle-id requires a value"
        bundle_id="$1"
        ;;
      --bundle-dylibs)
        bundle_dylibs=1
        ;;
      --sign)
        shift
        [[ $# -gt 0 ]] || die "--sign requires a value"
        sign_identity="$1"
        ;;
      --ad-hoc-sign)
        sign_identity="-"
        ;;
      --smoke)
        smoke=1
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown option: $1"
        ;;
    esac
    shift
  done

  if [[ "${bundle_dylibs}" == "1" && -z "${sign_identity}" ]]; then
    sign_identity="-"
    echo "==> --bundle-dylibs rewrites Mach-O signatures; enabling ad-hoc signing"
  fi

  local cargo_args=(-p bmz-player)
  if [[ "${profile}" == "release" ]]; then
    cargo_args+=(--release)
  fi
  if [[ -n "${target_triple}" ]]; then
    cargo_args+=(--target "${target_triple}")
  fi

  if [[ "${skip_build}" -eq 0 ]]; then
    echo "==> Building bmz-player (${profile})"
    cargo build "${cargo_args[@]}"
  fi

  local target_base="${root}/target"
  if [[ -n "${target_triple}" ]]; then
    target_base="${target_base}/${target_triple}"
  fi
  local binary="${target_base}/${profile}/bmz-player"
  [[ -x "${binary}" ]] || die "missing executable: ${binary}"

  local version
  version="$(cargo_version)"
  [[ -n "${version}" ]] || die "failed to read workspace version"
  [[ -f "${root}/data/skins/default/select.json" ]] || die "missing bundled default skin"
  [[ -f "${root}/data/skins/Rmz-skin/play7main.luaskin" ]] || \
    die "missing Rmz-skin contents; run: git submodule update --init --recursive data/skins/Rmz-skin"
  [[ -f "${root}/data/skins/mz-select/music_select.luaskin" ]] || \
    die "missing mz-select contents; run: git submodule update --init --recursive data/skins/mz-select"
  [[ -f "${root}/data/songs/sample-playable/sample-playable.bms" ]] || \
    die "missing bundled sample song"
  [[ -f "${root}/assets/app-icon/bmz-player.icns" ]] || die "missing macOS app icon"

  local app_dir="${out_dir}/${app_name}.app"
  local contents_dir="${app_dir}/Contents"
  local macos_dir="${contents_dir}/MacOS"
  local resources_dir="${contents_dir}/Resources"
  local frameworks_dir="${contents_dir}/Frameworks"

  echo "==> Creating ${app_dir}"
  rm -rf "${app_dir}"
  mkdir -p "${macos_dir}" "${resources_dir}/skins" "${resources_dir}/songs" "${resources_dir}/licenses"

  copy_file "${binary}" "${macos_dir}/bmz-player"
  chmod 755 "${macos_dir}/bmz-player"

  copy_dir "${root}/data/skins/default" "${resources_dir}/skins/default"
  copy_dir "${root}/data/skins/Rmz-skin" "${resources_dir}/skins/Rmz-skin"
  copy_dir "${root}/data/skins/mz-select" "${resources_dir}/skins/mz-select"
  copy_dir "${root}/data/songs/sample-playable" "${resources_dir}/songs/sample-playable"
  copy_file "${root}/LICENSE" "${resources_dir}/licenses/BMZ-GPL-3.0-only.txt"
  copy_file "${root}/docs/licenses.md" "${resources_dir}/licenses/license-notes.md"
  copy_file "${root}/assets/app-icon/bmz-player.icns" "${resources_dir}/bmz-player.icns"

  write_info_plist "${contents_dir}/Info.plist" "bmz-player" "bmz-player.icns" "${app_name}" "${bundle_id}" "${version}"
  printf 'APPL????' > "${contents_dir}/PkgInfo"
  plutil -lint "${contents_dir}/Info.plist" >/dev/null

  if [[ "${bundle_dylibs}" == "1" ]]; then
    echo "==> Bundling non-system dylib dependencies"
    bundle_dylib_dependencies "${macos_dir}/bmz-player" "${frameworks_dir}"
  fi

  if [[ -n "${sign_identity}" ]]; then
    echo "==> Signing bundle (${sign_identity})"
    sign_bundle "${app_dir}" "${sign_identity}"
  fi

  if [[ "${smoke}" == "1" ]]; then
    echo "==> Running packaged smoke test"
    local smoke_data
    smoke_data="$(mktemp -d "${TMPDIR:-/tmp}/bmz-player-app-smoke.XXXXXX")"
    BMZ_DATA_DIR="${smoke_data}/data" "${macos_dir}/bmz-player" \
      --boot-play-sample \
      --smoke-exit-after-frames 3
  fi

  echo "==> Done"
  echo "${app_dir}"
}

main "$@"
