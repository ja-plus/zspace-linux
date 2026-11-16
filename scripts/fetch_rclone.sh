#!/bin/bash
# Download upstream rclone v1.70.2 (the version the official ZSpaceMount.exe
# forks) and place the Linux amd64 binary in native/work/rclone/rclone.
#
# The binary is NOT committed (native/work/ is gitignored).
set -euo pipefail

VERSION="1.70.2"
ZIP_SHA256="7bfa813f426820d20436e48a4e0b9bf21977fcd513a48f2d28eede3136b4bacb"
BIN_SHA256="d1d99f58c1ad31592caa093eb400607725afd74eb3655bdeb6dc1886fed8fda8"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/native/work/rclone"
BIN="$DIR/rclone"
ZIP="$DIR/rclone-v${VERSION}-linux-amd64.zip"
URL="https://downloads.rclone.org/v${VERSION}/rclone-v${VERSION}-linux-amd64.zip"

mkdir -p "$DIR"
if [ -x "$BIN" ] && "$BIN" version 2>/dev/null | sed -n '1p' | grep -q "v${VERSION}"; then
    echo "rclone v${VERSION} already present: $BIN"
    exit 0
fi

if [ ! -f "$ZIP" ]; then
    echo "== downloading $URL =="
    curl -fL --connect-timeout 15 --max-time 300 -o "$ZIP" "$URL"
fi
echo "$ZIP_SHA256  $ZIP" | sha256sum -c -

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
unzip -q "$ZIP" -d "$tmp"
install -m 0755 "$tmp/rclone-v${VERSION}-linux-amd64/rclone" "$BIN"
echo "$BIN_SHA256  $BIN" | sha256sum -c -
rm -f "$ZIP"
"$BIN" version | head -2
echo "installed: $BIN"
