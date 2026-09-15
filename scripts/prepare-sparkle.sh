#!/usr/bin/env bash
set -euo pipefail
# Pinned official distribution; update the digest together with the version.
destination="${1:?usage: prepare-sparkle.sh DESTINATION}"
mkdir -p "${destination}"
archive="${destination}/Sparkle-2.9.6.tar.xz"
curl --fail --location --proto '=https' --proto-redir '=https' \
  https://github.com/sparkle-project/Sparkle/releases/download/2.9.6/Sparkle-2.9.6.tar.xz \
  --output "${archive}"
printf '%s  %s\n' 52bf9e88cdd972fc0c81501377a880e90d47031bd8ca5462488f843e2609e192 "${archive}" | shasum -a 256 --check
tar -xJf "${archive}" -C "${destination}"
test -d "${destination}/Sparkle.framework"
