# ZSpace NAS Agent Toolkit 0.3.1

0.3.1 修复 0.3.0 thin 包在 Windows PowerShell 5.1 和 Git Bash 下的启动器兼容问题。CLI 业务工具、Agent 托管 `login --no-open --auth-code-stdin` 流程及 NAS/OAuth 服务端均未改变。

## 修复项

- PowerShell 启动器以 UTF-8 BOM 发布，运行时显式设置 UTF-8 控制台输入输出，避免 Windows PowerShell 5.1/GBK 解析和中文输出错乱。
- PowerShell 启动器显式加载 `System.Net.Http`；缺失时返回 `E_RUNTIME_DEPENDENCY`，下载异常包含可诊断的异常消息。
- PowerShell 使用 Windows 命令行引号规则构造子进程参数，保留 `--params` JSON 内层双引号，并透传 stdin/stdout/stderr 和退出码。
- POSIX 启动器支持 `MINGW*` / `MSYS*` / `CYGWIN*`，Git Bash 可直接选择 Windows x64/ARM64 制品。
- 两份启动器都将生产 URL 限制为 `https://update.zenithspace.net/`；回环 HTTP 仍只能在显式 fixture 开关下使用。
- POSIX 启动器可解析紧凑或缩进的制品清单，不再依赖 platform 字段所在行的固定格式。
- Windows 运行时自检可自动定位/下载当前平台 CLI，并支持 `py`、`python` 或 `python3`。
- 文档明确区分 thin 和 offline 的 `bin/` 边界，发布清单分别记录两种包的二进制路径。

## 启动与登录

- macOS/Linux/Windows Git Bash：`scripts/zspace-cli ...`
- Windows PowerShell：`scripts/zspace-cli.ps1 ...`
- Windows CMD：`scripts/zspace-cli.cmd ...`

PowerShell 例子：

```powershell
scripts\zspace-cli.ps1 --profile home file list --params '{"path":"/sata1/my/data","num":1}'
```

JSON 在 PowerShell 层使用单引号包围，不要手工改为 `'{\"path\":...}'`；启动器会负责原生进程引号编码。登录仍由 Agent 启动 `login --no-open --auth-code-stdin`，从顶层 `event/url` 字段取授权链接，用户回复一次性码后只写入当前 CLI stdin。

## 完整性和边界

thin 包不含 `bin/`，首次运行从包内固定 URL 下载当前平台 CLI；offline 包携带六平台二进制。包内文件或缓存在每次执行前都校验字节数和 SHA-256。SHA-256 不等同于代码签名；当前产物尚未签名或公证。

当前 `0.3.1` 已完成本地六平台构建和 thin/offline 包验证，但新版本 CDN 上传未获得单独的外部发布授权，因此 `runtime-artifacts.json` 保持 `published:false` 且 URL 为空。offline 包可供本地验收；thin 包在发布 CDN 并重新打包前会以 `E_RUNTIME_DOWNLOAD` 安全失败，不应对外交付。

Windows 0.3.0 验收已证明 CLI 业务回归、原生写入自检和 CDN 制品本身通过；0.3.1 启动器修复仍需在 Windows PowerShell 5.1 原生环境复验，不以 macOS 静态检查或交叉构建代替。Linux ARM64 和 Intel Mac 仍待原生验收。
