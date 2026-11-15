---
name: zspace-snapshot
description: Inspect and manage ZSpace storage pool snapshots, schedules, and retention rules.
---

# 存储池快照（阶段性功能验收版）

先读 zspace-shared.md。命令 `file snapshots --params '{"action":"..."}'`。
所有写入需用户授权后显式 `--yes`；restore 为整个存储池恢复，不能按单文件理解。
先使用 tasks 查询真实 pid（非负整数），list 查询该池快照名称，不推断名称或池 ID。

| action | 参数 | 行为 |
| --- | --- | --- |
| tasks | 可选 pids 整数数组，默认全部 | 只读计划及初始化状态，编码为 pid[] |
| list | pid | 只读该池快照列表、可恢复/固定标志、浏览路径 |
| create | pid；可选 fixed:false、annotation:"" | 创建手动快照；备注最多 49 个 Unicode 字符 |
| edit | pid、name；fixed/annotation 至少一个 | 修改固定标志和备注，空备注显式清除 |
| delete | pid、name；可选 force:false | 删除快照，高风险；固定快照需要 force:true |
| on/off | pid | 开启/关闭该池快照计划 |
| plan | pid、plan、shour/smin、ehour/emin、interval | 更新计划并自动启用；小时 0～23、分钟 0～59、间隔 1～12 小时 |
| rule | pid、rule；num 或 day | 更新保留规则，会影响自动清理，高风险 |
| init | all:true，可选 encrypted_key_env | 对所有池登记初始化，高风险；需重启生效 |
| restore | pid、name、confirm_pool_restore:true；可选 encrypted_key_env | 预检列表中的 restorable:1，再登记整池恢复；需重启生效 |

plan:0 每日不传日期，1 每周必须传 weekday:1～7，2 每月必须传 monthday:1～31，
不混传日期。可选 skip:false（是否跳过休眠）、notify:false（成功通知）。
rule:0 按数量，必填 num:1～256；rule:1 按天数，必填 day:1～366；二选一。
rule、delete、init、restore 按 destructive 处理，其余写入按 write 处理。

encrypted_key_env 指向含 NAS RSA Base64 密文的环境变量；用于有加密池时的设备密钥校验。
也可使用 storage_key_env 引用存储池业务密钥，CLI 从 NAS 获取公钥并加密后发送；
两种字段二选一，不能同时提供。业务密钥不放入命令参数，亦不读取 NAS 私钥。
加密请求协议有本地测试；加密池初始化与恢复仍待专用环境真机验收。
init/restore 成功返回 submitted:true、completed:false、reboot_required:true；CLI 不自动重启。
HTTP 成功仅表示已登记待重启动作，不能报告为快照恢复完成。
快照 list 中的 path.my/public 等可交给现有 file list/read/download 进行只读浏览与取回；
访问权限仍由 NAS 验证，保险箱路径还受解锁条件约束。

协议为 POST form `/zspool/snapshot/*`，网关保留路径并注入 Auth-Response；写 handler 校验主账号，
除 init 外的写路由还要求快照初始化完成。证据：Web fileSnap.js、fileSnapSettingDlg、snapSaveRuleDlg、
initDlg、restoreSnapDlg；后端 zspoolv3/internal/api/router.go、handlers/snapshot*.go 和 logic/snapshot。
创建可耗时，按设备情况显式加大 --timeout；超时后先查询 list，按备注/名称核对，不能直接重试创建。
当前任务状态和列表已真机通过；计划、保留规则、初始化、整池恢复未执行真机写入验收。

独立快照的创建、备注/固定修改、拒绝未 force 的固定快照删除、force 删除及列表无残留已在真机通过。
没有修改现有计划或保留规则，也没有执行 init/restore。
