#!/bin/bash
# Download upstream syncthing v1.27.7 (the version the official ZSpaceSync.exe
# is forked from) and place the Linux amd64 binary in native/work/syncthing/.
#
# The binary is NOT committed (native/work/ is gitignored). Run this before
# building the native deb / launching the sync feature.
set -euo pipefail

VERSION="1.27.7"
SHA256="068f3cf972113d101666678dd7b043d9363356667b8c4a81c84b1b54d4d342c3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/native/work/syncthing"
BIN="$DIR/syncthing"
TAR="$DIR/syncthing-linux-amd64-v${VERSION}.tar.gz"
URL="https://github.com/syncthing/syncthing/releases/download/v${VERSION}/syncthing-linux-amd64-v${VERSION}.tar.gz"

mkdir -p "$DIR"
if [ -x "$BIN" ] && "$BIN" --version 2>/dev/null | sed -n '1p' | grep -q "v${VERSION} "; then
    echo "syncthing v${VERSION} already present: $BIN"
    exit 0
fi

if [ ! -f "$TAR" ]; then
    echo "== downloading $URL =="
    curl -fL --connect-timeout 15 --max-time 300 -o "$TAR" "$URL"
fi
echo "$SHA256  $TAR" | sha256sum -c -

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
tar -xzf "$TAR" -C "$tmp"
install -m 0755 "$tmp/syncthing-linux-amd64-v${VERSION}/syncthing" "$BIN"
rm -f "$TAR"
"$BIN" --version
echo "installed: $BIN"
