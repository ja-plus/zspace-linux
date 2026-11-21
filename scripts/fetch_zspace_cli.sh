#!/usr/bin/env bash
# 下载 zspace-cli（官方 Agent Toolkit 里的命令行工具）到 native/work/，
# 不进 git：仓库只存下载地址与校验和。
#
#   scripts/fetch_zspace_cli.sh            # 按当前架构下载
#   scripts/fetch_zspace_cli.sh --force    # 已存在也重下
#
# 产物：native/work/zspace-cli/zspace-cli（native/work/ 已 gitignore）
#
# 版本与校验和来自官方 release manifest：
#   native/research-remote/zspace-nas-agent-toolkit/runtime-artifacts.json
# 升级版本时同时改这里和那份 manifest，别只改一边。
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

VERSION="0.3.11"
BASE="https://update.zenithspace.net/AgentToolkit/20261023"

case "$(uname -m)" in
    x86_64|amd64)  PLAT="linux-x64";   SHA256="97ed9ea7fdeb88129498838eac3493a546008d2ef4e20778fd4d5538016616a7" ;;
    aarch64|arm64) PLAT="linux-arm64"; SHA256="200099ea0cbae6e1a4769a68975e628493301a6a19b2912043053f1a29b33fa2" ;;
    *) die "不支持的架构：$(uname -m)" ;;
esac

OUT_DIR="$ROOT/native/work/zspace-cli"
OUT="$OUT_DIR/zspace-cli"
mkdir -p "$OUT_DIR"

FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1
if [ -x "$OUT" ] && [ "$FORCE" -eq 0 ]; then
    ok "已存在：$OUT（加 --force 可重下）"
    exit 0
fi

URL="$BASE/zspace-cli-v${VERSION}-${PLAT}"
info "下载 zspace-cli $VERSION ($PLAT)"
curl -fL --retry 2 -o "$OUT.part" "$URL" || die "下载失败：$URL"
mv "$OUT.part" "$OUT"
chmod +x "$OUT"

GOT="$(sha256sum "$OUT" | cut -d' ' -f1)"
if [ "$GOT" != "$SHA256" ]; then
    warn "校验和不匹配：期望 $SHA256，实际 $GOT"
    warn "官方可能已发新版，请更新本脚本里的 VERSION/SHA256"
    exit 1
fi
ok "完成：$OUT（$(human_size "$(stat -c %s "$OUT")")）"
