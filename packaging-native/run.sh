#!/bin/bash
# 极空间 Linux 原生版启动器
# 与 Wine 版 run.sh 的区别：不做 WINEPREFIX / 字体修正 / wineserver 处理，
# 也不加 --disable-gpu-compositing（那是 wined3d 的补丁，原生 Chromium 不需要）。
set -uo pipefail

HERE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
# run.sh 自己就在 files/ 里：electron/ 与 app/ 是它的同级目录。
FILES="$HERE"
ELECTRON_DIR="$FILES/electron"
APP_DIR="$FILES/app"
ELECTRON_BIN="${ZSPACE_ELECTRON:-$ELECTRON_DIR/electron}"

die() { echo "zspace-native: $*" >&2; exit 1; }

[ -x "$ELECTRON_BIN" ] || die "找不到可执行的 Electron 运行时：$ELECTRON_BIN"
[ -f "$APP_DIR/package.json" ] || die "应用目录不完整（缺 package.json）：$APP_DIR"

# Electron 的 resourcesPath 会用来解析应用名；包内已建好 resources/app 软链接，
# 这里只在缺失时兜底（普通用户对 /opt 无写权限，失败不致命）。
[ -e "$ELECTRON_DIR/resources/app" ] || ln -sfn "$APP_DIR" "$ELECTRON_DIR/resources/app" 2>/dev/null || true

# 部分环境无法启用 Chromium setuid sandbox；本包刻意不提供 4755 的 chrome-sandbox。
export ELECTRON_DISABLE_SANDBOX=1

# 这些变量会让 Electron 退化成纯 Node，直接崩在启动阶段。
unset NODE_OPTIONS ELECTRON_RUN_AS_NODE ELECTRON_NO_ATTACH_CONSOLE NODE_PATH

# 桌面代理工具会注入 http(s)_proxy，应用自身的 127.0.0.1 服务会被代理掉。
unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY
export no_proxy="localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16"
export NO_PROXY="$no_proxy"

# 默认不要追加 Chromium 开关：应用会把多余命令行参数当成“待上传文件”，
# 下次启动弹上传框（详见 .agents/memory.md 里 2026-11 的相关记录）。
# 调试时显式开：ZSPACE_NATIVE_ARGS="--remote-debugging-port=9222" run.sh
# shellcheck disable=SC2086
exec "$ELECTRON_BIN" "$APP_DIR" ${ZSPACE_NATIVE_ARGS:-} "$@"
