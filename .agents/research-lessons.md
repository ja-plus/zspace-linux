# research-lessons.md —— 原生 Linux / 逆向调研的方法论与教训

> 来源：`research/linux-native-re` 分支「极空间客户端原生化」调研（2026-11-12）。
> **本文沉淀可复用的方法与纪律**；具体目标的事实/契约在根目录
> `REPORT-linux-native-re.md`，逐次决策与产物在 `memory.md`。三者分工：
> 报告=「这个目标是什么」，记忆=「我们决定了什么」，本文=「以后怎么干」。

---

## 1. 多平台符号策略（本次最大的杠杆）

- **同一闭源库的不同平台构建，strip 策略往往不一致。** 极空间 libzrpc：
  Windows dll 与 Android .so 都 stripped，**唯独 mac dylib 保留完整符号**
  （75k 符号 / 45 个源文件名 / 完整类层次）。
- **纪律：下「stripped 黑盒、逆不动」的结论前，先把所有平台构建都验一遍。**
  官方升级接口对每个平台都有包（`app_id` 见 sources.md），mac 包常是突破口。
- 符号带来的不是源码，是**地图**：源文件名（`__GLOBAL__sub_I_*.cpp`）、
  命名空间层次、类/方法名。据此可把「逆向黑盒」降级为「按图重写已知架构」，
  并精确锁定要研究的子集（如 LAN = direct_connection/discover_utils）。
- 多构建还能**交叉验证契约**：桌面 JS 的 ffi 签名表 + Android JNI 导出名
  （`Java_com_zrpc_service_ZrpcService_*`）互相印证 API 语义。
- 跨时间版本差分（bindiff）是导航辅助，不是源码恢复；旧版本可能加固更弱。

## 2. Deepin/Linux 无 root 逆向工具链（实测可用）

环境约束：sudo 需密码、`python3 -m pip` 缺失、venv 无 pip、GNU binutils
不支持 Mach-O。绕法：

- **装 Python 包免 root**：从 PyPI 下 wheel 解压 + PYTHONPATH。
  ```bash
  URL=$(python3 -c "import urllib.request,json;d=json.load(urllib.request.urlopen('https://pypi.org/pypi/capstone/json'));print([u['url'] for u in d['urls'] if 'x86_64' in u['filename'] and 'manylinux' in u['filename'] and u['filename'].endswith('.whl')][0])")
  curl -s -o capstone.whl "$URL" && mkdir pylibs && (cd pylibs && unzip -oq ../capstone.whl)
  PYTHONPATH=pylibs python3 -c "import capstone"   # 验证
  ```
- **Mach-O 符号**：GNU objdump/nm 读不了 → 纯 Python 解析 `LC_SYMTAB`
  （nlist_64：`<IBBH Q`，N_SECT=0x0e 判定义符号），字符串表取名字，
  `c++filt` 批量 demangle（mac 名前缀 `__Z` 要先去掉一个 `_`）。
  脚本：`build/research-material/macho_sym.py`、`demangle_analysis.py`。
- **Mach-O 反汇编**：capstone（CS_ARCH_X86/CS_MODE_64）反汇编 `__TEXT,__text`；
  函数地址→文件偏移 = `sym_value - text.addr + text.offset`；函数边界用
  下一个符号地址；`lea rip+disp` 的目标落到 `__cstring/__const` 就解出字符串引用。
  脚本：`build/research-material/disasm.py`（`PYTHONPATH=pylibs python3 disasm.py <dylib> <mangled子串> [maxfn]`）。
- **DMG 解包**：`7z x` 可解（UDZO/HFS+）；**中文路径通配符匹配会失败**
  （`'*libzrpc.dylib'` 命中 0），需全量解包再 find。
- **PE（Windows dll）**：GNU objdump 可读（`objdump -p` 看导入/导出，
  `-h` 看节）；.pdata 大小÷12 ≈ 函数数；GNU 节布局（.edata/.idata/.xdata）
  提示 MinGW 工具链。
- **Go 二进制**：strings 找 `go1.x` 与 `github.com/...` 模块路径即可定性
  （ZSpaceSync = syncthing fork 就是这么认出的）。

## 3. 逆向判断纪律（踩过的坑，务必记住）

1. **别凭字符串表判加密算法。** OpenSSL 串表含*所有*算法名（aes-128-cbc、
   aes-256-gcm…都在）。本次据此误判 AES-128-CBC，看实际 `call` 才发现是
   `EVP_aes_256_gcm`。**判据是调用指令，不是字符串。**
2. **固定种子 PRNG 的「加密」是混淆，不是安全。** 看到 AES 先别判死刑，
   追密钥派生：极空间两处加密都用 **MT19937 固定种子 4728423**（boost
   mersenne_twister，624 状态字/乘子 0x6C078965/取 64 数）→ SHA-256 →
   key=hex[0:32]、iv=hex[0:16]。固定种子 = 可在 Rust 逐字节复现 = 实质透明。
3. **先读调用方，再逆被调方。** JS↔DLL 边界往往全透明：ffi 签名表、
   入参构造、密钥派生都在 background.js 里。本次 FFI 层零逆向成本。
4. **区分「读懂客户端」与「服务端认你」。** 远程 P2P 的墙不在客户端代码
   （有符号就读得懂），而在协议另一半住在闭源云+NAS 固件、服务端可校验/改协议。
5. **符号统计要 demangle 后再分类。** 拿 mangled 名匹配 `std::` 前缀会漏
   （mac libc++ 是 `std::__1::` 且处于 mangled 态），导致私有函数数虚高。

## 4. 可行性切分原则（战略）

- **按「协议定义者」切，不按「代码量」切。**
  - 协议是标准且可观测的（LAN NAS HTTP，官方网页版证明）→ 我方可自由重写 → 可行。
  - 协议是私有且对端闭源的（远程 P2P，云+NAS 固件）→ 重写语言不解决知识缺失 → 不立项。
- **「放弃协议重写」≠「放弃功能」。** 远程访问可用官方标准通道承接：
  官方 Skill CLI（Rust，Linux 原生）证明 **zconnect HTTPS 云代理 + OAuth2**
  不经过 libzrpc 就能管 NAS；Tailscale 自建组网可把远程变虚拟局域网提速。
- **官方工具是存在性证明。** 网页版证明 LAN 是标准 HTTP；Skill 证明远程有
  标准 HTTPS 通道、官方有 Rust 工程能力。先找官方有没有「绕过闭源库」的现成路。
- **同引擎替换优于重写。** 播放器官方用 libvlc（win/mac/android 皆是）→
  Linux 换同款 libvlc（可编译官方同 commit），格式兼容不赌、VA-API 硬解；
  不要逆向整个 Qt 播放器。开源依赖（boost.asio/protobuf/Simple-Web-Server/
  cxxopts/pugixml）Linux 直接链接同款，只重写私有命名空间。
- **Mach-O/PE 二进制不能直接跑 Linux**（格式/ABI/系统库不同）；「复用」指
  符号蓝图 + 开源依赖清单 + 跨平台 JS/资源，不是可运行二进制。加载原二进制
  + 翻译系统 API = 重新发明 Wine，与「原生」矛盾。

## 5. 安全与流程

- **离线优先（offline-first）。** 静态分析（反汇编官方包）能回答生死问
  （LAN 是否加密）就**不必碰 NAS**。本次 Phase 0 全程离线，NAS 零接触。
- **运行时抓包是被动只读**：用现有官方客户端做日常操作 + tcpdump 读包，
  不注入/不构造/不模糊测试。NAS 看到的与平时一致。
- **对 NAS 的硬禁区**：不写/删文件（除非逐次明确同意）、不碰系统设置/
  存储池/RAID/固件/Docker 配置、不发异常包、不利用漏洞。自研组件联调
  先只测只读接口。
- **下载阈值**：>100MB 先问用户（AGENTS.md 规则）；官方包用升级接口取
  直链 + package_size 校验完整性。
- **物料卫生**：大文件（dmg/apk/解包树）用完即删；只留小体积关键物料
  （目标 .so/.dylib、符号 dump、分析脚本）于 `build/research-material/`。
- **口径**：原生/逆向分支已豁免「不修改不破解不替换」（用户 2026-11-12），
  但**星火商店红线不随项目口径豁免**（改了官方二进制不能投稿）；外部
  EULA/版权风险仍在，个人使用为宜。master 的 Wine 链路仍守原口径。

## 6. 物料与复现

- `build/research-material/`（gitignore，约 37MB）：
  - `libzrpc-mac-x64.dylib`（符号蓝图）、`libzrpc-android-arm64.so`、
    `libplaysdk-mac-x64.dylib`
  - `libzrpc-mac-x64-symbols.txt`（32k demangled 符号）、`libzrpc-architecture.txt`
  - `macho_sym.py` / `demangle_analysis.py` / `disasm.py` / `pylibs/`(capstone)
- 复现反汇编：`cd build/research-material && PYTHONPATH=pylibs python3 disasm.py libzrpc-mac-x64.dylib "<mangled子串>" 1`
- 目标包直链与 SHA256：见报告 §3.4/§6 与升级接口（sources.md）。
- 关键事实速查：Electron 33.3.1；libzrpc 发现 = UDP 5577 + JSON
  `{"key":"tdxnassearch"}` + AES-256-GCM（MT19937 种子 4728423）；
  响应字段 id/ip/hostname；NAS API 走标准 HTTPS，约定 webagent=v2 + rnd。