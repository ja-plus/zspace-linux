# rc.3登录机制验收确认与0.2.3-rc.4处置

依据《zspace-nas-agent-toolkit v0.2.3-rc.3 登录机制验收报告》。报告作为待核对证据；本轮仅修改客户端，不修改OAuth/NAS服务端，不处理验收账号或profile。

| 编号 | 核查结论 | rc.4处理 |
| --- | --- | --- |
| L1 | 确认。Windows `rpassword` 读取控制台而非stdin，rc.3文档中的Agent注入路径不可移植 | 新增 `login --auth-code-stdin`，跨平台只从stdin读取一行；授权码仍禁止argv、环境变量、文件与日志。Agent规范固定使用 `--no-open --auth-code-stdin` |
| L2 | 确认。rc.3授权链接仅写stderr | 登录开始时在stdout输出单行JSON事件，字段为 `event/url/state_prefix/expires_in/code_input`；提示和错误保留stderr |
| L3 | 确认。rc.3只在输入返回后检查耗时，不能终止阻塞读取 | 输入读取放入独立线程，主流程使用180秒 `recv_timeout`；到点返回E_OAUTH并退出，不兑换或保存凭据 |
| L4 | 确认。无控制台被映射为E_IO且已先输出链接 | 默认隐藏输入模式先检查交互终端；无终端时不输出链接，直接返回E_OAUTH并提示使用 `--auth-code-stdin`。stdin模式支持headless |
| L5 | 确认。rc.3仅在保存凭据时加profile锁 | login从生成state前到凭据保存完成一直持有profile独占锁；同profile第二个login在既有3秒等待后返回E_PROFILE_BUSY |
| L6 | 行为确认，非凭据回退 | 当前客户端有意不持久化授权码或摘要；服务端拒绝复用，客户端继续核对state、NAS绑定、状态和有效期。文档补充说明 |
| L7 | 观察成立，但客户端不能单方修复 | 当前OAuth服务端契约未提供PKCE挑战/校验。本轮不伪造客户端参数、不改服务端；继续保留随机state、一次性码、180秒进程期限和NAS绑定，并记录PKCE为服务端协议演进项 |

新增回归覆盖stdin消费及结构化stdout、headless提前失败、同profile并发拒绝和短时超时行为。真实账号登录、Windows控制台与原生权限仍由测试团队复验；本机测试不使用真实凭据。
