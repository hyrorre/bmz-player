#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/package-flatpak.sh [OPTIONS]

Build a Linux Flatpak bundle for BMZ Player.

Options:
  --install       Install the built app for the current user.
  --bundle        Build a single-file .flatpak bundle (default).
  --no-bundle     Skip single-file bundle creation.
  --smoke         Run a short Flatpak smoke launch after building.
  --out-dir DIR   Output directory (default: dist/flatpak).
  -h, --help      Show this help.

Environment:
  BMZ_FLATPAK_OUT_DIR  Default output directory override.
  BMZ_FLATPAK_INSTALL=1
  BMZ_FLATPAK_BUNDLE=0
  BMZ_FLATPAK_SMOKE=1

Notes:
  This initial manifest allows network access during the build so Cargo can
  fetch crates. For Flathub-style reproducible builds, replace that with a
  generated cargo-sources.json before submission.
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

main() {
  local root
  root="$(repo_root)"
  cd "${root}"

  local app_id="net.hyrorre.BMZPlayer"
  local manifest="${root}/installer/flatpak/${app_id}.yml"
  local out_dir="${BMZ_FLATPAK_OUT_DIR:-${root}/dist/flatpak}"
  local install_app="${BMZ_FLATPAK_INSTALL:-0}"
  local build_bundle="${BMZ_FLATPAK_BUNDLE:-1}"
  local smoke="${BMZ_FLATPAK_SMOKE:-0}"

  while (($# > 0)); do
    case "$1" in
      --install)
        install_app=1
        ;;
      --bundle)
        build_bundle=1
        ;;
      --no-bundle)
        build_bundle=0
        ;;
      --smoke)
        smoke=1
        ;;
      --out-dir)
        shift
        [[ $# -gt 0 ]] || die "--out-dir requires a value"
        out_dir="$1"
        ;;
      -h|--help)
        usage
        return 0
        ;;
      *)
        die "unknown option: $1"
        ;;
    esac
    shift
  done

  need_command flatpak
  need_command flatpak-builder

  [[ -f "${manifest}" ]] || die "missing manifest: ${manifest}"
  [[ -f "${root}/data/skins/default/select.json" ]] || die "missing bundled default skin"
  [[ -f "${root}/data/skins/Rmz-skin/play7main.luaskin" ]] || \
    die "missing Rmz-skin contents; run: git submodule update --init --recursive data/skins/Rmz-skin"
  [[ -f "${root}/data/skins/mz-select/music_select.luaskin" ]] || \
    die "missing mz-select contents; run: git submodule update --init --recursive data/skins/mz-select"
  [[ -f "${root}/data/songs/sample-playable/sample-playable.bms" ]] || \
    die "missing bundled sample song"
  [[ -f "${root}/assets/app-icon/bmz-player-window-windows.png" ]] || die "missing Flatpak app icon"

  local version
  version="$(cargo_version)"
  [[ -n "${version}" ]] || die "failed to read workspace version"

  local build_dir="${out_dir}/build"
  local repo_dir="${out_dir}/repo"
  local bundle_path="${out_dir}/bmz-player-${version}-linux.flatpak"

  mkdir -p "${out_dir}"

  local builder_args=(
    --force-clean
    --user
    --disable-rofiles-fuse
    --install-deps-from=flathub
    "--repo=${repo_dir}"
  )
  if [[ "${install_app}" == "1" || "${smoke}" == "1" ]]; then
    builder_args+=(--install)
  fi

  echo "==> Building Flatpak"
  flatpak-builder "${builder_args[@]}" "${build_dir}" "${manifest}"

  if [[ "${build_bundle}" == "1" ]]; then
    echo "==> Creating bundle ${bundle_path}"
    flatpak build-bundle \
      "${repo_dir}" \
      "${bundle_path}" \
      "${app_id}" \
      --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
  fi

  if [[ "${smoke}" == "1" ]]; then
    echo "==> Running Flatpak smoke test"
    flatpak run "${app_id}" --boot-play-sample --smoke-exit-after-frames 3
  fi

  echo "==> Done"
  if [[ "${build_bundle}" == "1" ]]; then
    echo "${bundle_path}"
  else
    echo "${repo_dir}"
  fi
}

main "$@"
