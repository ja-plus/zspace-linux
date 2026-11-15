---
name: zspace-shared
description: Authenticate, select profiles, diagnose connectivity, and understand output or credential storage for zspace-cli.
---

# 登录与连接

1. Agent 在当前会话运行 `zspace-cli --profile <name> login --no-open --auth-code-stdin`，保持进程等待 stdin；不要让用户另开终端或自行运行 CLI。
2. CLI 在 stdout 输出一行 `oauth_authorization` JSON。Agent 只把其中的 `url` 发给用户点击；用户在浏览器完成极空间账号授权后，将页面显示的一次性授权码回复给当前 Agent。
3. Agent 只把授权码和换行写入仍在等待的 CLI stdin，不复述、不记录，也不放入命令参数、环境变量、文件或日志。进程在 180 秒后退出；超时或进程已退出时重新发起完整登录，不向旧事务提交或复用授权码。
   如果当前 Agent 无法保持 stdin 会话，立即停止；不得改用临时文件、剪贴板、环境变量或命令参数中转。
4. 登录成功后执行 `status`，再执行 `doctor`。`doctor` 会验证凭证、选路和 `/account/oauth2/userinfo`。同一 profile 的并发登录会返回 `E_PROFILE_BUSY`。

```bash
scripts/zspace-cli --profile <profile> login --no-open --auth-code-stdin
scripts/zspace-cli --profile <profile> status
scripts/zspace-cli --profile <profile> doctor
```

全局连接参数：

- `--mode auto|lan|cloud`：保存到 profile；`auto/cloud` 默认可信 HTTPS 云代理；`lan` 需显式可信 HTTPS host。
- `--host <URL>`：保存固定内网地址；只允许 loopback、私网和链路本地 IP。
- `--allow-insecure-http`：仅本次调用兼容 HTTP；必须同时显式传 `--host`，不持久化许可，存在链路令牌泄露风险。
- `--clear-host`：清除当前 profile 保存的 `--host`，恢复按 `--mode` 选路。
- `--credential-store auto|keyring|file`：默认 `auto`。不要把该设置误当作凭证值。
- `--timeout <seconds>`：2~300 秒。
- `--local-root <path>`：限制上传和下载可访问的本地目录；相对 `local_path` 以它为基准。

详见 `reference/authentication.md` 与 `reference/connection.md`。
