# ZSpace NAS Agent Toolkit 0.3.0-rc.1

本版新增两种客户端包：小于2.5MiB的 thin 在线包，以及包含六平台二进制的 offline 完整包。CLI业务行为、91个命名工具和 Agent 托管 OAuth stdin 流程继承自0.2.5；NAS/OAuth服务端未修改。

## 在线运行时

thin 包首次执行 `scripts/zspace-cli` 时按当前 OS/CPU 选择 `runtime-artifacts.json` 中的固定版本制品。启动器依次检查包内文件、版本隔离缓存和 CDN；任何文件都必须同时匹配固定字节数与 SHA-256 后才会执行。下载写入唯一临时文件，成功校验后原子安装；stdout 保留给 CLI JSON，下载诊断写 stderr。

缓存位置：

- macOS：`~/Library/Caches/zspace-nas-agent-toolkit/<version>/<platform>/`
- Linux：`${XDG_CACHE_HOME:-~/.cache}/zspace-nas-agent-toolkit/<version>/<platform>/`
- Windows：`%LOCALAPPDATA%\ZSpace\AgentToolkit\cache\<version>\<platform>\`

Skill升级使用新版本清单和新缓存目录，不读取可变 `latest`。删除某一版本缓存后，下次 thin 启动会重新下载并校验。离线或受限网络环境使用 offline 包。

## 当前发布状态

2026-10-22 已经用户明确授权调用 `uploadBaishan` 发布六个平台的不可变版本化制品。发布脚本已将六个 CDN 对象逐一回下载，并与本地制品核对字节数和 SHA-256；`runtime-artifacts.json` 现包含固定 HTTPS URL 且 `published:true`。thin 包已具备联网交付条件，offline 包仍供离线环境使用。

## 登录

```bash
scripts/zspace-cli --profile <name> login --no-open --auth-code-stdin
```

Agent从stdout中找到顶层 `event` 等于 `oauth_authorization` 的JSON，读取其顶层 `url` 字段。用户完成网页授权后将一次性码回复给Agent；Agent只写入仍在等待的CLI stdin，不放入参数、环境、文件或日志。180秒超时后重新发起，不复用旧码。

## 安全与平台边界

- 生产清单只接受固定 `https://update.zenithspace.net/` URL；测试HTTP仅在显式fixture开关与loopback主机同时满足时允许。
- SHA-256是完整性校验，不等同发布者签名；包和二进制尚未签名或公证。
- Windows原生写入及本版PowerShell下载器仍待Windows原生验收；Linux ARM64、Intel Mac仍待原生验收。
- Windows可运行 `scripts/windows-write-selftest.py` 和 `scripts/windows-runtime-selftest.ps1`；交叉构建不算原生通过。
- 服务端、PKCE、NAS Compose限制、审阅机型能力与下载器服务端DNS/BT peer边界均未改变。
- 依赖检查沿用2026-10-19 RustSec快照，保留 `rsa 0.9.10 / RUSTSEC-2023-0071` 告警；生产仅使用公钥加密路径。

CDN发布脚本需要显式 `ZSPACE_CONFIRM_CDN_PUBLISH=YES`，且只能在用户明确授权上传后运行。它会逐个平台上传、回下载、校验字节数和SHA-256，六项全部成功后才写入已发布清单。
