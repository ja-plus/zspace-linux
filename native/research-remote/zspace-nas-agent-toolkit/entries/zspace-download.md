---
name: zspace-download
description: Inspect and manage ZSpace download tasks and read backup, sync, mail, FTP, cloud, and rescue status.
---

# 下载与备份

```bash
scripts/zspace-cli --profile <profile> download tasks --params '{"start":0,"num":50}'
scripts/zspace-cli --profile <profile> backup status --params '{"targets":["center","sync","mail"]}'
```

新增下载需要已存在的 NAS 目录，并会让 NAS 主动访问目标 URL；确认 URL 与目录后再执行：

```bash
scripts/zspace-cli --profile <profile> download manage --params '{"action":"add","uri":"https://example.com/file.iso","dir":"/<pool>/my/data/下载"}' --allow-nas-fetch --yes
```

CLI 只接受 HTTP、HTTPS、FTP 和 magnet；FTP 是 NAS 下载器与兼容连接器明确支持的协议。HTTP、HTTPS
和 FTP URL 会先规范化主机并拒绝 userinfo、回环、链路本地、IPv4-mapped IPv6 与云元数据目标。
暂停、继续和删除使用 `ids` 数组；删除任务前说明不会自动删除还是会删除已下载文件的固件差异，
必要时在客户端复核。

## 文档同步（阶段性功能验收版）

`backup sync --params '{"action":"folders"}'` 查看 NAS Syncthing 任务；写操作需要 --yes。完整 schema、仅 NAS 端创建与跨设备范围、文件保留语义见 `reference/function-expansion.md`。不要把 NAS 端任务登记成功当作电脑端已经完成配对。

## 下载器管理扩展（阶段性功能验收版）

新增 download control、settings、torrent；基线 tasks/manage 保持兼容。具体参数见扩展参考；BT 预解析是写缓存，需要 --yes，单任务删除默认保留文件。后台忙碌返回 E_DOWNLOAD_BUSY，先查询 tasks 再决定后续动作。全局批量动作、设置及引擎切换不得作为普通功能验收自动执行。

`backup center` 支持任务查询、独立本地备份创建/规则/计划及运行控制；默认手动计划，不自动发起备份。任务删除保留输出文件。`backup cloud` 按供应商调用客户端已核对的网盘接口；不可用或未验证的操作不会用通用 raw 冒充完成。

下载器联网写操作另需每次明确传 `--allow-nas-fetch`，并保留原有 `--yes`；默认拒绝。停止、删除与只读查询不受此额外开关限制。CLI 无法约束 NAS 最终 DNS、重定向及 BT peer，详见 `reference/release-0.2.3-rc.2.md`。

## --allow-nas-fetch 动作映射

下表只约束 `/downloader/` 命名工具；开启标志仍不代表 NAS 最终连接受限。

| 工具 | action | --yes | --allow-nas-fetch |
| --- | --- | --- | --- |
| download tasks | 全部查询 | 否 | 否 |
| download manage | add / start | 是 | 是 |
| download manage | stop / delete | 是 | 否 |
| download control | protocols / details / directories / engine_status | 否 | 否 |
| download control | stop / delete / pin / unpin / stop_all / stop_seeding_all / delete_all | 是 | 否 |
| download control | start / verify / start_all / start_seeding_all / engine_switch / engine_confirm / storage_move / storage_confirm | 是 | 是 |
| download settings | get / basic | 否 | 否 |
| download settings | set | 是 | 是 |
| download torrent | generic / trackers / peers / files | 否 | 否 |
| download torrent | tracker_delete / cancel_parse | 是 | 否 |
| download torrent | generic_set / file_set / tracker_add / parse_nas / parse_upload / effect / add_nas / add_upload | 是 | 是 |

`control details` **不接受业务参数**：`{"action":"details"}` 返回 download_queue_size/loading_queue_size，不是分页任务列表。task_type/start/size 属于 `directories`；任务分页用 `download tasks`。

此开关不扩散到 file upload、backup sync 或 backup cloud；这些工具仍按自己的 action 确认写入。
