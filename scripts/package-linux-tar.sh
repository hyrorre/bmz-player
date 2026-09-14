#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 0 ]]; then
  echo 'Usage: scripts/package-linux-tar.sh (CONTAINER_ENGINE=docker or podman)' >&2
  exit 1
fi
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
engine=${CONTAINER_ENGINE:-docker}
[[ $(uname -m) == x86_64 ]] || { echo 'Linux x86_64 host required' >&2; exit 1; }
[[ $(uname -s) == Linux ]] || { echo 'Linux host required' >&2; exit 1; }
git diff --quiet HEAD -- || { echo 'Commit tracked changes before packaging' >&2; exit 1; }
mkdir -p dist/linux-tar
work=$(mktemp -d "$root/dist/linux-tar/.build.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/source" "$work/extracted"
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
context="$work/source/installer/linux-tar"
"$engine" build --target build -t bmz-linux-tar-build "$context"
"$engine" run --rm \
  -v "$work/source:/source:ro" -v "$work:/output" \
  bmz-linux-tar-build bash /source/installer/linux-tar/build.sh
archive=$(find "$work" -maxdepth 1 -name 'bmz-player-*-linux-x86_64.tar.gz')
[[ -f "$archive" ]]
tar -xzf "$archive" -C "$work/extracted"
package=$(find "$work/extracted" -mindepth 1 -maxdepth 1 -type d)
"$engine" build --target verify -t bmz-linux-tar-verify "$context"
"$engine" run --rm --network=none \
  -v "$package:/opt/BMZ Player:ro" bmz-linux-tar-verify
cp "$archive" dist/linux-tar/
(cd dist/linux-tar && sha256sum "$(basename "$archive")" > SHA256SUMS.txt)
echo "Verified archive: dist/linux-tar/$(basename "$archive")"
