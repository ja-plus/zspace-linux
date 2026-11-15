# ZSpace NAS Agent Toolkit 0.3.5

0.3.5 逐项复核 T2S-UCP5（ARM64/RK3588）和 Z2Pro-V89F（ARM64/A53）的旧版全量回归报告，修复当前版本仍存在的输入 schema 与跨存储池文档问题。未修改 NAS/OAuth 服务端。

## 当前仍存在并已修复

- `system_info`、`disk_info`、`device_info`、`service_status`、`media_list`、`media_search`、`media_detail`、`backup_status`、`security_status` 九个旧式只读工具现在统一校验顶层字段。未知 `action` 或任意未声明字段均在联网前返回 `E_PARAM`，无论是否附带 `--yes`。
- 当前入口文档不再把 `/sata1/my/data` 写成可直接照抄的路径，统一改为 `/<pool>/my/data`，要求先从 `file space` 读取当前设备的真实个人空间路径；适用于 `sata11`、`nvme11` 等单盘 ARM 机型。

## 旧报告问题的当前结论

- 已由 0.3.3 修复并保留回归测试：超过 8 MiB 的分块下载、回收站列表端点与池级恢复路径提示、直播 draft/patch 只读字段、Compose 项目存在性、`file write --params-file`、mkdir 默认冲突失败、BT 成功非 JSON 响应。
- `file duplicates group_files/clean` 当前已接受 `group_id`、`scan_id`、非空 `file_ids`，并在删除前核对当前扫描 ID、完成状态和完整结果集合；旧报告中的不可达问题不再存在。
- 搜索新建文件的索引延迟、设备返回的 CPU 型号、应用未安装以及媒体 AI 502 属于设备或服务状态，不在客户端伪造成成功。
- cloud 模式的外部可观测性和更丰富的未知 action 提示属于增强建议，不作为本版阻断缺陷。

## 验收边界

本版通过本地协议测试和六平台构建只能证明客户端逻辑与制品完整性。T2S-UCP5、Z2Pro-V89F 仍需使用 0.3.5 对上述闭环做原生业务复测；不得用旧版报告或交叉构建替代。Windows PowerShell 5.1/CMD 的 0.3.4 stdin 修复、Intel Mac、Linux ARM64 与 WorkBuddy macOS 沙箱同样保持原生待验收。
