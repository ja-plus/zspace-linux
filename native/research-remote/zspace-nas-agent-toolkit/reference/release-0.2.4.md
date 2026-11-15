# ZSpace NAS Agent Toolkit 0.2.4

0.2.4 是面向外部使用的客户端补丁版本，提供 macOS、Linux 与 Windows 的 x86_64/ARM64 六个平台预编译文件。它包含 91 个命名工具、Agent 驱动的交互式 OAuth 登录，以及文件、相册、媒体、下载、备份、系统、容器、虚拟机、iSCSI 和安全状态等能力。

## 本版变化

- 修正登录任务的实际路由入口 `entries/zspace-shared.md`，删除旧的“用户回终端粘贴授权码”流程。
- `SKILL.md`、登录入口、认证参考和示例命令现在统一使用 `login --no-open --auth-code-stdin`。
- Agent 从 stdout 取得授权链接并交给用户；用户将一次性授权码回复给当前 Agent，由 Agent 只写入仍在等待的 CLI stdin，不另开终端。
- 保留 0.2.3 的登录机制、安全整改、91 个命名工具和业务能力；未修改 NAS 或 OAuth 服务端。

## 安装与完整性校验

解压发布 ZIP 后，在 macOS/Linux 使用 `scripts/zspace-cli`，在 Windows 使用 `scripts/zspace-cli.ps1` 或 `scripts/zspace-cli.cmd`。`bin/SHA256SUMS` 用于核对六个二进制文件的完整性。包尚未进行代码签名或公证，SHA-256 不等同于发布者签名。

## 登录

Agent 在当前会话启动：

```bash
scripts/zspace-cli --profile <name> login --no-open --auth-code-stdin
```

CLI 在 stdout 输出一行 `oauth_authorization` JSON。Agent 将其中的 `url` 发给用户；用户在浏览器完成授权后，将页面显示的一次性授权码回复给当前 Agent。Agent 只把授权码和换行写入仍在等待的进程 stdin，不复述、不记录，也不放入命令参数、环境变量、文件或日志。登录事务在 180 秒后自动过期；同一 profile 的并发登录会被拒绝。

OAuth 服务端协议未修改。客户端验证 state 与 NAS 绑定并依赖服务端一次性授权码语义；服务端目前没有提供 PKCE 协议，本版不宣称实现 PKCE，也不在本地持久化授权码摘要。

## 平台支持边界

| 平台产物 | 当前证据 | 发布定位 |
| --- | --- | --- |
| macOS ARM64 | 本机测试、Clippy、构建和包内入口验证 | 已验证客户端运行平台 |
| macOS x86_64 | 六平台构建；Rosetta 历史进程检查 | 兼容性产物，尚无 Intel Mac 原生验收 |
| Linux x86_64 musl | 六平台构建；历史 Linux NAS 合成凭据检查 | 已有 Linux x64 运行证据，业务能力按覆盖矩阵解释 |
| Linux ARM64 musl | 六平台构建 | 预览，尚无 ARM64 原生验收 |
| Windows x86_64 | 六平台构建；rc.3 原生登录报告；rc.4 自动契约修复 | 预览，登录与写入修复尚待 Windows 原生复验 |
| Windows ARM64 | 六平台构建 | 预览，尚无 Windows ARM64 原生验收 |

六平台构建通过不等于六个平台的全部原生行为或 NAS 业务已验收。Windows 可在不连接 NAS 的情况下运行：

```powershell
py -3 scripts/windows-write-selftest.py bin/zspace-cli-windows-x64.exe
```

成功条件是退出码为 0 且 JSON 中 `passed` 为 10；该自检不替代真实 OAuth 或 NAS 业务验收。

## 已知限制

- 服务端、OAuth 协议和 NAS 固件不属于本客户端发布范围。
- Windows 原生 W1/V24/V33 写入以及 stdin、超时、无控制台和并发登录修复尚待原生复验。
- Linux ARM64 与 Intel Mac 尚未原生验收；部分特定设备、账号或硬件流程仍待验收。
- 影视审阅在不具备匹配转码能力的 NAS 上会返回 `N001999`；客户端没有绕过保护或删除历史对象。
- NAS Compose `env_save` 将 YAML 路径当目录是已知服务端限制，客户端只提供明确错误。
- CLI 无法约束 NAS 服务端最终 DNS、重定向或 BT peer。
- 依赖检查沿用 2026-10-19 RustSec 快照，保留 `rsa 0.9.10 / RUSTSEC-2023-0071` 告警；生产路径仅使用公钥加密，不执行私钥操作。这不是“零漏洞”声明。

功能覆盖见 `coverage-functions-2026-10-15.md`；登录验收修复见 `acceptance-remediation-0.2.3-rc.4.md`；安全状态见 `security-release-status-2026-10-19.md`。
