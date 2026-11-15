# OAuth 与凭证

以下描述对应当前客户端源码；已发布的旧包应以各自包内文档为准。

1. Agent 在当前会话运行 `zspace-cli --profile <name> login --no-open --auth-code-stdin`。CLI 生成随机 state，在 stdout 输出一行包含 `event/url/state_prefix/expires_in/code_input` 的 JSON，并等待 stdin。Agent 将 `url` 发给用户点击；不要求用户另开终端或自行运行 CLI。
2. 用户在浏览器完成授权，把页面给出的一次性授权码回复给当前 Agent。Agent 只把该码和换行写入仍在等待的 CLI stdin，不复述、不记录，也不放入命令参数、环境变量、文件或日志。该一次性码会经过当前对话，须按短时机密处理。
   Agent 无法保持活跃 stdin 时不能继续此流程；禁止使用临时文件、剪贴板、参数或环境变量替代 stdin。
3. CLI 通过 HTTPS 查询 `/auth/oauth2/code/status?state=...&client_id=ZspCli`，只接受本次 state 对应、未过期且未消费的码。不匹配时不会兑换或保存凭据。
4. CLI 以表单提交 client_id、auth_code、state 到 `/auth/oauth2/code/token`。本地超过 180 秒的输入事务拒绝兑换，重新执行 login 即可。
5. 成功仅输出脱敏 NAS ID、到期时间、后端类型及残留清理状态。授权与刷新请求禁止重定向，响应受流式大小限制。
6. access token 临期 60 秒自动刷新；同一 profile 锁内重读并提交配对的 access/refresh token。已注销或新登录后的旧请求不能提交刷新结果。

授权链接、state 和输入通道属于同一次登录事务。进程等待满180秒会返回 `E_OAUTH` 并退出；CLI已退出或用户拿到旧页面授权码时，Agent必须重新运行完整命令并提供新链接。相同profile同一时刻只允许一个login，第二个事务返回 `E_PROFILE_BUSY`。

CLI 在输出授权链接前预检 profile 目录的临时写入、原子发布和删除能力。受限 Agent 沙箱不允许默认目录的原子操作时，CLI 在消耗授权码前返回 `E_STORAGE_UNAVAILABLE`；Agent 只能将 `ZSPACE_CLI_HOME` 设为用户授权的、权限收紧且不会被打包/同步的私有持久目录，不得降级原子写入或 no-follow 保护。

人工直接使用CLI时可省略 `--auth-code-stdin`，CLI仅在真实交互终端中使用隐藏输入；无控制台时会在输出链接前返回具名 `E_OAUTH` 提示。授权链接事件在stdout，操作提示和错误在stderr。

客户端不持久化授权码或授权码摘要；一次性与重放拒绝由服务端状态以及客户端的state/NAS绑定共同执行。当前服务端授权协议未提供PKCE参数，客户端不能单方面启用；PKCE仍是服务端协议演进事项。

`raw POST /account/oauth2/userinfo --params '{}'` 由内部注入当前令牌。外部令牌字段、任何位置的 `@session_token` 都拒绝。授权码、OAuth token 不进入参数、环境变量或报告。

## Profile 与后端

`--profile` 隔离账号状态。名称为 1～64 位 ASCII 字母、数字、点、下划线或连字符；点目录、Windows 设备名及末尾点不允许。

`auto` 仅在保存新登录时优先系统安全存储，不可用才写私有文件。加载时以 `session-authority.json` 的唯一权威后端为准，不会因另一后端恢复而切回旧账号。旧 profile 依据已记录后端迁移；无记录且同时存在两份凭据时要求明确重新登录。

保存前先写失效标记，再写凭据并发布新会话 ID。迁移清理失败会保留待清理状态。注销先持久化失效标记，再删除凭据；清理失败返回 E_CREDENTIAL，不会声称完全删除。即使清理失败，残留副本也不会被本 CLI 自动重新加载。旧版二进制不认识此标记，不应与整改版混用。

Unix 目录 0700、文件 0600，逐级目录句柄与 no-follow 打开，拒绝异常所有者/硬链接，原子替换。Windows 使用进程 SID 和创建时私有 DACL，拒绝 reparse point；Windows 第二用户原生验收仍是发布门槛。

`logout` 是本地失效和删除，不等同于服务端撤销 refresh token。服务端强制 state 校验与安全随机数补丁已准备，尚未部署；完整补丁见源码仓库 `audit/server-patches/`，运行包不包含该目录。
