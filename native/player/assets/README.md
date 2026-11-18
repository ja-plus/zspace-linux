# assets —— 从官方播放器提取的界面素材

这些不是我们自己画的，是从官方 `UZPlayer.exe` 里原样取出来的资源。
用途只有一：让 Linux 原生播放器的控制栏/弹层和 Windows 版逐像素对齐。

## 来源

- 二进制：官方 Windows 安装包里的 `resources/app/uzplayer/UZPlayer.exe`
- 版本：`V2.46.2026101501`（2026-10-15）
- 提取工具：`../tools/extract_uzplayer_assets.py`
- 重新生成的命令：

```bash
python3 tools/extract_uzplayer_assets.py <解包后的 UZPlayer.exe> assets
```

## 内容

| 路径 | 说明 |
|---|---|
| `official.qss` | 官方内联样式表汇总（173 条规则），颜色/圆角/字号的原值都从这里来 |
| `icons/controlbar/` | 底部控制栏图标，80×80 的 2x 资源，含 normal / Hover / Disable 三态 |
| `icons/titlebar/` | 标题栏图标（置顶、信息、360、最小化/最大化/关闭） |
| `icons/<其它目录>/` | 弹层、加载、引导、选集、字幕设置等 15 个目录 |
| `icons/*.png` | 没被 QSS 引用的散图（背景、滑块、加载帧、倍速面板底图） |

`icons/ZSpaceTmPlayer_*.qm` 是官方翻译文件，只是提取时顺带落盘的，播放器不用，
保留是为了将来做多语言时有对照。

## 为什么文件名是可靠的

`qres` 头在这个 exe 里被裁掉了，资源只剩两块裸数据：名字表和按序排列的数据块。
两块之间的对应关系是靠**类型一致性 + 动态规划保序对齐**恢复出来的
（见 `extract_uzplayer_assets.py` 的 `align()`）：

- 名字表 234 个文件节点；数据区 234 块
- 前 2 个 `.qm` 名字没有独立数据（同一批翻译只存了一份）
- 中间 1 块是多余动画帧、末尾 1 块无对应名字，两块都是孤儿
- 对齐结果：类型零冲突，且逐个用图标语义复核过
  （`playWhite2x.png` 是播放三角、`blurayWhite2x.png` 是蓝光圆盘、
  `winCloseHover2x.png` 是红底叉等）

## 授权

这些图标的版权属于极空间。本项目只在**个人使用**的前提下拿它们做界面还原，
跟仓库其它部分一样不对外再分发。要公开发布这个 deb 之前，必须先把
`tools/` 和 `assets/` 一起删掉换回自绘图标，或者去确认官方授权。
