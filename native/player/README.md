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
VLC 3.0.21（运行时 + 插件 + 私有 FFmpeg）。程序会自动探测（按顺序）：

1. Debian/Deepin multi-arch：`/usr/lib/<arch>/libvlc.so.5` + `/usr/lib/<arch>/vlc/plugins`
2. Deepin 应用商店版 VLC：`/opt/apps/org.videolan.www.vlc/files/lib/...`

探测到捆绑版 VLC 时，程序会带上它自己的 `ffmpeg445/lib` 重新 `exec` 一次，
否则 VLC 插件会找不到 `libavcodec.so.58`（Deepin 应用商店版是 AppImage 布局）。
这次 self re-exec 只用一个内部标记 `ZSPACE_PLAYER_ENV_READY` 防重入，不是用户配置。

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
  只用于窗口标题与日志。**没有额外命令行选项，也不读取任何用户可配置的环境变量**；
  日志统一写 stderr，自测用 shell 重定向收集。

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
| `5/3` | `5/2` 的刷新结果：保存新的 `playList`，不自动切换 |
| `11/3` | seek/字幕切换后的新地址：同上 |
| `7/3` | 码流切换结果：同上 |
| `7/5` | 码流列表：更新 `format_list`（供 `7/1` 切换） |
| `6/3` | 音轨切换结果：有 `uri` 时重新打开；无 `uri` 时视为 `6/2` 的空回包（官方回包本身不带列表） |
| `12/1` | 剧集列表：保存 `videoList`，用于上一集/下一集（`3/1`） |
| `12/2` | 字幕列表（`subtitleList`，camelCase）：更新列表；`subtitle_index>=0` 时按 `subtitle_list[].url` 加载外挂字幕 |
| `12/3` | 合集信息：保存 `collectionInfo`（title/skip/history），暂不自动应用 skip |
| `12/0` | 片头片尾设置回包：记录 `code/message`（发送入口需要设置 UI） |
| `9/x` | 在线字幕结果：保存 `info_list`；带 `subtitle_list`+`subtitle_index` 时加载字幕（无搜索 UI） |
| `10/x` | 本地/外挂字幕结果：更新 `subtitle_list`，按 `subtitle_index` 加载；`10/3` 只记下载结果（本地文件入口待取证） |
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
| `3/1` | 切换剧集：`info` 为目标集 `file_path`（有加密路径时一并带 `encrypted_path`），等 `3/2` |
| `7/1` | 切码流：`result`=format 索引、`seek`、`audiotrack`、`subtitle`；等 `7/3` 后按新 uri 续播 |
| `10/1` | 请求主进程打开本地字幕选择；主进程回 `10/1`（`subtitle_list` + `subtitle_index`） |

所有上行消息都带 `client:{ip,pid,port}`（`port` 为字符串），与原版日志一致。
seek / 切音轨 / 切字幕都先等服务端回复；超过 8 s 未回复或回复错误码时，
回退到本地操作并写日志（这是兜底，原版没有该回退，见偏离项）。

> 字段命名差异来自官方代码，不是笔误：`12/2` 用 camelCase `subtitleList`，
> `10/x` 的回包用 snake_case `subtitle_list`（`tc()` 里 `t.subtitle_list=...`）。
> 播放器对两者都做了兼容读取。

### 3.3 未实现 / 待取证（明确列出）

**收包已实现、发送端缺 UI 入口**（按证据先做对回包处理）：

| 消息 | 缺什么入口 |
|---|---|
| `5/2` 刷新播放列表 | 官方在列表/数据变化时触发；当前无对应 UI 动作，只有 `5/3` 收包 |
| `6/2` 请求音轨列表 | 官方音轨菜单；当前只有 `6/3` 空回包处理（回包不带列表） |
| `7/4` 请求码流列表 | 清晰度菜单；当前只有 `7/5` 收包（`7/1` 用已有列表，`Y` 键触发） |
| `9/1/2/5/6` 在线字幕搜索/预览 | 搜索框/结果列表；当前只保存 `info_list` |
| `9/3/7` 在线字幕下载 | 搜索结果点击；当前回包里的 `subtitle_list` 会加载（自测用服务端回包模拟） |
| `12/0` 片头片尾设置 | 设置 UI（previous/start/end/preview）；当前只记回包 |
| `10/2` 本地文件上传 | 拖拽/文件选择；当前只有回包处理 |
| `10/3` URL 下载 | `9/x` 结果入口；当前只记下载结果 |

**仍未实现**：`9/4` 只记删除结果；`7/2` 未单独处理（按官方分支与 `7/1` 同路径）；
`12/0` 的 skip 值写入未接（没有设置 UI）。

待取证：

- `12/3` 的 `skip`/`history` 是否应在换集时自动应用（当前只保存，不自动 seek）。
- `10/3` 主进程下载到 `<userData>/Subtitles/<basename>` 后的本地加载路径（当前只记日志）。
- `subtitle_list[].index`（NAS 索引，实测 2/3/4…）与 VLC SPU 轨道序号的对应关系；
  当前外挂字幕按 `url` 直接加载，内嵌字幕仍按 VLC 轨道序号循环。

影响：

- **码流切换**（清晰度菜单）没有 UI，`7/1` 未实现。
- 在线字幕搜索、片尾跳过、Blu-ray 标题选择不在 MVP 内。

### 3.4 与原版的偏离（必须显式标注）

1. **引擎版本**：官方是 VLC 4.0.0-dev（含 `libplacebo` vout、360、Dolby Vision）；
   这里是系统 VLC 3.x。因此 **HDR / Dolby Vision 会 tone-map 成 SDR**，
   没有 `--vout=placebo,any`，也没有官方 HDR 设置项。
2. **UI**：官方是 Qt6 自绘标题栏/控制条/整套弹层。这里的底部控制栏与弹层用
   cairo 自绘（`src/bar.rs`，X11 override-redirect 窗口，不引入 Qt/GTK），
   **配色/圆角/字号/图标已按官方 QSS 与官方资源 1:1 对齐**（见 4.0），
   已覆盖控制栏全部按钮 + 6 个单层弹层（倍速/字幕/音轨/清晰度/选集/更多），
   键盘快捷键全部保留。**仍缺**：自绘标题栏、片头片尾浮层、结束页、
   加载/错误态、小窗、360、分享、蓝光标记，以及多层设置面板
   （画面调节、解码方式、字幕样式、睡眠定时细项）。
3. **seek/切轨**：已按原版走 `11/1`/`6/1` 让主进程重新出地址，等待 `11/3`/`6/3`
   后按新 uri + `seek` 续播；字幕只切时走 `11/2`。唯一附加的是 **8 s 超时本地回退**
   （原版没有这一步，属于兜底，日志会写「回退本地」）。
4. **`8/1` 错误**：原版弹消息框，这里只写日志。
5. **窗口尺寸**：按 `1280x720 * zoomFactor` 估算并居中到给定中心点，
   官方实际尺寸由 playercfg 决定，未逆向。
6. **外挂字幕**：`10/1`（主窗口选择本地字幕）已接通，回包按
   `subtitle_list[].url` 加载；`10/2`（本地文件上传）和 `10/3`（URL 下载）
   的发送入口依赖 UI/`9/x`，尚未接；`10/3` 的本地文件路径待取证。
7. **剧集/合集**：`12/1` 已用于上一集/下一集（`3/1`），但没有官方剧集列表 UI；
   `12/3` 的 skip/history 只保存，不自动应用。
8. **码流切换**：`7/1` 已实现（`Y` 键循环已有 `format_list`），但没有官方清晰度菜单；
   `7/4` 的发送入口（打开菜单时刷新列表）未接，`7/5` 回包可更新列表。
9. **音乐播放**（`playsdk.dll` 路径）不经过本进程；原生版补丁会禁用音乐入口，
   界面提示「音乐播放器未初始化」。这是明确偏离，不在本播放器范围内。

## 4. 界面：控制栏与弹层

底部控制栏固定在播放窗口下沿（**高度 66 px**，与 Windows 版一致），
鼠标移入窗口 3 秒内显示，播放中 3 秒无鼠标动作自动隐藏
（暂停/拖动进度/弹层打开时不隐藏）。

布局照 Windows 版实测（`docs/WINDOWS-UI-MEASURED.md`）：

```
进度条（左右边距 24、距顶 5、高 4）
[暂停] [‹10] [30›]  00:25:55 / 01:40:50 ............ 1.0x  原画  [字幕] [音轨] [齿轮] [音量] [全屏]
```

按钮位宽 56px（图标中心间距实测 56），图标按 40px 绘制（资源是 80×80 的 @2x，
墨迹落在 18–22px）。**没有**关闭按钮、没有 `⋯`（齿轮就是设置）、没有常驻音量滑杆
——喇叭平时只是个图标，点击即静音。

### 4.0 样式与官方 QSS 的对齐（`src/style.rs` + `assets/`）

控制栏的颜色、圆角、字号、图标全部换成官方原值，不再是自己调的一套：

| 元素 | 取值 | 出处 |
|---|---|---|
| 控制栏底色 | **纯色** `#141620`（逐像素实测 `(20,22,32)`，无渐变） | Windows 版实测 |
| 主色（进度已播放、选中） | `#425FFF` | `QSlider::sub-page:horizontal` |
| 进度轨道 | 高 4px、圆角 2、左右边距 24、距栏顶 5 | Windows 版实测 |
| 未播放余量 | `rgba(228,228,228,0.15)` | `QSlider::add-page:horizontal` |
| 进度滑块 | **静止时不画**；鼠标悬停进度条才浮出 12px 白点 | Windows 版实测 |
| 倍速 | 纯文字 `1.0x`，15px（官方实测无胶囊底色，hover 才有淡块） | Windows 版实测 |
| 清晰度 | 纯文字 `原画`，15px（两字宽 30px） | Windows 版实测 |
| 弹层容器 | `rgba(0,0,0,0.85)` + 圆角 8 + 1px `rgba(179,193,205,0.15)` 描边 | `QWidget#widLiveBase` |
| 弹层标题栏 | 14px `rgba(255,255,255,0.55)`，底边 1px `rgba(255,255,255,0.15)`，右侧关闭叉（手绘 1.6px 白线，hover 加圆底） | `QWidget#widTitle` + `QLabel` + `btnClose` |
| 弹层项 | 16px，选中 `rgba(130,149,255,0.12)`、圆角 4 | 弹层 `QPushButton` 规则 |
| 按钮组分隔线 | 1px `rgba(255,255,255,0.15)` | `QWidget#widButtons { border-right }` |

图标是官方 `:/resource/controlbar/*.png`（80×80 的 2x 图，按 40px 绘制），
normal / Hover / Disable 三态齐全，鼠标悬停会切到 Hover 图
（官方 Hover 图自带深色圆角底，所以悬停效果的底色是图片给的，不是画的）。

**控制栏没有关闭按钮** —— 官方控制栏本来就没有，关闭在标题栏/`q` 键/关窗。
`⋯` 右侧直接是最后一项。

`assets/official.qss` 是 173 条 QSS 原文，`assets/icons/` 是 234 个原图，
提取脚本和推导过程见 `tools/extract_uzplayer_assets.py` 与 `assets/README.md`。

**仍未对齐的部分**（官方值拿不到或依赖 Qt 能力）：

- 弹层的**窗口形状**：圆角是画在图片内侧的，X11 override-redirect 窗口本身
  还是矩形（没有 ARGB visual），所以四角外侧能看到一点底色。
- 弹层的关闭叉是**手绘**的，不是官方图标：资源里 `titlebar/winClose2x.png` 是
  104×88 的按钮整图（缩到 20px 笔画就没了），`popup/popupClose2x.png` 又因为
  那一段映射串位、提取出来是下箭头。手绘的好处是粗细/颜色/hover 都可控。
- 按钮的精确 x 坐标与间距：官方由 Qt layout 计算，QSS 里没有，
  这里按 40px 图标位 + 2px 间距排。
- `popup/` 目录那一段图标的「数据块 ↔ 文件名」映射还没校准
  （提取出来的 `popupClose2x.png` 实际是下箭头），所以弹层关闭叉暂时用
  已验证正确的标题栏 `winClose2x.png`。做设置面板时要先把这段逐个核对。
- 面板里「声道 / HDR/杜比 / 图像 / 解码 / 解封装 / 个性化 / 快捷键」只还原了
  外观，点了不会真的改播放器行为（libvlc 3 没有对应接口）。
- 面板和弹层的**窗口本身仍是矩形**：没有 ARGB visual，四角不能真透明，
  官方是 85% 黑叠在视频上，这里是接近不透明的深色。
- 清晰度项没有官方的编解码副标题（`hevc flac 8.60Mbps`）：协议 `7/x` 只给名字，
  码率/编码不在里面。

（±10 s 是官方的「圆弧箭头 + 圈内 10」图标，快进是圈内 30；
倍速文案 `1.0x` / `1.25x` 与官方一位小数起步一致。）

- 上一集/下一集没有目标时切官方 Disable 态图标（`previousVideoWhiteDisable2x.png`
  / `nextVideoWhiteDisable2x.png`），点击无效。
- 倍速、清晰度、字幕、音轨点开都是小弹层：**320×360 固定窗口**，底边贴控制栏顶、
  水平中心对准触发按钮；**没有标题栏、没有关闭叉**，每项是「单选圆圈 + 文字」。
  单行项高 46、双行项（带灰色副标题）高 74，圆圈中心 x=20、文字 x=53。
- 字幕弹层顶部多一行 `搜索 / + 添加 / 调整`（当前页签是实心药丸），下方 1px 分隔线。
  这三个按钮暂时只关闭弹层并写日志（在线搜索/本地添加/字幕样式还没做）。
- 再点同一个按钮收起（官方是同按钮重开，视觉上等于不收起；这里按体验做成收起），
  点画面或控制栏空白处也会关。
- 齿轮打开**设置面板**（官方 `ConfigPopup`）：**720×490 两栏**，左栏 178 宽、
  9 个分组（声道 / HDR·杜比 / 图像 / 解码 / 解封装 / 个性化 / 快捷键 /
  循环播放 / 定时播放），行高 45，选中项是圆角淡蓝底 + 蓝字；右栏是
  分组标题 + 分隔线 + 内容（单选行距 52）。
  - 已接通：「循环播放」（顺序/单视频/全部）与「定时播放」的按时长
    （10/15/30/60 分钟 → 睡眠定时）
  - 只画不接：声道、HDR/杜比、解码、解封装、图像滑杆、个性化步进器、快捷键开关
    —— 这些在 libvlc 3 里没有运行时切换接口，或后端协议还没有对应字段
- 弹层/面板都是同一个 X11 窗口，按内容调整大小并重绘，不依赖任何 UI 框架。

### 4.1 覆盖层（`src/overlay.rs`）

| 状态 | 触发 | 表现 |
|---|---|---|
| 加载 | 打开媒体到出首帧之间、等 `11/3`/`6/3`/`7/3` 重新出地址时 | 画面正中「正在加载…」小卡片 |
| 结束 | libvlc `STATE_ENDED`（循环播放关闭时） | 整块暗底 + 「播放结束」+ 重播 / 下一集 |
| 错误 | 主进程 `8/1`、libvlc 报错、打开媒体失败 | 整块暗底 + 「播放失败」+ 原因（超宽截断） |
| 跳过片头/片尾 | `12/3` `collectionInfo.skip.start` / `.end` 有值，且播放位置落在区间内 | 画面右下「跳过片头 8s」小卡片，点击 seek 到跳点，离开区间自动收起 |

窗口没有 ARGB visual，所以结束/错误页用不透明暗底（官方也是暗底结束页），
加载与跳过用小卡片，不遮住整幅画面。

### 4.2 全屏

控制栏的全屏按钮、`F` 键、双击画面走同一条路径，两个方向都做了实测。

进入：`_NET_WM_STATE_FULLSCREEN` 必须发到**根窗口**（掩码
`SubstructureRedirect|SubstructureNotify`），WM 才会真正接管全屏。
之前发到自己的窗口 + `NO_EVENT`，WM 根本收不到，只剩「把窗口改成屏幕尺寸」
生效——表现就是**窗口变大盖不住任务栏**，看着不像全屏。现在 WM 接管后
还会显式 `stack ABOVE` 抬一层（有的 WM 只改尺寸不抬层级）。
300 ms 后如果窗口仍没铺满，才退化为自己 `configure` 到屏幕尺寸。

退出：先发 EWMH 取消，**立刻**用 `_NET_MOVERESIZE_WINDOW` 请求还原，
坐标是**根坐标**（`translate_coordinates` 取），并且要减掉
`_NET_FRAME_EXTENTS` 里的装饰宽度——WM 的移动请求算的是外框坐标，
不减就会差一个标题栏高度。装饰尺寸必须在**进全屏前**读好，全屏期间
WM 会把装饰去掉，那时读回来是 0。之后按 150 ms 间隔校验最多 4 次，
尺寸对上就收工。

实测（2160×1440 屏）：`1280x720 @ (207,178)` → 全屏 `2160x1440 @ (0,0)`
且 `_NET_WM_STATE_FULLSCREEN` 置位 → 退出精确回到 `1280x720 @ (207,178)`，
连按两次往返都一致。

## 5. 键盘（控制栏之外仍全部保留）

| 键 | 作用 |
|---|---|
| 空格 | 暂停/继续 |
| ← / → | -5 s / +5 s |
| ↑ / ↓ | 音量 ±5 |
| F | 全屏切换 |
| Esc | 退出全屏；非全屏时退出播放器 |
| A | 下一音轨 |
| S | 下一字幕轨（循环到关闭） |
| N / P | 下一集 / 上一集（官方为剧集列表点击，MVP 用快捷键） |
| L | 选择本地字幕（发 `10/1`，由主窗口弹选择框） |
| Y | 下一个码流（发 `7/1`，官方为清晰度菜单） |
| M | 静音切换 |
| `[` / `]` | 倍速 -0.25 / +0.25 |
| Q | 退出 |
| 双击 | 全屏切换 |

## 6. 自测

```bash
cd native/player
./selftest/run_selftest.sh
```

脚本会：

1. 用 ffmpeg 生成 45 s 多音轨（2×AAC）+ 多字幕（2×SRT）样片，并生成三条外挂 SRT；
2. 起一个假主进程：TCP 控制服务（先发 `100/0` + 多余帧制造粘包，
   再把 `5/5` 拆成 3 段制造拆包；随后发 `12/3`、`12/1`、`5/3`、`7/5`、`6/3`、
   `12/0`、`9/3`、`12/2`）+ `/splay/<token>/<file>` HTTP 服务（带 Range/206）；
3. 用**官方命令行**拉起播放器，截图、按 N/A/S/L/Y/]/F/空格/→；
4. 校验上行消息（含 `3/1`、`7/1`、`10/1`）、新转码/码流地址、外挂字幕实际拉取、
   进度、窗口画面、全屏，输出 `selftest/out/summary.txt`。

最近一次结果：**45/45 通过**（证据在 `selftest/out/`）：
`mock.jsonl`（双向消息流水）、`player.log`、`shot-window.png`、
`shot-fullscreen.png`、`window-info*.txt`。
其中转码场景覆盖：`Right` 触发 `11/1` → 假主进程回 `11/3`（`/splay/TRANSCODE/...`）
→ 播放器按新 uri + seek 续播；`A` 触发 `6/1` → `6/3` 新地址；`S` 触发 `11/2` 只回索引；
`Y` 触发 `7/1` → `7/3`（`/splay/QUALITY/...`）。

## 7. HDR / 360 可行性（暂缓，不实现）

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

## 8. 已知限制

- 需要 X11（Wayland 会话走 XWayland）。
- VLC 3 的 HDR/DV 降级为 SDR；360 全景未接（`libvlc_video_update_viewpoint` 可用但未实现）。
- 未做真机 NAS 联调（不接触 NAS 实机）；真实主进程的兼容性由消息表 + 官方日志对齐保证，
  本机原生 Electron 主进程的 `13610` 端口可被本程序正常握手（已实际验证到 `100/0`）。
