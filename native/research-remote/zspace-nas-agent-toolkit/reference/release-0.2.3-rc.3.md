# 客户端 0.2.3-rc.3 验收候选版

本版只修正 Agent 登录交互约定及对应 CLI 提示，不修改 OAuth 协议、服务端或 NAS 业务接口。

> 历史记录，不得作为当前登录操作说明。rc.3 的隐藏输入流程已被后续版本废止；当前版本必须按 `SKILL.md` 与 `reference/authentication.md` 使用 Agent 托管 stdin 流程。

## 登录流程

rc.3 曾尝试让 Agent 驱动 CLI 的终端隐藏输入，但 Windows `rpassword` 会直接读取控制台，不能可靠接收 Agent 写入的 stdin，因此该候选流程已废止。本段只记录版本沿革，不提供可执行命令；当前版本必须使用带 `--auth-code-stdin` 的命令，并从 `event` 为 `oauth_authorization` 的 stdout JSON 读取顶层 `url` 字段。

随机 state、state/NAS 绑定、一次性码校验、180 秒有效期、HTTPS、禁止重定向、凭据安全存储和脱敏要求继续有效。

## 继承的验收边界

0.2.3-rc.2 的客户端安全修复、业务能力和限制全部保留。Windows 原生 W1/V24/V33、ACL、reparse、原子替换与失败清理仍待测试团队验收；Linux ARM64和Intel Mac原生仍待验收。审阅能力须在支持对应转码组件的机型复验，Compose `env_save` 仍为服务端限制。

本版仅交付客户端本地包，不部署服务端、不处理历史业务对象。包无代码签名或公证；SHA-256只核对完整性。详细既有处置见 `release-0.2.3-rc.2.md` 与 `acceptance-remediation-0.2.3-rc.2.md`。
