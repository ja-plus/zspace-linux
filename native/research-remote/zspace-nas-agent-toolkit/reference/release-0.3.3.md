# ZSpace NAS Agent Toolkit 0.3.3

0.3.3 根据四台 NAS 的旧版全量回归报告，修复当前客户端仍可确认的问题。未修改 NAS/OAuth 服务端，不把交叉构建视为设备原生验收。

## 已修复

- 文件下载兼容 NAS 返回 HTTP 200 且携带精确 `Content-Range` 的分块响应；仍严格校验块长度、范围、总长度和强 ETag。
- 回收站列表改用 `/v2/file/nb/list`，默认限定 `/.recycle/my`；局部恢复只接受列表返回的池级 `/.recycle/my/` 路径，错误路径返回 `E_PATH`。
- `live_draft_patch` 可接收并丢弃 `live_draft.list` 中已知只读字段，仍拒绝任意未知字段；`delete_ids` 缺省为 `[]`。
- Compose 项目动作先读取项目列表并验证 `project_id`，不存在时返回 `E_NOT_FOUND`，不再把空日志当成功。
- BT `add_upload` 等接口对成功的非 JSON 2xx 响应返回结构化 accepted 结果。
- `file mkdir` 默认使用 rename:0，不再静默创建 `-N` 副本；部分固件对已有同名目录返回既有目录信息（幂等成功）。明确需要自动改名时才传 `rename:1`。
- 命令增加 `--params-file`，从 `--local-root` 内读取最大 16 MiB 的 UTF-8 JSON，避免大文本受操作系统命令行长度限制。授权码和 token 禁止使用该通道。
- 当前使用文档中的固定 `--profile home` 示例改为显式 `<profile>` 占位。

## 已确认但未改为缺陷

- 下载 `offset` 当前是“完整重传并核对前缀”，响应与文档明确标注实际传输量；它不是节省带宽的续传。本版不伪装为增量传输。
- SMART 的 SN 来源、应用服务间歇 403、OAuth 过期、设备未安装应用及受限账号回收站权限属于测试前置或设备状态，不在客户端中掩盖。
- cloud 选路可观测性属于诊断增强建议，不影响当前连接契约。

## 验收边界

上述业务修复已进行本地协议与回归测试，但仍需在报告涉及的 Q4-9Y70、Z4Pro+-9JYG、Z4Pro-OKNG、Z425-BCRV 上进行业务复测。Windows、Intel Mac、Linux ARM64 以及 WorkBuddy macOS 沙箱登录边界沿用 0.3.2，未原生复测的平台不得标记为通过。
