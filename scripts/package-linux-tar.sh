#!/usr/bin/env bash
set -euo pipefail

archive=""
sources=""
if [[ $# == 3 && $1 == --verify ]]; then
  archive=$(realpath -- "$2")
  sources=$(realpath -- "$3")
  [[ -f "$archive" && -f "$sources" ]]
elif [[ $# != 0 ]]; then
  echo 'Usage: scripts/package-linux-tar.sh [--verify RUNTIME.tar.gz SOURCES.tar.gz] (CONTAINER_ENGINE=docker or podman)' >&2
  exit 1
fi
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
engine=${CONTAINER_ENGINE:-docker}
[[ $(uname -m) == x86_64 ]] || { echo 'Linux x86_64 host required' >&2; exit 1; }
[[ $(uname -s) == Linux ]] || { echo 'Linux host required' >&2; exit 1; }
mkdir -p dist/linux-tar
work=$(mktemp -d "$root/dist/linux-tar/.build.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/extracted"
context="$root/installer/linux-tar"
if [[ -z "$archive" ]]; then
  git diff --quiet HEAD -- || { echo 'Commit tracked changes before packaging' >&2; exit 1; }
  mkdir "$work/source"
  # Only committed objects enter the build context, never runtime data or secrets.
  git archive HEAD | tar -x -C "$work/source"
  for skin in Rmz-skin mz-select Luxez-Flat; do
    path="data/skins/$skin"
    commit=$(git rev-parse "HEAD:$path")
    git -C "$path" cat-file -e "$commit^{commit}"
    git -C "$path" archive "$commit" | tar -x -C "$work/source/$path"
    printf '%s %s\n' "$commit" "$path" >> "$work/source/BUILD-SUBMODULES"
  done
  git rev-parse HEAD > "$work/source/BUILD-COMMIT"
  python3 installer/linux-tar/manifest.py snapshot "$work/source"
  context="$work/source/installer/linux-tar"
  "$engine" build --target build -t bmz-linux-tar-build "$context"
  image_id=$("$engine" image inspect --format '{{.Id}}' bmz-linux-tar-build)
  "$engine" run --rm \
    -e "BMZ_BUILD_IMAGE_ID=$image_id" \
    -v "$work/source:/source:ro" -v "$work:/output" \
    bmz-linux-tar-build bash /source/installer/linux-tar/build.sh
  built_archive=$(find "$work" -maxdepth 1 -name 'bmz-player-v*-linux-x64.tar.gz')
  [[ -f "$built_archive" ]]
  # Preserve the completed archive even if the runtime check fails, so it can
  # be diagnosed and rechecked without recompilation. CI uploads only on success.
  archive="$root/dist/linux-tar/$(basename "$built_archive")"
  sources="${archive%.tar.gz}-sources.tar.gz"
  mv "${built_archive%.tar.gz}-sources.tar.gz" "$sources"
  mv "$built_archive" "$archive"
  python3 "$context/archives.py" "$archive" "$sources" --write-checksums
fi
python3 "$context/archives.py" "$archive" "$sources" --extract-to "$work/extracted"
package=$(find "$work/extracted/runtime" -mindepth 1 -maxdepth 1 -type d)
source_package=$(find "$work/extracted/sources" -mindepth 1 -maxdepth 1 -type d)
"$engine" build --target verify -t bmz-linux-tar-verify "$context"
"$engine" run --rm --network=none \
  -v "$package:/opt/BMZ Player:ro" bmz-linux-tar-verify
"$engine" build --target source-verify -t bmz-linux-tar-source-verify "$context"
"$engine" run --rm --network=none \
  -v "$source_package:/archives/sources:ro" bmz-linux-tar-source-verify
python3 "$context/report.py" "$archive" "$sources" "$work/extracted/verified.json"
echo "Verified archives: $archive $sources"
