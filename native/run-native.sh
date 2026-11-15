#!/bin/bash
# Native Linux launcher (M3) — Linux Electron + patched official app + native libzrpc.so
# No Wine. Reuses the proxy-clean rationale from packaging/run.sh.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$HERE/work"
ELECTRON="$WORK/electron/dist/electron"
APP="$WORK/app/extracted"

# Chromium sandbox cannot work in some environments; the app rejects --no-sandbox.
export ELECTRON_DISABLE_SANDBOX=1
# These would make Electron boot as plain Node and crash.
unset NODE_OPTIONS ELECTRON_RUN_AS_NODE ELECTRON_NO_ATTACH_CONSOLE NODE_PATH

# Force direct connect: desktop proxy tools inject http(s)_proxy that the app's
# Node-side axios honors unconditionally, breaking its own 127.0.0.1 services.
unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY
export no_proxy="localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16"
export NO_PROXY="$no_proxy"

# readPackageJson (renderer env-paths) resolves the app name via
# process.resourcesPath/app(.asar). Running from an extracted dir, that path
# must exist or the renderer gets name=null and path.join(null) crashes Vue.
ln -sfn "$APP" "$WORK/electron/dist/resources/app"

# Our libzrpc.so (LAN discovery + local proxy + zrpl relay). Build it when a
# compiler is available and the artifact is missing; otherwise expect a
# prebuilt file shipped next to this script.
SO="$HERE/libzrpc/target/release/libzrpc.so"
if [ ! -f "$SO" ] && command -v cargo >/dev/null 2>&1; then
    echo "== building libzrpc.so =="
    (cd "$HERE/libzrpc" && cargo build --release) || { echo "libzrpc build failed"; exit 1; }
fi

# Native player (replaces uzplayer/UZPlayer.exe). Same build-on-demand rule.
PLAYER="$HERE/player/target/release/zspace-player"
if [ ! -f "$PLAYER" ] && command -v cargo >/dev/null 2>&1; then
    echo "== building zspace-player =="
    (cd "$HERE/player" && cargo build --release) || { echo "player build failed"; exit 1; }
fi

if [ -d "$APP" ]; then
    # Electron doubles as Node for the background.js syntax check.
    [ -x "$ELECTRON" ] && export ZS_NODE="$ELECTRON"
    # Idempotent: dll branch + 6 player patches + sdk/libzrpc.so + uzplayer/zspace-player
    python3 "$HERE/patches/patch_app.py" "$APP" "${SO:-}" "${PLAYER:-}" \
        || { echo "app patch failed"; exit 1; }
else
    echo "warning: $APP missing; app not patched" >&2
fi

exec "$ELECTRON" "$APP" "$@"