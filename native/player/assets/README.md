# assets —— 播放器界面素材

本目录是 Linux 原生播放器使用的界面资源（图标 + 样式取值），
目的是让控制栏与弹层的观感与官方 Windows 版保持一致。

## 权利声明（重要）

- 这些图标与样式资源的**著作权归极空间（原厂）所有**，本项目不主张任何权利。
- 仅限**作者本人的本地构建**使用，**不得再分发**：不要把本目录、也不要把它
  打进去的产物上传、发布、投稿或用于任何商业用途。
- 如果你 fork 本仓库并打算公开或发布产物，**请先删除本目录**。
  播放器在缺少图标时会退回矢量绘制（`src/bar.rs` 的 fallback），功能不受影响；
  样式取值可参考 `docs/WINDOWS-UI-MEASURED.md` 里的数值表，那部分是可自由使用的
  测量数据。
- 需要替换成自绘素材时，图标规格：80×80 的 @2x PNG，按 40px 绘制，
  每项含 normal / Hover / Disable 三态。

## 内容

只保留播放器**实际绘制**的 22 个图标，全部在 `icons/controlbar/`：

| 键 | 文件 |
|---|---|
| play / pause | `playWhite2x.png` `pauseWhite2x.png`（+ Hover） |
| rewind / forward | `rewind2x.png` `fastForward2x.png`（+ Hover） |
| subtitle | `subtitleWhite2x.png`（+ Hover） |
| audio | `audioTrackWhite2x.png`（+ Hover） |
| volume / mute | `volumeWhite2x.png` `muteWhite2x.png`（+ Hover） |
| full / exit_full | `fullScreenWhite2x.png` `exitFullScreenWhite2x.png`（+ Hover） |
| more（齿轮） | `moreWhite2x.png`（+ Hover） |

`official.qss` 是样式**取值**的出处（颜色、圆角、字号），不参与编译。

> 2026-11-21：原目录里有 224 张 PNG + 1 张背景 JPG + 6 个 SVG + 1 个 `.qm`，
> 但代码只引用 22 张（其余属于选集/字幕设置/引导/加载/标题栏等尚未实现的界面，
> 以及被手绘替代的关闭叉）。未使用的 210 个文件已删除，画面无变化。
