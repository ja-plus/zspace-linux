# ZSpace NAS Agent Toolkit 0.3.2

0.3.2 针对 macOS Agent 沙箱登录验收的持久化问题进行修复，不修改 NAS/OAuth 服务端协议和业务工具契约。

## 登录持久化修复

- `login` 在生成 OAuth state 和输出授权 URL 前，先在当前 profile 目录执行“创建唯一临时文件→写入→同目录原子发布→删除”预检。
- 预检失败时返回 `E_STORAGE_UNAVAILABLE`，错误包含操作、目标路径、OS 原因和 `ZSPACE_CLI_HOME` 处置建议；不再让用户先完成网页授权后才发现本地不可写。
- 凭据和配置原子写入的创建、写入和发布失败也统一保留上下文，不再只返回笼统 `E_IO`。
- 不使用绝对路径 rename 作为降级；保留目录句柄、no-follow、同目录原子发布和 0600/0700 权限边界。

Agent 沙箱不允许默认配置目录原子写入时，应为 `ZSPACE_CLI_HOME` 选择用户授权的私有持久目录。该目录不得位于 Skill 包目录、不得被打包或同步，也不得作为临时目录清理。

## 登出语义

`logout` 首先发布 `logged_out` authority，使任何旧文件或 Keychain 凭据都不会再被加载。系统安全存储不可访问时，登出返回 `logged_out:true` 及 `credential_cleanup_pending:true`，不再硬失败；后续 logout 可重试清理。本地 authority 或文件凭据无法安全失效时仍会失败。

## 授权码通道

授权码仍只能写入 Agent 当前托管的 `login --no-open --auth-code-stdin` 进程 stdin。Agent 无法保持活跃 stdin 时必须停止，不得改用临时文件、剪贴板、命令参数或环境变量。

## 验收边界

- 0.3.2 需在原 WorkBuddy macOS 沙箱复验：默认目录应在输出链接前返回具名错误；授权的私有 `ZSPACE_CLI_HOME` 应完成登录。
- Windows PowerShell 5.1 loader 修复仍待 Windows 原生复验；Linux ARM64 和 Intel Mac 仍待原生验收。
- SHA-256 是完整性校验，不等同代码签名；当前制品尚未签名或公证。
