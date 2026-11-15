# ZSpace NAS Agent Toolkit 0.3.4

0.3.4 针对 Windows 11 / PowerShell 5.1 / CMD 的 0.3.1 thin 验收报告进行启动器修复。CLI 业务能力、Agent 托管登录协议及 NAS/OAuth 服务端均未改变。

## 已确认并修复

- Windows PowerShell 启动器此前未将 stdin 转发给子 CLI，导致 `login --no-open --auth-code-stdin` 无法收到 Agent 写入的一次性授权码。现在仅在参数明确包含 `--auth-code-stdin` 时，通过原始标准输入流异步转发给 CLI；授权码不进入参数、环境变量、文件或日志。
- PowerShell/CMD 包装层此前可能在 CLI 已结束后继续等待上游 stdin EOF。现在普通命令不读取 stdin，并用子进程退出码直接结束包装进程。
- Windows 原生运行时自检增加两个无 NAS/OAuth、无真实凭据的本地探针：stdin 保持打开时验证授权输入转发，以及普通命令无需 EOF 即退出。
- Unix 启动器现在把制品清单中的 `bytes=0` 归类为 `E_RUNTIME_CACHE`，与 PowerShell 启动器一致。

## 报告中无需重复修复的项目

- 0.3.1 报告中的 Git Bash 冷启动耗时属于 shell 与进程启动开销观察，不影响完整性或功能正确性；本版未用未经原生回归的新 loader 替换稳定实现。
- 0.3.1 的 CDN 未发布状态是真实历史状态。0.3.4 只有在六个平台制品全部上传并逐项 HTTPS 回读核对字节数与 SHA-256 后，才生成 `published:true` 清单和最终包，验证记录不再把待发布 URL 写成已发布事实。

## 验收边界

本地静态检查、macOS ARM64 运行时下载验证和六平台交叉构建不能替代 Windows 原生验证。请在 Windows PowerShell 5.1 与 CMD 分别运行 `scripts/windows-runtime-selftest.ps1`，再执行真实 Agent 托管登录。Windows 原生复验完成前，此修复仍标记为“待原生验收”。Intel Mac、Linux ARM64、四台 NAS 的 0.3.3 业务修复以及 WorkBuddy macOS 沙箱边界也保持待验收。
