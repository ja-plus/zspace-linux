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
