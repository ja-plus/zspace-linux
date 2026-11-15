#!/usr/bin/env bash
# 端到端自测：假主进程 + /splay HTTP 样片 + 窗口播放 + 键盘控制 + 截图证据。
# 依赖：ffmpeg、xdotool、xwininfo、ImageMagick(import/convert/identify)、python3。
set -uo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
OUT="$HERE/out"
MEDIA="$HERE/media"
BIN="$ROOT/target/release/zspace-player"
CONTROL_PORT=${CONTROL_PORT:-13800}
export DISPLAY=${DISPLAY:-:0}

mkdir -p "$OUT" "$MEDIA"
rm -f "$OUT"/*.jsonl "$OUT"/*.log "$OUT"/*.out "$OUT"/*.png "$OUT"/*.txt

echo "== 1/6 构建 =="
(cd "$ROOT" && cargo build --release) || { echo "FAIL: 构建失败"; exit 1; }

echo "== 2/6 生成样片 =="
if [ ! -f "$MEDIA/multitrack.mkv" ]; then
  printf '1\n00:00:00,500 --> 00:00:04,000\nEnglish subtitle line one\n\n2\n00:00:05,000 --> 00:00:09,000\nEnglish subtitle line two\n\n' > "$MEDIA/sub1.srt"
  printf '1\n00:00:00,500 --> 00:00:04,000\n中文字幕一号\n\n2\n00:00:05,000 --> 00:00:09,000\n中文字幕二号\n\n' > "$MEDIA/sub2.srt"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i testsrc2=size=960x540:rate=25:duration=45 \
    -f lavfi -i sine=frequency=440:duration=45 \
    -f lavfi -i sine=frequency=880:duration=45 \
    -i "$MEDIA/sub1.srt" -i "$MEDIA/sub2.srt" \
    -map 0:v -map 1:a -map 2:a -map 3:s -map 4:s \
    -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -b:a 96k -c:s srt \
    -metadata:s:a:0 language=eng -metadata:s:a:1 language=chi \
    -metadata:s:s:0 language=eng -metadata:s:s:1 language=chi \
    "$MEDIA/multitrack.mkv" || { echo "FAIL: 样片生成失败"; exit 1; }
fi

# 外挂字幕样片（12/2 与 10/1 用）
printf '1\n00:00:00,000 --> 00:00:20,000\n外挂字幕（12/2）\n\n' > "$MEDIA/sub-chi.srt"
printf '1\n00:00:00,000 --> 00:00:20,000\n本地字幕（10/1）\n\n' > "$MEDIA/sub-local.srt"
printf '1\n00:00:00,000 --> 00:00:20,000\n在线字幕（9/3）\n\n' > "$MEDIA/sub-search.srt"

echo "== 3/6 清理残留实例 =="
for pid in $(pgrep -x zspace-player 2>/dev/null); do kill "$pid" 2>/dev/null; done
for pid in $(pgrep -f "mock_main.py --media-dir $MEDIA" 2>/dev/null); do
  case "$pid" in "$$") ;; *) kill "$pid" 2>/dev/null ;; esac
done
sleep 0.3

echo "== 4/6 启动假主进程 =="
python3 "$HERE/mock_main.py" --media-dir "$MEDIA" --media multitrack.mkv \
  --port "$CONTROL_PORT" --close-after 36 --log "$OUT/mock.jsonl" \
  > "$OUT/mock.out" 2>&1 &
MOCK_PID=$!
trap 'kill $MOCK_PID 2>/dev/null; for pid in $(pgrep -x zspace-player 2>/dev/null); do kill $pid 2>/dev/null; done' EXIT

for _ in $(seq 1 50); do
  grep -q '"event": "ready"' "$OUT/mock.out" 2>/dev/null && break
  sleep 0.1
done
if ! grep -q '"event": "ready"' "$OUT/mock.out" 2>/dev/null; then
  echo "FAIL: 假主进程未就绪"; cat "$OUT/mock.out"; exit 1
fi
HTTP_PORT=$(python3 -c "import json;print(json.load(open('$OUT/mock.out'))['http_port'])" 2>/dev/null || true)
echo "control=$CONTROL_PORT http=$HTTP_PORT"

echo "== 5/6 启动播放器 =="
ENC=$(python3 - <<PY
import json, urllib.parse
print(urllib.parse.quote(json.dumps({"server": {"ip": "127.0.0.1", "port": $CONTROL_PORT}}, separators=(",", ":"))))
PY
)
"$BIN" "$ENC" "1" "1080,720" "zspace" zh "username=tester" \
  > "$OUT/player.out" 2> "$OUT/player.log" &
PLAYER_PID=$!

WID=""
for _ in $(seq 1 100); do
  WID=$(xdotool search --class ZspacePlayer 2>/dev/null | head -1)
  [ -n "$WID" ] && break
  sleep 0.1
done
if [ -z "$WID" ]; then
  echo "FAIL: 未找到播放器窗口"; grep -a "\[player\]" "$OUT/player.out" | tail -20; exit 1
fi
echo "window=$WID"

sleep 4
import -window root "$OUT/shot-window.png" 2>/dev/null
xwininfo -id "$WID" > "$OUT/window-info.txt" 2>&1

# 键盘控制：
#   n = 下一集（3/1 → 3/2）
#   a = 切音轨（6/1 → 6/3 新地址）
#   s = 切字幕（11/2）
#   l = 本地字幕（10/1）
#   y = 下一个码流（7/1 → 7/3）
#   ] = 倍速、f = 全屏
xdotool key --window "$WID" n 2>/dev/null; sleep 4.0
xdotool key --window "$WID" a 2>/dev/null; sleep 1.2
xdotool key --window "$WID" s 2>/dev/null; sleep 0.8
xdotool key --window "$WID" l 2>/dev/null; sleep 1.2
xdotool key --window "$WID" y 2>/dev/null; sleep 1.2
xdotool key --window "$WID" bracketright 2>/dev/null; sleep 0.4
xdotool key --window "$WID" f 2>/dev/null; sleep 1.8
import -window root "$OUT/shot-fullscreen.png" 2>/dev/null
xwininfo -id "$WID" > "$OUT/window-info-fullscreen.txt" 2>&1

# 暂停/继续 + seek（11/1 → 11/3 新地址）
xdotool key --window "$WID" space 2>/dev/null; sleep 1.0
xdotool key --window "$WID" space 2>/dev/null; sleep 0.6
xdotool key --window "$WID" Right 2>/dev/null; sleep 3.0

# 等播放器自己退出（mock 在 26s 后下发 f_type=0）
for _ in $(seq 1 400); do
  kill -0 "$PLAYER_PID" 2>/dev/null || break
  sleep 0.1
done
if kill -0 "$PLAYER_PID" 2>/dev/null; then
  echo "WARN: 播放器未在预期时间内退出，强制结束"
  kill "$PLAYER_PID" 2>/dev/null
fi
wait "$PLAYER_PID" 2>/dev/null
PLAYER_RC=$?
echo "player_rc=$PLAYER_RC"

# 等 mock 收尾
for _ in $(seq 1 30); do
  kill -0 "$MOCK_PID" 2>/dev/null || break
  sleep 0.1
done
kill "$MOCK_PID" 2>/dev/null
sleep 0.2

echo "== 6/6 校验 =="
python3 "$HERE/check_result.py" --out "$OUT" --rc "$PLAYER_RC" | tee "$OUT/summary.txt"
RESULT=${PIPESTATUS[0]}
if [ $RESULT -eq 0 ]; then
  echo "SELFTEST PASS"
else
  echo "SELFTEST FAIL"
fi
exit $RESULT
