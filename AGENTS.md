# AGENTS.md —— 极空间 Linux 原生版

> **这个仓库优先由 AI Agent 维护。** 人负责做决定，Agent 负责执行、自检和留痕。
> 读完这份文件就能开工；细节规则在 `.agents/` 里。

---

## 0. 开工前必读

| 文件 | 作用 | 什么时候读 |
|---|---|---|
| `.agents/memory.md` | 跨会话记忆（决定、产物 SHA256、踩过的坑） | **每次会话开始** |
| `.agents/sources.md` | 官方版本号与上游二进制版本（rclone / syncthing） | 要查版本时 |
| `docs/WINDOWS-UI-MEASURED.md` | Windows 版播放器的逐像素实测基线 | 动播放器 UI 时 |
| `docs/PLAYER-FEATURE-PARITY.md` | 播放器功能对照表（已做/未做） | 排优先级时 |

**这个仓库不碰 Wine。** Wine 版打包在 `ja-plus/zspace-wine`，
两边包名不同（`com.zspace.native.spark` vs `com.zspace.spark`），可以并存安装。

---

## 1. 用户说「打包 / 打个 deb」时

```bash
pgrep -f "^bash scripts/build_native_deb.sh"   # 先确认没有别的实例在跑
scripts/build_native_deb.sh --rev N
```

**硬规则：**

1. **版本号先看现有产物。** `ls build/*.deb | sort -V | tail -3`，
   取最大 rev + 1。另一个会话可能已经用掉几个号，别覆盖别人的包。
2. **不能并发打包。** 两个实例共用 `build/native-work`，会互相 `rm -rf`，
   产物变成 1.7 KB 的空包。而且对方的"清理阶段"也在竞争，
   `pgrep` 看不到进程不代表安全——最稳的是打包前后都确认目录没被改。
3. **必须跳出沙箱跑**，并且要清掉 `BASH_ENV` 注入的 `rm` 包装：

   ```bash
   env -u BASH_ENV -u 'BASH_FUNC_rm%%' -u 'BASH_FUNC_unlink%%' \
       -u 'BASH_FUNC_rmdir%%' bash -c 'bash scripts/build_native_deb.sh --rev N'
   ```

   不清会报 `safe-bin/rm: 权限不够`，卡在第一步清理。

4. **载荷在 `native/work/`**：`electron/dist` + `app/extracted`。
   缺了会退化成假载荷（只验结构），**假载荷产出的包不能装**。
   真载荷构建约 2 分钟，产物约 117 MB。

### 你对产物负责

脚本返回 0 ≠ 完成。必须核对：

- 自检项全 PASS（脚本会打印 `结论：全部检查通过`）
- `dpkg-deb -x` 解出来，比对包内 `uzplayer/zspace-player` 的 SHA256
  与本机 `native/player/target/release/zspace-player` 一致（否则是旧版）
- 把产物路径、大小、SHA256 追加进 `.agents/memory.md`

---

## 2. 播放器改动（`native/player/`）

```bash
cd native/player
cargo build --release          # 单文件产物，约 1.1 MB
cargo test --release           # 11 个单测
bash selftest/run_selftest.sh  # 端到端 52 项，需要 DISPLAY
```

**测试纪律：**

- 跑自测前先 `pgrep -x zspace-player`，有残留就 `kill -9`。
  残留实例会占着窗口和端口，`xdotool` 会把按键发给旧窗口，
  表现为"键盘全失效、自测掉到 33/52"的假失败。
- **所有交互测试必须写在同一次 Bash 调用里。** 工具调用结束会杀掉后台进程，
  "启动播放器 → 下一次调用再点击"必然失败（面板卡住 / 进程消失）。
- 测试媒体在 `selftest/media/`（gitignore），要更长的片就
  `ffmpeg -stream_loop 9 -i multitrack.mkv -c copy long.mkv`，用完删掉。

**UI 对齐的唯一依据是 `docs/WINDOWS-UI-MEASURED.md`**（Wine 真机逐像素实测）。
不要凭 QSS 或录屏推断尺寸——之前就是这么错的（控制栏 96 vs 实测 66）。

---

## 3. 事实与口径

- 本包**改动了官方 Electron 载荷与配置**（补丁在 `native/patches/`），
  所以**不能投稿星火商店、不能公开分发**，只供个人自用。
- 不绑死某个发行版：依赖写 `libvlc5` 之类通用名，不硬写 deepin 专有包。
- 星火红线同样适用：装到 `/opt/durapps/<包名>/`、desktop 名等于包名、
  禁止 `chmod 4755` / `chown root`、deb 文件名不能有中文。

## 4. 禁止事项

- 不要在没跑完自检的情况下宣称产物可用。
- 不要把 `build/*.deb`、`native/work/`、`*.exe` 提交进 git（见 `.gitignore`）。
- 不要为了让测试通过而放宽自检项；要么修代码，要么如实报告没过。
- 不要在没问过用户的情况下下载大文件（>100 MB）。

## 5. 目录一览

```
zspace-linux/
├── AGENTS.md            ← 你在读的这份
├── README.md            ← 给人看的
├── .agents/             ← 记忆与来源
├── docs/                ← 播放器文档与 Windows 实测基线
├── native/
│   ├── libzrpc/         替代官方 libzrpc.dll
│   ├── player/          原生播放器（Rust + cairo + libvlc）
│   ├── mount/  sync/    挂载线 / 同步线
│   ├── proto/ patches/ test/
│   └── work/            载荷工作区（不进 git）
├── packaging-native/    deb 静态素材
├── scripts/
│   ├── build_native_deb.sh
│   ├── play_native.sh
│   ├── fetch_rclone.sh  fetch_syncthing.sh
│   └── common.sh
├── build/               打包产物（不进 git）
└── logs/                运行日志（不进 git）
```
