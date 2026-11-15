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

exec "$ELECTRON" "$APP" "$@"