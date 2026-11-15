# ZSpace NAS Agent Toolkit 0.2.3

0.2.3 是面向外部使用的客户端版本，提供 macOS、Linux 与 Windows 的 x86_64/ARM64 六个平台预编译文件。它包含 91 个命名工具、Agent 驱动的交互式 OAuth 登录、默认安全的远程访问策略，以及文件、相册、媒体、下载、备份、系统、容器、虚拟机、iSCSI 和安全状态等能力。

## 安装与完整性校验

解压发布 ZIP 后，在 macOS/Linux 使用 `scripts/zspace-cli`，在 Windows 使用 `scripts/zspace-cli.ps1` 或 `scripts/zspace-cli.cmd`。`bin/SHA256SUMS` 用于核对六个二进制文件的完整性；SHA-256 不等同于发布者签名。本版尚未进行代码签名或公证。

## 登录

Agent 在当前会话启动：

```bash
scripts/zspace-cli --profile <name> login --no-open --auth-code-stdin
```

CLI 在 stdout 输出一行 `oauth_authorization` JSON。Agent 将其中的授权链接发给用户；用户在浏览器完成授权后，将页面显示的一次性授权码回复给当前 Agent。Agent 只把授权码写入仍在等待的登录进程 stdin，不放入命令参数、环境变量、文件或日志，也不另开终端。登录事务在 180 秒后自动过期；同一 profile 的并发登录会被拒绝。

OAuth 服务端协议未修改。客户端验证 state 与 NAS 绑定并依赖服务端一次性授权码语义；服务端目前没有提供 PKCE 协议，本版不宣称实现 PKCE，也不在本地持久化授权码摘要。

## 主要变化

- 修复 Agent 登录在 Windows 上忽略管道 stdin、授权链接仅写 stderr、180 秒后进程不退出、无控制台时报 `E_IO` 以及同 profile 并发登录的问题。
- 保留 rc.2 的 Windows 私有文件写入与覆盖/续传修复、安全存储、profile 锁、HTTPS 默认路径、显式 HTTP 降级、响应大小限制、路径约束与机密脱敏。
- NAS 下载器联网写操作默认拒绝，必须同时取得用户授权并临时传入 `--allow-nas-fetch`；该开关不持久化。
- 保留稳定 JSON 输出、91 个命名工具与包内功能、参数和验收参考文档。

## 平台支持边界

| 平台产物 | 当前证据 | 发布定位 |
| --- | --- | --- |
| macOS ARM64 | 本机测试、Clippy、构建、包内入口与登录 smoke | 已验证客户端运行平台 |
| macOS x86_64 | 六平台构建；Rosetta 历史进程检查 | 兼容性产物，尚无 Intel Mac 原生验收 |
| Linux x86_64 musl | 六平台构建；历史 Linux NAS 合成凭据检查 | 已有 Linux x64 运行证据，业务能力仍按覆盖矩阵解释 |
| Linux ARM64 musl | 六平台构建 | 预览，尚无 ARM64 原生验收 |
| Windows x86_64 | 六平台构建；rc.3 原生登录报告；rc.4 自动契约修复 | 预览，rc.4 登录与写入修复尚待 Windows 原生复验 |
| Windows ARM64 | 六平台构建 | 预览，尚无 Windows ARM64 原生验收 |

“六平台构建通过”只说明目标格式、架构与版本一致，不等于六个平台的全部原生行为或 NAS 业务已验收。Windows 可在不连接 NAS 的情况下运行：

```powershell
py -3 scripts/windows-write-selftest.py bin/zspace-cli-windows-x64.exe
```

成功条件是进程退出码为 0 且 JSON 中 `passed` 为 10。该自检覆盖本地 profile、下载落盘、覆盖/续传和并发写入，但不替代真实 OAuth 或 NAS 业务验收。

## 已知限制

- 服务端、OAuth 协议和 NAS 固件不属于本客户端发布范围；本版没有修改或部署服务端。
- Windows 原生 W1/V24/V33 写入场景以及 rc.4 的 stdin、超时、无控制台和并发登录修复尚待原生复验。
- Linux ARM64 与 Intel Mac 尚未原生验收；部分需要特定设备、账号或硬件的业务流程仍待验收。
- 影视审阅在不具备匹配转码能力的 NAS 上会返回 `N001999`；客户端没有绕过保护或删除历史对象。
- NAS Compose `env_save` 将 YAML 路径当目录是已知服务端限制，客户端只提供明确错误。
- CLI 能要求每次显式允许 NAS 下载器联网写入，但不能约束 NAS 服务端最终 DNS、重定向或 BT peer。
- 依赖检查沿用 2026-10-19 RustSec 快照，存在 `rsa 0.9.10 / RUSTSEC-2023-0071` 告警。生产路径仅使用公钥加密，不执行私钥操作；这不是“零漏洞”声明。

功能覆盖与真机证据见 `coverage-functions-2026-10-15.md`；登录验收修复见 `acceptance-remediation-0.2.3-rc.4.md`；安全状态见 `security-release-status-2026-10-19.md`。
