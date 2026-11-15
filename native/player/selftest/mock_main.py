#!/usr/bin/env python3
"""假的极空间主进程：TCP 控制服务 + /splay HTTP 样片服务。

行为按 build/research-reuse/playback-plan.md 的消息表复刻：
  * 连接建立后下发 100/0（cookie/apiHost）
  * 下发 5/5（playList），故意把一帧拆成多段发送，并让两帧粘在一个 write 里
  * 记录播放器上行的所有 JSON（1/2、4/1、4/3、1/3 ...）
  * 到时间后下发 f_type=0 关闭播放器
"""
import argparse
import base64
import codecs
import json
import os
import socket
import socketserver
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlparse

LOG_LOCK = threading.Lock()
LOG_PATH = None
START = time.time()


def log(entry):
    entry["t"] = round(time.time() - START, 3)
    line = json.dumps(entry, ensure_ascii=False)
    with LOG_LOCK:
        print(line, flush=True)
        if LOG_PATH:
            with open(LOG_PATH, "a", encoding="utf-8") as fh:
                fh.write(line + "\n")


def frame(obj):
    payload = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    b64 = base64.b64encode(payload)
    return b"len=%d{%s" % (len(b64), b64)


class MediaHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        pass

    def _resolve(self):
        path = unquote(urlparse(self.path).path)
        parts = path.strip("/").split("/")
        if len(parts) != 3 or parts[0] != "splay":
            return None
        name = os.path.basename(parts[2])
        candidate = os.path.join(self.server.media_dir, name)
        return candidate if os.path.isfile(candidate) else None

    def _serve(self, with_body=True):
        target = self._resolve()
        if not target:
            self.send_error(404, "not found")
            return
        size = os.path.getsize(target)
        start, end = 0, size - 1
        status = 200
        range_header = self.headers.get("Range")
        if range_header and range_header.startswith("bytes="):
            spec = range_header.split("=", 1)[1].split(",")[0]
            a, _, b = spec.partition("-")
            if a:
                start = int(a)
            if b:
                end = int(b)
            end = min(end, size - 1)
            if start > end:
                self.send_response(416)
                self.send_header("Content-Range", f"bytes */{size}")
                self.end_headers()
                return
            status = 206

        length = end - start + 1
        self.send_response(status)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(length))
        if status == 206:
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.send_header("ETag", '"mock-media"')
        self.end_headers()
        log({"dir": "http", "method": self.command, "path": self.path,
             "range": range_header, "status": status, "bytes": length})
        if not with_body:
            return
        remaining = length
        with open(target, "rb") as fh:
            fh.seek(start)
            while remaining > 0:
                chunk = fh.read(min(64 * 1024, remaining))
                if not chunk:
                    break
                try:
                    self.wfile.write(chunk)
                except (BrokenPipeError, ConnectionResetError):
                    return
                remaining -= len(chunk)

    def do_GET(self):
        self._serve(True)

    def do_HEAD(self):
        self._serve(False)


class ControlServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class ControlHandler(socketserver.BaseRequestHandler):
    def handle(self):
        sock = self.request
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        log({"dir": "net", "event": "player-connected", "peer": self.client_address[1]})
        state = {
            "frames": 0, "progress": [], "closed": False,
            "audio": 0, "sub": 0,
            "seek_requests": [], "audio_requests": [], "subtitle_requests": [],
            "episode_requests": [], "local_subtitle_requests": [],
            "quality_requests": [],
        }
        stop = threading.Event()
        send_lock = threading.Lock()

        def reply(obj):
            obj.setdefault("server", {"ip": "127.0.0.1", "port": self.server.port})
            with send_lock:
                try:
                    sock.sendall(frame(obj))
                except OSError:
                    pass
            log({"dir": "main->player", "msg": obj})

        def loop():
            # 播放器 -> 主进程是裸 JSON（无长度前缀），可能两条消息粘在一起。
            text = ""
            dec = codecs.getincrementaldecoder("utf-8")()
            decoder = json.JSONDecoder()
            while not stop.is_set():
                try:
                    data = sock.recv(65536)
                except OSError:
                    break
                if not data:
                    break
                text += dec.decode(data)
                while True:
                    stripped = text.lstrip()
                    if not stripped:
                        text = ""
                        break
                    try:
                        msg, idx = decoder.raw_decode(stripped)
                    except json.JSONDecodeError:
                        break
                    text = stripped[idx:]
                    state["frames"] += 1
                    log({"dir": "player->main", "msg": msg})
                    ft, sty = msg.get("f_type"), msg.get("s_type")
                    if ft == 4 and sty == 3:
                        state["progress"].append(msg.get("sec"))
                    elif ft == 1 and sty == 3:
                        state["closed"] = True
                    elif ft == 11 and sty == 1:
                        # 转码流 seek：主进程重新请求 NAS 地址后回 11/3。
                        seek = float(msg.get("seek") or 0)
                        state["seek_requests"].append(seek)
                        item = self.server.item(
                            token="TRANSCODE", seek=seek,
                            audio_index=state["audio"], subtitle_index=state["sub"],
                        )
                        reply({"f_type": "11", "s_type": "3", "code": 200, **item})
                    elif ft == 6 and sty == 1:
                        # 转码流切音轨：回 6/3，带新地址。
                        index = int(msg.get("result") or 0)
                        state["audio_requests"].append(index)
                        state["audio"] = index
                        item = self.server.item(
                            token="TRANSCODE-AUDIO",
                            seek=float(msg.get("seek") or 0),
                            audio_index=index, subtitle_index=state["sub"],
                        )
                        reply({"f_type": "6", "s_type": "3", "code": 200, **item})
                    elif ft == 11 and sty == 2:
                        # 只切字幕：不回新地址，只回索引。
                        sidx = msg.get("subtitle_index")
                        state["subtitle_requests"].append(sidx)
                        state["sub"] = sidx
                        reply({"f_type": "11", "s_type": "3", "code": 200,
                               "subtitle_index": sidx})
                    elif ft == 3 and sty == 1:
                        # 选剧集：按 info(file_path) 找目标集，回 3/2 新地址。
                        info = msg.get("info") or msg.get("encrypted_path")
                        state["episode_requests"].append(info)
                        ep = self.server.episode_by_path(info)
                        item = self.server.item(
                            token="EPISODE", seek=0,
                            audio_index=state["audio"], subtitle_index=state["sub"],
                            file_path=ep["file_path"], label=ep["label"],
                        )
                        reply({"f_type": "3", "s_type": "2", "code": 200, **item})
                    elif ft == 10 and sty == 1:
                        # 本地字幕：真实主进程会弹选择框/上传后回 10/1；
                        # 这里直接返回一条新字幕。
                        state["local_subtitle_requests"].append(1)
                        reply({"f_type": "10", "s_type": "1", "code": 200,
                               "subtitle_list": self.server.local_subtitles(),
                               "subtitle_index": 6})
                    elif ft == 7 and sty == 1:
                        # 切码流：按 result 返回新地址 + 新 format_index。
                        index = int(msg.get("result") or 0)
                        state["quality_requests"].append(index)
                        item = self.server.item(
                            token="QUALITY",
                            seek=float(msg.get("seek") or 0),
                            audio_index=state["audio"], subtitle_index=state["sub"],
                        )
                        item["format_index"] = index
                        item["current_fmt"] = "HD" if index == 1 else "OP"
                        reply({"f_type": "7", "s_type": "3", "code": 200, **item})

        reader = threading.Thread(target=loop, daemon=True)
        reader.start()

        # 1) 两帧粘在一个 write 里（100/0 + 12/1），验证粘包
        cookie = {
            "server": {"ip": "127.0.0.1", "port": self.server.port},
            "f_type": "100", "s_type": "0",
            "apiHost": f"http://127.0.0.1:{self.server.http_port}",
            "cookie": {"app": "video", "token": "TESTTOKEN", "plat": "pc",
                       "nasid": "TESTNAS", "clientPublicIp": "127.0.0.1",
                       "version": "2.46.2026101501", "device_id": "test",
                       "device": "test-device"},
            "code": 200,
        }
        episode_list = {"server": {"ip": "127.0.0.1", "port": self.server.port},
                        "f_type": "12", "s_type": "1", "videoList": [], "code": 200}
        sock.sendall(frame(cookie) + frame(episode_list))

        # 2) 5/5 拆成 3 段发送，验证拆包
        playlist = self.server.playlist()
        raw = frame(playlist)
        third = len(raw) // 3
        for chunk in (raw[:third], raw[third:2 * third], raw[2 * third:]):
            time.sleep(0.05)
            sock.sendall(chunk)
        log({"dir": "net", "event": "playlist-sent", "bytes": len(raw), "fragments": 3})

        # 3) 官方播放列表后紧接着的 12/3（合集）、12/1（剧集），
        #    1.5 s 后再来一条 12/2（字幕列表 + 自动选择）。
        reply(self.server.collection_msg())
        reply(self.server.episode_msg())
        # 无发送入口的收包路径：用服务端主动回包模拟，验证播放器处理正确。
        reply(self.server.playlist_refresh_msg())   # 5/3
        reply(self.server.format_list_msg())        # 7/5
        reply(self.server.audio_ack_msg())          # 6/3（无 uri）
        reply(self.server.skip_ack_msg())           # 12/0
        reply(self.server.online_sub_msg())         # 9/3（带 subtitle_list/index）
        time.sleep(1.5)
        reply(self.server.subtitle_msg())

        deadline = time.time() + self.server.close_after
        while time.time() < deadline and not stop.is_set():
            time.sleep(0.2)
            if state["closed"]:
                break

        if not state["closed"]:
            log({"dir": "main->player", "msg": {"f_type": 0}})
            try:
                sock.sendall(frame({"f_type": 0}))
            except OSError:
                pass

        time.sleep(1.0)
        stop.set()
        try:
            sock.close()
        except OSError:
            pass
        log({"dir": "net", "event": "player-disconnected",
             "frames": state["frames"], "progress": state["progress"]})


def build_item(port, http_port, media_dir, name, token="TESTSECRET", seek=0.0,
               audio_index=0, subtitle_index=0, file_path=None, label="自测样片"):
    path = os.path.join(media_dir, name)
    duration = 45
    uri = f"http://127.0.0.1:{http_port}/splay/{token}/{name}"
    return {
        "index": 0, "active": 1, "label": label, "file_name": name,
        "file_path": file_path or ("/video/" + name),
        "file_size": os.path.getsize(path),
        "uri": uri, "duration": duration, "video_duration": duration,
        "seek": seek, "start_time": seek,
        "video_codec": "h264", "audio_index": audio_index,
        "subtitle_index": subtitle_index,
        "subtitle_hash": "0123456789abcdef0123456789abcdef01234567",
        "subtitle_setting": {"offset": "0.0", "position": "", "colour": "", "size": ""},
        "audio_list": [
            {"index": 0, "codec_name": "aac", "codec_type": "audio", "tags": {"language": "eng", "title": "English"}},
            {"index": 1, "codec_name": "aac", "codec_type": "audio", "tags": {"language": "chi", "title": "中文"}},
        ],
        "subtitle_list": [
            {"index": 0, "codec_name": "subrip", "codec_type": "subtitle", "tags": {"language": "eng"}, "data_source": 0},
            {"index": 1, "codec_name": "subrip", "codec_type": "subtitle", "tags": {"language": "chi"}, "data_source": 0},
        ],
        "format_list": [
            {"index": 0, "mod": "OP", "w": 960, "h": 540, "name": "原画", "transcoded": 0},
            {"index": 1, "mod": "HD", "w": 640, "h": 360, "name": "高清", "transcoded": 1},
        ],
        "format_index": 0, "current_fmt": "OP", "play_chapter": "",
    }


def build_playlist(port, http_port, media_dir, name):
    item = build_item(port, http_port, media_dir, name)
    return {
        "server": {"ip": "127.0.0.1", "port": port},
        "playList": [item],
        "skip": {"previous": 0, "start": 0, "end": 0, "preview": 0},
        "collectionInfo": {"collection_id": "testcoll", "classification_id": ""},
        "f_type": "5", "s_type": "5", "code": 200,
    }


def build_episodes(port, http_port, media_dir, name):
    return [
        {"index": 0, "label": "第1集", "file_path": "/video/" + name, "episode_path": "", "active": 1},
        {"index": 1, "label": "第2集", "file_path": "/video/ep2.mkv", "episode_path": "", "active": 0},
    ]


def main():
    global LOG_PATH
    parser = argparse.ArgumentParser()
    parser.add_argument("--media-dir", required=True)
    parser.add_argument("--media", required=True)
    parser.add_argument("--port", type=int, default=13612)
    parser.add_argument("--http-port", type=int, default=0)
    parser.add_argument("--close-after", type=float, default=22.0)
    parser.add_argument("--log", default=None)
    args = parser.parse_args()
    LOG_PATH = args.log
    if LOG_PATH and os.path.exists(LOG_PATH):
        os.remove(LOG_PATH)

    httpd = ThreadingHTTPServer(("127.0.0.1", args.http_port), MediaHandler)
    httpd.media_dir = args.media_dir
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    srv = ControlServer(("127.0.0.1", args.port), ControlHandler)
    srv.port = args.port
    srv.http_port = httpd.server_address[1]
    srv.close_after = args.close_after
    srv.item = lambda **kw: build_item(
        args.port, httpd.server_address[1], args.media_dir, args.media, **kw
    )
    srv.playlist = lambda: build_playlist(args.port, httpd.server_address[1], args.media_dir, args.media)
    episodes = build_episodes(args.port, httpd.server_address[1], args.media_dir, args.media)
    srv.episodes = lambda: episodes
    srv.episode_by_path = lambda info: next(
        (e for e in episodes if e["file_path"] == info), episodes[0]
    )
    srv.episode_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "12", "s_type": "1", "code": 200, "videoList": episodes,
    }
    srv.collection_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "12", "s_type": "3", "code": 200,
        "collectionInfo": {
            "collection_id": "testcoll", "title": "自测合集",
            "skip": {"previous": 0, "start": 0, "end": 0, "preview": 0},
            "history": {},
        },
    }
    srv.subtitle_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "12", "s_type": "2", "code": 200,
        "file_path": "/video/" + args.media,
        "subtitleList": [{
            "index": 5, "codec_name": "subrip", "codec_type": "subtitle",
            "tags": {"language": "chi", "title": "外挂字幕"},
            "url": "/splay/TESTSECRET/sub-chi.srt", "path": "",
            "hash": "ext5", "extraction": 0, "support_switch": True,
            "external": True, "data_source": 2,
        }],
        "subtitle_index": 5,
    }
    srv.local_subtitles = lambda: [{
        "index": 6, "codec_name": "subrip", "codec_type": "subtitle",
        "tags": {"language": "chi", "title": "本地添加"},
        "url": "/splay/TESTSECRET/sub-local.srt", "path": "",
        "hash": "local6", "extraction": 0, "support_switch": True,
        "external": True, "data_source": 4,
    }]
    srv.playlist_refresh_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "5", "s_type": "3", "code": 200,
        "playList": [build_item(args.port, httpd.server_address[1], args.media_dir, args.media)],
    }
    srv.format_list_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "7", "s_type": "5", "code": 200,
        "file_path": "/video/" + args.media,
        "format_list": [
            {"index": 0, "mod": "OP", "w": 960, "h": 540, "name": "原画", "transcoded": 0},
            {"index": 1, "mod": "HD", "w": 640, "h": 360, "name": "高清", "transcoded": 1},
            {"index": 2, "mod": "SD", "w": 480, "h": 270, "name": "标清", "transcoded": 1},
        ],
    }
    srv.audio_ack_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "6", "s_type": "3", "code": 200,
    }
    srv.skip_ack_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "12", "s_type": "0", "code": 200, "message": "ok",
    }
    srv.online_sub_msg = lambda: {
        "server": {"ip": "127.0.0.1", "port": args.port},
        "f_type": "9", "s_type": "3", "code": 200,
        "file_path": "/video/" + args.media,
        "subtitle_list": [{
            "index": 7, "codec_name": "subrip", "codec_type": "subtitle",
            "tags": {"language": "chi", "title": "在线下载"},
            "url": "/splay/TESTSECRET/sub-search.srt", "path": "",
            "hash": "search7", "extraction": 0, "support_switch": True,
            "external": True, "data_source": 5,
        }],
        "subtitle_index": 7,
    }

    print(json.dumps({"event": "ready", "control_port": args.port,
                      "http_port": httpd.server_address[1]}, ensure_ascii=False), flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        srv.server_close()


if __name__ == "__main__":
    main()
