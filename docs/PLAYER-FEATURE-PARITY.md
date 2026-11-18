# 官方播放器功能还原对照（Windows / macOS）

> 来源：`uzplayer/UZPlayer.exe`（Qt6 Widgets + libVLC 4）里的 QSS 对象名、
> `:/resource/...` 图标清单、`*Popup::*` 符号，以及主进程 `background.js`
> 里已有的协议分支。**这是从二进制还原的清单，不是猜的。**

## 1. 底部控制栏（官方 `widgets/controlbar`）

| 官方控件 | 图标资源 | 协议 / 实现方式 | 我们现状 |
|---|---|---|---|
| `btnPlay` / `btnPause` | playWhite2x / pauseWhite2x | 本地 `libvlc` 播放控制 | ✅ 控制栏按钮 + 空格 |
| `btnPreviousVideo` / `btnNext`（含 Disable 态） | previousVideoWhite2x / nextVideoWhite2x | `3/1` 切集 | ✅ 按钮 + 禁用态（无目标时灰显且点击无效） |
| `btnFastForward` / `btnRewind` | fastForward2x / rewind2x | 本地 seek ±10s | ✅ 控制栏 ±10s |
| `slider` + `sliderbtn` + `labPlayTime` | slider2x / sliderbtn2x | 本地 seek + 显示 | ✅ 颜色/粗细/滑块图均按 QSS 对齐 |
| `btnVolume` + `volumewidget`（滑杆）+ 静音 | volumeWhite2x / muteWhite2x | 本地音量 | ✅ 图标静音 + 可拖动滑杆 |
| `btnSpeed` | — | 本地 `rate` | ✅ 弹层 0.5/0.75/1.0/1.25/1.5/2.0x |
| `btnSubtitle` | subtitleWhite2x | `11/2` 选字幕；`10/x` 加字幕 | 🟡 弹层按 `12/2` 列表选（含关闭）；无「添加本地/在线搜索」入口 |
| `btnAudioTrack` | audioTrackWhite2x | `6/1` 选音轨 | ✅ 弹层列 libvlc 真实轨道名，选中发 `6/1` |
| `btnAudioChannel` | — | 本地 VLC 声道（立体声/左/右） | ⬜ 未做 |
| `btnEpisode` | episodesWhite2x | `12/1` 剧集列表 | ✅ 弹层列 `12/1` 列表，选中发 `3/1` |
| `btnFormat`（清晰度/码流） | blurayWhite2x | `7/1` 选码流 | ✅ 弹层列 `7/5` 码流，选中发 `7/1` |
| `btnFullScreen` / 退出全屏 | fullScreenWhite2x / exitFullScreenWhite2x | 本地窗口 | ✅ 控制栏按钮 + F 键 + 双击 |
| `btnSmallWindow` / 还原 | smallWindow2x / smallWindowReset2x | 本地窗口缩放 | ⬜ 未做 |
| `btnMore` | moreWhite2x | 打开 ConfigPopup | 🟡 弹层只有循环播放 + 睡眠定时；画面调节/信息/分享未做 |
| `IntroOutroFloating`（片头片尾跳转浮层） | IntroOutroFloating.png | `skip.start` / `skip.end` | ✅ 右下小卡片，带剩余秒数，点击 seek |

## 2. 弹层

| 官方弹层 | 内容 | 协议 | 我们现状 |
|---|---|---|---|
| `ConfigPopup`（设置面板） | 清晰度、音轨、字幕、解码方式、兼容模式、HDR、Dolby、倍速、音量、循环、睡眠定时、快捷键、信息、分享、画面调节 | 混合（`7/x`、`6/x`、`11/x` + 本地） | ⬜ 全缺 |
| `ConfigPopup` 画面调节 | 亮度/对比度/饱和度/色相 + 重置（`btnReset`） | 本地 VLC `video_set_adjust` | ⬜ 未做 |
| `ConfigPopup` 解码 | `硬解` / `软解`、`AVCodec` / `VLC` 解复用、兼容模式 | 本地 VLC | ⬜ 未做（Wine 下硬解无意义，Linux 可走 VA-API） |
| `VideoListPopup`（选集） | 剧集列表、升/降序、搜索、显示序号/文件名、片头片尾、连播策略（`After1/2/3/5 Ep`、`AfterCurrent`、`AllPlay`、随机/顺序/单集） | `12/1` 列表 + `3/1` 切集 | ⬜ 未做 |
| `SubtitlePopup` | 添加本地（`10/1`）、添加 NAS、在线搜索（`9/x`）、删除 | `10/x`、`9/x` | ⬜ 未做 |
| `SubtitleSetPopup` | 字幕延迟、字号、位置、颜色 | `11/2` 带 `subtitle_setting` | ⬜ 未做 |
| `SubtitleSearchPopup` | 搜索框 + 结果列表 + 下载 | `9/1`~`9/7` | ⬜ 未做 |
| `LiveListPopup` | 直播分组/频道列表 | 直播协议（未实现） | ⬜ 未做 |

## 3. 其它界面

| 官方 | 说明 | 我们现状 |
|---|---|---|
| `titlebar`（置顶 `pin`、信息 `info`、`360d`、最小化/最大化/关闭；mac 用 `mac/*.png`） | 自绘标题栏 | ⬜ 用系统标题栏 |
| `videoloading` / `errorshow` / `subnoserch` | 加载中、错误、字幕无结果 | 🟡 加载卡片 + 错误页已做；字幕无结果未做 |
| `playbegin`（开始播放引导）+ `guide`（鼠标操作提示） | 首次使用引导 | ⬜ 未做 |
| `widEnd` / `Eoftip`（播放结束：重播、下一集） | 结束页 | ✅ 暗底 + 重播/下一集（按 `12/1` 是否有下一集决定按钮） |
| `btnVideo360` / `ApplyVideo360Viewpoint` / `btnRestoreScreen` | 360° 全景（鼠标拖拽视角） | ⬜ 未做 |
| 视频信息面板（Format/Encode/BitRate/FrameRate/DataRate/Resolution/Size） | `btnInfo` | ⬜ 未做 |
| `btnShare` / `btnBluray` | 分享链接、蓝光码流标记 | ⬜ 未做 |

## 4. 实施顺序（按"像不像官方"的收益排序）

1. **控制栏重排 + 全部按钮在位**（含 ±10s、快进快退、禁用态）——视觉骨架
2. **通用弹层菜单**，先接：倍速、音轨、字幕、清晰度、选集
3. **音量滑杆** + 静音
4. **更多面板**（循环、睡眠定时、信息、画面调节）
5. **片头片尾浮层**、结束页、加载/错误态
6. 小窗、360、直播、分享、蓝光标记

**当前进度（2026-11-18）**：1–3 已完成并实测；4 只做了循环播放和睡眠定时，
播放信息/画面调节未做；5、6 未开始。**样式已按官方 QSS 1:1 对齐**（见第 5 节），
图标全部换成官方原图。

协议侧需要新增的上行：`6/1`（音轨）、`11/2`（字幕）、`7/1`（码流）已经能用具体索引，
弹层做好即可直接接；`12/1` 的剧集列表已经在 `st.episode_list` 里。

## 5. 样式基线（已对齐，2026-11-18）

素材来源：官方 `UZPlayer.exe`（V2.46.2026101501）里的 Qt 资源。
提取脚本 `native/player/tools/extract_uzplayer_assets.py`，
产物 `native/player/assets/`（`official.qss` 173 条规则 + 234 个原图）。

| 项 | 官方值 | QSS 出处 |
|---|---|---|
| 控制栏底色 | 黑色渐变叠 `#141620`（0% 透明 → 55% 黑 .6 → 100% 黑 .75） | `QWidget#widControl` |
| 主色 | `#425FFF` | `QSlider::sub-page:horizontal` |
| 进度轨道 | `rgba(255,255,255,0.15)` / 4px / 圆角 2 | `QSlider::groove:horizontal` |
| 未播放余量 | `rgba(228,228,228,0.15)` | `QSlider::add-page:horizontal` |
| 滑块 | `sliderbtn2x.png` | `:/resource/controlbar` |
| 倍速 | 文字胶囊 `rgba(255,255,255,0.15)` / 圆角 8 / 16px / padding 8 | `QPushButton#btnSpeed` |
| 清晰度 | 文字按钮 16px / padding 8 | `QPushButton#btnFormat` |
| 弹层项 | 16px / 选中 `rgba(130,149,255,0.12)` / 圆角 4 | 弹层 `QPushButton` |
| 按钮组分隔 | 1px `rgba(255,255,255,0.15)` | `QWidget#widButtons` |
| 弹层容器 | `rgba(0,0,0,0.85)` + 圆角 8 + 1px `rgba(179,193,205,0.15)` | `QWidget#widLiveBase` |
| 弹层标题栏 | 14px `rgba(255,255,255,0.55)` + 底边分隔线 + 右侧关闭叉 | `QWidget#widTitle` / `QLabel` / `btnClose` |
| 图标 | `controlbar/*.png`（80×80 2x，normal/Hover/Disable 三态） | `url(:/resource/...)` |

**拿不到、仍未对齐的**：

- 弹层的**窗口形状**：圆角画在图片内侧，窗口本身还是矩形（override-redirect
  没有 ARGB visual），四角外侧能看到一点底色。
- 按钮精确坐标与间距：由 Qt layout 计算，QSS 里没有。
- `popup/` 目录那一段的「数据块 ↔ 文件名」映射未校准
  （`popupClose2x.png` 提取出来是下箭头），弹层关闭叉暂用标题栏的
  `winClose2x.png`；做设置面板前要逐个核对这一段。
- `ConfigPopup` 两列设置面板的排版：未逆向，属于功能缺口而不是样式缺口。

**控制栏没有关闭按钮**：官方控制栏本来就没有，关闭走标题栏 / `q` / 关窗。
