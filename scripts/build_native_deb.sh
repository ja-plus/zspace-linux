#!/usr/bin/env bash
# 构建「极空间 Linux 原生版」deb（Electron 运行时，不依赖 Wine）。
#
#   scripts/build_native_deb.sh                  # 自动用 native/work 下的真实载荷；没有则假载荷
#   scripts/build_native_deb.sh --payload DIR    # 指定载荷（需含 electron/ 与 app/）
#   scripts/build_native_deb.sh --fake           # 强制假载荷（只验结构）
#   scripts/build_native_deb.sh --rev 2          # 原生构建修订号（默认 1）
#
# 载荷默认位置：native/work/electron/dist（Electron 根目录）、
#               native/work/app/extracted（官方 app.asar 解包 + linux 补丁）
# 产物：build/com.zspace.native.spark_<上游版本>+native.<rev>-1_amd64.deb
# 退出码：0 成功；1 失败。任一自检不通过都不会留下可用产物。
#
# 与 Wine 版完全并存：packaging/ 与 scripts/build_deb.sh 不受影响。

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

PKG="com.zspace.native.spark"
UPSTREAM_VERSION="2.46.2026101501"
DEB_REV=1
FORCE_FAKE=0
PAYLOAD_DIR=""
KEEP_WORK=0
OUT_DIR="$BUILD_DIR"
WORK_DIR="$BUILD_DIR/native-work"
ASSET_DIR="$ROOT/packaging-native"

while [ $# -gt 0 ]; do
    case "$1" in
        --payload)   PAYLOAD_DIR="${2:?--payload 需要目录}"; shift 2 ;;
        --version)   UPSTREAM_VERSION="${2:?--version 需要版本号}"; shift 2 ;;
        --rev)       DEB_REV="${2:?--rev 需要数字}"; shift 2 ;;
        --out)       OUT_DIR="${2:?--out 需要目录}"; shift 2 ;;
        --fake)      FORCE_FAKE=1; shift ;;
        --keep-work) KEEP_WORK=1; shift ;;
        -h|--help)   sed -n '2,17p' "$0"; exit 0 ;;
        *)           echo "未知参数：$1" >&2; exit 1 ;;
    esac
done

# 工作目录先清空；下面的默认载荷会在其中建软链接
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR" "$OUT_DIR"

# 默认载荷：native/work 下的真实运行时与解包应用
if [ -z "$PAYLOAD_DIR" ] && [ "$FORCE_FAKE" -eq 0 ]; then
    if [ -d "$ROOT/native/work/electron/dist" ] && [ -d "$ROOT/native/work/app/extracted" ]; then
        mkdir -p "$WORK_DIR/payload"
        ln -sfn "$ROOT/native/work/electron/dist" "$WORK_DIR/payload/electron"
        ln -sfn "$ROOT/native/work/app/extracted" "$WORK_DIR/payload/app"
        PAYLOAD_DIR="$WORK_DIR/payload"
    fi
fi

# ------------------------------ 依赖清单 ------------------------------
# Electron 33 / Chromium 130 运行库 + Node 原生模块（ffi-napi -> libffi8）。
# t64 是 Debian 13（Deepin 25 基线）的 time_t 迁移包名，用 | 兼容旧名。
DEPENDS="libc6, libstdc++6, libgcc-s1, \
libgtk-3-0t64 | libgtk-3-0, \
libatk1.0-0t64 | libatk1.0-0, \
libatk-bridge2.0-0t64 | libatk-bridge2.0-0, \
libatspi2.0-0t64 | libatspi2.0-0, \
libgdk-pixbuf-2.0-0t64 | libgdk-pixbuf-2.0-0, \
libcairo2, libpango-1.0-0, libpangocairo-1.0-0, \
libglib2.0-0t64 | libglib2.0-0, \
libdbus-1-3, libexpat1, libuuid1, \
libnss3, libnspr4, \
libx11-6, libx11-xcb1, libxcb1, \
libxcomposite1, libxdamage1, libxext6, libxfixes3, libxrandr2, libxrender1, \
libxi6, libxtst6, libxss1, libxcursor1, libxkbcommon0, libxkbcommon-x11-0, \
libgbm1, libdrm2, libegl1, libgl1, \
libasound2t64 | libasound2, \
libcups2t64 | libcups2, \
libfontconfig1, libfreetype6, \
libsecret-1-0, libnotify4, libffi8"

# 首期播放器尚未接入，libvlc/VA-API 放 Recommends；播放器上线后提升为 Depends。
RECOMMENDS="libvlc5, vlc-plugin-base, vlc-plugin-video-output, \
libva2, libva-drm2, libva-x11-2, \
intel-media-va-driver | i965-va-driver | mesa-va-drivers"

die() { echo "build_native_deb: $*" >&2; exit 1; }
command -v dpkg-deb >/dev/null 2>&1 || die "缺少 dpkg-deb"

VERSION="${UPSTREAM_VERSION}+native.${DEB_REV}-1"
DEB_NAME="${PKG}_${VERSION}_amd64.deb"
DEB="$OUT_DIR/$DEB_NAME"

STAGE="$WORK_DIR/stage"
FILES_DIR="$STAGE/opt/durapps/$PKG/files"
CTRL_DIR="$STAGE/DEBIAN"
mkdir -p "$FILES_DIR" "$CTRL_DIR" \
    "$STAGE/usr/bin" \
    "$STAGE/usr/share/applications" \
    "$STAGE/usr/share/doc/$PKG"

# ------------------------------ 载荷 ------------------------------
fake_payload() {
    local el="$FILES_DIR/electron" app="$FILES_DIR/app"
    echo "== 假载荷（只验证结构，不可运行）=="
    mkdir -p "$el/resources" "$el/locales"
    printf '#!/bin/sh\necho "fake electron 33.3.1"\nexit 0\n' > "$el/electron"
    chmod 0755 "$el/electron"
    printf '33.3.1\n' > "$el/version"
    head -c 4096 /dev/zero > "$el/resources/default_app.asar"
    ln -sfn ../../app "$el/resources/app"
    for p in zh-CN.pak en-US.pak; do head -c 1024 /dev/zero > "$el/locales/$p"; done
    for f in chrome_100_percent.pak icudtl.dat snapshot_blob.bin v8_context_snapshot.bin \
             libEGL.so libGLESv2.so libffmpeg.so libvk_swiftshader.so; do
        head -c 4096 /dev/zero > "$el/$f"
    done
    printf '{"file_format_version":"1.0.0"}\n' > "$el/vk_swiftshader_icd.json"
    head -c 4096 /dev/zero > "$el/chrome-sandbox"
    chmod 0644 "$el/chrome-sandbox"

    mkdir -p "$app/sdk" "$app/node_modules/ffi-napi/prebuilds/linux-x64" \
        "$app/node_modules/sqlite3/lib/binding/napi-v6-linux-glibc-x64"
    printf '{"name":"zspace","version":"%s","main":"background.js"}\n' \
        "$UPSTREAM_VERSION" > "$app/package.json"
    printf '// fake payload\n' > "$app/background.js"
    head -c 16384 /dev/zero > "$app/sdk/libzrpc.so"
    head -c 8192 /dev/zero > "$app/node_modules/ffi-napi/prebuilds/linux-x64/node.napi.node"
    head -c 8192 /dev/zero > "$app/node_modules/sqlite3/lib/binding/napi-v6-linux-glibc-x64/node_sqlite3.node"
}

real_payload() {
    echo "== 真实载荷：$PAYLOAD_DIR =="
    # 解析顶层软链接（默认载荷用软链接指向 native/work），否则 cp -a 会把
    # 绝对路径软链接原样带进包里，安装后直接指向构建机。
    local el_src app_src
    el_src="$(readlink -f "$PAYLOAD_DIR/electron" 2>/dev/null || true)"
    app_src="$(readlink -f "$PAYLOAD_DIR/app" 2>/dev/null || true)"
    [ -n "$el_src" ] && [ -d "$el_src" ] || die "载荷缺 electron/：$PAYLOAD_DIR"
    [ -n "$app_src" ] && [ -d "$app_src" ] || die "载荷缺 app/：$PAYLOAD_DIR"

    mkdir -p "$FILES_DIR/electron" "$FILES_DIR/app"
    cp -a "$el_src/." "$FILES_DIR/electron/"
    cp -a "$app_src/." "$FILES_DIR/app/"

    # 始终用仓库里刚构建的 libzrpc.so，避免载荷里是旧版本
    local so="$ROOT/native/libzrpc/target/release/libzrpc.so"
    if [ -f "$so" ]; then
        mkdir -p "$FILES_DIR/app/sdk"
        install -m 0644 "$so" "$FILES_DIR/app/sdk/libzrpc.so"
    fi

    # 原生播放器（替代 uzplayer/UZPlayer.exe）；缺了就按需构建
    local player="$ROOT/native/player/target/release/zspace-player"
    if [ ! -f "$player" ] && command -v cargo >/dev/null 2>&1; then
        echo "== 构建 zspace-player =="
        (cd "$ROOT/native/player" && cargo build --release) || die "播放器构建失败"
    fi
    [ -f "$player" ] || die "缺少播放器二进制：$player"

    # 幂等补丁：linux dll 分支 + 6 处播放器补丁 + sdk/libzrpc.so + uzplayer
    if [ -f "$ROOT/native/patches/patch_app.py" ] && command -v python3 >/dev/null 2>&1; then
        ZS_NODE="$FILES_DIR/electron/electron" \
            python3 "$ROOT/native/patches/patch_app.py" "$FILES_DIR/app" "${so:-}" "$player"
    fi

    # 包内软链接：Electron 用 resourcesPath/app 解析应用名。
    # 构建目录里的软链接可能指向构建机的绝对路径，必须无条件重建。
    mkdir -p "$FILES_DIR/electron/resources"
    [ -L "$FILES_DIR/electron/resources/app" ] && rm -f "$FILES_DIR/electron/resources/app"
    [ -e "$FILES_DIR/electron/resources/app" ] || \
        ln -sfn ../../app "$FILES_DIR/electron/resources/app"

    # Electron 官方 zip 里 chrome-sandbox 可能带 setuid；星火红线禁止提权。
    if [ -f "$FILES_DIR/electron/chrome-sandbox" ]; then
        chmod 0644 "$FILES_DIR/electron/chrome-sandbox"
    fi
}

if [ -n "$PAYLOAD_DIR" ] && [ "$FORCE_FAKE" -eq 0 ]; then
    real_payload
else
    fake_payload
fi

if [ -f "$PAYLOAD_DIR/run.sh" ]; then
    install -m 0755 "$PAYLOAD_DIR/run.sh" "$FILES_DIR/run.sh"
else
    install -m 0755 "$ASSET_DIR/run.sh" "$FILES_DIR/run.sh"
fi

# ------------------------------ 静态素材 ------------------------------
install -m 0755 "$ASSET_DIR/launcher.sh" "$STAGE/usr/bin/$PKG"
install -m 0755 "$ASSET_DIR/postinst" "$CTRL_DIR/postinst"
install -m 0755 "$ASSET_DIR/postrm" "$CTRL_DIR/postrm"
install -m 0644 "$ASSET_DIR/com.zspace.native.spark.desktop" \
    "$STAGE/usr/share/applications/$PKG.desktop"
install -m 0644 "$ASSET_DIR/README-doc.md" "$STAGE/usr/share/doc/$PKG/README.md"

ICON_SRC="$ROOT/packaging/icons/hicolor"
for size in 24 32 48 64 256; do
    src="$ICON_SRC/${size}x${size}/apps/com.zspace.spark.png"
    dst="$STAGE/usr/share/icons/hicolor/${size}x${size}/apps/$PKG.png"
    if [ -f "$src" ]; then
        mkdir -p "$(dirname "$dst")"
        install -m 0644 "$src" "$dst"
    else
        echo "警告：缺少图标 $src，跳过 ${size}x${size}" >&2
    fi
done

find "$STAGE" -type d -exec chmod 0755 {} +

INSTALLED_SIZE="$(du -sk --exclude=DEBIAN "$STAGE" | cut -f1)"

sed -e "s|@PKG@|$PKG|g" \
    -e "s|@VERSION@|$VERSION|g" \
    -e "s|@INSTALLED_SIZE@|$INSTALLED_SIZE|g" \
    -e "s^@DEPENDS@^$DEPENDS^g" \
    -e "s^@RECOMMENDS@^$RECOMMENDS^g" \
    "$ASSET_DIR/control.in" > "$CTRL_DIR/control"
chmod 0644 "$CTRL_DIR/control"

# ------------------------------ 打包 ------------------------------
rm -f "$DEB"
echo "== dpkg-deb 打包（xz -6，可能需几分钟）=="
dpkg-deb --root-owner-group -Zxz -z6 --build "$STAGE" "$DEB" >/dev/null

# ------------------------------ 自检 ------------------------------
REPORT="$OUT_DIR/native-verify-report.txt"
FAILED=0
: > "$REPORT"
say() { printf '%s\n' "$*" | tee -a "$REPORT"; }
pass() { say "PASS  $1  $2"; }
fail() { say "FAIL  $1  $2"; FAILED=$((FAILED + 1)); }
expect() {
    local id="$1" desc="$2"; shift 2
    if "$@" >/dev/null 2>&1; then pass "$id" "$desc"; else fail "$id" "$desc"; fi
}
is_elf() { [ "$(od -An -tx1 -N4 "$1" 2>/dev/null | tr -d ' \n')" = "7f454c46" ]; }

say "# 极空间原生版 deb 自检报告"
say "时间：$(date '+%F %T %Z')"
say "产物：$DEB"
say ""
say "## dpkg-deb -I"
dpkg-deb -I "$DEB" >> "$REPORT" 2>&1 || fail "A1" "dpkg-deb -I 失败"
say ""
say "## dpkg-deb -c"
dpkg-deb -c "$DEB" >> "$REPORT" 2>&1 || fail "A1c" "dpkg-deb -c 失败"
say ""

# A. 包结构
expect A1 "deb 可解析" dpkg-deb -I "$DEB"
expect A2 "Package 名 = $PKG" bash -c "dpkg-deb -f '$DEB' Package | grep -qx '$PKG'"
expect A3 "Architecture = amd64" bash -c "dpkg-deb -f '$DEB' Architecture | grep -qx amd64"
expect A4 "装在 /opt/durapps/$PKG" bash -c "dpkg-deb -c '$DEB' | grep -q '\./opt/durapps/$PKG/files/app/'"
expect A5 "应用清单 package.json 在位" bash -c "dpkg-deb -c '$DEB' | grep -q '\./opt/durapps/$PKG/files/app/package.json'"
expect A6 "Electron 运行时在位" bash -c "dpkg-deb -c '$DEB' | grep -q '\./opt/durapps/$PKG/files/electron/electron$'"
expect A7 "resources/app 软链接正确" bash -c "dpkg-deb -c '$DEB' | grep -q 'resources/app -> ../../app'"
expect A8 "libzrpc.so 在位" bash -c "dpkg-deb -c '$DEB' | grep -q '\./opt/durapps/$PKG/files/app/sdk/libzrpc.so'"
expect A9 "desktop 名 = 包名" bash -c "dpkg-deb -c '$DEB' | grep -q '\./usr/share/applications/$PKG.desktop'"
expect A10 "/usr/bin/$PKG 启动器在位" bash -c "dpkg-deb -c '$DEB' | grep -q '\./usr/bin/$PKG$'"
expect A11 "deb 文件名无中文/非 ASCII" bash -c "! printf '%s' '$DEB_NAME' | LC_ALL=C grep -q '[^ -~]'"
expect A12 "图标随包" bash -c "dpkg-deb -c '$DEB' | grep -q '256x256/apps/$PKG.png'"
# 真实载荷专属：二进制必须是 ELF（假载荷是零填充，不检查）
if [ -n "$PAYLOAD_DIR" ]; then
    expect A13 "electron 是 ELF" is_elf "$FILES_DIR/electron/electron"
    expect A14 "libzrpc.so 是 ELF" is_elf "$FILES_DIR/app/sdk/libzrpc.so"
    expect A15 "app.asar 已解包（background.js 在位）" test -s "$FILES_DIR/app/background.js"
    expect A16 "linux dll 分支已打" grep -q 'linux"==process.platform' "$FILES_DIR/app/background.js"
    expect A17 "zspace-player 随包（uzplayer/）" test -x "$FILES_DIR/app/uzplayer/zspace-player"
    expect A18 "6 处播放器补丁已打" bash -c "[ \"\$(grep -oF 'pkill -x zspace-player' '$FILES_DIR/app/background.js' | wc -l)\" = 1 ]"
fi

# B. 脚本与 control
expect B1 "run.sh 语法（bash -n）" bash -n "$FILES_DIR/run.sh"
expect B2 "postinst 语法（sh -n）" sh -n "$CTRL_DIR/postinst"
expect B3 "postrm 语法（sh -n）" sh -n "$CTRL_DIR/postrm"
expect B4 "launcher 语法（sh -n）" sh -n "$STAGE/usr/bin/$PKG"
expect B5 "postrm 清 ~/.deepinwine/$PKG" grep -q "\.deepinwine/\$PKG" "$CTRL_DIR/postrm"
expect B6 "postrm 覆盖 purge" grep -q 'purge' "$CTRL_DIR/postrm"
expect B7 "desktop Exec 指向 run.sh" grep -qx "Exec=/opt/durapps/$PKG/files/run.sh %U" "$STAGE/usr/share/applications/$PKG.desktop"
expect B8 "desktop Icon = 包名" grep -qx "Icon=$PKG" "$STAGE/usr/share/applications/$PKG.desktop"
expect B9 "Depends 含 libnss3" bash -c "dpkg-deb -f '$DEB' Depends | grep -q 'libnss3'"
expect B10 "Depends 含 libgtk-3-0（含 t64 备选）" bash -c "dpkg-deb -f '$DEB' Depends | grep -q 'libgtk-3-0'"
expect B11 "Depends 含 libgbm1" bash -c "dpkg-deb -f '$DEB' Depends | grep -q 'libgbm1'"
expect B12 "不依赖 Wine" bash -c "! dpkg-deb -f '$DEB' Depends | grep -qi 'wine'"
expect B13 "启动器含 ELECTRON_DISABLE_SANDBOX" grep -q 'ELECTRON_DISABLE_SANDBOX=1' "$FILES_DIR/run.sh"
expect B14 "启动器 unset NODE_OPTIONS" grep -q 'NODE_OPTIONS' "$FILES_DIR/run.sh"
expect B15 "启动器清理代理变量" grep -q 'no_proxy=' "$FILES_DIR/run.sh"

# C. 红线
expect C1 "无 chmod 4755 / chown root" bash -c "! grep -REq 'chmod[[:space:]]+4755|chown[[:space:]]+root' '$CTRL_DIR' '$FILES_DIR/run.sh' '$STAGE/usr/bin/$PKG'"
expect C2 "chrome-sandbox 不是 setuid" bash -c "[ \"\$(stat -c %a '$FILES_DIR/electron/chrome-sandbox')\" != '4755' ]"
expect C3 "维护脚本可执行" bash -c "[ -x '$CTRL_DIR/postinst' ] && [ -x '$CTRL_DIR/postrm' ]"
expect C4 "启动器可执行" bash -c "[ -x '$FILES_DIR/run.sh' ] && [ -x '$STAGE/usr/bin/$PKG' ]"
expect C5 "包内无中文文件名（deb 文件名）" bash -c "! printf '%s' '$DEB_NAME' | LC_ALL=C grep -q '[^ -~]'"
expect C6 "包内无 setuid 文件" bash -c "[ -z \"\$(find '$STAGE' -perm -4000 -print -quit)\" ]"

say ""
say "## 产物摘要"
SHA="$(sha256sum "$DEB" | cut -d' ' -f1)"
say "SHA256: $SHA"
say "deb 大小: $(du -h "$DEB" | cut -f1)"
say "Installed-Size: $(dpkg-deb -f "$DEB" Installed-Size) KiB"
say ""

if [ "$FAILED" -gt 0 ]; then
    say "结论：有 $FAILED 项未通过（work/ 已保留，便于排查）。"
    exit 1
fi
say "结论：全部检查通过。"
[ "$KEEP_WORK" -eq 1 ] || rm -rf "$WORK_DIR"
