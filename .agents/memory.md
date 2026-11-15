# memory.md —— 跨会话记忆

> **只追加，不改写历史。** 每次会话把有价值的东西写在文件末尾。
> 新会话开始时先读完本文件，再动手。

## 记忆规则

1. **写什么**：用户拍板的决定、产物记录（路径/版本/SHA256）、踩过的坑、
   官方接口的变化。不写过程流水账、不写临时路径。
2. **怎么写**：分节，带日期。格式见下方模板。
3. **改口径时**：旧结论不要删，在下面加一行"**已被 XXX 取代**"并说明原因。
4. **不存敏感信息**：密码、token、内网地址一律不写。

## 记录模板

```
## YYYY-MM-DD <一句话标题>
- 背景：
- 决定/结论：
- 证据：
- 影响：
```

---

# 以下是实际记录（按时间追加）

## 2026-11-04 项目建立

- 背景：要把极空间 Windows 客户端长期用 deepin-wine 跑在统信上，
  希望官方出新版时能自动发现、下载、重新打包，并且由 AI Agent 维护。
- 决定/结论：
  - 项目落在 `~/Desktop/zspace-wine`，用 git 管理，Agent 以 `AGENTS.md` 为行为契约。
  - 版本来源用官方接口 `upgrade.zenithspace.net/upgrade_server/v2/check_upgrade`
    （`app_id=APP_ZSPACE_DESKTOP_WIN`, `channel=zspace`），每日缓存一次。
  - 打包链路：NSIS exe → `$PLUGINSDIR/app-64.7z` → `app/` → 按星火规范组 deb。
  - 中文字体必须回落到微软雅黑（原因与无效做法见 `.agents/workflow.md`）。
- 证据：`scripts/check_update.sh` 实测拿到 `V2.46.2026101501`，
  `scripts/build_deb.sh` 实测产出 deb 且结构复核通过。
- 影响：后续版本更新只需 `download.sh` + `build_deb.sh`，无需重新摸索。

## 2026-11-04 中文字体修正（极重要）

- 背景：应用字体栈
  `PingFang SC, Source Han Sans CN, WenQuanYi Micro Hei, Microsoft YaHei, sans-serif`
  在 Linux 桌面上会命中第 3 项「文泉驿微米黑」，中文偏细、西文偏窄。
- 决定/结论：注入排除 `wqy` 目录的 `FONTCONFIG_FILE`，让字体栈落到微软雅黑；
  老容器还要清掉已写死的 `wqy-microhei` 注册表条目（**清之前先让 wineserver 退出**，
  否则它退出时会把旧注册表写回）。
- 证据：canvas 像素哈希，修复前 `appStack == WenQuanYi Micro Hei`，
  修复后 `appStack == Microsoft YaHei`；13px 墨量 雅黑 145173 / 思源黑体 141905 /
  等线 102192 / 默认回退 99754。
- 影响：`packaging/run.sh` 里的字体段落是这次的结论，改动前务必重新验证中文渲染。

## 2026-11-04 首次打包 2.46.2026101501-1

- 背景：项目脚本写完后的第一次完整打包，作为后续流程的基线。
- 决定/结论：
  - 产物 `build/com.zspace.spark_2.46.2026101501-1_amd64.deb`（309.7 MB）
  - SHA256 `f499e8493305b0f91a49f46c6dccc93c5ca18dee6d695086fada224e0b7351ca`
  - 源包 `packages/zspace_windows_2.46.2026101501_101619.exe`
  - 验收：A1–A10 全过，B1/B2/B6/B7 全过（18/18）；
    B3–B5 需装机后验证，C1–C3 需 root，留给用户确认。
- 证据：`dpkg-deb -I/-c` 复核通过；启动器 `bash -n` 通过；
  包内含 `files/app/极空间.exe`、`files/app/resources/app.asar`、`usr/bin/com.zspace.spark`。
- 影响：后续版本更新沿用同一条链路，产物信息照此格式追加。

### 打包链路踩过的坑（脚本已修，记录以免回退）

1. `dpkg-deb -c | grep -q` 会让 grep 提前关管道，tar 收到 SIGPIPE 报
   「tar 子进程被信号(断开的管道) 终止」——**包本身是好的**，是校验姿势不对。
   改成先落盘再 grep。
2. `set -e` 下函数末尾写 `[ -e ] && warn`，目录已删时整条返回 1，
   函数返回值带出去会中止脚本。删除类函数必须显式 `return 0`/`return 1`。
3. staging 目录用**唯一名**（带 PID），不要"先删后拷"：
   既避免残留内容被 `cp -a` 拷成 `app/app`（包体翻倍、条目翻倍），
   也避开受限环境下批量删除被安全策略拦下的情况。

## 2026-11-04 去掉 deepin-wine 绑定，产物 2.46.2026101501-2

- 背景：用户问「是不是只有 Deepin 环境能跑」。结论是卡在**运行时依赖**，不是桌面环境：
  原 Depends 里的 `deepin-wine-helper` 是硬依赖且**从未被任何脚本调用**，
  加上 run.sh 只认 `deepin-wine*`，把其它发行版挡在门外。
- 决定/结论：
  - Depends 改为 `deepin-wine10-stable | deepin-wine11-stable | deepin-wine8-stable
    | deepin-wine | wine64 | wine | wine-stable`（去掉 deepin-wine-helper）。
    apt 从左往右挑，Deepin 上仍选 deepin-wine10，行为不变。
  - run.sh 探测链加 `wine64`/`wine`/`wine-stable`，并支持 `WINE=<命令>` 强制指定。
  - `zc_find_wineserver` 改成从**选中的** wine 往回推配套 wineserver
    （包装脚本 → 解析它 exec 的真 wine；symlink → readlink -f 后看真实目录），
    **兜底故意不猜 `/opt/deepin-wine*`**：同时装两种 wine 时猜错会杀掉另一套的
    wineserver、报 version mismatch；找不到就返回失败让调用方跳过 kill。
  - 非 Deepin 环境的中文渲染依赖发行版自带 CJK（`zc_have_yahui` 不成立时跳过字体修正）。
- 证据：`scripts/selftest_wineserver.sh` 5 场景全绿（包装脚本/普通 wine/symlink/
  无配套/不存在）；`build_deb.sh --rev 2` 复核 A+B+E 共 19 项全过，
  Depends 已无 deepin-wine-helper、含 wine64。
- 影响：任何装了 Wine 的发行版都能装能跑；星火投稿不受影响（Deepin 上 apt 仍选第一项）。
  **E6（非 Deepin 真机/容器冒烟）尚未做**，留给有需要时装个容器补测。

### 产物 2.46.2026101501-2

- 路径：`build/com.zspace.spark_2.46.2026101501-2_amd64.deb`（309.7 MB）
- SHA256 `24b83b7f8e35f41e2715102516438e3c889a37bfb5f17eacedcd24a112ff7833`
- 源包：`packages/zspace_windows_2.46.2026101501_101619.exe`
- 上一条记录里的 `-1` **已被本条取代**（原因：依赖与运行时探测不具可移植性）。

## 2026-11-04 「字太细」的真正原因：可变字体默认实例是 Thin/Light

- 背景：上一轮把文泉驿排掉后，用户仍反馈「文字还是太细」。
- 决定/结论：真因不是文泉驿，而是页面 CSS 字体栈写成了**全角逗号**
  `"Microsoft YaHei，Arial，Verdana，sans-serif"` —— Chromium 当成一个字体名，
  必然找不到，于是走默认回退，命中 `winfonts/NotoSansSC-VF.ttf`（可变字体，
  默认实例 Thin/Light）。让这个文件不出现在 Wine 的字体扫描目录里，
  回退就落到 `Noto Sans CJK SC` 常规字重，用户确认观感正常。
- 证据：CDP `CSS.getPlatformFontsForNode` 直读实际渲染字体：
  修复前正文 = `Noto Sans SC[系统]`，修复后 = `Noto Sans CJK SC[系统]`。
  辅助：`fc-match "Microsoft YaHei，Arial，Verdana，sans-serif"` 也说明这串不是列表。
- 影响：`packaging/run.sh` 增加私有字体目录 `~/.deepinwine/<包名>/fonts`
  （软链接 winfonts 全部文件、跳过 `NotoSansSC-VF.ttf`），fontconfig 的 `<dir>`
  用私有目录替换 winfonts；注册表清理从 `wqy-microhei` 扩到 `NotoSansSC-VF`。
  **`<rejectfont>` 对 Wine 无效**（Wine 直接扫目录），只清注册表也会被重新写回。

## 2026-11-04 性能：关掉 GPU 合成，30fps → 60fps

- 背景：用户反馈软件卡顿。实测默认配置下窗口被钉在 30fps，空转 CPU ~17%。
- 决定/结论：启动器默认加 `--disable-gpu-compositing`（保留 GPU 光栅）。
  恢复默认可用 `ZSPACE_GPU_COMPOSITING=1`。
- 证据（同一台机器，CDP 180 帧动画 + /proc 时间差采 CPU）：
  | 配置 | fps | p95 | 空闲 CPU |
  |---|---|---|---|
  | ANGLE-D3D11 + GPU 合成 | 30.4 | 38.7ms | ~17% |
  | `--disable-gpu-compositing` | 60.4 | 17.5ms | ~9% |
  | `--disable-gpu` 全软件 | 60.6 | 17.8ms | ~8% |
  | DXVK(Vulkan) + GPU 合成 | 60.3 | 17.7ms | ~11.5% |
- 影响：
  - `--use-angle=gl` 在 Wine 下不可用（无 EGL，退化成 SwiftShader 软件渲染）。
  - DXVK 2.7.1 实测能跑（x64 d3d11/dxgi/d3d10core 放 system32 +
    `WINEDLLOVERRIDES=d3d11,dxgi,d3d10core=n,b`，日志确认走 NVIDIA MX250 的 Vulkan），
    但收益为 0、还多依赖，**不默认启用**；以后要试按 workflow.md §3.5 的表复测。
  - 踩坑：`ps` 的 `%CPU` 是生命周期均值，刚启动虚高，对比方案必须按 /proc 时间差采样。

### 产物 2.46.2026101501-3

- 路径：`build/com.zspace.spark_2.46.2026101501-3_amd64.deb`（309.8 MB）
- SHA256 `4f3c38ee23240400f1ec9e2971a500f1af2ccdd047bfdcbcdabb378c65af3c6a`
- 源包：`packages/zspace_windows_2.46.2026101501_101619.exe`（官方无更新，`STATUS=up-to-date`）
- 验收：A1–A10 全过；B1–B8 全过（B5 用 CDP 实测正文 = `Noto Sans CJK SC`，
  私有目录 681 个字体、无 `NotoSansSC-VF.ttf`，注册表 `NotoSansSC` 计数 0）；
  F1 参数在位；F2 空闲时 60.4fps / p95 17.5ms（应用自身后台轮询时会掉到 46–59fps，
  属应用行为，非启动器问题）；F3 空闲 CPU ~9–17%（同样受应用自身轮询影响）；F4 截图无异常。
  C1/C2 待用户自行 `apt install` 后确认。
- 偏差：中间目录清理被环境拦下（`safe-delete` 需确认），不影响产物；
  `build/work-*`、`build/stage-*` 需手动清。

## 2026-11-04 「还是不如原生流畅」的根因：Wine 给不出真 vsync

- 背景：修到 60fps 后用户仍觉得不如原生。拿**页面里真实存在的 el-table 列表**
  做合成滚动手势（`Input.synthesizeScrollGesture`）量化，才发现瓶颈不在帧率。
- 决定/结论：
  - 四种渲染配置（关合成 / DXVK+Vulkan / 关遮挡节流 / 全软件）结果**完全一致**：
    52fps、p95 44~45ms、3 秒里 17 帧超 33ms。→ 换渲染后端没用。
  - 慢帧期间 `PerformanceObserver('longtask')` 计数为 **0**。→ 也不是页面 JS。
  - 真因：Chromium 在 Wine 下等的 vsync 是假的，周期性空等 1~2 帧。
  - 解法：`--disable-gpu-vsync --disable-frame-rate-limit` 与 `--disable-gpu-compositing`
    一起用 → 65fps、p95 20ms、超 33ms 帧 0~1、抖动 σ 10.5→6。
  - **两个开关必须成对**：只关 vsync 会在假 vsync 上空转，空闲 CPU 15%→76%。
- 证据：三次取中位数，`/proc` 时间差采 CPU；细节表见 `workflow.md §3.5`。
- 影响：`packaging/run.sh` 默认参数改为三个开关；代价是渲染不再受 60Hz 节流
  （动画可能超 60 帧、个别场景可能轻微撕裂），`ZSPACE_GPU_COMPOSITING=1` 可回退。
  仍然存在的差距：输入链路多两跳（X11→XWayland→Wine 消息循环→Chromium），
  这是 Wine 的结构性成本，包装层改不掉。

### 产物 2.46.2026101501-4（默认参数改为三段式）

- 路径：`build/com.zspace.spark_2.46.2026101501-4_amd64.deb`（309.8 MB）
- SHA256 `8f921c76c7163f0844fdd5c136acb163c85782dc420c989c79c5bd162e16916a`
- 变更：启动参数 = `--disable-gpu-compositing --disable-gpu-vsync --disable-frame-rate-limit`
- 验收：固定负载滚动 p95 18.5ms / 超 33ms 帧 0~1（对比默认节流 43~45ms / 16~17 帧）；
  静止空闲 CPU ~11%；语法/结构自检全过。
- 已知取舍：不限帧后，页面有持续动画时 CPU 会升高（抓到过 96%，随后回落）；
  省电优先可用 `ZSPACE_GPU_COMPOSITING=1` 回到默认节流。
- 测法翻车记录：一开始拿页面自带的 el-table 测，每次启动视图不同导致结果漂移
  （有一次直接「没有可滚动区域」）——**基准必须用注入的固定负载**。

## 2026-11-04 输入延迟实测：事件投递只要 2~3ms，不是瓶颈

- 背景：用户反馈「输入还有办法解决吗」。用 `xdotool` 从 X11 侧发真实事件，
  页面里打 `Date.now()` 时间戳对齐系统时钟，算出端到端投递延迟。
- 证据：鼠标移动 10 次中位 **3ms**、滚轮 10 次中位 **2ms**（含 xdotool 4ms 进程启动开销，
  实际接近 1ms）。页面事件全部按时收到，没有丢。
- 结论：X11→XWayland→Wine 消息循环→Chromium 这条链路**不堵**，
  能感觉到的「钝」来自「输入→画面更新」的最后一段（出帧等待 + XWayland 多一层合成），
  结构性成本约 1 帧，包装层没有可用开关再压。
- 影响：不再为此投入；省下的时间放在帧节奏（已修）上更值。

## 2026-11-04 启动弹「上传文件到」= 我们自己的命令行参数被当成文件

- 背景：用户反馈每次启动都弹「上传文件到：」对话框。
- 根因：应用把 `process.argv` 里不是它自己的参数一律当「待上传文件」，
  写进 `userData/ContextUploaderItems.txt`（`{"updateTime":…,"list":["--disable-gpu-compositing"]}`），
  首页加载时 `AutoCheckClipboard` 读取该文件，2 分钟内有效就弹窗。
  我们传的 Chromium 开关（以及调试端口参数）全都中招。
- 解决的两种错法（都试过）：
  - `chmod 444` 锁文件 → 主进程抛 `EPERM: operation not permitted` 未捕获异常，**不能用**；
  - 直接删文件 → 下次启动又被写回来。
- 有效做法：把该文件换成 `-> /dev/null` 软链接。写入照样「成功」（内容被丢弃），
  读取永远是空，`JSON.parse` 抛异常被应用自己 catch，于是永远不会弹。
- 证据：替换后 CDP 查 DOM `hasDlg=false`，main.log 无 uncaughtException。

## 2026-11-04 启动时的系统提示是启动器自己发的

- 原因：`run.sh` 里 `notify "正在启动…"` 每次启动都发一条桌面通知。
- 处理：删掉这行，只在「首次初始化容器」「释放程序文件」时提示。

### 产物 2.46.2026101501-5

- 路径：`build/com.zspace.spark_2.46.2026101501-5_amd64.deb`（309.7 MB）
- SHA256 `e4feb8d9a3d87d0aa8012bfc25a06bfa4efad11b1fa40d81036cacf376b76055`
- 变更：① 去掉每次启动的桌面通知；② 上传队列文件改 `-> /dev/null`（修启动弹窗）；
  ③ `.desktop` 加 `StartupWMClass=Wine`（DDE 上图标仍未解决，见 workflow §3.7）
- 验收：启动后 CDP 查 `hasDlg=false`；main.log 无 uncaughtException；
  X11 会话固定负载滚动 80–98fps、p95 18.7ms、0 帧 >33ms。

## 2026-11-04 视频放不了：代理环境变量污染（connect ECONNREFUSED 127.0.0.1:7890）

- 背景：点开视频弹「connect ECONNREFUSED 127.0.0.1:7890」，同时同步/应用商店也在报同样的错，
  但海报墙和列表能正常显示，容易误判成「某一部片源有问题」或「解码器坏了」。
- 根因：桌面会话被代理工具（Watt Toolkit/Steam++、Clash、iKuuu 之流）注入了
  `http_proxy/https_proxy/all_proxy=http://127.0.0.1:7890`，Wine 原样透传进容器，
  Electron 主进程 Node 侧（axios）会无条件走代理；这些工具给的 `no_proxy` 只有
  10.*/172.16.*/192.168.* 和 `api.steampp.net`，**不含 127.0.0.1**，
  于是连应用自己的本机服务也被塞进代理：
  `127.0.0.1:13579`（内置网页代理）与 `127.0.0.1:8384`（syncthing）全部失败。
  代理一停（工具退出/换端口/只开 SOCKS）就是全线 ECONNREFUSED。
- 为什么界面还能用：Chromium 侧读的是 Windows 系统代理（本机 gsettings 是 mode=none → 直连），
  只有主进程发出的请求挂掉。**判断「网络是否正常」不能只看界面能不能开。**
- 决定：`packaging/run.sh` 在启动前 unset 所有代理变量，并把
  `no_proxy/NO_PROXY` 兜底写成含 `localhost,127.0.0.1,::1` 的内网列表；
  临时要恢复继承来的代理用 `ZSPACE_KEEP_PROXY=1`。极空间是局域网 NAS 客户端，直连是正确默认。
- 证据：
  - `tr '\0' '\n' < /proc/<pid>/environ` 里能看到上述三个变量；
  - `curl 127.0.0.1:13579` → 302；同一请求加 `http_proxy=…:7890` → curl exit 7；
  - 修复后重启，新增 1307 行日志里 `7890` 出现 0 次，13579/8384 正常监听，
    player.log 出现 `picture displayed late`（有画面帧在跑）。
- 顺带：本机 gsettings 里 `org.gnome.system.proxy.http host=127.0.0.1 port=7890` 是残留脏数据
  （mode 已是 none），DDE 会话环境变量也没被清 —— 属于代理工具没收拾干净，与本项目无关。

### 产物 2.46.2026101501-6

- 路径：`build/com.zspace.spark_2.46.2026101501-6_amd64.deb`（309.8 MB）
- SHA256 `ce80abf8fbb6d5db99a3e9194521ab63497cfefb06cc5ddceae7dea0c20f970f`
- 源包：`packages/zspace_windows_2.46.2026101501_101619.exe`
- 变更：唯一实质改动是 `run.sh` 新增「强制直连」段落（unset 代理环境变量 + no_proxy 兜底，
  可 `ZSPACE_KEEP_PROXY=1` 关闭）。
- 验收：A1–A10、B1/B2/B6/B7、E1–E4、F1 全过；B3–B5、F2–F4 沿用 -5 结论（字体/性能相关代码未动）；
  C1/C2 待用户安装确认。
- 修复效果实测（用仓库脚本直接拉起，非安装版）：新增 1307 行日志中 `7890` 出现 0 次，
  13579/8384 正常监听，player.log 出现 `picture displayed late`，视频出画面。

## 2026-11-04 强制 d3d11va 实测：不降反升，Wine 的「硬解」是 avcodec 披皮

- 背景：用户质疑「Wine 连游戏都能跑，为什么硬解不行」，要求实测验证。
- 做法：在容器 `%APPDATA%\vlc\vlcrc` 注入 `[avcodec] avcodec-hw=d3d11va` + `[core] verbose=2`
  （已证实 libvlc 会读这份配置，不是我瞎猜）。同一片源（复联4 UHD 3840x1608 HDR10 HEVC）播放采样。
- 证据：
  - 日志 `looking for video decoder module matching "any": 18 candidates` →
    **`using video decoder module "avcodec"`**（解码器没变）；
  - 同时列出 4 个硬件格式 `dxva2_vld / d3d11va_vld / d3d11 / cuda`，并 `trying format d3d11va_vld`；
  - `surface dimensions (3840x1664) differ from avcodec dimensions (3840x1632)`；
  - vout 用 `direct3d9`。
  - **CPU：默认软解 329% → 强制 d3d11va 426%（+30%）**；
    画面出现大量 `picture is too late to be displayed`（掉帧比默认更严重）。
- 结论：Wine 提供了 `ID3D11VideoDecoder` 这层接口（VLC 因此「看得见」硬解），
  但底层仍是 avcodec 软件解码，解完再拷进 d3d11 纹理 —— 白白多一次显存往返，所以更慢。
  上游 Wine 所有模块都不链接 libva（`ldd` 零命中），没有 GPU 解码后端；
  DXVK 也只做渲染、不实现任何 D3D11 video API。**此路不通，不要再试。**
- 影响：`vlcrc` 实验配置已移除（备份 `/tmp/vlcrc.experiment.bak`），保持默认软解。
  4K 原盘想不卡只有两条：① 客户端切 NAS 转码档（解 1080p H.264）；
  ② 抓 `/splay/<token>/<file>` 流给 Linux 原生播放器走 VA-API
  （实测同一片源：软解 5.14s CPU/5s vs VA-API 3.58s，且原生播放无回读开销）。

## 2026-11-12 在新检出位置重打 2.46.2026101501-6

- 背景：仓库迁到 `/home/cja/mycode/zspace-wine`（旧 `~/Desktop/zspace-wine`）。
  fresh checkout 的 `packages/`、`build/` 为空（物料不进 git）。官方仍 V2.46.2026101501
  （`STATUS=no-package`），用户批准下载后重打。
- 决定/结论：
  - 下载官方包 337291072 字节（MZ 头 + 体积与接口 `package_size` 相符）。
  - 用 `--rev 6`（**非默认 1**）：当前启动器代码 = 已记录的 -6 状态
    （git HEAD `91084c7` 代理修复，其后无启动器改动）。按 workflow §5，
    同官方版本下代码未变不应回退修订号到 1（1 对应旧的非可移植依赖）。
  - 产物 `build/com.zspace.spark_2.46.2026101501-6_amd64.deb`（309.8 MB / 324796848 字节）
  - SHA256 `07de921c335391df847aa77369f5193e536c68023739b526a579b5a6bf8f3842`
  - 源包 `packages/zspace_windows_2.46.2026101501_101619.exe`
  - SHA 与旧 -6 记录（`ce80abf8…`）不同：**deb 非字节可复现**（mtime/条目排序/Installed-Size），
    代码与官方二进制相同，功能等价。
- 验收（对**包内**文件 + 活体容器/CDP 实测，不是只看源码）：
  - A1–A10 全过；B1/B2/B6/B7 全过；E1–E5 全过
    （E5 `selftest_wineserver.sh` 5/5；E3 功能：`WINE=不存在` → 明确报错 exit 1）。
  - B3/B4/B8：活体容器 `fontconfig.conf` 含 `zc-fontconf-v3`；`system.reg`
    `wqy-microhei`=0、`NotoSansSC`=0；`fonts/` 681 个、无 `NotoSansSC-VF.ttf`。
  - B5：CDP `CSS.getPlatformFontsForNode` 登录页正文 = `Microsoft YaHei`（非 Thin 可变字体）→ 过。
  - F1：包内 run.sh 三段式开关在位，活体进程 cmdline 确认。
  - F2：注入固定负载 180 帧 = 681fps / p95 3ms / 0 帧>33ms → 过。
  - F3：**真 idle（窗口隐藏、无调试器）3 次中位 2.4% → 过**；
    可见登录页持续绘制时 ~15–17%（不限帧的已知取舍，见 -4），非打包回归。
  - F4：截图登录页中文清晰、无黑块/花屏 → 过。
  - C1/C2/C3 需 root/用户装机确认；E6 需非 Deepin 容器，未做。
- 偏差：`--rev 6`（理由如上）；F3 区分「可见绘制」与「真 idle」两种口径；
  B5 在登录页测得 Microsoft YaHei（主界面此前为 Noto Sans CJK SC，均属合格）。
- 影响：新位置产物与旧 -6 功能等价；后续在此位置沿用同链路。

### 本次新增的可复用教训

1. **F3 空闲 CPU 必须区分口径**：窗口可见且页面有持续绘制时，不限帧会让 CPU 到 ~15%；
   窗口隐藏（托盘）不绘制时才 ~2%。测「空闲」要把窗口隐藏/最小化，否则量到的是「可见绘制」。
2. **CDP `/json` 在窗口隐藏时返回 `[]`**：这恰好是「调试器已脱离、真 idle」的状态，
   适合接着采 F3；不要误判成应用崩了（`CrBrowserMain` 仍活着）。
3. **deb 不字节可复现**：同代码同官方包重打，SHA 也会变。比对产物要看「代码状态 + 官方包」，
   不能拿 SHA 相等当「同一产物」的判据。

## 2026-11-12 调研：不用 Wine 原生运行的可行性（结论：完整原生被闭源 SDK 卡死）

- 背景：用户问能否不走 Wine 让极空间在 Linux 原生运行。解剖了官方 exe
  （NSIS → app-64.7z → app.asar，26.7 MB + unpacked 562 MB），并做了外部调研。
- 决定/结论：
  - **路线 A（Linux Electron 重打包）不可行**，致命阻塞是闭源核心库 **libzrpc**
    （设备发现/登录链路/本地 13579 代理全走它，ffi-napi 加载，
    只有 `libzrpc_x64.dll` 与 mac 的 `libzrpc.dylib`，**没有 Linux .so**）。
    播放器 UZPlayer（Qt6+VLC+playsdk）、同步 ZSpaceSync、挂载 ZSpaceMount
    也全是闭源 win/mac 二进制，无 Linux 版。这些不是「重编 .node」能解决的，
    只有官方出 Linux 构建才能解锁（mac 版证明官方有能力移植）。
  - **路线 B（官方网页版原生封装）可行**：官方网页版 `zconnect.cn`（远程）或
    局域网 `NAS_IP:5055`，用原生 Electron/浏览器壳包一层即可，无 Wine、无闭源二进制、
    授权上最干净。代价：没有文档同步、挂载盘、手机备份、原生播放器。
  - 官方无 Linux 桌面客户端（下载页实抓确认）；社区无 asar 重打包项目
    （GitHub/Gitee 搜索均 0 命中）；官方「极空间 Skill」CLI 已支持 Linux（管理工具，非 GUI）。
- 证据（包解剖）：
  - Electron **33.3.1** / Chromium 130（exe 内 UA 字符串）。
  - JS 侧原生模块反而不是问题：`@lwahonen/ffi-napi`、`ref-napi` **asar 里自带
    linux-x64/arm64 prebuilds**；sqlite3 只有 win32 binding，但 napi-v6 有官方
    linux 预编译可直接补，无需编译。
  - `background.js`（5.5 MB webpack）里 "linux" 22 处全来自 systeminformation
    等第三方库；应用自身只有 win32/darwin 分支。
  - `ue="darwin"==process.platform?"libzrpc.dylib":"libzrpc_x64.dll"` —— linux 会
    落到 dll 分支，dlopen 必失败；`startWebProxy`/`searchDevice`/`get_nas_status`
    全经由该库，无降级路径。
  - ZSpaceMount.exe 字符串含 rclone/fuse/WinFsp/macFUSE（Go+rclone 魔改）；
    ZSpaceSync.exe 是 Go 二进制（syncthing 系，8384 端口）。
  - zsviewer 是 PDFNet **WASM**，跨平台；渲染层是普通 Vue web 应用。
  - 随包开源工具（adb/scrcpy、rclone、librespeed-cli）有 Linux 版可换，但调用路径要改 JS。
- 影响：本项目继续走 Wine 包装（完整功能）；若用户要「原生」，推荐路线 B 另起炉灶
  （与现有 deb 并存），不要投入路线 A 的逆向/仿制（违反本项目「不修改不破解」口径，
  且 EULA 风险）。临时解包目录 build/research-native 已清理。

## 2026-11-12 REA/craft-anything 逆向迁移评估（分支 research/linux-native-re，仅报告未实施）

- 背景：用户问能否用 REA（github.com/morluto/rea）+ craft-anything 把闭源库
  逆向迁移到 Linux。拉分支 `research/linux-native-re`，报告在仓库根
  `REPORT-linux-native-re.md`。**未安装/运行任何逆向工具，未改官方二进制。**
- 决定/结论：
  - 完整移植 libzrpc 不现实；**LAN-only 薄壳（R2）有条件可行**，估 4~8 周、
    成功率 50~70%，生死判据 = 局域网发现/握手是否私有加密（Phase 0 判定）。
  - 工具选 REA（MIT、MCP、Electron/Go/PE/抓包全对口）；craft-anything 是
    alpha 且比对环节为网站/游戏设计，不装。REA 需 Node≥24.22.1（本机 npx
    默认 24.21.0，接入前要升）。
  - R2 与 AGENTS.md「不修改不破解不替换」及星火红线**直接冲突**：做之前必须
    用户拍板改口径，产物仅个人使用、不得投稿/分发。官方 EULA 未核对，开工前必查。
  - 性价比最高仍是向官方要 Linux .so（栈全跨平台，mac dylib 已在）。
- 证据（本次取证，解包后 objdump/strings 实测）：
  - libzrpc_x64.dll：13.25 MB，MinGW C++（GNU 节布局），stripped，
    .pdata≈**40,753 函数**；静态链接 Asio/OpenSSL/protobuf C++/KCP/uTP/
    STUN/TURN/curl；私有 proto `message.tp/tls.proto`、145 个 PEM；
    20 个 libzrpc_* 导出，**ffi 签名表在 background.js 里全可见**。
  - ZSpaceSync.exe：43.4 MB，**Go 1.22.12 的 syncthing fork**（+quic-go）；
    ZSpaceMount.exe 24.5 MB Go+rclone；playsdk.dll 仅 119 KB（Qt6→libvlc 桥）；
    uzplayer 全目录 263 MB。
  - REA showcase 量级 = 单函数重建（DX-Ball 63 字节函数），无大型网络栈重写先例。
- 影响：后续若启动，只做 Phase 0（纯观察：FFI 契约书 + Wine 容器抓包），
  再决定是否进 Phase 1。临时目录 build/research-re 已清理。

## 2026-11-12 用户决定：research 分支豁免「不修改不破解不替换」口径

- 背景：用户看完 REA 评估后追问「全量移植有什么问题」，并明确
  `research/linux-native-re` 分支不受 AGENTS.md「不修改、不破解、不替换
  官方二进制」约束。
- 决定/结论：
  - 口径豁免**仅限本分支**；master 的 Wine 打包链路仍守原口径。
  - 星火商店「不得改动官方二进制」是商店规则，不随项目口径豁免，
    本分支产物仍不得投稿；外部 EULA/版权风险仍在，个人使用为宜。
  - 全量移植（R1）的技术判定不变：杀手是 libzrpc 远程 P2P 要与
    云端调度 + NAS 固件两个闭源对端对齐，私有握手/PKI、无测试预言机、
    永久维护债。报告 §3.1 已展开，§5 新增「2~3 天可行性尖峰」选项
    （提 proto schema / 抓 dispatch 通道 / 测私有函数占比）。
- 影响：REPORT-linux-native-re.md 已同步更新（头部口径声明、§0 表、
  §3.1、§4.1、§5）。等用户决定是否做尖峰或 Phase 0。

## 2026-11-12 libzrpc 闭源核验 + JS↔DLL 边界完全透明（R2 大幅降险）

- 背景：用户追问 libzrpc 是否闭源、作用、能否用 Rust 按需重写。
- 决定/结论：
  - 闭源核验通过：PE 无版本资源/版权串，全库唯一 URL 是 protobuf 自带
    描述符路径；网上同名「zRPC」是 go-zero 的 Go 组件，无关。自研闭源，高置信。
  - 作用 = 客户端连接引擎：设备发现、选路（LAN 直连/P2P/中继）、
    13579 本地代理、状态诊断。UI 一切请求经它。
  - **JS↔DLL 边界零逆向成本**（全部从 background.js 读出）：
    init 入参纯 JSON；start_service 明文 = {local_port, remote_port:5055,
    type:1, nas_id, ts}，JS 侧 AES-256-GCM；密钥 = MT19937(固定种子
    4728423)×64 拼接→SHA-256 hex，key=[0:32] iv=[0:16]，确定性可复现；
    返回普通 JSON；search_device 返回 {data:[{nas_id,...}]}。
  - 用户思路「Rust 按需重写、能替则替、用不到就砍」= R2 正确打法；
    唯一真实未知 = LAN 设备发现的 UDP 格式与代理是否附加鉴权头。
- 影响：R2 估时维持 4~8 周但置信度上升；报告 §2.1 已补「边界透明」节。
  下一步仍是 Phase 0 观察（Wine 容器抓包）定生死。

## 2026-11-12 播放器选型定为原生 libvlc（用户提议，取证支持）

- 背景：用户质疑「为什么用 mpv 而不和原版一样用 VLC」。
- 决定/结论：播放器引擎**默认 libvlc**，mpv 降为备选。取证：官方
  UZPlayer bundled **VLC 4.0.0-dev（git eee47c59a9，Otto Chriek）**，
  Windows 包 VA-API 插件 0 命中（与 Wine 无硬解结论互证）。
  理由：同引擎格式兼容不赌、C API 直接链接（libvlc-sys）比起 mpv 子进程
  +JSON IPC 更简、可编译官方同 commit 精确对齐、LGPL 干净（libmpv GPL 传染）。
  版本两选：① 同 commit 4.0.0-dev（功能对齐，含全景视频等新特性）
  ② 发行版 3.0 稳定（省事）。硬解两者都有 VA-API，非 mpv 优势。
- 影响：报告 §2.2 表与 §3.2 Phase 2 已改 libvlc 默认。不变的工作量：
  应用↔播放器控制协议（playsdk/IPC）仍需观测复现，播放器窗口 UI 需重做。

## 2026-11-12 远程访问定性修正：放弃的是协议重写，功能可经自建隧道承接

- 背景：用户追问「远程为什么架构性放弃」。
- 决定/结论：
  - 放弃的准确对象 = 重写官方私有远程协议（dispatch 注册/打洞/隧道/中继，
    对端闭源云+NAS 固件，可能设备校验、随时改协议）——分布式协议对齐问题，
    非工作量问题，Rust 重写不解决。
  - **远程功能本身可不丢**：NAS 端 Docker 装 Tailscale（极空间有社区教程，
    x86 Docker 型号可行）+ 客户端组网 → 远程变虚拟局域网 → LAN 薄壳直连
    100.x 虚拟 IP，零逆向。官方 P2P/中继本质是托管隧道服务，自建即绕过。
  - 前提：确认用户 NAS 型号支持 Docker；兜底 zconnect 网页/Wine。
- 影响：报告新增 §3.4；R2 功能清单远程行从 ❌ 改 ✅（经自建隧道）。
  待用户确认 NAS 型号。

## 2026-11-12 libzrpc 是全平台 SDK 核心（DEVT_ 枚举取证）

- 背景：用户问 iOS 是否也用 libzrpc。
- 证据：DLL 内嵌设备类型枚举 DEVT_PC/MAC/IOS/ANDROID/OHOS(鸿蒙)/TV/
  APPLETV/NAS/H5/WEB/MINI_PROGRAM/UNKNOWN —— 同一 C++ 核心跨平台编译，
  **NAS 固件本身也用**（DEVT_NAS，远程协议两端同库）。
- 结论：iOS/Android 高置信同核心（枚举+全端 P2P 功能一致；100% 确认需解
  IPA/APK）。官方已移植 6+ 平台 → R3（要官方 Linux 版）更成立。
  Android 端 libzrpc.so 是 ELF，若留符号则是比 stripped dll 更好的逆向参照
  （可选验证：APK jniLibs）。iOS Mach-O 对 Linux 无直接帮助。
- 顺带：社区资料证实极空间 Docker 可跑 lucky/Tailscale（host 网络），
  Z4 Pro 等型号内置「节点小宝」P2P 组网——§3.4 隧道绕过有现成先例。
- 影响：报告 §2.1 已补跨平台节。用户给了官网下载页 download.zspace.cn，待查。

## 2026-11-12 官网下载页核验 + 官方 Skill = Rust CLI（R2 远程方案重大降险）

- 背景：用户给出官网下载页 download.zspace.cn。
- 证据/结论：
  - 平台矩阵实抓：浏览器版 zconnect.cn / iOS（仅 App Store，无 IPA 直链）/
    Android（升级接口有 APK 直链，V2.5.653，272.6MB）/ 亲情版 / 小程序 /
    PC（Win/Win7/Mac×3）/ TV / 车机 / Skill。**无 Linux GUI 客户端**。
  - **极空间 Skill（zspace-nas-agent-toolkit V0.3.11）= 官方 Rust CLI**，
    六平台二进制（linux-x64 6.96MB / linux-arm64 5.46MB，直链+SHA256 在
    包内 runtime-artifacts.json）。`--mode auto|lan|cloud`：cloud 走
    www.zconnect.cn HTTPS 云代理 + oauth.zconnect.cn 标准 OAuth2，
    **不经过 libzrpc**，90+ 管理操作全通。
  - linux-x64 二进制实测（SHA256 校验过）：ELF 静态 stripped，Rust panic
    串留模块名（api.rs/album_*.rs/file_metadata.rs）与 crate（tokio 1.53.2）；
    端点 /account/oauth2/userinfo、/api/v1|v2/、/file/list、
    /zvideo/video/playlist/info —— 与网页版同套 NAS HTTP API。
  - **R2 远程方案升级**：薄壳远程模式 = 13579→zconnect HTTPS 云代理+OAuth2，
    零 P2P 逆向，API 层有官方 Linux 参考实现可研究；Tailscale 降为提速选项。
    报告 §3.4 已重写。云代理带宽/功能限制待 Phase 0 实测（网页版无同步）。
  - 官方有 Rust 工程师（二进制内开发者路径）→ R3 要 Linux 版更成立。
- 影响：R2 成功率置信度再升；iOS 同核心结论不变（DEVT_ 枚举），
  Android APK（272.6MB>100MB）下载验证 libzrpc.so 需用户批准，未下。
  临时目录 build/research-re 已清理；Skill 直链见报告 §3.4。

## 2026-11-12 Android APK 实锤 libzrpc.so（用户批准后下载验证）

- 背景：用户批准下载 Android APK 验证 libzrpc。
- 证据：官方 APK V2.5.653（285,822,878 字节，与接口 package_size 一致）内
  `lib/arm64-v8a/libzrpc.so`：12.75MB，ELF aarch64 stripped，仅 arm64 ABI；
  同栈（asio 759/protobuf 96/OpenSSL 导出在 dynsym）、同 DEVT 枚举（无 OHOS）。
  导出 = JNI `Java_com_zrpc_service_ZrpcService_*` 31 方法，与桌面 20 FFI
  核心一一对应 + fileTransfer*7/pushRegister/setCallBack/netStatusChanged/
  getNasDrIp/active。APK 播放也用 libvlc（libvlc.so+libvlcjni.so）。
- 结论：**iOS/Android 同 libzrpc 核心实锤**（Android 直接证据；iOS 枚举+
  同架构高置信，IPA 无官方渠道不取）。R2 契约获 JS ffi 表 + JNI 名双重验证。
  android .so 是 bionic 链接，glibc Linux 不能 dlopen，仅作 RE 参照；
  APK 的 DEX/Java 层比 C++ 易读，Phase 0 可用作契约参照（APK 已删可重取）。
- 物料：`build/research-material/libzrpc-android-arm64.so`（12.75MB，留存）；
  APK 与其余解包物已删。报告 §2.1 已更新。

## 2026-11-12 mac 版 libzrpc.dylib 未 strip —— 多版本逆向的钥匙（重大突破）

- 背景：用户问「多版本能否逆向闭源库」，批准下载两个 mac 包，明确最终目标
  = **Deepin 25（amd64）可安装运行的原生版**。
- 证据（官方 mac V2.46.2026101501，Intel+ARM 均下载，解 x64）：
  `极空间.app/.../app.asar.unpacked/sdk/libzrpc.dylib`（15.8MB，Mach-O x86_64）
  **未 strip**：LC_SYMTAB 75,302 符号，定义函数 55,479（local 40,922），
  C++ mangled 27,957，去重 32,172 可读符号。**45 个源文件名**
  （libzrpc.cpp/direct_connection/p2p_connection/tunnel_connection/
  signal_connection/zrpl_connector/zrps_delegate/nas_status_checker/
  message.tls.pb.cc/message.tp.pb.cc…）。私有命名空间 zsi/zrpc/psl/basefw/
  msg/live_p2p_transfer/p2pnetwork/BaseHcdn；开源依赖 boost.asio/protobuf/
  **Simple-Web-Server**/cxxopts/pugixml（Linux 可链接同款）。
  Win/Android 都 stripped，唯 mac 保留 → 用户「多版本」直觉证实。
- 结论：闭源库从黑盒变「有类图/源文件清单的已知架构」。**R2（LAN-only）
  升为高置信主线**（LAN 类精确锁定 direct_connection/discover_utils/
  nas_confirm/nas_status_checker）；完整远程 P2P 仍不推荐（服务端墙：
  读懂客户端≠服务端认你）。mac 二进制不能直接跑 Linux（Mach-O≠ELF），
  价值=符号蓝图+开源依赖清单+跨平台 JS/资源。
- 方法：GNU objdump 不支持 Mach-O；纯 Python 解析 LC_SYMTAB + c++filt
  批量 demangle（脚本 macho_sym.py/demangle_analysis.py 已存物料）。
  DMG 用 7z 全量解（中文路径通配符匹配失败，需全解）。
- 物料（build/research-material/，37MB）：libzrpc-mac-x64.dylib、
  libzrpc-android-arm64.so、libplaysdk-mac-x64.dylib、
  libzrpc-mac-x64-symbols.txt（32k 符号）、libzrpc-architecture.txt、两脚本。
  mac dmg（750MB）与解包树已删。
- 影响：报告新增 §2.4（符号突破）、§3.5（mac→Linux 复用盘点 + Deepin 25
  产物组件清单），§0/§3.1/§3.2 判定修订。下一步 Phase 0：Wine 容器抓包
  定 LAN 发现格式 + 按符号研究 direct_connection/discover_utils。

## 2026-11-12 Phase 0 静态实测：LAN 发现协议逆向，🟢 GREEN LIGHT（离线，NAS 零接触）

- 背景：用户批准离线静态分析（担心 NAS 安全，确认只看不碰）。sudo 需密码，
  capstone 改用 PyPI wheel 免 root 装（PYTHONPATH=build/research-material/pylibs）。
- 证据（mac dylib + 自写 Mach-O 解析 + capstone，脚本 disasm.py 已存物料）：
  - 调用链 discover_session::boardcast → discover_utils::make_discover_request
    → make_cipher_key + zsi::crypto::aes::encrypt → udp_socket::broadcast。
  - **UDP 广播端口 5577（0x15c9）**；请求明文 JSON {"key":"tdxnassearch",...}
    （json::FastWriter）；**AES-128-CBC**（key/IV 各 16B）。
  - **make_cipher_key = boost MT19937 固定种子 0x482667=4728423**，624 状态字、
    乘子 0x6C078965、64 个随机数 stringstream 拼接 → key/IV。
    **与 JS service-config 加密同种子**（已确认可 Rust 复现）。
- 结论：🟢 LAN 发现密钥固定可复现，实质透明，**R2 立项放行**。
  中途一度误判「纯明文」，反汇编到底见尾部 AES，但密钥可复现结论不变。
  Phase 1 待补：key/IV 精确切片、响应 JSON 字段、13579 代理行为。
  **运行时抓包降级为可选确认**（静态已答生死问），不必碰 NAS。
- 影响：报告新增「Phase 0 静态实测结果」节（§1 前）。任务#11/#13 完成，
  #12 改可选。物料增 disasm.py + pylibs(capstone)；capstone.whl 已删。

## 2026-11-12 Phase 0 细节挖完：加密与 JS 完全同构 + 响应字段 + 代理约定

- 背景：用户「继续挖细节」，离线补齐三项（NAS 零接触）。
- 证据/结论（mac dylib 反汇编）：
  - **加密修正**：实际 call `EVP_aes_256_gcm`（0x3ec400）→ **AES-256-GCM**，
    密文 hex 编码（输出 2×）。中途据 OpenSSL 通用串表误判 AES-128-CBC，已纠正
    （教训：通用串表含所有算法名，须看实际 call，不能凭字符串）。
  - **密钥与 JS 完全同构**：make_cipher_key = MT19937(4728423)×64 →
    stringstream → `zsi::crypto::sha256::calc`(0x44c60) → hex →
    key=hex[0:32]、iv=hex[0:16]。与 JS service-config 同一套，Rust 直接复用。
  - **发现响应字段**：make_discover_response 含 id/ip/hostname，内部
    nas_id/local_ip/型号(Z04A)，"%s:%d" 拼 ip:port。
  - **代理约定**：http_header_generator 维护 uri_arr/template/user_agent；
    模板实例 = NAS API 端点（/v2/file/download、/system/recent、
    /system/diskusage3、/zspool/info、/manager/services）+ **webagent=v2**
    标记 + **rnd=<ts>_<rand>** 缓存破坏。nas_confirm 用 https_client
    （boost.asio.ssl）→ NAS API 走标准 HTTPS/TLS（reqwest 可处理）。
- 影响：三项细节基本挖完，契约成形；剩微细节（key/iv 切片实测、完整响应字段、
  UA 串、代理鉴权头）Phase 1 复现+对照真实包定稿。报告 Phase 0 节已更新。
  **R2 可进 Phase 1（写 Rust libzrpc.so）**，等用户发话。

## 2026-11-12 调研经验沉淀为项目文档

- 背景：用户要求把调研经验总结进项目文档。
- 决定/结论：新建 **`.agents/research-lessons.md`**（可复用方法论与纪律），
  并在 AGENTS.md §0 阅读表挂索引。文档分工明确：
  REPORT-linux-native-re.md=目标事实/契约；memory.md=逐次决策/产物；
  research-lessons.md=以后怎么干（方法/坑）。
- 内容六节：① 多平台符号策略（mac 常保留符号，下结论前验全平台）
  ② Deepin 无 root 工具链（PyPI wheel+PYTHONPATH 装 capstone、纯 Python
  解析 Mach-O+c++filt、7z 解 DMG 中文路径需全解）③ 逆向判断纪律
  （凭 call 不凭字符串表判算法、固定种子 PRNG 是混淆、先读调用方、
  读懂客户端≠服务端认你、demangle 后再统计）④ 可行性切分（按协议定义者切、
  放弃协议≠放弃功能、官方工具是存在性证明、同引擎替换）⑤ 安全流程
  （离线优先、抓包被动只读、NAS 禁区、下载阈值、物料卫生、口径豁免边界）
  ⑥ 物料与复现（脚本/物料位置、复现命令、关键事实速查）。
- 影响：后续会话做原生/逆向工作时按 AGENTS.md 索引读此文档。改动文件：
  .agents/research-lessons.md（新增）、AGENTS.md（+1 行索引）、memory.md。

## 2026-11-12 Phase 1 启动：M0+M1 完成（native/libzrpc Rust 工程）

- 背景：用户批准进 Phase 1。计划锚点 ~/.qoder-cn/plans/windy-coast-asp.md，
  里程碑 M0~M5（任务#14~19）。工具链就绪：Rust 1.98.1/Node 24.22.3/gcc。
- M0 加密复现（native/libzrpc/src/crypto.rs）：自实现 MT19937(init_genrand)，
  sha2/aes-gcm crate（AES-256-GCM 用 U16 nonce 匹配 Node 16B IV）。
  **cargo test 4/4 过**：MT19937 对齐标准向量 seed5489=[3499211612,...]；
  派生密钥确定 = **key(ascii) 3891346e92151849d58e70de02a05c59 /
  iv 3891346e92151849**（MT19937(4728423)×64→SHA256 hex→[0:32]/[0:16]）。
- M1 FFI 壳（src/ffi.rs）：20 个 #[no_mangle] extern "C" 导出（JS 注册 18 +
  get_ipv6_priority/set_sdk_location），OnceLock<Mutex<State>> 状态，
  线程局部 CString 保活字符串返回，调用日志写 init 配置的 filepath。
  **Node 加载测试过**（native/test/ffi-load.js）：用官方 asar 自带的
  @lwahonen/ffi-napi linux prebuild 加载，18+2 导出全通，类型映射全对。
- 关键坑：① npm 装 @lwahonen/ffi-napi 走 node-gyp 源码编译失败 → 改用
  官方 asar 解出的 node_modules（含 linux prebuild）+ NODE_PATH。
  ② node-gyp-build 优先选 build/Release 的 **win32** binding.node（invalid
  ELF）→ **删掉 win32 build/Release/*.node** 回退 prebuilds/linux-x64 即好。
  ③ asar 全量解包需先解出 app.asar.unpacked（否则 extract 报 ENOENT）。
- 物料：native/work/app/extracted/node_modules（应用依赖+prebuild，删了win32）；
  native/libzrpc/target/release/libzrpc.so（416KB，20 导出）。
- 影响：M0/M1 完成。下一步 M2（发现 UDP5577 + 13579 代理）；真实 NAS 验证
  仍待批准（任务#12 可选）。start_service 入参是 **base64**(AES-GCM)，
  发现是 **hex**(AES-GCM)——两种编码不同，M2 注意。

## 2026-11-12 M2 完成：发现 + 代理，完整 LAN 链路离线验证通过

- native/libzrpc/src/discovery.rs：make_request=hex(AES-GCM({"key":"tdxnassearch"}))，
  parse_response 解 {nas_id/id,ip,hostname,port}，discover() UDP 广播 5577 +
  设备缓存 remember/lookup_ip（nas_id 含 . 视为 IP 直返）。
- native/libzrpc/src/proxy.rs：阻塞式 HTTP/1.1 反向代理（reqwest blocking +
  rustls，danger_accept_invalid_certs），线程/连接，normalize_path 自动加
  webagent=v2 + rnd=<ms>_<rand>，跳过 hop-by-hop 头，全 body 缓冲
  （流式/WebSocket 留 M5）。start/stop/stop_all 按 local_port 管理。
- ffi start_service：base64 解码→AES-GCM 解密→{local_port,remote_port,nas_id,
  scheme?}→lookup_ip→proxy::start（scheme 默认 https，可配 http 兼容网页版）。
- **cargo test 9/9 过**（含 proxy_forwards_to_mock）；**Node 端到端过**
  （native/test/ffi-proxy.js）：Node 同款 AES-GCM 加密配置→start_service code0
  →代理转发 mock 上游→响应带 webagent=v2+rnd。完整 LAN 链路离线自洽。
- 依赖：sha2/aes-gcm/hex/base64/serde/serde_json/reqwest(blocking,rustls-tls)。
- 影响：M0/M1/M2 完成，Rust libzrpc.so 核心（加密+FFI+发现+代理）就绪。
  **真实 NAS 互通仍未验**（发现帧格式/响应字段/代理鉴权头需抓包确认，任务#12）。
  下一步 M3：Linux Electron 33.3.1 下载 + asar linux 分支补丁 + 组装启动。
  native/work/app/extracted 留有完整 asar+node_modules（M3 补丁用）。

## 2026-11-12 真实 NAS 验证受阻：NAS 是远程的（不在局域网）

- 背景：用户批准验真实 NAS。被动监听 UDP5577 + 主动发现广播均 0 回应。
- 排查：应用缓存里 NAS 旧 IP 与本机当前网段不同；ping 旧 IP 100% 丢包；
  扫当前网段 5055 无 NAS。**用户确认 NAS 不在局域网（远程）**。
- 结论：LAN 发现/代理的真实验证暂缓（任务#12 挂起）；加密+代理已离线 mock
  验证。**原生版主路径其实是远程**（zconnect HTTPS 云代理 + OAuth2，官方
  Skill 已证明），LAN 发现只是同网段便利路径。
- 设计影响：广播发现仅同网段有效，跨网段靠缓存 IP/云端——发现逻辑要兼顾。
  登录页是账号登录/IP直连/离线登录，**无「搜索设备」按钮**（之前假设错）。
- 安全：未向 NAS 写入任何东西；主动探测仅标准发现广播（无状态变更）。
  不记录具体内网 IP（敏感）。拉起的官方 Wine 客户端仍在运行。
- 影响：转 M3（组装+启动到登录页，离线可行，登录页不需 NAS）。

## 2026-11-12 M3 完成：原生版启动并渲染登录页（Deepin 25，无 Wine）

- 组装：Linux Electron 33.3.1（npmmirror 下 106MB）+ 解包官方 app
  （native/work/app/extracted）+ 补丁 + 自制 libzrpc.so + sqlite3 linux prebuild
  （napi-v6-linux-glibc-x64，npmmirror）。补丁脚本 native/patches/patch_app.py：
  ① background.js dll 加载加 linux→libzrpc.so 分支；② 删 win32 build/Release
  binding（M1 已删）；③ 装 sdk/libzrpc.so。启动器 native/run-native.sh。
- **关键坑（渲染器空白根因）**：登录页 loaded 但 Vue 不挂载、窗口空白。
  CDP 查得 `path.join(null)`，逐模块定位到 **env-paths 的 readPackageJson
  (webpack 模块 f236) 返回 {name:null}** → appName=null → path.join(null)。
  原因：readPackageJson 按 process.resourcesPath/app.asar、/app、cwd 找
  package.json；Wine 里 app 在 resources/app.asar 找得到，**解包目录直跑时
  这些路径都没有 app**。修复：**把 app 软链到 electron/dist/resources/app**
  （run-native.sh 已固化 ln -sfn）。修复后 Vue 挂载、登录页完整渲染、中文清晰。
- 调试手法（可复用）：CDP（--remote-debugging-port=9222）+ websockets 库；
  webpack4 用 `webpackJsonp.push([["c"],{"m":fn(m,e,r){window.__wr=r}},[["m"]]])`
  捕获 __webpack_require__，直接调内部模块（os=8e57/env-paths=5273/
  readPkg=f236）逐个隔离 null。脚本 native/test/cdp_inspect.py。
- 验证：CDP DOM appLen=11837/inputs=4/buttons=12，中文「账号登录/IP直连/
  离线登录…」；截图与 Wine 版一致。"load dll success"（我们的 .so 经 ffi-napi
  加载无错）。注意：空白是 JS 错误非 GPU 问题，--disable-gpu 或非必需（待验）。
- 影响：M0~M3 完成。原生版可启动到登录页。下一步 M4（登录+浏览，NAS 远程
  需走 zconnect 云代理路径）/M5（播放+deb）。播放器 exe 报错预期内（M5）。
  官方 Wine 客户端与原生版可同时运行（userData 不同）。

## 2026-11-12 M4 解密 bug 修复：app 的 encodeService 不附加 GCM auth tag

- 背景：用户原生版登录报「无法连接到设备」。日志查得 shim 返回 decrypt failed。
- 排查：CDP+webpack 注入确认渲染器 readPackageJson 正常后，定位 start_service
  解密失败。用 app 的 J 类（标准 MT19937 init_genrand，genrand_int32 返回
  **无符号** >>>0）在 Node 派生密钥，**与 Rust 密钥完全一致**
  （3891346e92151849d58e70de02a05c59）。逐字节复刻 encodeService 发现：
  **app 只 concat([update,final]).toString('base64')，不 append getAuthTag()**！
  我们的 aes-gcm decrypt 需要 ct||tag → 无 tag 必失败。
- 修复：crypto.rs 加 `decrypt_gcm_no_tag`（无 tag GCM = AES-CTR + GCM 计数器：
  16B IV 时 J0=GHASH_H(IV||0^64||[128]_64)，数据计数器 inc32(J0)；用
  aes/ctr/ghash crate）。start_service 改无 tag 解密 + with-tag 回退。
  **cargo test 10/10 过**（含 gcm_no_tag_roundtrip）；Node app 精确编码验证
  no-tag/with-tag 均解密成功。
- 调试手法：ffi-appencode.js 用 app 的 J 派生 de/he 复刻 encodeService 喂 shim，
  对比 no-tag/with-tag 隔离 bug。eval 提取官方包 J 类（本地可信文件，RE 测试用）。
- **M4 真正难点浮现**：解密修好后返回 "nas ip unknown"——**NAS 是远程的**，
  app 传 nas_id 但 ip=''，shim 的 LAN 代理查不到 IP。远程 NAS 官方靠 libzrpc
  的 P2P/中继隧道（我们暂缓的硬骨头）。远程承接三选：① Tailscale/WireGuard
  把远程 NAS 变虚拟局域网（shim LAN 代理即可用，最实际）② 实现 zconnect
  HTTPS 云代理路径（Skill CLI 证明可行，需研究云代理 API+OAuth）③ 完整 P2P 逆向。
- 影响：M0~M3 完成 + M4 解密修复。远程连接待用户选路径。shim 有临时调试转储
  /tmp/zrpc-input.b64（M4 调试用，后续可删）。

## 2026-11-13 远程通道定案：zconnect HTTP CONNECT 云代理（用户否决 Tailscale）

- 背景：用户明确「NAS 不能装东西，不能要求用户安装，要和官方客户端一样透明
  连远程」。Tailscale 排除。唯一可行=复刻官方远程通道。
- 取证（官方 Skill CLI linux-x64，update.zenithspace.net/AgentToolkit 真实直链
  + 包内 reference/connection.md、authentication.md）：
  - **cloud 模式 = 把 https://oauth.zconnect.cn 当 HTTP CONNECT 代理**
    （hyper-util proxy/tunnel.rs + matcher.rs），带路由头 **nas-id / remote-port
    (=5055) / request-purpose** + **Proxy-Authorization: Bearer <token>**，
    CONNECT 隧道穿透到远程 NAS。标准 TLS 链+主机名校验，禁重定向。
  - OAuth：client_id=**ZspCli**；oauth.zconnect.cn/wbui/authorize（浏览器授权）
    → /auth/oauth2/code/status?state&client_id 轮询 → /auth/oauth2/code/token
    （表单 client_id/auth_code/state）换 token；access_token 临期 60s 刷新
    （www.zconnect.cn/auth/oauth2/retoken）；/account/oauth2/userinfo 验证。
  - NAS API 端点与桌面渲染器一致（/v2/file/、/system/recent、/zvideo/…）。
  - 桌面 background.js 也用 OIDC（access_token/id_token/refresh_token），
    webBase=https://www.zconnect.cn，含 cloudPubKey/nasPubKey（P2P 加密用）。
- 实现方向：shim 的 reqwest 代理改为经 oauth.zconnect.cn CONNECT 隧道，
  注入 nas-id/remote-port/Proxy-Authorization 头。**关键待解=token 来源**：
  ① 复用桌面 app 登录 token（最透明，需验证与云代理兼容）② 复刻 Skill 的
  ZspCli OAuth（一次性浏览器授权，保证兼容）。
- URL 纪律（用户多次强调）：只用真实官方地址，严禁 routify 等临时代理替换；
  zconnect 端点是任务目标机制本身非替代；每次调用核对来源。
- 影响：M4-remote（任务#20）进行中。物料 native/research-remote/（Skill CLI
  二进制+文档）。下一步验证桌面 token 能否用于云代理（需用户登录原生版）。

## 2026-11-13 中继信令尖峰成功：远程可用原版登录 token（无需浏览器）

- 背景：用户否决浏览器 OAuth（「登录逻辑要与原版统一」）。转研究 libzrpc
  强制中继信令（mac 完整符号）。任务#21 完成。
- **决定性发现**：桌面 background.js `c.ztoken = a.a.state.user.token`——
  中继鉴权 ztoken **就是用户正常账号密码登录的 user.token**。远程无需浏览器、
  无需额外授权，登录体验与原版完全一致。shim 复用同一 token。
- 协议（详见 native/research-remote/RELAY-PROTOCOL.md）：
  配置 zrpl-ucloud.zenithspace.net/platform/config/zrp/zrpc/v16；
  调度 dispatch-{bsnew,cs,yz}.zenithspace.net:3388 / dispatch.ud.link:3388；
  流程 guide(zrpc_connect_guide{nas_id}→zrpc_guide_response{real_addr,
  useful_addr}) → connect(zrpc_connect_request{nas_id,ztoken=user.token,
  zrpc_addr,request_uuid}) → 自定义 TLS 握手(msg.tls: client_hello/client_rand
  → server_hello/server_pub_key → client_key_exchange/pre_master_secret
  → server_finish) → greet(zrpl_greet_request_v2{http_header}) → 按 nas_id
  转发 HTTP。protobuf 消息 msg.tp.* / msg.tls.*，schema 内嵌 dylib 可提取。
- 可行性：**远程可行，登录与原版统一**。主要工作量=自定义 TLS 握手公钥密码层
  （对应桌面 cloudPubKey/nasPubKey）+ dispatch 信令 + 隧道转发接进 shim 代理。
  P2P 模式(p2p_connect_request)比中继复杂，先做强制中继。
- 安全/纪律：不从磁盘采集凭据（分类器正确拦截过 Cookies/JWT 扫描）；token 经
  app 正常登录合法取得。URL 只用真实官方来源，无 routify 替换。
- 影响：远程路径定案=复刻 libzrpc 强制中继（非云代理 OAuth）。下一步提取完整
  proto schema + 反汇编握手密码学评估难度，再实现。

## 2026-11-15 分支瘦身：native/work 与 Rust target 误入 git，推送 1.85 GiB

- 症状：research/linux-native-re 推送极慢。原因：两个提交把
  native/work/（Electron 运行时 186 MB + zip 106 MB + 解包的官方 app）
  和 native/libzrpc/target/（Rust 构建产物）共 9600+ 文件提交进了历史。
- 处理：分支未推过远端 → git filter-repo --invert-paths 重写这两个提交
  剔除上述目录（源码/文档全保留，已 diff 验证），.gitignore 补
  native/work/ 与 native/libzrpc/target/，推送载荷降到 ~10 MiB。
- 纪律：native/work/ 是运行时解包/下载的工作目录，target/ 是构建产物，
  一律不进 git（同 packages/build 的口径；官方二进制也不宜再分发）。
- 遗留：本地留有备份分支 backup/research-linux-native-re-pre-slim
  （含旧大对象，.git 仍 ~672 MiB）。确认无用后可删分支 +
  git reflog expire --expire=now --all + git gc --prune=now 缩到 ~10 MiB。

### 2026-11-15 后续：备份分支已清理

- 用户确认后删除 backup/research-linux-native-re-pre-slim，
  reflog expire + gc --prune=now 完成；.git 从 672 MiB 降到 ~4 MiB。
  上一条的「遗留」已了结，无备份分支残留。

## 2026-11-15 远程中继协议攻关：Windows DLL 未 strip，schema/握手/帧格式全部坐实

- 背景：research/linux-native-re 的下一站是「提取完整 proto + 反汇编握手密码学」。
  旧结论认为 Windows dll masked（stripped），只剩 mac dylib 可用；本轮复核推翻。
- 决定/结论：
  - **更正**：`resources/app.asar.unpacked/sdk/libzrpc_x64.dll` 导出表含
    **30,586 个 C++ 符号**（含地址），`zsi::tls` / `zsi::transport` /
    `zsi::crypto` / `msg::tp` / `msg::tls` 全部可直读。不需要再下载 394 MB 的 mac 包。
    旧结论「Windows/Android 都 stripped，唯独 mac 保留符号」**已被本条取代**
    （至少 Windows 版保留了导出符号；Android 未复验）。
  - proto schema 从 DLL 内嵌 FileDescriptorProto 提取，落盘
    `native/proto/message.tp.proto`、`native/proto/message.tls.proto`
    （脚本 `build/research-material/extract_proto.py`）。
  - 握手（`zsi::tls::tls_connection`）：
    client_hello{proto_version=1, client_rand=BN_rand(256)+BN_bn2hex 大写64位,
    encode_pubkey=true} → server_hello{server_rand, server_pub_key(PEM
    `BEGIN PUBLIC KEY`, PEM_read_bio_RSA_PUBKEY)} →
    pre_master_secret=同款随机 hex64，`RSA_public_encrypt(PKCS1_PADDING)`+base64 →
    master_secret=`sha256(client_rand‖server_rand‖pre_master)` →
    client_verify_data=`base64(aes::encrypt(master_secret, server_rand))` →
    server_finish=`base64(aes::encrypt(master_secret, client_rand))`。
  - **关键坑**：`zsi::crypto::sha256::result` 用 lower-case hex 表，但**每字节先写低
    半字节再写高半字节**（nibble 交换），不是标准 hex；master_secret 与
    aes 的 key/iv 派生都走它。必须原样复刻，否则对端校验必失败。
  - `aes::encrypt`：digest=sha256(secret) 上述 nibble-swap hex（64 字符）→
    key=digest[0..32]（ASCII）、iv=digest[0..16]（ASCII）、AES-256-GCM 16B IV，
    **不追加 auth tag**。与已有 `decrypt_gcm_no_tag` 同一构造。
  - 帧格式：每个 protobuf 消息前 4B 大端长度（`htonl(ByteSize)`）；
    握手阶段明文；握手完成后 `base_extranet_connection::async_write_data`
    把内存流按 0x4000 切块，每块加密后包成
    `transport_header{AC_DATA_SEGMENT=1}` + `data_segment_body{size, data}` 两帧。
- 证据：Windows DLL 导出表（30586 项）；`native/proto/*.proto`；
  `build/research-material/proto-dump.txt`；反汇编函数
  `write_client_hello / generate_rand / generate_master_secret /
  generate_client_verify_data / generate_server_verify_data /
  check_server_verify_data / rsa::load_public_key_from_str / rsa::encrypt /
  sha256::result / aes::encrypt / message_io::async_write_message_list /
  base_extranet_connection::async_write_data`。
- 影响：`native/libzrpc/src/relay.rs` 已实现 proto 编解码、帧编解码、握手状态机、
  nibble-swap sha256、session cipher、PKCS#1 RSA；`cargo test` 17/17 过。
  仍未完成：dispatch 信令（signal/guide/connect 的连接与消息流）、greet 与
  隧道双向转发、真实远程 NAS 联调（需用户登录 token + 一台远程 NAS）。
- 物料：`native/work/win/out/libzrpc_x64.dll`（13 MB，gitignore）保留供继续反汇编；
  320 MB 的 app-64.7z 用完即删。

## 2026-11-15 中继信令打通：greet 前缀、guide/connect、隧道数据面

- 背景：上一轮只做到 msg.tls 握手；本轮把 guide/connect 信令与隧道数据面补齐，
  并与官方 dispatch 做了活体联调。
- 决定/结论：
  - **信令前必须先发 `zrpl_greet_request{greet_type}`**：guide 用 4
    (`ZRPLGT_ZRPC_GUIDE`)，connect 用 1 (`ZRPLGT_ZRPC_CONNECT`)；问候帧与
    消息帧同批发出。只发 guide 会被服务端直接断开。
  - 平台配置 `https://zrpl-ucloud.zenithspace.net/platform/config/zrp/zrpc/v16`
    实测返回 `servers.zrpl.addr = dispatch-yz.zenithspace.net:9091`
    （注意是 **9091**，不是 3388；confirm_server 才是 3388）。
  - 流程：TCP → greet(4)+guide{nas_id,guide_ver=2} → guide_response
    （real_addr/useful_addr/cluster_id）→ 若指向别的节点则重连 →
    greet(1)+connect{nat_id,remote_port,service_type,request_purpose,
    device_type,neigh_ip,ztoken,zrpc_addr,request_uuid,is_forward,isp} →
    zrpc_response → msg.tls 四步握手 → 数据面：
    0x4000 明文切块 + AES-256-GCM（无 tag）+ data_segment 两帧。
  - 客户端**不发** greet v2（`zrpl_greet_request_v2` 在客户端无构造点，
    旧文档的「greet 带 http_header」判断已被本条取代）。
  - 远程连接用 `request_purpose=10`（RP_REMOTE_NAS_ACCESS）、`device_type=5`、
    `ztoken=user.token`；本机主代理（simpleStartSerivce）不传 ztoken。
- 证据：
  - `cargo test` 19 项全过，其中 `tunnel_roundtrip_against_mock_relay`
    用本地 mock zrpl 跑通 guide→connect→握手→加密回显；
  - `cargo test live_dispatch_guide_probe -- --ignored` 对官方 dispatch
    实测：bogus nas_id 返回 `guide result_code=8`（RC_ZRPC2ZRPL_GUIDE_ERROR），
    证明问候/guide/4B 大端帧格式与真实服务端一致。
- 影响：`relay.rs` / `proxy.rs` / `ffi.rs` 已串起来：远程 NAS 在 `start_service`
  里走 `start_tunnel`，每个本地连接一条隧道透明转发。剩余仅「用真实 ztoken
  与远程 NAS 跑通 connect + 隧道」，需要用户登录态。

### 2026-11-15 口径升级（用户，来自并行会话）

- 最终目标：Linux 原生运行极空间 + 打 deb。
- 优先级：① 复用官方其他平台包（Windows/Android/macOS）里的资产/二进制/SDK；
  ② 不好复用的先找 Linux 开源替代或小改可用；③ 最后才 REA 逆向 + Rust 重写。
- 协作：另一条会话在 `build/research-reuse/` 下只新建文件；**不要清理/删除该目录，
  也不要把它纳入 git 改动**（build/ 本身已 gitignore）。
- 汇报：阶段性产出或卡点要立刻发简报，不要攒到最后。

## 2026-11-15 原生链路收尾：ABI 核对、远程分支接入、行为对齐

- 背景：中继协议打通后，按「先复用、后开源、最后重写」的口径把连接层收尾。
- 决定/结论：
  - **主代理不带 ztoken**：`startWebProxy` → `we(13579,5055,nasId)` 的配置是
    `{local_port,remote_port,type:1,nas_id}`（含可选 `connect_mode=1`）；
    带 ztoken 的只有 remote-access/云手机 helper。故远程判定改为
    「无局域网 IP 或 connect_mode=1 → 走中继」，**不再以 ztoken 为门槛**，
    `request_purpose` 原样透传（不再把 0 改成 10）。
  - `libzrpc_set_private_network_access` 的 IP 要参与选路（IP 直连登录），
    返回值 app 以 `>1` 判成功 → 有 IP 返回 2、清空返回 1。
  - `get_nas_status` 补齐 app 真正消费的字段：`nas_id/local_network/connected/conn_type`。
  - `run-native.sh` 启动前自动构建/复用 `libzrpc.so` 并跑 `patch_app.py`
    （linux dll 分支 + 删 win32 .node + 装 sdk/libzrpc.so），再接 Electron。
- 证据：
  - ABI：app.asar 的 18 项 ffi 表逐项对照 release `.so`，`nm -D` 全命中；
    ctypes 实调 init/set_device_type/set_private_network_access/start_service(坏输入)/
    get_nas_status 等均正常，`set_private_network_access` 有 IP 返回 2。
  - 测试：`cargo test` 20 过 1 ignored，新增 `ffi_start_service_routes_remote_through_relay`
    （加密配置 → start_service → 本地端口 → 中继 → 回显），全程无真实 NAS。
  - 对照 `build/research-reuse/opensource-mapping.md`：连接层无可用开源替代，
    与矩阵「重写」结论一致；代理按 remote_port 泛化，覆盖 purpose 7/8 等。
- 影响：原生版远程 NAS 路径已接到「只差真实登录凭据」；`build/research-reuse/`
  只读未改。

## 2026-11-15 原生版真机启动验证（无 Wine，到登录页）

- 背景：用户批准下载 Electron 33.3.1 linux-x64，要求把原生链路跑到登录页并留证。
- 组装：Electron 33.3.1 linux-x64（npmmirror 106 MB）解到 native/work/electron/dist；
  官方 app-64.7z 里取 app.asar + app.asar.unpacked 的 node_modules/icons/dialog，
  asar 解包后覆盖合并；sqlite3 补 napi-v6-linux-glibc-x64（npmmirror
  registry.npmmirror.com/-/binary/sqlite3/v5.1.4/…tar.gz）；
  patch_app.py 打 linux dll 分支并删 2 个 win32 .node。
- 结果：
  - 主进程 `load dll success`，证明 ffi-napi 成功加载我们的 libzrpc.so；
  - `Main window loaded: app://./index.html#/login`；CDP 取到
    title=极空间 - 家庭私有云、appLen=11835、inputs=4、buttons=12、无 JS 错误；
    CDP 点击「IP直连」切页成功（IPv4 输入出现）。
  - 截图与日志：build/research-native-run/evidence/（login-page.png、
    ip-direct.png、app-launch.log、summary.md）。
- 坑：
  - devtools 端口被残留实例占用会报 "Cannot start http server for devtools"；
    先 `pkill -x electron` + 删 SingletonLock/Socket/Cookie，再换端口启动。
  - 后台进程要用 `setsid nohup`，否则命令结束时被回收。
  - urllib 访问 127.0.0.1 会走残留代理变量，需 unset 或 no_proxy。
  - 已知非阻塞：VlcPreload.exe/UZPlayer.exe 是 Windows 版，播放层后续换 libvlc。
- 停点：登录需真实账号；按约定不索要凭据，停在登录页。

## 2026-11-15 LAN 代理改流式（修 4K 播放 OOM）+ 自测

- 背景：播放计划指出 `proxy.rs` 用 `resp.bytes()` 全量缓冲，播 11.5 GB 片源会 OOM。
- 改动：
  - 响应体改为 64 KiB 缓冲循环 `read` → 直接写回，不落内存；
  - 保留 `206`/`Content-Range`/`Content-Length`/`ETag`/`Last-Modified`，
    去掉重复 content-length；无 content-length 时对客户端用 chunked 重编码；
  - 明确 `Connection: close`；支持 `HEAD`（不回 body）；
  - `/splay/` 路径不再注入 `webagent/rnd`（官方模板里没有，避免影响 Range/seek）；
  - reqwest 去掉 60s 总超时（4K 播放会超过），只保留 10s connect_timeout。
- 验证：`cargo test` 23 过 1 ignored，其中
  `streams_206_and_preserves_range_headers`（4 MB + 后 400 ms 延迟，断言首块
  <300 ms 到达、Range 原样透传、/splay 无额外参数）、`streams_chunked_response`、
  `head_request_has_no_body`。
- 影响：LAN 直连播放/seek 不再受内存限制；远程隧道路径本来就是透传，不受影响。

## 2026-11-15 原生 deb 产物（真实载荷）

- 路径：`build/com.zspace.native.spark_2.46.2026101501+native.1-1_amd64.deb`
- 大小：92,245,376 B；Installed-Size：327,864 KiB
- SHA256 `9bc8b37022f6c3f722d1970e8106744052e130c5d8d7e62f8e9902f142c6c8e2`
- 源载荷：Electron 33.3.1 linux-x64 + 官方 app.asar 解包 + 补丁 + 自研 libzrpc.so
- 自检：`build/native-verify-report.txt` 全过（A1–A16/B1–B15/C1–C6）
- 构建：`scripts/build_native_deb.sh`（默认自动用 native/work 载荷）；
  静态素材在 `packaging-native/`，与 Wine 版 `packaging/` 并存互不影响。

## 2026-11-15 登录后弹「局域网设备扫描」：根因与阶段性修复

- 现象：原生版账号登录成功后弹「局域网设备扫描 / 没有在局域网中发现设备」，
  Windows 原版不出现。
- 证据链（用户实机日志 + CDP）：
  - `~/.config/zspace/logs/main.log`：`simpleStartSerivce` 返回 200，
    随后 `http://127.0.0.1:13579/key/pub` 报 `AxiosError Network Error`；
  - 渲染器 `chunk-3b0dd3ec`：`handleLostConnToServer()` 才把
    `noConnDlgVisible` 置 true（面板只在「取 NAS 公钥失败」时出现）；
  - 我们的日志：`start_service: relay tunnel` → 说明走了中继，但隧道没建起来。
- 中继三个真实差异（已修）：
  1. guide 返回的 `real_addr/useful_addr` **只有 IP 没有端口**：官方客户端沿用
     当前 dispatch 的端口（实测 9091）。已加 `ensure_port()`。
  2. 握手消息必须包在 **data-segment 帧**里（裸 `[4B len][msg]` 服务端不回）；
     已加 `send_handshake_message/read_handshake_message`。
  3. `server_hello.server_pub_key` 是 **base64 后的 PEM**；已加
     `normalize_server_pub_key()`。
- 当前卡点：完整握手能跑到 `server_finish`，但服务端返回 **result_code=2
  (RC_VERIFY_DATA_FAILED)**。已穷举但全部失败：
  - sha256 hex 风格（nibble-swap / 标准）、master 拼接顺序 6 种、
    key/iv 切片 4 种、verify 输入 client/server、RSA OAEP、proto 字段交换、
    encode_pubkey=false。
  - 该 rc 同时覆盖「RSA 解密失败」和「verify 不匹配」，静态无法区分。
- 下一步（未做）：用官方 Windows 客户端 + 本地 mock 中继（自控 RSA 密钥）
  抓一次真实握手，反推 pre_master/master/verify 的精确公式；或继续静态分析
  `write_server_hello` 里的服务端校验分支。
- 临时调试开关（**交付前必须清理**）：`ZRPC_HEX_STD`、`ZRPC_KEY_STD`、
  `ZRPC_ORDER`、`ZRPC_VERIFY_CLIENT`、`ZRPC_SLICE`、`ZRPC_RSA_OAEP`、
  `ZRPC_SWAP_FIELDS`、`ZRPC_NO_ENCODE_PUBKEY`。

## 2026-11-15 原生播放器 MVP：协议兼容的 zspace-player（libvlc）

- 背景：替代 Windows `UZPlayer.exe`，Linux 原生进程直连主进程 TCP、用系统 libvlc 播放。
  口径（用户强调）：只替掉那一个二进制，参数/协议/配置回落对齐原版；
  不加原版没有的兼容层/播放器适配/新选项；不可避免的偏离要明说。
- 产物：
  - 代码 `native/player/`（Rust；`src/protocol.rs` 帧协议、`src/vlc.rs` dlopen libvlc、
    `src/ui.rs` x11rb 窗口、`src/main.rs` 状态机）
  - 二进制 `native/player/target/release/zspace-player`（约 1 MB）
  - 文档 `native/player/README.md`；自测 `native/player/selftest/`
- 协议要点（从 app.asar `background.js` 源码复核，校正了 plan 的两处细节）：
  - 下行帧 = `len=<base64字符数>{<base64(JSON)`，**无结尾花括号**；
    上行是**裸 JSON**（不是帧），主进程直接 `JSON.parse(chunk)`，
    所以播放器必须一条消息一次 write + TCP_NODELAY + 消息间隔 25ms。
  - 官方启动参数 6 个：urlencoded `{server:{ip,port}}`、zoom、`x,y`、appName、lang、`username=`；
    取不到就回落 `<userData>/<appName>/zspace.conf` 的 `playerPort`。
  - 只有 `4/1` 心跳不写日志；`1/2`、`4/3`、`1/3` 的字段实测见 `main.old.log`。
- 关键坑：
  1. **字幕 API**：`libvlc_video_get_track_description` 是视频轨；字幕必须用
     `libvlc_video_get_spu_description` / `libvlc_video_set_spu`。
  2. Deepin 商店版 VLC 是 AppImage 布局，插件依赖 `files/ffmpeg445/lib` 的
     `libavcodec.so.58` 等；`LD_LIBRARY_PATH` 必须在进程启动前生效，
     程序检测到捆绑 VLC 后会用新的 `LD_LIBRARY_PATH` **exec 自身一次**
     （`ZSPACE_PLAYER_ENV_READY=1` 防重入），并 `VLC_PLUGIN_PATH` 指向插件目录。
  3. 系统 `libvlc5` 有运行库但**没有插件目录**，单用会播放失败；
     解析 VLC 路径时必须把 (lib, core, plugins) 作为一套来选。
  4. `--plugin-path` 在 VLC 3.0 已废弃，插件目录只能靠 `VLC_PLUGIN_PATH`。
  5. 控制器线程结束时主线程还停在 UI 事件循环 → 进程不退出；
     用 `JoinHandle::is_finished()` 收尾（否则 mock 发 0 后播放器看似卡住）。
- 已实现：下行 `100/0`、`5/5|5/1`、`3/2`、`11/3`、`7/3`、`6/3`、`8/1`（仅日志）、`0`；
  上行 `1/2`、`4/3`、`4/1`、`1/3`；播放/暂停/seek/音量/全屏/关闭/倍速/音轨/字幕（本地）。
- 未实现：`3/1`、`5/2`、`6/1|2`、`7/1|4`、`11/1|2`、`9/x`、`10/x`、`12/0|1|2|3`。
- 偏离（README 已列）：VLC 3 无 placebo → HDR/DV 降 SDR；无官方 Qt 控制条/菜单；
  seek/切轨只本地做，不走 `11/1`/`6/1` 重新取地址；`8/1` 不弹窗。
- 自测：`native/player/selftest/run_selftest.sh`（假主进程 + Range HTTP + 多轨样片 +
  xdotool + 截图），**21/21 通过**；证据 `selftest/out/`（mock.jsonl/player.log/截图）。
- 注意：本机 `~/.config/zspace/zspace.conf` 的 `playerPort=13610` 指向另一会话正在跑的
  原生 Electron 主进程；无参数启动会真实连上它（已验证能收到 `100/0`）。后续自测
  不要用默认端口，避免打扰。

## 2026-11-15 播放器收尾：服务端 seek/切轨 + 补丁规格

- 口径（用户强调）：只替掉 `UZPlayer.exe` 一个二进制；参数/协议/行为对齐原版，
  不新增官方没有的选项或兼容层；不得已偏离要明说。
- 播放器改动（native/player）：
  - seek 改走 `11/1`（带 `seek`/`subtitle_index`），等主进程 `11/3` 回新 uri + seek 后续播；
    切音轨走 `6/1`（`result`/`seek`/`subtitle`），等 `6/3`；只切字幕走 `11/2`，等 `11/3` 只回索引。
  - 8s 无回复/错误码 → 本地回退（原版无此兜底，README 已标偏离）。
  - `1/2` 只在换片/换集（5/x、3/2）时发，seek/切轨重新出地址不再重复。
  - 删掉之前自加的调试 CLI 选项（`--server/--headless/--geometry/...`），
    只保留官方 6 个位置参数 + `zspace.conf` 的 `playerPort` 回落；测试开关放环境变量。
- 自测：`selftest/` 假主进程新增 11/1→11/3、6/1→6/3、11/2 回包，
  用 `/splay/TRANSCODE*` 区分“重新出地址”；**25/25 通过**，单测 9/9。
- 文档：`README.md` 更新协议表/偏离项/HDR-360 可行性（暂缓，不实现）；
  新增 `native/player/PATCH-SPEC.md`（6 处 background.js 补丁，含原文/替换串/边界/验证）。
- 未改 `native/patches/patch_app.py`、`native/run-native.sh`（归主线），未执行 git 操作。
- 生成物 `selftest/media/`（22MB 样片）与 `selftest/out/` 已 gitignore，不应提交。

## 2026-11-15 中继 rc=2 攻破：标准 hex + RSA 双 base64；播放器补丁接入

- **rc=2 根因（已实证）**：
  1. `master_secret`/AES key 派生用的是**标准 lowercase hex**，不是之前静态误判的
     nibble-swap；换标准 hex 后真实中继握手通过（server_finish 不再报 2）。
  2. `encrypt_pre_master_secret` 对 RSA 输出做了**两次 base64**
     （`rsa::encrypt` 内一次 + 外层再一次）；已改为双 base64。
  - 证据：`ZRPC_NAS_ID=<真实 nas> cargo test live_relay_http_probe -- --ignored`
    输出 `connect ok via 223.111.129.15:9091`，不再出现 server_finish rc=2。
  - 已把标准 hex 固化为默认并删除全部 ZRPC_HEX_STD/KEY_STD/ORDER/SLICE/
    VERIFY_CLIENT/RSA_OAEP/SWAP_FIELDS/NO_ENCODE_PUBKEY 调试开关。
- **仍卡住**：握手成功后，隧道写入任何数据（裸字节 / 加密 data-segment /
  先发 greet v2 / public greet v2）都会立刻被对端关闭（读 0 字节 EOF）。
  未发控制帧。下一步：对照官方客户端在握手后发的第一条隧道消息
  （可能需 `zrpl_greet_request` 或 zrps 侧 greet），或抓官方客户端真实隧道。
- **播放器补丁（已完成）**：
  - `patch_app.py` 接入 PATCH-SPEC 的 6 处补丁（幂等、写前用 Electron
    `--check` 语法校验），并把 `zspace-player` 装到 `app/uzplayer/`（0755）。
  - `run-native.sh` 自动构建 libzrpc.so / zspace-player 后打补丁；
    `build_native_deb.sh` 同样要求播放器随包，并新增 A17/A18 自检。
  - 实机：main.log 的 `VLC video player path:` 已指向 `uzplayer/zspace-player`，
    无 VlcPreload 报错；用官方 6 参数手动拉起播放器，主进程日志出现
    `Player server been connected`，播放器日志 `已连接主进程`。
    **100/0 需从界面发起播放（需登录），暂未验证到那一步。**
- 一致性审计（本轮）：
  - 已对齐：`set_private_network_access` 返回 2/1（app 以 >1 判成功）、
    `get_nas_status` 的 connected/conn_type、远程选路（无 IP 或 connect_mode=1，
    ztoken 可选）、握手 hex/双 base64、播放器 6 处补丁与 kill 命令。
  - 已知偏离（未修，非阻塞）：`detect_nas` 返回空（官方会探测 LAN IP）、
    `check_ports` 返回空、`network_diagnose`/`link_diagnose` 为空壳、
    `get_nas_status.online` 恒 true；`/splay` 不注入 webagent/rnd 待 NAS 实测。
  - 启动器差异（必要）：`ELECTRON_DISABLE_SANDBOX=1`、unset 代理与 NODE_*、
    不追加 Chromium 开关；与官方 Windows 行为不同但为 Linux 运行所需。

### 产物 native.3（含播放器与握手修复）

- 路径：`build/com.zspace.native.spark_2.46.2026101501+native.3-1_amd64.deb`
- 大小：92,610,120 B；Installed-Size：328,840 KiB
- SHA256 `5fab7042ba0079831d27256a7b9c86269495a51d27bc586fec1672b935e05de8`
- 自检：A1–A18、B1–B15、C1–C6 全过（含 zspace-player 随包、6 处播放器补丁）

## 2026-11-15 mock 中继实证：隧道首帧不是 greet，是直接加密数据段

- 方法：官方 Windows 客户端（`/tmp/zspace-probe-app` Wine 隔离副本 + 临时 prefix）
  → 本机 mock HTTPS 配置（`plat_url`）→ 本机 TCP mock 中继（自控 RSA + 自签证书）。
  **未改 hosts / 未动系统证书信任 / 未用账号凭据 / 未连真实 NAS**。
  脚本 `build/research-material/mock_relay.py`（build/ 已被 gitignore）。
- 握手成功后**没有**任何 greet/注册/信令。客户端第一条数据就是：
  `00000002 0801`（transport_header{AC_DATA_SEGMENT}）+
  `00000066 0862 1262 <98B cipher>`（data_segment_body{size=98,data}）；
  解密后是应用层裸 HTTP：`GET /home/static/version HTTP/1.1...`。
- 反向验证：mock 用同样帧回 `HTTP/1.1 200 OK`，本地 `curl` 经 SDK 13595 代理
  收到完整响应（rc=0）→ 隧道就是加密后的裸字节透传。
- mock 端 `client_verify_data` 之前 False 的原因：漏了 `sha256(master)` 再取
  hex 切片（`zsi::crypto::aes::encrypt`）；修掉后 True。
- 官方 `tcp_specific` connect 请求实抓：
  `0a065a50524f424510bf2718012002280568ffffffffffffffffff01`
  = `request_purpose=2(RP_MESSAGE)`、`isp=-1`，**不含** zrpc_addr/request_uuid/is_forward。
- 已改 `native/libzrpc/src/relay.rs`：purpose 10→2、isp 0→-1、可选字段按官方省略；
  新增 `connect_request_matches_official_wire` 逐字节单测；`cargo test` 25 过 2 ignored。
- 结论：帧形态没问题，真实中继 EOF 的剩余差异在 connect 参数；
  真实中继复测需 NAS 在线，按约定本轮不碰。

## 2026-11-16 真实中继复测通过（NAS 在线）

- 前提：用户确认 NAS 在线；只用本机日志里已有的真实 nas_id 做协议探测，
  **未用账号凭据、未修改 NAS**，只请求公开接口。
- 命令：`ZRPC_NAS_ID=<本机日志 nas_id> cargo test live_relay_http_probe -- --ignored --nocapture`
- 结果：guide `dispatch-yz:9091` → 中继 `223.111.129.16:9091`；`connect ok`；
  `/key/pub?webagent=v2&rnd=probe` 返回 779B `HTTP/1.1 200 OK`（NAS 公钥 JSON）；
  `/home/static/version` 返回 325B `HTTP/1.1 200 OK`；**不再 0 字节 EOF**。
- 真实链路帧形态（临时 `ZRPC_WIRE_LOG` 抓取，验证后已移除）：
  上行 `00000002 0801` + `00000047 0843 1243 <67B>`；
  下行 `00000002 0801` + `0000014b 08c502 12c502 <325B>`；
  与官方客户端抓包（`00000002 0801` + `00000066 0862 1262 <98B>`）同构。
- 结论：远程中继路径 guide→connect→msg.tls→加密切块 端到端可用；
  后续远程 NAS 访问不再需要协议层排障。

## 2026-11-16 产物 native.4 + 打包自检（含两处修复）

- 产物：`build/com.zspace.native.spark_2.46.2026101501+native.4-1_amd64.deb`
- 大小：92,628,712 B；Installed-Size：328,832 KiB
- SHA256 `3c1046da9b42f4c94a84c8881dda00298df42edad40d96733423eb9c8f662f6a`
- 载荷：官方 V2.46.2026101501 解包 + Electron 33.3.1 linux-x64 +
  当前 `libzrpc.so`（SHA256 `66895fb9…`，已在包内核对）+ `zspace-player`
- 脚本自检：A1–A19、B1–B15、C1–C6 全过（`build/native-verify-report.txt`）
- 本轮发现并修复：
  1. **`ffi.rs` 的 request_purpose 默认值 0 → 2**：JS 的 `start_service`
     不传该字段，官方 SDK 对 tcp_specific 实际发 RP_MESSAGE(2)。
     实测 purpose=0 也能通，但按原版口径改为 2。
  2. **包内 `background.js` 是 0600**（asar 解包带入），普通用户启动读不到。
     构建脚本增加 `chmod -R a+rX $FILES_DIR`，并新增自检 A19；
     checklist 增加 A11（包内文件普通用户可读）。
- 按 `.agents/checklist.md` 映射（原生包）：
  - 适用且通过：A1/A3/A4/A7/A8/A9/A10/A11、B1/B6、C1(脚本层)/C3、D1/D2/D3、E1
  - 不适用：A2（包名是 `com.zspace.native.spark`）、A5/A6（不打包 exe/asar，
    改为解包 app 目录）、B2–B5/B8（Wine 字体链）、E2–E5（不依赖 Wine）、F1（原生 Chromium 不加 wgpu 补丁）
  - 待实测：B5（原生 Electron 的字体回退 CDP）、B7（run.sh 未 unset
    WAYLAND_DISPLAY，treeland 需真机验证）、F2–F4（性能/画面）、C1/C2（用户安装启动）、E6
- 偏差：原生包走 `packaging-native/`，与 Wine 包 `packaging/` 并存；
  不投稿星火、仅个人使用（改动了官方产物）。
- D3：`.agents/sources.md` 已有 V2.46.2026101501，无需追加。



