#!/usr/bin/env bash
# 挂载线自测：mock WebDAV → 生成 conf → mount → 读 → 写 → rc status → shutdown → 清理
#
# 依赖：本机可用的 FUSE3（/dev/fuse + fusermount3）、python3。
# 运行：native/mount/selftest/run_selftest.sh
# 环境变量：MON（监控器路径）、RCLONE（rclone 路径）
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
MON=${MON:-$HERE/../target/release/zspace-mount-monitor}
RCLONE=${RCLONE:-$ROOT/build/mount-runtime/rclone-v1.70.2-linux-amd64/rclone}
OUT=$HERE/out
WORK=$(mktemp -d /tmp/zspace-mount-selftest.XXXXXX)
SERVE_PID=""
MON_PID=""
PASS=0
FAIL=0

rm -rf "$OUT"
mkdir -p "$OUT" "$WORK/nas" "$WORK/home" "$WORK/cache"

log() { printf '%s\n' "$*" >>"$OUT/selftest.log"; printf '%s\n' "$*"; }
ok() { PASS=$((PASS + 1)); log "PASS  $*"; }
bad() { FAIL=$((FAIL + 1)); log "FAIL  $*"; }

cleanup() {
  if [ -n "$MON_PID" ] && kill -0 "$MON_PID" 2>/dev/null; then
    kill -TERM "$MON_PID" 2>/dev/null || true
    wait "$MON_PID" 2>/dev/null || true
  fi
  if [ -n "$SERVE_PID" ] && kill -0 "$SERVE_PID" 2>/dev/null; then
    kill -TERM "$SERVE_PID" 2>/dev/null || true
    wait "$SERVE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

[ -x "$MON" ] || { log "monitor not built: $MON"; exit 1; }
[ -x "$RCLONE" ] || { log "rclone not found: $RCLONE"; exit 1; }
[ -e /dev/fuse ] || { log "no /dev/fuse"; exit 1; }

PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')
log "workdir: $WORK"
log "monitor: $MON"
log "rclone:  $RCLONE ($("$RCLONE" version | head -1))"

# mock NAS：rclone 自带 webdav server；预置读样本（避免 dir-cache 影响）。
printf 'from-nas\n' > "$WORK/nas/read.txt"
printf 'hidden\n' > "$WORK/nas/.hidden.txt"
"$RCLONE" serve webdav "$WORK/nas" --addr "127.0.0.1:$PORT" \
  --log-file "$OUT/webdav.log" >"$OUT/webdav.stdout" 2>&1 &
SERVE_PID=$!
READY=0
for _ in $(seq 1 50); do
  if python3 -c "import socket,sys;s=socket.socket();sys.exit(0 if s.connect_ex(('127.0.0.1',$PORT))==0 else 1)"; then
    READY=1
    break
  fi
  sleep 0.2
done
if [ "$READY" -eq 1 ]; then ok "mock WebDAV 在 127.0.0.1:$PORT 就绪"; else bad "mock WebDAV 未就绪"; exit 1; fi

MP="$WORK/cache/极空间-个人"
CONF="$WORK/home/mount.conf"

# 1) 生成 conf（与官方 config --add 同参数）
"$MON" config --add --program="$RCLONE" --home-dir="$WORK/home" \
  --url="http://127.0.0.1:$PORT" --user=alice --pass=secret --obscure \
  >"$OUT/01-config.json"
if grep -q '"status":200' "$OUT/01-config.json"; then ok "config --add 返回 status=200"; else bad "config --add 失败"; fi
if grep -q '^\[uzmount\]' "$CONF" && grep -q '^type = webdav' "$CONF" && grep -q "^url = http://127.0.0.1:$PORT" "$CONF"; then
  ok "mount.conf 含 [uzmount]/webdav/正确 url"
else
  bad "mount.conf 字段不符"
fi
if grep -q '^pass = secret$' "$CONF"; then bad "pass 未 obscure"; else ok "pass 已 obscure"; fi

# 2) check（官方挂载前预检）
"$MON" check --cache-dir="$WORK/cache" --volume=zspace >"$OUT/02-check.json"
if python3 -c "import json,sys;d=json.load(open(sys.argv[1]))['data'];sys.exit(0 if d['can_read'] and d['can_write'] and d['is_volume_valid'] else 1)" "$OUT/02-check.json"; then
  ok "check 返回 can_read/can_write/is_volume_valid"
else
  bad "check 预检字段不满足"
fi

# 3) 挂载（官方根命令 + 同参数）
"$MON" --program="$RCLONE" --home-dir="$WORK/home" --cache-dir="$WORK/cache" \
  --cache-max-size=1G --volume-label='极空间-个人' --volume=zspace \
  >"$OUT/03-mount.stdout" 2>"$OUT/03-mount.stderr" &
MON_PID=$!

MONTED=0
for _ in $(seq 1 100); do
  if grep -Fq " $MP " /proc/self/mountinfo; then MONTED=1; break; fi
  if ! kill -0 "$MON_PID" 2>/dev/null; then break; fi
  sleep 0.2
done
if [ "$MONTED" -eq 1 ]; then ok "挂载点 $MP 已在 mountinfo 中"; else bad "挂载点未出现"; fi
if [ -s "$WORK/home/mount.port" ]; then ok "mount.port 已写出"; else bad "mount.port 缺失"; fi

if [ "$MONTED" -eq 1 ]; then
  # 4) 读 NAS 侧预置文件
  GOT=$(cat "$MP/read.txt" 2>/dev/null | tr -d '\r' || true)
  if [ "$GOT" = "from-nas" ]; then ok "从挂载点读到 NAS 文件 read.txt"; else bad "读取 NAS 文件失败: '$GOT'"; fi

  # 5) 写挂载点 → 回写 mock NAS
  printf 'hello-from-mount\n' > "$MP/hello.txt"
  SYNCED=0
  for _ in $(seq 1 60); do
    if [ -f "$WORK/nas/hello.txt" ]; then SYNCED=1; break; fi
    sleep 0.5
  done
  if [ "$SYNCED" -eq 1 ]; then ok "写挂载点后 mock NAS 侧出现 hello.txt"; else bad "写入未回写 NAS"; fi
  if grep -q '^hello-from-mount$' "$WORK/nas/hello.txt" 2>/dev/null; then ok "写回内容一致"; else bad "写回内容不一致"; fi

  # 官方默认用 --filter=- .*/ 与 --filter=- .* 隐藏点开头条目
  if [ ! -e "$MP/.hidden.txt" ]; then ok "默认隐藏点开头文件"; else bad "点开头文件未被隐藏"; fi
fi

# 6) rc /mount/status（官方 rclone fork 端点，由监控器兼容实现）
RCPORT=$(tr -d '[:space:]' < "$WORK/home/mount.port")
"$MON" rc --rc-addr="http://127.0.0.1:$RCPORT/mount/status" >"$OUT/06-status.json"
if python3 -c "import json,sys;d=json.load(open(sys.argv[1]));sys.exit(0 if d['status']==200 and d['data']['is_running'] and d['data']['is_mounting'] else 1)" "$OUT/06-status.json"; then
  ok "rc /mount/status 返回 is_running+is_mounting"
else
  bad "rc /mount/status 不符"
fi

# 7) 卸载：官方 /shutdown → rclone 正常退出 → 监控器清理
"$MON" rc --rc-addr="http://127.0.0.1:$RCPORT/shutdown" >"$OUT/07-shutdown.json"
STOPPED=0
for _ in $(seq 1 100); do
  if ! kill -0 "$MON_PID" 2>/dev/null; then STOPPED=1; break; fi
  sleep 0.2
done
if [ "$STOPPED" -eq 0 ]; then
  kill -TERM "$MON_PID" 2>/dev/null || true
  wait "$MON_PID" 2>/dev/null || true
fi
if [ "$STOPPED" -eq 1 ]; then ok "shutdown 后监控器自行退出"; else bad "监控器未退出"; fi
if grep -Fq " $MP " /proc/self/mountinfo; then bad "挂载点仍挂载"; else ok "挂载点已卸载"; fi
if [ ! -e "$WORK/home/mount.port" ] && [ ! -e "$WORK/home/mount.lock" ]; then
  ok "mount.port / mount.lock 已清理"
else
  bad "挂载状态文件残留"
fi

# 8) 磁盘用量子命令（JS checkCacheSize 走 disk --size）
"$MON" disk --size --dir="$WORK/cache" >"$OUT/08-disksize.json"
if python3 -c "import json,sys;d=json.load(open(sys.argv[1]));sys.exit(0 if d['status']==200 and isinstance(d['data'],int) else 1)" "$OUT/08-disksize.json"; then
  ok "disk --size 返回整数"
else
  bad "disk --size 输出不符"
fi

log "----------------------------------------"
# 留证：监控器/rclone 日志与本次生成的 conf
cp "$WORK/home/mount_monitor.log" "$OUT/mount_monitor.log" 2>/dev/null || true
cp "$WORK/home/mount.log" "$OUT/rclone-mount.log" 2>/dev/null || true
cp "$CONF" "$OUT/mount.conf.txt" 2>/dev/null || true
log "PASS=$PASS FAIL=$FAIL"
log "证据目录: $OUT"
[ "$FAIL" -eq 0 ]
