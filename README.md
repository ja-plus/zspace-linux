# zspace-linux

极空间私有云的 **Linux 原生客户端**：不依赖 Wine，用官方 Electron 载荷
+ 自研 RPC 桥 + 自研原生播放器，打成符合星火规范的 deb。

> 这是从 [zspace-wine](https://github.com/ja-plus/zspace-wine) 拆出来的独立仓库。
> **提交历史完整保留**：最早几个提交属于 Wine 打包阶段（那部分代码没有随仓库带过来），
> 之后才转入 Linux 原生实现。
> 那边只管 **Wine 版打包**（把官方 Windows 客户端用 Wine 跑起来），
> 这边只管 **原生实现**，两者包名不同、可以同时安装。

---

## 组成

| 目录 / 脚本 | 作用 |
|---|---|
| `native/work/` | 载荷工作区：`electron/dist`（Electron 运行时）、`app/extracted`（官方 app.asar 解包 + Linux 补丁）。**不进 git** |
| `native/libzrpc/` | 替代官方 `libzrpc.dll` 的 Linux 实现（Rust 动态库） |
| `native/player/` | 播放器 `zspace-player`：**Rust + cairo 自绘 UI**，libvlc 3 后端，顶替官方 `UZPlayer.exe`；命令行、TCP 帧协议、上下行消息与原版对齐 |
| `native/mount/` | 挂载线（mountV2）的 monitor，配合 rclone 挂云盘 |
| `native/sync/` | 同步线，配合 syncthing |
| `native/proto/` `native/patches/` `native/test/` | 协议样本、官方文件补丁、联调脚本 |
| `packaging-native/` | deb 静态素材：control / desktop / 启动器 / postinst / postrm / 图标 |
| `scripts/build_native_deb.sh` | 打 deb + 全套自检 |
| `scripts/play_native.sh` | 不装包，直接用原生播放器播当前视频（走 VA-API 硬解） |
| `docs/` | 播放器协议对照、官方实现结构、Windows 版 UI 实测基线 |
| `.agents/memory.md` | 跨会话记忆：做过的决定、产物记录、踩过的坑 |

## 快速开始

```bash
# 1) 拉依赖（不进 git 的上游二进制）
scripts/fetch_rclone.sh        # rclone 1.70.2
scripts/fetch_syncthing.sh     # syncthing 1.27.7
scripts/fetch_zspace_cli.sh    # 官方 Agent Toolkit 的 zspace-cli（只用不提交）

# 2) 打 deb（载荷缺失时用假载荷，只验结构）
scripts/build_native_deb.sh --rev 1

# 3) 装
sudo dpkg -i build/com.zspace.native.spark_2.46.2026101501+native.1-1_amd64.deb
```

`build_native_deb.sh` 自带体检：包结构、desktop 名、启动器、依赖、图标、
无 setuid、启动器清理代理变量……**任何一项不过都不会留下可用产物**。

## 和 Wine 版的关系

| | 本仓库 | zspace-wine |
|---|---|---|
| 包名 | `com.zspace.native.spark` | `com.zspace.spark` |
| 运行时 | Electron（Linux 原生） | deepin-wine / wine |
| 播放器 | `zspace-player`（Rust + libvlc） | 官方 `UZPlayer.exe` |
| 安装目录 | `/opt/durapps/com.zspace.native.spark` | `/opt/durapps/com.zspace.spark` |

两者可以并存安装，互不影响。

## 当前状态（native.23）

- 客户端主体：登录、文件列表、上传下载、挂载、同步均已接通
- 播放器：控制栏 / 弹层 / 设置面板按 Windows 版**真机逐像素实测**重排
  （见 `docs/WINDOWS-UI-MEASURED.md`），`selftest` 52/52
- 未完成：画面调节、字幕样式面板、在线字幕搜索、直播、360、小窗

## 免责

本仓库是**个人自用**构建：改动了官方二进制与配置，不投稿星火商店，
也不得公开分发。官方用户协议可能不允许再分发，公开发布前请自行确认授权。
