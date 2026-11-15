# ZSpace NAS Agent Toolkit 0.3.0

0.3.0 是可对外交付的客户端正式版，同时提供小于 2.5 MiB 的 thin 在线包和包含六平台二进制的 offline 完整包。CLI 业务行为、91 个命名工具和 Agent 托管 OAuth stdin 流程保持不变；NAS/OAuth 服务端未修改。

## 在线运行时

thin 包首次执行 `scripts/zspace-cli` 时，按当前 OS/CPU 从包内 `runtime-artifacts.json` 选择固定版本制品。启动器依次检查包内文件、版本隔离缓存和 CDN；只有字节数与 SHA-256 同时匹配才执行。下载写入唯一临时文件，校验后原子安装；下载诊断仅写 stderr，不污染 CLI JSON stdout。

缓存位置：

- macOS：`~/Library/Caches/zspace-nas-agent-toolkit/<version>/<platform>/`
- Linux：`${XDG_CACHE_HOME:-~/.cache}/zspace-nas-agent-toolkit/<version>/<platform>/`
- Windows：`%LOCALAPPDATA%\ZSpace\AgentToolkit\cache\<version>\<platform>\`

Skill 升级使用新版本清单和新缓存目录，不跟随可变 `latest`。删除单个版本缓存后，下次 thin 启动会重新下载和校验。离线或受限网络环境使用 offline 包。

## 发布完整性

0.3.0 的六个平台制品使用包含版本与平台的不变文件名发布。发布流程会将六个 CDN 对象逐一回下载，与本地产物核对字节数和 SHA-256，全部通过后才生成 `published:true` 的固定 URL 清单。thin/offline ZIP 分别生成 SHA-256 与独立包验证记录。

## 登录

```bash
scripts/zspace-cli --profile <name> login --no-open --auth-code-stdin
```

Agent 从 stdout 中找到顶层 `event` 等于 `oauth_authorization` 的 JSON，读取其顶层 `url` 字段。用户完成网页授权后将一次性码回复给 Agent；Agent 只写入仍在等待的 CLI stdin，不放入参数、环境、文件或日志。180 秒超时后重新发起，不复用旧码。

## 安全与验收边界

- 生产清单只接受固定 `https://update.zenithspace.net/` URL；测试 HTTP 仅在显式 fixture 开关与 loopback 主机同时满足时允许。
- SHA-256 提供完整性校验，不等同于发布者代码签名；包与二进制尚未签名或公证。
- macOS ARM64 已完成本地 thin/offline 与真实 CDN 运行验证。
- Windows 原生写入和 PowerShell 下载器、Linux ARM64、Intel Mac 仍待对应原生环境验收；六平台交叉构建不等于原生验收。
- Windows 可运行 `scripts/windows-write-selftest.py` 和 `scripts/windows-runtime-selftest.ps1`。
- 服务端、PKCE、NAS Compose 限制、审阅机型能力与下载器服务端 DNS/BT peer 边界未改变。
- 依赖审计沿用 2026-10-19 RustSec 快照，保留 `rsa 0.9.10 / RUSTSEC-2023-0071` 告警；生产仅使用公钥加密路径。
