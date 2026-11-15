# ZSpace NAS Agent Toolkit 0.2.5

0.2.5 是面向外部使用的客户端文档一致性补丁版本，提供 macOS、Linux 与 Windows 的 x86_64/ARM64 六个平台预编译文件，并保留 91 个命名工具与 0.2.4 的客户端行为。

## 本版变化

- README 不再把当前登录描述为用户操作的“隐藏输入”，明确登录进程由 Agent 在当前会话托管。
- README 与 `SKILL.md` 明确授权事件结构：`oauth_authorization` 是顶层 `event` 字段的值，授权链接来自同一行 JSON 的顶层 `url` 字段，不存在 `oauth_authorization.url` 对象路径。
- `release-0.2.3-rc.3.md` 增加醒目的历史废止提示，并移除可被误执行的旧登录命令；当前登录只以 `SKILL.md`、`entries/zspace-shared.md` 和 `reference/authentication.md` 为准。
- 未修改 NAS 或 OAuth 服务端，未改变授权码、token 或凭据存储规则。

## 当前登录流程

Agent 在当前会话启动并保持进程：

```bash
scripts/zspace-cli --profile <name> login --no-open --auth-code-stdin
```

CLI 在 stdout 输出一行类似以下结构的 JSON：

```json
{"code_input":"stdin","event":"oauth_authorization","expires_in":180,"state_prefix":"...","url":"https://oauth.zconnect.cn/wbui/authorize?..."}
```

Agent 先确认顶层 `event` 等于 `oauth_authorization`，再读取同一对象的顶层 `url` 字段并发给用户。用户在浏览器完成授权后，将页面显示的一次性授权码回复给当前 Agent；Agent 只把授权码和换行写入仍在等待的 CLI stdin，不复述、不记录，也不放入命令参数、环境变量、文件或日志。登录事务在 180 秒后自动过期；同一 profile 的并发登录会被拒绝。

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

六平台构建通过不等于六个平台的全部原生行为或 NAS 业务已验收。Windows 写入自检命令为：

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
