# zspace-player —— 极空间播放器的协议兼容实现（libvlc 后端）

替代官方 `UZPlayer.exe` 的 Linux 原生进程：**只做「替掉那一个二进制」**，
命令行参数、TCP 帧协议、上下行消息、配置文件回落都与原版对齐。
不改官方主进程、渲染器，也不引入原版没有的播放器选项或外部播放器适配。

- 代码：`native/player/`
- 产物：`native/player/target/release/zspace-player`（单文件，约 1 MB）
- 引擎：系统 libvlc 3.x（运行时 `dlopen`，不依赖 `libvlc-dev`）
- 依据：`build/research-reuse/playback-plan.md`（协议与行为调研结论）
- 接入官方主进程的补丁规格：`PATCH-SPEC.md`（6 处 `background.js` 改动，供主线实施）

---

## 1. 构建

```bash
cd native/player
cargo build --release
cargo test --release        # 协议帧解析 9 项单测
```

依赖：Rust 1.7+，系统有 `libvlc.so.5` 与 VLC 插件目录。
本机（Deepin 25）实测用的是 `/opt/apps/org.videolan.www.vlc` 自带的
VLC 3.0.21（运行时 + 插件 + 私有 FFmpeg）。程序会自动探测：

1. `ZSPACE_VLC_LIB` / `ZSPACE_VLC_PLUGINS`（仅调试用，不属于用户选项）
2. Debian/Deepin multi-arch：`/usr/lib/<arch>/libvlc.so.5` + `/usr/lib/<arch>/vlc/plugins`
3. Deepin 应用商店版 VLC：`/opt/apps/org.videolan.www.vlc/files/lib/...`

探测到捆绑版 VLC 时，程序会带上它自己的 `ffmpeg445/lib` 重新 `exec` 一次，
否则 VLC 插件会找不到 `libavcodec.so.58`（Deepin 应用商店版是 AppImage 布局）。

## 2. 启动方法（与原版一致）

官方主进程这样拉起播放器（`background.js` 的 `yc()`）：

```
zspace-player "<encodeURIComponent(JSON.stringify({server:{ip,port}}))>" \
              "<zoomFactor>" "<x,y>" "<appName>" "<lang>" "username=<user>"
```

例：

```bash
ENC=$(python3 -c 'import json,urllib.parse;print(urllib.parse.quote(json.dumps({"server":{"ip":"127.0.0.1","port":13612}},separators=(",",":"))))')
./target/release/zspace-player "$ENC" "1" "1080,720" "zspace" "zh" "username=tester"
```

- 第 1 个参数是 URL 编码的 JSON，解析后连 `server.ip:server.port`。
- 拿不到第 1 个参数时，按原版逻辑回落 `<userData>/<appName>/zspace.conf` 的 `playerPort`
  （本机实测 `~/.config/zspace/zspace.conf`）。
- 两个都拿不到就报错退出（原版同样无法启动）。
- `zoomFactor` 用于窗口尺寸，`"x,y"` 是主窗口中心点，`appName`/`lang`/`username`
  只用于窗口标题与日志。**没有额外命令行选项**。

自测开关只在环境变量里，不属于产品行为：

| 变量 | 作用 |
|---|---|
| `ZSPACE_PLAYER_LOG=<path>` | 追加日志到文件 |
| `ZSPACE_PLAYER_HEADLESS=1` | 不开窗口（dummy 输出，CI 自测） |
| `ZSPACE_PLAYER_DEBUG=1` | libvlc `--verbose=2` |
| `ZSPACE_VLC_LIB` / `ZSPACE_VLC_PLUGINS` | 手工指定 VLC 运行时 |
| `ZSPACE_VLC_ARGS` | 追加 libvlc 参数（排障用） |

## 3. 协议实现

### 3.1 帧格式

下行（主进程 → 播放器）：

```
len=<base64 字符数>{<base64(UTF-8 JSON)>
```

注意：长度是 **base64 字符数**，`{` 只是分隔符，**没有结尾花括号**；
实现见 `src/protocol.rs`，覆盖粘包、拆包、前缀噪声、超长/非法帧，含 9 项单测。

上行（播放器 → 主进程）是裸 JSON，一条消息一次 `write`，`TCP_NODELAY`，
两条消息之间至少间隔 25 ms，避免 Electron 侧把两条 JSON 拼进同一个 `data` 事件。

### 3.2 已实现（MVP）

下行：

| f_type/s_type | 行为 |
|---|---|
| `100/0` | 记录 `apiHost` 与 cookie（当前播放链路不回调 NAS API，仅保存） |
| `5/5`、`5/1` | 解析 `playList`，取 `active=1`（否则第一项）播放；`skip.start` 在 `seek=0` 时作为片头跳过 |
| `3/2` | 切集：用消息内 `uri`/`seek` 重新打开 |
| `11/3` | seek/字幕切换后的新地址：同上 |
| `7/3` | 码流切换结果：同上 |
| `6/3` | 音轨切换结果：同上 |
| `8/1` | 记录 `info_type/show_info/reason`（不弹窗，见偏离项） |
| `0` | 停止播放、上报 `1/3`、退出进程 |

上行：

| f_type/s_type | 触发 |
|---|---|
| `1/2` | 首次进入播放状态（每个播放会话一次；seek/切轨重新出地址不重复发） |
| `4/3` | 每 10 s；暂停/继续、seek、切音轨/字幕时也发；带 `sec/chapter/audio_index/subtitle_index/subtitle_hash/subtitle_setting/video_duration` |
| `4/1` | 每 5 s 心跳，带 `info/videotime/result` |
| `1/3` | 收到 `f_type=0`、播放结束、进程退出前（只发一次），带 `url` |
| `11/1` | seek：请求主进程重新取播放地址（转码流必须），等 `11/3` 回到目标位置 |
| `6/1` | 切音轨：请求主进程重新取地址，等 `6/3` 后按新 uri + `seek` 续播 |
| `11/2` | 只切字幕：主进程只回索引，不重新出地址 |

所有上行消息都带 `client:{ip,pid,port}`（`port` 为字符串），与原版日志一致。
seek / 切音轨 / 切字幕都先等服务端回复；超过 8 s 未回复或回复错误码时，
回退到本地操作并写日志（这是兜底，原版没有该回退，见偏离项）。

### 3.3 未实现（明确列出）

上行：`3/1`（请求某集地址）、`5/2`（刷新播放列表）、`6/2`（音轨列表）、
`7/1`/`7/4`（码流切换/列表）、`9/1..7`（在线字幕搜索/下载/预览）、
`10/1..3`（本地/外挂字幕）、`12/0`（片头片尾设置）。

下行：`12/1`（剧集列表）、`12/2`（字幕列表）、`12/3`（合集信息）、
`9/x`、`10/x` 的其余分支（收到只记日志）。

影响：

- **码流切换**（清晰度菜单）没有 UI，`7/1` 未实现。
- 剧集列表 UI、在线字幕、片尾跳过、Blu-ray 标题选择不在 MVP 内。

### 3.4 与原版的偏离（必须显式标注）

1. **引擎版本**：官方是 VLC 4.0.0-dev（含 `libplacebo` vout、360、Dolby Vision）；
   这里是系统 VLC 3.x。因此 **HDR / Dolby Vision 会 tone-map 成 SDR**，
   没有 `--vout=placebo,any`，也没有官方 HDR 设置项。
2. **UI**：官方是 Qt6 自绘标题栏/控制条/菜单；MVP 只有窗口 + 键盘快捷键，
   没有控制条、菜单、播放列表、字幕设置面板。
3. **seek/切轨**：已按原版走 `11/1`/`6/1` 让主进程重新出地址，等待 `11/3`/`6/3`
   后按新 uri + `seek` 续播；字幕只切时走 `11/2`。唯一附加的是 **8 s 超时本地回退**
   （原版没有这一步，属于兜底，日志会写「回退本地」）。
4. **`8/1` 错误**：原版弹消息框，这里只写日志。
5. **窗口尺寸**：按 `1280x720 * zoomFactor` 估算并居中到给定中心点，
   官方实际尺寸由 playercfg 决定，未逆向。
6. **外挂字幕**：`libvlc_media_player_add_slave` 已封装，但 `10/x` 消息未接。
7. **音乐播放**（`playsdk.dll` 路径）不经过本进程；原生版补丁会禁用音乐入口，
   界面提示「音乐播放器未初始化」。这是明确偏离，不在本播放器范围内。

## 4. 键盘（MVP 控制方式）

| 键 | 作用 |
|---|---|
| 空格 | 暂停/继续 |
| ← / → | -5 s / +5 s |
| ↑ / ↓ | 音量 ±5 |
| F | 全屏切换 |
| Esc | 退出全屏；非全屏时退出播放器 |
| A | 下一音轨 |
| S | 下一字幕轨（循环到关闭） |
| M | 静音切换 |
| `[` / `]` | 倍速 -0.25 / +0.25 |
| Q | 退出 |
| 双击 | 全屏切换 |

## 5. 自测

```bash
cd native/player
./selftest/run_selftest.sh
```

脚本会：

1. 用 ffmpeg 生成 45 s 多音轨（2×AAC）+ 多字幕（2×SRT）样片；
2. 起一个假主进程：TCP 控制服务（先发 `100/0` + 多余帧制造粘包，
   再把 `5/5` 拆成 3 段制造拆包）+ `/splay/<token>/<file>` HTTP 服务（带 Range/206）；
3. 用**官方命令行**拉起播放器，截图、按 F/A/S/]/空格/→；
4. 校验上行消息、进度递增、窗口画面、全屏，输出 `selftest/out/summary.txt`。

最近一次结果：**25/25 通过**（证据在 `selftest/out/`）：
`mock.jsonl`（双向消息流水）、`player.log`、`shot-window.png`、
`shot-fullscreen.png`、`window-info*.txt`。
其中转码场景覆盖：`Right` 触发 `11/1` → 假主进程回 `11/3`（`/splay/TRANSCODE/...`）
→ 播放器按新 uri + seek 续播；`A` 触发 `6/1` → `6/3` 新地址；`S` 触发 `11/2` 只回索引。

## 6. HDR / 360 可行性（暂缓，不实现）

以下是为对齐官方能力所需的评估，不是已实现功能，也不新增官方没有的选项。

### 6.1 HDR / Dolby Vision

官方做法：`UZPlayer.exe` 在 HDR 模式使用 `--vout=placebo,any`（libplacebo 视频输出），
Windows 下配合 D3D11/DComp；兼容模式退回 `direct3d9`（SDR）。

| 项 | 结论 |
|---|---|
| 系统 VLC 3.x 能否直接 HDR | **不能**。VLC 3 没有 libplacebo vout，`--vout=placebo,any` 无对应插件 |
| 现状 | 默认 tone-map 到 SDR，颜色基本正确，但丢失 HDR 亮度/色域；Dolby Vision 同样降级 |
| 需要的组件 | 自建 VLC 4.x（含 libplacebo、ffmpeg 6/7、Vulkan/OpenGL 插件），随包分发运行时 + 插件 |
| 能否复用官方参数 | 可以：VLC 4 上 `--vout=placebo,any` 可直接用；VLC 3 上该参数无效（不认识） |
| 输出限制 | X11 桌面通常只能 tone-map 到 SDR；真 HDR 直通依赖合成器/显示器与内核链路 |
| 工作量 | 构建/打包 VLC 4+libplacebo 3–5 人日，参数与开关 1–2 人日，HDR 片源验证 2–3 人日 |
| 风险 | 高：VLC 4 API 未稳定、插件 ABI/依赖复杂、包体增大；DV 支持本身有限 |
| 建议 | 暂缓；在 UI/说明中标注「HDR 将转 SDR」，不假装支持 |

### 6.2 360 全景

官方做法：`panorama` 字段 + 自定义 `Video360Control` + VLC 4 球形视频；
VLC-Qt fork 提供 `updateViewpoint` / `isCurrentVideo360`。

| 项 | 结论 |
|---|---|
| VLC 3 是否有基础 API | **有**：`libvlc_video_update_viewpoint` 自 3.0 起可用，配合 GL vout |
| 需要补的部分 | 鼠标拖拽 → yaw/pitch/fov 映射、双击复位、按 `panorama`/球形元数据启用 |
| 能否复用官方参数 | 官方没有额外命令行参数；`--vout=gl`（或 xcb GL）即可，非新选项 |
| 阻塞 | 需要一片真 360 片源验证；`panorama` 字段取值只见过空串，语义未确认 |
| 工作量 | 2–3 人日（交互 + 验证），不含 UI 菜单 |
| 风险 | 中：VLC 3 球形管线可用但不保证与官方 VLC 4 片源完全一致 |
| 建议 | 暂缓；优先级低于 HDR |

## 7. 已知限制

- 需要 X11（Wayland 会话走 XWayland）。
- VLC 3 的 HDR/DV 降级为 SDR；360 全景未接（`libvlc_video_update_viewpoint` 可用但未实现）。
- 未做真机 NAS 联调（不接触 NAS 实机）；真实主进程的兼容性由消息表 + 官方日志对齐保证，
  本机原生 Electron 主进程的 `13610` 端口可被本程序正常握手（已实际验证到 `100/0`）。
