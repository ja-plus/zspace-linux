# 评估报告：用 REA / craft-anything 逆向迁移极空间闭源库到 Linux

- 日期：2026-11-12
- 分支：`research/linux-native-re`
- 状态：**仅评估，未开始实施**（未安装/运行任何逆向工具，未改动官方二进制）
- 口径：用户已在本分支明确豁免「不修改、不破解、不替换官方二进制」约束
  （2026-11-12）。外部 EULA/版权风险不随分支口径消失，见 §4。
- **最终目标：Deepin 25（amd64）上可安装、可运行的原生版**（用户 2026-11-12 明确）。
- **关键突破（2026-11-12）：mac 版 libzrpc.dylib 未 strip，保留完整符号表
  （75,302 符号 / 45 个源文件名 / 完整类层次）——闭源库架构已透明，见 §2.4。**
- 前置报告：`.agents/memory.md` 2026-11-12「不用 Wine 原生运行的可行性」

---

## 0. 结论速览

| 目标 | 判定 | 一句话理由 |
|---|---|---|
| **完整移植 libzrpc**（含远程 P2P） | ⚠️ 客户端可逆，远程互操作仍存疑 | mac 符号表已拿到完整架构（§2.4），客户端理解成本骤降；但远程协议另一半在闭源云+NAS 固件，服务端可校验/改协议，墙还在 |
| **LAN-only 替代实现**（libzrpc.so，局域网） | ✅ 高置信可行 | FFI 契约 JS+JNI 双重可见；mac 符号给出 LAN 精确类图（direct_connection/discover_utils/nas_status_checker）；官方网页版证明 LAN 是标准 HTTP |
| 远程访问功能 | ✅ 可不丢 | 官方 Skill 证明 zconnect HTTPS 云代理+OAuth2 可达（§3.4）；Tailscale 提速；不必重写 P2P |
| 播放器 / 同步 / 挂载 | 🔁 替换/延后 | 播放器换原生 libvlc（官方同款引擎）；同步/挂载是 Go，二期可补；不阻塞主链路 |
| 工具 | ✅ REA | MIT 的 MCP 逆向工作台；mac 符号+bindiff 是主力，craft-anything（alpha）不装 |
| 合规性 | ⚠️ 项目口径已豁免 | 本分支豁免「不修改不破解不替换」；仍不能投稿星火；外部 EULA/版权风险仍在，个人使用为宜 |

**总判断（mac 符号突破后修订）**：用户的「多版本逆向」直觉被证实——
**mac 构建未 strip，是打开闭源库的钥匙**。Windows/Android 都 stripped，
唯独 mac 保留 75,302 个符号、45 个源文件名、完整命名空间层次。这把
「逆向一个 4 万函数的黑盒」变成了「按图重写一个已知架构的库」，
客户端理解成本降一个量级。据此：
- **LAN-only 原生版（R2）从「有条件可行」升为「高置信可行」**，是主线。
- **完整远程 P2P 重写**仍不推荐立项——不是因为读不懂客户端（现在读得懂），
  而是因为协议另一半在服务端、互操作无保障、维护债永久。远程功能用
  官方 HTTPS 云代理 / 自建隧道承接即可，不必啃 P2P。
- 目标产物：**Deepin 25 可 dpkg/apt 安装的原生 deb**（Linux Electron +
  自制 libzrpc.so + 原生 libvlc），与现有 Wine 完整版并存。

---

## Phase 0 静态实测结果（2026-11-12，完全离线，未碰 NAS）

**方法**：mac libzrpc.dylib 符号 + 自写 Mach-O 解析器 + capstone 反汇编
（脚本存 `build/research-material/`）。无网络、无 NAS 参与。

**LAN 发现协议契约（已逆向）**：
- 调用链：`discover_session::boardcast()` → `discover_utils::make_discover_request()`
  → `make_cipher_key()` + `zsi::crypto::aes::encrypt()` → `udp_socket::broadcast(port, data)`
- **传输**：UDP 广播，**端口 5577（0x15c9）**，逐网卡发送
- **请求明文**：JSON `{"key":"tdxnassearch", ...}`（tdx=天顶星；json::FastWriter 序列化）
- **加密（已坐实，修正中途误判）**：**AES-256-GCM**——encrypt 包装实际调用
  `EVP_aes_256_gcm`（0x3ec400）+ `EVP_CIPHER_CTX_new`；密文再 **hex 编码**
  （输出长度 2×）。中途曾据 OpenSSL 通用串表误判 AES-128-CBC，**已纠正**：
  通用串表含所有算法名，须看实际 call。
- **密钥派生（关键，与 JS 路径同构）**：`make_cipher_key()` = **boost MT19937，
  固定种子 0x482667 = 4728423**，624 状态字、乘子 0x6C078965、标准 tempering、
  取 **64 个随机数**经 stringstream 拼接 → **`zsi::crypto::sha256::calc`**
  （0x44c60）→ hex → **key=hex[0:32]（32B）、iv=hex[0:16]（16B）**。
  **与 JS service-config 加密完全同一套**（同种子/同 SHA-256/同切片/同 GCM），
  那条路已确认可用 Rust（mt19937+sha2+aes-gcm）逐字节复现 → 发现加密可直接复用。
- **响应字段**（`make_discover_response()` 反汇编）：JSON 含 **`id` / `ip` /
  `hostname`**，内部用 `nas_id` / `local_ip` / 型号代码（如 `Z04A`），
  `"%s:%d"` 拼 ip:port。NAS 端用同一库构造（DEVT_NAS），客户端
  `invoke_search_handler()` 解析。

**生死判定：🟢 GREEN LIGHT。**
LAN 发现的 AES-256-GCM 密钥是**固定种子确定性派生，且与已破解的 JS 路径
完全同构**——属混淆而非真安全，Rust 复现成竹在胸。LAN 发现对我们实质透明。

**代理行为（13579，已挖）**：
- `start_service` 解析 JS 传来的 AES-GCM 加密配置（含 `service`/`ts` 字段），
  日志 "start service params/result"。
- `http_header_generator` 维护 URI 模板数组（`http_header_uri_arr`）+
  `http_header_template` + `http_header_user_agent`，按 URI 归一化请求头。
  模板实例揭示 NAS API 端点与约定：
  `/v2/file/download?webagent=v2&request_purpose=5&show_hidden=0&path=…`、
  `/system/recent?rnd=<ts>_<rand>&webagent=v2`、`/system/diskusage3`、
  `/zspool/info`、`/manager/services` —— **`webagent=v2` 标记 + `rnd` 时间戳
  缓存破坏参数**是请求约定。
- `nas_confirm::start()` 用 `https_client`（boost.asio.ssl）→ NAS API 走
  **标准 HTTPS/TLS**（非私有加密），Rust reqwest 可直接处理（自签证书跳过校验）。

**Phase 1 收尾微细节**（均不阻塞，实现时复现+对照真实包即可定稿）：
- key/IV 从 sha256 hex 的精确切片验证（已与 JS 对齐，待实测确认）
- 发现响应完整字段集 / UA 具体字符串 / 代理是否注入鉴权头
- 补齐方式：复现密钥后对照一个真实广播包，或继续反汇编；
  **运行时抓包为可选确认，非生死关口**（静态已答）。

**NAS 安全**：本阶段全部离线，NAS 零接触。

---

## 1. 工具评估

### 1.1 REA（morluto/rea，Reverse Engineer Anything）

来源：README 实抓（2026-11-12），仓库 7,284 commits，912 open issues，活跃。

- **形态**：TypeScript / MIT / npm 包 `rea-agents`，`npx rea-agents setup`
  一键注册 MCP server + 工作流指令。支持 Claude Code、Codex、Cursor、
  Gemini CLI、Grok Build 等 agent（标准 MCP，本 CLI 也能接）。
- **定位**：「One MCP for reverse engineering across binaries, applications,
  and runtime behavior」。agent 通过 MCP 调工具检查目标、追代码，
  **返回带证据（evidence）和局限声明的结论**，再由 agent 解释/实现/测试。
- **与本目标相关的能力**（README 原文清单）：
  - 原生二进制：伪代码、汇编、字符串、符号、调用与引用 —— 后端接
    **Hopper / Ghidra / IDA / Kuna**（Ghidra 免费，其余商业）；
    离线也支持 objdump / Rizin。
  - **JavaScript / Electron**：模块、imports、source maps、路由、IPC、
    原生 add-on 分析 —— 正好覆盖「从 background.js 提取 FFI 契约」。
  - **Go 二进制**：内嵌编译器版本、模块、replace、build settings
    （静态，无需 Go 环境）—— 正好覆盖 ZSpaceSync/ZSpaceMount。
  - PE imports / 证书 / 加固标志、进程行为捕获（Linux/macOS PTY）、
    pcap/HAR 网络抓包分析（tshark/mitmproxy）。
  - Linux 支持：Ghidra/JADX/静态 JS 分析需要 procps 兼容的 `ps`。
- **能力边界的实证**（官方 showcase，注意量级）：
  - DX-Ball：重建一个声音声像计算函数，**63 字节**的函数体，3,205 个
    用例验证 —— 函数级。
  - TH04（东方旧作）：恢复 DOS 弹幕环角度计算 —— 函数级。
  - Notion：追踪 Electron 剪贴板桥（renderer→preload→IPC→main）——
    链路级理解，不是重写整个 Notion。
  - **没有任何「完整重写大型闭源网络服务」的公开案例。**
- **环境要求**：Node 22.x(≥22.23.4) / 24.x(≥24.22.1) / 26.12.1+。
  本机 volta 默认 npx 跑的是 v24.21.0，**略低于门槛，接入前要升 Node**。
- **匹配度：高**。本项目要做的「读 JS 契约 + 观察运行时 + 抓包 +
  看伪代码」正是它的主场。

### 1.2 craft-anything（storytold/craft-anything）

来源：README 实抓（2026-11-12），934 commits，**early alpha**。

- **形态**：纯 Rust / MIT+Apache-2.0，320 个工具（300 引擎内置），
  桌面 app（egui）+ CLI + MCP 三形态；自带解码器/反汇编器/反编译器，
  Deep 模式可加 Ghidra。
- **工作流**：Study → Build → Check → Ready。让 coding agent
  （Claude Code 优先，Codex/Copilot/Grok 等 ACP 适配）在指定技术栈
  重建目标，再逐项比对、迭代。
- **两种模式**：
  - Clean room（默认）：**不读目标内部**，只看外观行为（截图/布局/
    可见文本）——对闭源 DLL 毫无用处。
  - Deep：读代码和文件、用全部引擎 —— 对本目标必须开 Deep。
- **与本目标的错位**：
  - 比对环节为「网站三档宽度截图 / 游戏自测 / app 运行」设计；
    一个无 GUI 的网络 SDK 没有可比对的「外观」，其 app 行为检查
    官方自认「partly there」。
  - 官方 Status 明示 alpha：API/工具集持续变动，不建议生产投入。
  - 社区评测一致提醒：拿商业闭源软件做复刻有法律风险，输出只是草稿。
- **匹配度：中低**。它的 Advanced 工作台（函数列表 + 伪 C + 证据账本）
  可以当分析界面用，但整体不如 REA 贴合；**不建议两个都上，选 REA**。

### 1.3 关键认知：这类工具改变的是什么

它们把「逆向理解」从人工逐函数阅读变成 agent 驱动的批量问答，
**降低的是理解成本，不是重写成本**。重写仍然要：
协议逐字段对齐、加密握手逐字节对齐、状态机逐分支对齐，且没有
官方测试套件兜底。对 4 万函数的库，理解本身也是无底洞。
所以可行性取决于**把目标切到多小**，而不是工具多强。

---

## 2. 目标取证（本次实测，非猜测）

解包链路：官方 exe → `$PLUGINSDIR/app-64.7z` → `resources/app.asar`
+ `app.asar.unpacked`。工具：7z、@electron/asar、objdump、strings。

### 2.1 libzrpc_x64.dll —— 核心连接 SDK（致命阻塞，也是唯一值得 RE 的）

| 项 | 实测值 |
|---|---|
| 体积 | 13,250,440 字节；.text 7.5 MB |
| 格式 | PE32+ DLL，stripped to external PDB，GNU 风格节布局（.edata/.idata/.xdata/.bss）→ **MinGW GCC 工具链** |
| 语言 | C++（无 Go runtime 标记、无 Rust 标记；protobuf C++ 源码路径内嵌） |
| 函数规模 | .pdata 489,036 字节 ÷ 12 ≈ **40,753 个函数**（含静态链接库） |
| 静态链接栈 | **Asio**（2752 处）、**OpenSSL**、**protobuf C++**（12194 处）、**KCP**、**uTP**、**STUN/TURN**、libcurl、boost 片段 |
| 导入 DLL | WS2_32 / MSWSOCK / bcrypt / CRYPT32 / IPHLPAPI / msvcrt 等，纯 Windows 网络+加密 |
| 私有协议 | 内嵌 proto：`message.tp.proto`、`message.tls.proto`；消息名如 `tls.handshake_context`；**145 个 PEM 证书/密钥块** |
| 服务端配合 | dispatch-bsnew/cs-yz.zenithspace.net、proxy.zenithspace.net、api.zenithspace.net、*.direct.zspace.link、zrpl-ucloud.zenithspace.net |
| 导出 | **20 个 `libzrpc_*` C ABI 函数**（init/uninit/refresh/search_device/detect_nas/start_service/stop_service/stop_all_service/check_ports/get_nas_status/network_diagnose/link_diagnose/check_conn_ability/set_device_type/set_private_network_access/set_ipv6_priority/get_force_tunnel/set_force_tunnel/set_sdk_location） |

**定性**：这是一个完整的私有 P2P + 中继网络栈（设备发现、NAT 穿透、
加密握手、隧道、本地代理），不是小工具。协议细节在二进制里，
调度逻辑在服务端，**两头都不在我们手上**。

**闭源核验（2026-11-12 补证）**：
- PE 无版本信息资源（无 CompanyName/Copyright/FileDescription），
  无任何 license/copyright 字符串；
- 全库唯一源码 URL 是 `github.com/golang/protobuf/protoc-gen-go/descriptor`
  （protobuf 自带的描述符路径，不是本项目源码）；
- 导出 API 全是 NAS 专属语义（detect_nas/get_nas_status/
  set_private_network_access），官方云端配置路径自称 `zrp/zrpc`；
- 公开网络检索无此库的开源仓库（搜到的「zRPC」是 go-zero 框架的
  Go RPC 组件，同名无关）；仅随官方安装包分发，mac 版为 libzrpc.dylib。
- 结论：**极空间自研闭源库**（zrpc ≈ ZSpace RPC），高置信。
- **跨平台范围（2026-11-12 三次取证）**：DLL 内嵌设备类型枚举
  `DEVT_PC / DEVT_MAC / DEVT_IOS / DEVT_ANDROID / DEVT_OHOS（鸿蒙）/
  DEVT_TV / DEVT_APPLETV / DEVT_NAS / DEVT_H5 / DEVT_WEB /
  DEVT_MINI_PROGRAM / DEVT_UNKNOWN` —— 同一 C++ 核心跨平台编译，
  iOS/Android 客户端与 **NAS 固件本身**（DEVT_NAS）都用它。
  推论：① 官方已移植 6+ 平台，Linux 对其只是再加一个目标（R3 更成立）；
  ② **Android 已实锤（2026-11-12 下载官方 APK V2.5.653 验证）**：
  `lib/arm64-v8a/libzrpc.so`（12.75 MB，ELF aarch64，stripped，仅 arm64
  一个 ABI），同栈（asio/protobuf/OpenSSL）、同 DEVT 枚举；导出为 JNI
  `Java_com_zrpc_service_ZrpcService_*` 共 31 个方法，与桌面 20 个 FFI
  导出核心一一对应（另多 fileTransfer* 7 个、pushRegister、setCallBack、
  netStatusChanged、getNasDrIp、active）。**契约获 JS+JNI 双重交叉验证**。
  APK 的 Java/DEX 层比 stripped C++ 易读得多，可作 Phase 0 契约提取参照
  （APK 直链见 §6，已删，可随时重取）；android .so 是 bionic 链接，
  不能在 glibc Linux 直接 dlopen，仅作逆向参照。物料留存：
  `build/research-material/libzrpc-android-arm64.so`。
  ③ Android 播放同样用 libvlc（APK 内 libvlc.so+libvlcjni.so），VLC 选型再获佐证；
  ④ iOS 二进制是 Mach-O 且仅 App Store 分发，无法直接取证，同核心为高置信推论。

**但有两个关键利好**：
1. **FFI 契约完全可见**：`background.js` 里 ffi-napi 的签名表把 20 个
   函数的参数/返回类型全部列了出来（多为 string→string 的 JSON 出入参）。
   替代实现不需要猜接口。
2. **局域网 API 被官方网页版证明是普通 HTTP**：浏览器直连 `NAS_IP:5055`
   就能用全部核心功能。libzrpc 在局域网场景大概率只是
   「发现设备 + 起一个 13579→5055 的本地代理 + 状态查询」，
   私有加密栈主要服务于远程 P2P。**LAN 子集可能薄得出乎意料。**

**JS↔DLL 边界已完全透明（2026-11-12 二次取证，全部来自 background.js，
无需碰二进制）**：
- `libzrpc_init` 入参 = 纯 JSON：`{log:{level,type,capacity,filepath},
  env:{data_dir,cache_dir,sdk_location:"C"}, option:{p2p:{actions:...}}}`；
  release 版不带 plat_url（那是 dev 环境配置）。
- `start_service` 入参 = **JS 侧** AES-256-GCM 加密的 base64。明文极简：
  `{local_port, remote_port:5055, type:1, nas_id, ts}`。
- 密钥派生是**固定种子的确定性混淆**：MT19937(seed=4728423) 取 64 个
  genrand_int32 拼成十进制串 → SHA-256 hex → key=[0:32]、iv=[0:16]。
  Rust 用 mt19937 + sha2 + aes-gcm crate 可逐字节复现。
- 返回值 = 普通 JSON 字符串（JS 用 JSON.parse 直接解析）；
  `search_device` 返回 `{data:[{nas_id,...}]}`，JS 侧过滤 P0 前缀设备。
- 结论：**FFI 层零逆向成本**。真正的未知只剩 DLL↔NAS/云端的线路协议，
  而 LAN 段大概率是明文 HTTP + 某种 UDP 发现广播。

### 2.2 其它闭源组件

| 组件 | 体积 | 实测语言/栈 | 处置建议 |
|---|---|---|---|
| ZSpaceSync.exe | 43.4 MB | **Go 1.22.12**，`github.com/syncthing/syncthing`（15281 处模块路径）+ quic-go + gogo/protobuf → **syncthing 魔改 fork** | 不 RE。Go 符号保留好、REA 有 Go 专项支持，真要做也是二期；首期禁用同步 |
| ZSpaceMount.exe | 24.5 MB | Go + rclone + FUSE（WinFsp/macFUSE 字符串）| 不 RE。Linux 有原生 rclone/FUSE，但应用 IPC 私有；首期禁用挂载 |
| rclone1.exe | 27.4 MB | rclone（开源 MIT） | 直接换 Linux 官方二进制 |
| UZPlayer.exe + playsdk.dll + VLC | 263 MB | Qt6 C++；playsdk.dll 仅 119 KB、42 导出，是 libvlc 的 Qt 桥；**bundled VLC = 4.0.0-dev（git eee47c59a9）**，Windows 包无 VA-API 插件 | 不 RE 引擎。**默认换原生 libvlc**（可编译官方同 commit 精确对齐，或发行版 3.0 稳定版），Linux 下 VA-API 硬解；mpv 仅备选（libmpv GPL 传染、无版本对齐）。应用↔播放器控制协议仍需观测复现，播放器窗口 UI 需重做/简化 |
| mounts3.dll / zsyncoi / mountThumb | 小 | C++，Windows 资源管理器集成 | Linux 无对应物，直接砍 |
| adb/scrcpy、librespeed-cli | 小 | 开源 | 换 Linux 版（手机备份功能二期） |

### 2.3 JS 侧（上一轮已确认，此处汇总）

- Electron 33.3.1 / Chromium 130；Linux 运行时官方有现成 zip。
- ffi-napi / ref-napi **asar 自带 linux-x64/arm64 prebuilds**；
  sqlite3 补一个 napi-v6 linux 预编译即可。**JS 侧原生模块零编译。**
- 应用自身只有 win32/darwin 分支，linux 分支要补（改 JS = 改官方代码，
  见 §4 合规）。

### 2.4 mac 版符号突破（2026-11-12，多版本逆向的关键钥匙）

下载官方 mac 客户端（Intel + ARM，均 V2.46.2026101501，与 Windows 同版本），
解包 `极空间.app/Contents/Resources/app.asar.unpacked/sdk/libzrpc.dylib`
（15.8 MB，Mach-O x86_64）。**与 Windows/Android 不同，mac 版未 strip**：

- LC_SYMTAB **75,302 符号**；定义函数 55,479（external 14,557 / **local 40,922**）；
  C++ mangled 名 27,957；去重后 **32,172 个可读符号**（已 dump 存物料）。
- 20 个 `libzrpc_*` 导出齐全，与桌面 FFI / Android JNI 完全对应。
- **45 个源文件名**（`__GLOBAL__sub_I_*`）暴露完整代码结构：
  `libzrpc.cpp / api.cpp / application.cpp / connect_router.cpp /
  connection_pool.cpp / direct_connection.cpp / p2p_connection.cpp /
  tunnel_connection.cpp / public_connection.cpp / ipv6_connection.cpp /
  base_extranet_connection.cpp / signal_connection.cpp / zrpl_connector.cpp /
  zrps_delegate.cpp / gslb_manager.cpp / heartbeat.cpp / nas_confirm.cpp /
  nas_status_checker.cpp / nas_type.cpp / message.tls.pb.cc / message.tp.pb.cc /
  livep2ptransfer.cpp / debug_http_handlers.cpp / http_header_generator.cpp …`
- **私有命名空间层次清晰**：`zsi::`（ZSpace Internal，核心）、`zrpc::`
  （application/core/initializer）、`psl::`、`basefw::`、`msg::`、
  `live_p2p_transfer::`、`p2pnetwork::`、`BaseHcdn::`。
  `zsi::transport` 下按连接类型分类：direct / p2p / tunnel / public /
  ipv6 / https / tcp，另有 discover_utils（发现）、nas_status_checker、
  connect_history、gslb_manager、upnp_checker、firewall_rules 等。
- **开源依赖确认**（Linux 可直接链接同款）：boost.asio、protobuf、
  **Simple-Web-Server**（header-only HTTP/WebSocket，本地代理大概率基于它）、
  cxxopts、pugixml。私有命名空间符号约 5~6 千（去重后 distinct 函数更少）。

**意义**：闭源库从「4 万函数黑盒」变成「有完整类图/源文件清单的已知架构」。
- LAN 子集精确锁定：`direct_connection` + `discover_utils` + `nas_confirm`
  + `nas_status_checker` + `zsi::transport::api`，可按名研究 disassembly。
- 远程子集也读得懂了（signal_connection/zrpl/zrps/p2p），但**服务端墙仍在**
  （见 §3.1）——读懂客户端 ≠ 服务端认你。
- 物料：`build/research-material/`（mac dylib + android .so + playsdk dylib
  + 32k 符号 dump + 架构摘要 + 两个分析脚本 macho_sym.py/demangle_analysis.py）。
- 方法可复现：GNU objdump 不支持 Mach-O，用纯 Python 解析 LC_SYMTAB
  + c++filt 批量 demangle（脚本已存）。

---

## 3. 可行性：把战场切小

### 3.1 路线 R1：完整移植（❌ 技术判定，非口径问题）

把 libzrpc 全部功能（含远程 P2P/中继）在 Linux 重写。
口径豁免后，剩下的全是硬技术问题，按致命程度排序：

1. **对端不止一个，而且都不在我们手上**。全量移植不是「重写一个离线库」，
   是要同时与**云端调度集群**（dispatch-bsnew/cs/yz、proxy、zrpl-ucloud）
   和 **NAS 固件（ZOS）**两个闭源对端互通。协议的另一半实现住在服务器里，
   我们只能从客户端侧观测猜；服务端随时可以改协议、加风控、
   拒绝未知实现。这是与「移植一个单机工具」的本质区别。
2. **私有握手 + PKI 必须逐字节对齐**。145 个内嵌 PEM、`message.tls.proto`、
   `tls.handshake_context` 说明有自定义加密握手与证书体系；
   可能还有设备指纹/版本校验。代码重写对了，服务端未必认。
3. **规模**（mac 符号突破后修订）。私有核心约 5~6 千命名符号
   （zsi/zrpc/psl/basefw/msg/live_p2p_transfer/p2pnetwork/BaseHcdn），
   开源库（boost.asio/protobuf/SimpleWeb/cxxopts/pugixml）Linux 直接链接。
   **mac 未 strip 给了完整类图与源文件名，客户端理解成本降一个量级**——
   「读不懂」已不是障碍。但 P2P 全局状态机的一致性、逐字节协议对齐，
   仍是要逐个啃的硬活，符号只是地图不是源码。
4. **没有测试预言机**。P2P 的状态空间 = NAT 类型 × IPv6 × 中继回退 ×
   多设备 × 弱网，无法穷举；错误表现为「偶发断连」，归因极难。
   官方没有测试套件可抄，只能黑盒对照 Windows 客户端。
5. **永久维护成本**。官方约每月更新，协议可能随版本变。Wine 路线
   免费跟随官方更新；全量移植 = 无源码 fork 一个网络 SDK，
   每次官方更新都要重新逆向对齐。这是一笔长期债，不是一次性工程。
6. **没有「直接复用原二进制」的捷径**。mac dylib 是 Mach-O，Linux 是 ELF，
   格式/ABI/系统库全不同，**不能直接加载**（加载 PE/Mach-O + 翻译系统 API
   等于重新发明 Wine，与「原生」矛盾）。mac 包的价值是**符号蓝图 +
   开源依赖清单 + 跨平台 JS/资源**，不是可运行的二进制。

估时：客户端理解因 mac 符号大幅缩短，但远程互操作对齐仍是人年级投入；
成功率：远程全功能仍低（服务端不可控）；token 成本：数千~数万元级。
**不推荐为远程 P2P 立项**——远程功能用 §3.4 的官方 HTTPS 云代理/自建隧道
承接，性价比远高于啃 P2P。**LAN-only（R2）则因符号突破升为高置信主线。**

**难度不均**（公平起见）：protobuf schema 可从内嵌描述符近乎免费提取；
ZSpaceSync 是 Go（符号保留好，REA 有专项支持）比 C++ 好逆一个量级；
ZSpaceMount 同理；mac dylib 证明代码本身可移植。
**真正的杀手只有 libzrpc 的远程 P2P 部分**——而它恰好是相对
「官方 zconnect 网页兜底」增量价值最低的部分。

### 3.2 路线 R2：LAN-only libzrpc.so（✅ 高置信可行，主线；mac 符号突破后升级）

**思路**：不「移植」原库，而是**按可见契约 clean-room 重写一个只支持
局域网的替代 .so**，远程功能返回「不支持/未连接」。

分阶段：

- **Phase 0 — 契约与行为取证（REA 主场，约 2~5 天）**
  1. REA 的 Electron/JS 分析：从 background.js 固化 20 个 FFI 函数的
     调用点、入参构造、返回值解析逻辑 → 得到《接口契约书》。
  2. 在现有 Wine 容器里跑真实客户端，REA 进程捕获 + Frida/抓包：
     记录每个 libzrpc_* 调用的真实入参/返回 JSON；
     抓 13579 代理与 NAS:5055 的流量、设备发现的 UDP 广播。
  3. 判定局域网链路是否加密（`tls.handshake_context` 用在哪一层）。
     **这是本路线的生死判据**：若 LAN 发现/握手也走私有加密，
     工作量与风险陡增，应止损。
- **Phase 1 — 薄壳实现（约 1~3 周，视 Phase 0 结论）**
  用 Rust/C++/Go 实现 `libzrpc.so`：
  `init/uninit` 生命周期、`search_device/detect_nas`（UDP 发现，照抄观测）、
  `start_service` = 13579→NAS:5055 的 HTTP 反向代理、
  `get_nas_status/check_ports/...` = 对 NAS API 的查询封装、
  远程相关函数返回安全失败值。
  验收标准：官方 asar 的 JS 一行不改逻辑、只补 linux 分支，
  能登录局域网 NAS 并浏览文件。
- **Phase 2 — 集成与 packaging（约 1~2 周）**
  Linux Electron 33.3.1 + asar + linux prebuilds + 薄壳；
  播放器接**原生 libvlc**（默认；官方同 commit 4.0.0-dev 或发行版 3.0，
  `/splay` 流 + VA-API 硬解）；同步/挂载/手机备份禁用并
  在 UI 上如实标注；按星火规范组 deb（**但见 §4：不能投稿**）。

- 总估时：**4~8 周**（agent 辅助、单人决策）；
- 成功率估计：**LAN 登录+浏览 50~70%**（主要不确定性在发现/握手加密），
  官方远程协议重写 0%（明确放弃，但见 §3.4——远程功能本身可不丢）；
- 产物形态：功能受限的「局域网原生版」，与现有 Wine 完整版并存。

### 3.3 路线 R3：等/要官方 Linux 构建（✅ 成本最低，建议并行走）

取证显示 libzrpc 的栈（Asio/OpenSSL/protobuf/KCP/curl）**全部跨平台**，
且官方已有 mac dylib —— 对官方来说 Linux .so 基本是「换工具链重编 +
平台分支补全」，成本远低于我们重写。ZSpaceSync 是 Go，交叉编译更是
一条命令。**向官方反馈 Linux 需求（附本报告取证）是性价比最高的动作。**
官方「极空间 Skill」CLI 已支持 Linux（还是 Rust 写的），说明有 Linux
投入意愿与 Rust 工程能力。

### 3.4 远程访问：不重写 P2P 协议，走官方标准 HTTPS 通道（2026-11-12 重大更新）

「架构性放弃」的准确含义是**放弃重写官方私有 P2P/中继协议**（dispatch 注册/
打洞信令/隧道会话，对端是闭源云+NAS 固件）。但远程功能本身有两条现成承接路：

**① 官方 zconnect HTTPS 云代理 + OAuth2（首选，有官方存在性证明）**
官网下载页的「极空间 Skill」（zspace-nas-agent-toolkit V0.3.11）是**官方
Rust CLI，原生支持 Linux x64/ARM64**，`--mode auto|lan|cloud`：
cloud 走 `www.zconnect.cn` HTTPS 云代理，登录走 `oauth.zconnect.cn`
标准 OAuth2，**完全不经过 libzrpc**。90+ 管理操作（文件/相册/影视/
下载/Docker/虚拟机…）全通。取证（linux-x64 二进制，SHA256 校验通过）：
- ELF 静态链接 stripped，但 Rust panic 串保留模块名（api.rs、album_*.rs、
  file_metadata.rs…）与 crate 路径（tokio 1.53.2 等）；
- 端点表可见：`/account/oauth2/userinfo`、`/api/v1/`、`/api/v2/`、
  `/file/list`、`/zvideo/video/playlist/info` 等 —— 与网页版同套 NAS HTTP API；
- 六平台二进制直链+SHA256 在包内 runtime-artifacts.json（linux-x64 仅 6.96 MB，
  `https://update.zenithspace.net/AgentToolkit/20261023/zspace-cli-v0.3.11-linux-x64`）。
**意义**：R2 的远程模式可在 Rust 薄壳里实现为「13579 → zconnect HTTPS
云代理」，OAuth2 登录，**零 P2P 逆向**；且 API 层有官方 Linux 参考实现
可研究（端点/鉴权/选路），不必黑盒猜。云代理带宽/功能可能有限制
（网页版无文档同步），大数据量场景用②补。

**② Tailscale/WireGuard 自建组网（性能选项）**
NAS 端 Docker 部署 Tailscale（极空间有社区教程，x86 Docker 型号可行；
Z4 Pro 等还内置「节点小宝」组网），客户端入网 → 远程变虚拟局域网 →
薄壳直连 100.x IP，WireGuard 直连速度通常优于云代理。
前提：NAS 支持 Docker；兜底 zconnect 网页 / Wine。

**结论**：远程行从 ❌ 改为 ✅（云代理默认 + 自建隧道提速），
都不需要重写私有 P2P 协议。R2 的 API 层风险因官方参考实现而显著下降。

### 3.5 mac→Linux 复用盘点 + Deepin 25 产物组件清单（2026-11-12）

用户问「mac 包有没有可直接复用的」。原则：**Mach-O 二进制一律不能直接跑在
Linux**（格式/ABI/系统库不同）；可复用的是「非二进制资产」与「开源依赖清单」。

| mac 包组件 | Linux 处置 | 类别 |
|---|---|---|
| app.asar（JS/Vue，53MB） | 直接用（与 Win 同源），补 linux 平台分支 | ✅ 直接复用 |
| ffi/ref-napi linux prebuilds | asar 已自带 | ✅ 直接复用 |
| zsviewer（PDFNet WASM）、icons、fonts | 直接用 | ✅ 直接复用 |
| **libzrpc.dylib 符号表** | 作逆向蓝图（§2.4），据此重写 Linux .so | 📖 参照 |
| boost.asio / protobuf / Simple-Web-Server / cxxopts / pugixml | Linux 链接同款开源库（repo 或随包） | ✅ 开源替换 |
| Electron Framework（mac） | 换官方 **Electron 33.3.1 linux-x64** zip | ✅ 开源替换 |
| libvlc.dylib / UZPlayer.app | 换 Linux **libvlc**（官方同款引擎，VA-API 硬解）；播放 UI 重做 | ✅ 开源替换 |
| libzrpc.dylib 本体 | **重写**为 libzrpc.so（Rust 或 C++，按符号类图） | 🔨 逆向重写 |
| libplaysdk.dylib（179KB Qt→VLC 桥） | 不重写，播放控制协议观测后用 libvlc API 复现 | 🔨 协议复现 |
| ZSpaceSync（Go syncthing fork）/ ZSpaceMount（Go+rclone） | 二期：Go 逆向或禁用；Linux 有原生 syncthing/rclone 但 NAS 协调私有 | ⏸ 延后 |
| fuse-t-macos-installer.pkg / CheckFileProvider | mac 专属，Linux 用 FUSE3，砍 | ❌ 不适用 |
| adb/fastboot/platform-tools | 换 Linux 版（手机备份二期） | ⏸ 延后 |

**Deepin 25 原生 deb 组件清单（目标产物）**：
- 运行时：Electron 33.3.1 linux-x64（随包）+ 官方 app.asar（补 linux 分支）
- 连接：**自制 libzrpc.so**（LAN：direct/discover/nas_status；远程：zconnect
  HTTPS 云代理 + OAuth2，参照官方 Skill CLI）
- 播放：系统 libvlc（Deepin 25 仓库有 vlc 3.x）或随包 libvlc，VA-API 硬解
- JS 原生模块：ffi/ref linux prebuilds（自带）+ sqlite3 napi-v6 linux 预编译
- 同步/挂载/手机备份：首期 UI 禁用并标注，二期补
- 打包：沿用本项目 deb 链路（`/opt/durapps/<name>.spark/`、postrm 清理等），
  **新包名**（如 `com.zspace.native.spark`），Deepin 25 用 `dpkg -i`/`apt install`
  本地安装；**不投稿星火**（改了官方二进制，商店红线）
- Deepin 25 依赖核对（实施时验）：Electron 33 需 libnss3/libatk-1.0/libgtk-3/
  libgbm/libasound2/libxkbcommon 等；libvlc 需 vlc 插件包；Deepin 25 为
  Debian 系 amd64 glibc，上述均在仓库。

---

## 4. 风险与红线（必须先读）

1. **项目口径**：AGENTS.md 原禁「不修改、不破解、不替换官方二进制」，
   星火红线禁改动官方二进制。**用户已在本分支豁免项目口径（2026-11-12）**。
   仍然成立的硬约束：产物**不得投稿星火商店**（那是商店规则，不是项目规则）；
   建议仅限个人使用、不公开分发。
2. **法律**。官方 EULA 尚未逐条核对（开工前必查）。中国
   《计算机软件保护条例》第 17 条只允许为「学习、研究软件内含的设计
   思想和原理」而使用，**没有欧盟式的互操作反编译例外**；
   clean-room 重写能降低版权风险但不能消除（协议/接口本身的可版权性、
   不正当竞争、EULA 违约均存在）。**个人自用、不分发、不牟利**是
   风险下限姿态；公开分发 = 高风险，不建议。
3. **工具风险**。REA 成熟但依赖反编译器（Ghidra 免费够用）；
   craft-anything alpha，不建议投入。两者都不会绕过 DRM/登录
   （craft-anything 明示），也不应被要求去绕。
4. **成本**。agent 驱动的逆向是 token 密集型工作，Phase 0 起就要
   设预算上限；REA 的 MCP 输出有分页/摘要机制，注意上下文开销。
5. **技术止损点**。Phase 0 若发现 LAN 链路也走私有加密握手，
   R2 成功率掉到 <30%，应止损转 R3/网页版路线。
6. **环境**。本机 npx 默认 Node v24.21.0 低于 REA 门槛（24.x 需
   ≥24.22.1），接入前升级；Linux 上 REA 需要 procps 的 `ps`。

---

## 5. 建议

1. **现在**：本报告归档；向官方提 Linux 客户端/libzrpc.so 需求（R3）。
2. **若用户拍板接受口径变更与法律姿态**：只启动 **Phase 0**（纯观察，
   不改任何东西），用 REA + 现有 Wine 容器产出《接口契约书》与
   「LAN 是否加密」的判定，再决定是否进 Phase 1。
   （口径已于 2026-11-12 由用户在本分支豁免。）
3. **若仍想评估全量移植**：先做 2~3 天「可行性尖峰」把估计变测量——
   ① 从内嵌描述符提取 message.tp/tls.proto 完整 schema；
   ② 在 Wine 容器里抓 dispatch 通道流量，判定可否观测、是否 TLS pinning；
   ③ 用符号/字符串切分开源库与私有代码的函数占比。
   三项结果出来后再决定 R1 是否值得立项。
4. **不装 craft-anything**；REA 按需 `npx rea-agents setup`（先升 Node）。
5. 日常「原生」需求继续用上一份报告的路线 B（官方网页版原生封装），
   它与 R2 不冲突，且零法律风险。

---

## 6. 来源

工具（README 实抓，2026-11-12）：
- REA：https://github.com/morluto/rea （MIT；showcase：https://rea.tools/showcase/ ）
- craft-anything：https://github.com/storytold/craft-anything （MIT/Apache-2.0，early alpha）
- 对比文章：https://m.toutiao.com/article/7706592424100987418/
- craft-anything 解析：https://blog.csdn.net/weixin_43017921/article/details/169971860

目标取证（本地实测，物料 `packages/zspace_windows_2.46.2026101501_101619.exe`，
SHA256 见 memory.md 2026-11-12 打包记录）：
- objdump/strings 实测：节布局、导入表、20 导出、.pdata 函数计数、
  协议关键字、proto 文件名、PEM 计数、Go 模块路径
- background.js（5.5 MB webpack bundle）：ffi 签名表、平台分支、
  13579/5055 代理逻辑
- 上一轮调研：官方无 Linux 客户端、网页版 zconnect.cn / NAS:5055、
  社区无重打包项目（详见 memory.md 同日记录）