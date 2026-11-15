#!/usr/bin/env python3
"""校验 run_selftest.sh 的产物，输出逐项 PASS/FAIL 并给出退出码。"""
import argparse
import json
import os
import re
import subprocess
import sys


def load_mock(path):
    entries = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                entries.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return entries


def parse_xwininfo(path):
    text = open(path, encoding="utf-8", errors="replace").read()
    def grab(pattern, default=0):
        m = re.search(pattern, text)
        return int(m.group(1)) if m else default
    return {
        "x": grab(r"Absolute upper-left X:\s+(-?\d+)"),
        "y": grab(r"Absolute upper-left Y:\s+(-?\d+)"),
        "w": grab(r"Width:\s+(\d+)"),
        "h": grab(r"Height:\s+(\d+)"),
        "viewable": "Map State: IsViewable" in text,
    }


def image_stddev(path):
    try:
        out = subprocess.check_output(
            ["identify", "-format", "%[fx:standard_deviation]", path], text=True
        )
        return float(out.strip())
    except Exception:  # noqa: BLE001
        return -1.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--rc", type=int, default=0)
    args = ap.parse_args()
    out = args.out

    checks = []

    def check(name, ok, detail=""):
        checks.append((name, bool(ok), detail))

    entries = load_mock(os.path.join(out, "mock.jsonl"))
    upstream = [e["msg"] for e in entries if e.get("dir") == "player->main" and isinstance(e.get("msg"), dict)]
    http = [e for e in entries if e.get("dir") == "http"]
    connections = [e for e in entries if e.get("event") == "player-connected"]

    check("TCP 连接建立", len(connections) == 1, f"connections={len(connections)}")
    check("播放地址走了 /splay", any("/splay/" in str(h.get("path", "")) for h in http), f"http_requests={len(http)}")
    check("HTTP 返回 206 Partial Content", any(h.get("status") == 206 for h in http),
          f"statuses={[h.get('status') for h in http]}")

    ack = [m for m in upstream if m.get("f_type") == 1 and m.get("s_type") == 2]
    check("上行 1/2 播放开始（初始 + 换集各一次）",
          len(ack) == 2 and all("/splay/" in str(m.get("info")) for m in ack),
          f"count={len(ack)}")

    progress = [m for m in upstream if m.get("f_type") == 4 and m.get("s_type") == 3]
    secs = [int(m.get("sec", 0)) for m in progress]
    check("上行 4/3 进度 ≥2 条", len(progress) >= 2, f"count={len(progress)} secs={secs}")
    check("进度推进到 seek 目标之后", len(secs) >= 2 and max(secs) >= 7, f"secs={secs}")
    check("进度带时长", any(int(m.get("video_duration", 0)) > 0 for m in progress),
          f"durations={[m.get('video_duration') for m in progress]}")
    check("进度带音轨/字幕索引",
          all("audio_index" in m and "subtitle_index" in m for m in progress),
          "")

    seek_req = [m for m in upstream if m.get("f_type") == 11 and m.get("s_type") == 1]
    check("上行 11/1 请求重新出地址", len(seek_req) >= 1 and float(seek_req[-1].get("seek", 0)) >= 3,
          f"count={len(seek_req)} seek={[m.get('seek') for m in seek_req]}")

    audio_req = [m for m in upstream if m.get("f_type") == 6 and m.get("s_type") == 1]
    check("上行 6/1 请求切音轨", len(audio_req) >= 1 and int(audio_req[-1].get("result", -1)) == 1,
          f"count={len(audio_req)} result={[m.get('result') for m in audio_req]}")

    sub_req = [m for m in upstream if m.get("f_type") == 11 and m.get("s_type") == 2]
    check("上行 11/2 请求切字幕", len(sub_req) >= 1 and int(sub_req[-1].get("subtitle_index", -1)) == 1,
          f"count={len(sub_req)} idx={[m.get('subtitle_index') for m in sub_req]}")

    transcode_paths = [str(h.get("path", "")) for h in http]
    check("seek/切音轨后使用新转码地址", any("TRANSCODE" in p for p in transcode_paths),
          f"paths={transcode_paths}")

    ep_req = [m for m in upstream if m.get("f_type") == 3 and m.get("s_type") == 1]
    check("上行 3/1 请求切集", len(ep_req) == 1 and "ep2.mkv" in str(ep_req[0].get("info")),
          f"count={len(ep_req)} info={[m.get('info') for m in ep_req]}")

    local_sub_req = [m for m in upstream if m.get("f_type") == 10 and m.get("s_type") == 1]
    check("上行 10/1 请求本地字幕", len(local_sub_req) == 1,
          f"count={len(local_sub_req)}")

    main_msgs = [e["msg"] for e in entries if e.get("dir") == "main->player" and isinstance(e.get("msg"), dict)]
    def has_main(ft, sty):
        return any(m.get("f_type") == ft and m.get("s_type") == sty for m in main_msgs)
    check("下行 12/1 剧集列表", has_main("12", "1"), f"main_msgs={len(main_msgs)}")
    check("下行 12/2 字幕列表", has_main("12", "2"))
    check("下行 12/3 合集信息", has_main("12", "3"))
    check("下行 5/3 刷新播放列表", has_main("5", "3"))
    check("下行 7/5 码流列表", has_main("7", "5"))
    check("下行 6/3 空回包", has_main("6", "3"))
    check("下行 12/0 设置回包", has_main("12", "0"))
    check("下行 9/3 在线字幕回包", has_main("9", "3"))

    quality_req = [m for m in upstream if m.get("f_type") == 7 and m.get("s_type") == 1]
    check("上行 7/1 请求切码流", len(quality_req) == 1 and int(quality_req[0].get("result", -1)) == 1,
          f"count={len(quality_req)} result={[m.get('result') for m in quality_req]}")
    check("切码流后使用新地址", any("QUALITY" in p for p in transcode_paths),
          f"paths={transcode_paths}")

    heartbeat = [m for m in upstream if m.get("f_type") == 4 and m.get("s_type") == 1]
    check("上行 4/1 心跳", len(heartbeat) >= 1, f"count={len(heartbeat)}")

    stop = [m for m in upstream if m.get("f_type") == 1 and m.get("s_type") == 3]
    check("上行 1/3 停止（且只发一次）", len(stop) == 1, f"count={len(stop)}")
    check("播放器正常退出", args.rc == 0, f"rc={args.rc}")

    log_path = os.path.join(out, "player.log")
    log = open(log_path, encoding="utf-8", errors="replace").read() if os.path.exists(log_path) else ""
    check("日志: 收到 playList", "收到播放列表: 1 项" in log)
    check("日志: 发送 11/1 并收到 11/3", "发送 11/1 请求重新出地址" in log and "收到 11/3 重新出地址" in log)
    check("日志: 发送 6/1 并收到 6/3", "发送 6/1 请求切换音轨" in log and "收到 6/3 音轨切换结果" in log)
    check("日志: 发送 11/2 并收到字幕结果", "发送 11/2 请求切换字幕" in log and "收到 11/3 字幕结果" in log)
    check("日志: 12/x 三条消息", all(k in log for k in ["收到剧集列表 12/1: 2 项", "收到字幕列表 12/2: 1 项", "收到合集信息 12/3"]))
    check("日志: 5/3 / 7/5 / 6/3 / 12/0", all(k in log for k in [
        "收到刷新后的播放列表 5/3", "收到码流列表 7/5: 3 项",
        "收到 6/3（无新地址", "收到 12/0 片头片尾设置回包"]))
    check("日志: 9/3 在线字幕", "9/3 返回字幕列表: 1 项" in log and "已加载外挂字幕 index=7" in log)
    check("日志: 7/1 切码流", "发送 7/1 请求切换码流" in log and "收到 7/3 码流切换结果" in log)
    check("日志: 12/2 自动加载外挂字幕", "已加载外挂字幕 index=5" in log)
    check("日志: 3/1 切集", "发送 3/1 切换剧集" in log and "收到新播放地址 (f_type=3 / s_type=2)" in log)
    check("日志: 10/1 本地字幕", "发送 10/1 请求主进程打开本地字幕选择" in log and "已加载外挂字幕" in log)
    check("外挂字幕实际被拉取", any(p.endswith("sub-chi.srt") for p in transcode_paths)
          and any(p.endswith("sub-local.srt") for p in transcode_paths)
          and any(p.endswith("sub-search.srt") for p in transcode_paths),
          f"paths={transcode_paths}")
    check("日志: 倍速", "倍速 → 1.25x" in log)
    check("日志: 暂停/继续", log.count("暂停状态切换") >= 2, f"count={log.count('暂停状态切换')}")

    win = parse_xwininfo(os.path.join(out, "window-info.txt"))
    check("窗口可见且有画面尺寸", win["viewable"] and win["w"] >= 320 and win["h"] >= 180,
          f"{win['w']}x{win['h']}")

    shot = os.path.join(out, "shot-window.png")
    cropped = os.path.join(out, "crop-window.png")
    sd = -1.0
    if os.path.exists(shot) and win["w"] > 0:
        subprocess.run(
            ["convert", shot, "-crop", f"{win['w']}x{win['h']}+{win['x']}+{win['y']}", "+repage", cropped],
            check=False,
        )
        sd = image_stddev(cropped)
    check("窗口截图非黑屏/有画面", sd > 0.05, f"stddev={sd:.3f}")

    full = parse_xwininfo(os.path.join(out, "window-info-fullscreen.txt"))
    try:
        gw, gh = subprocess.check_output(["xdotool", "getdisplaygeometry"], text=True).split()
        screen_w = int(gw)
    except Exception:  # noqa: BLE001
        screen_w = 0
    check("F 键全屏生效", screen_w > 0 and full["w"] >= int(screen_w * 0.95),
          f"fullscreen={full['w']}x{full['h']} screen_w={screen_w}")

    fshot = os.path.join(out, "shot-fullscreen.png")
    check("全屏截图存在", os.path.exists(fshot), fshot)

    print("\n===== 自测结果 =====")
    failures = 0
    for name, ok, detail in checks:
        mark = "PASS" if ok else "FAIL"
        if not ok:
            failures += 1
        print(f"[{mark}] {name}" + (f"  ({detail})" if detail else ""))
    print(f"\n{len(checks) - failures}/{len(checks)} 通过")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
