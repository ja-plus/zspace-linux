---
name: zspace-ops
description: Read ZSpace virtual machine, iSCSI, firewall, interface, and security status without changing configuration.
---

# 运维只读状态

```bash
scripts/zspace-cli --profile <profile> ops vm --params '{}'
scripts/zspace-cli --profile <profile> ops iscsi --params '{"include":["targets","sessions","luns"]}'
scripts/zspace-cli --profile <profile> ops security --params '{"include":["firewall","interfaces","services"]}'
```

`ops vm` 的 `vm_id` 应填写 VM 列表返回的 **name（VM 名称，如 zspace_xxx）**，后端按 name 查询，不是数字 ID。传该名称可查单台详情，但不提供开关机。iSCSI 与安全命令都只读，不创建 Target/LUN、
不修改防火墙、不改网口、Bond、Bridge、IPv6 或路由。iSCSI 的 `include` 可为 `info`、`storage`、
`targets`、`sessions`、`luns`、`groups`；CLI 会通过 OAuth userinfo 自动补齐 Target/LUN/组接口要求的
主账号 Cookie。若 NAS 仍拒绝访问，返回 `E_PERMISSION` 并提示重新授权或核对主账号权限。

空态提示只描述本次实际请求的集合；例如只请求 `sessions` 时，不会据此断言 Target 或 LUN 为空。

`ops vm-manage` 扩展虚拟机创建、配置补丁、磁盘、快照、运行控制、克隆/迁移/导出与VNC，完整参数见 [功能扩展](../reference/function-expansion.md)。删除永久移除磁盘；最后一台还会触发后端全池VM目录清理，需要confirm_last_vm_cleanup:true。

`ops iscsi-manage` 新增LUN/Target/组及映射、CHAP和设置管理；详见 [功能扩展](../reference/function-expansion.md)。默认不创建共享Target，删除Target保留LUN，LUN删除须confirm_data_delete:true；CHAP密码只接受环境引用。

`ops virus` 新增查杀扫描、精确记录/隔离文件处理、白名单、计划、病毒库和存储管理，详见 [功能扩展](../reference/function-expansion.md)。scan默认default_action:none，仅报告；文件处理须confirm_file_change:true，列表删除不得为空，病毒库初始化完成需info确认ready。

防火墙：`ops firewall --params '{"action":"info"}'`。先用 interfaces/countries/services 与 precheck(rules)读取和预检。apply规则完整替换需expected_info与confirm_replace:true，enable需expected_info与confirm_global:true；所有变更需主账号及--yes。预检201会阻止写入。参见[完整动作说明](../reference/function-expansion.md)。

Time Machine：`backup time-machine --params '{"action":"status"}'`；配置与删除动作限定当前账号，参考function-expansion。USB盘模式：`storage usb-mode --params '{"action":"status"}'`；扩容additional_gib为新增容量，写入需expected_status与对应显式确认。

GPU：`ops gpu --params '{"action":"info"}'`；任务支持process_name=zllamas/zaispace。弹出只提供显式全局范围，当前设备接口不可用。浏览器：`app browser --params '{"action":"open"}'`查看安装状态和设备指定元数据，install密码使用password_env；不在参数中传密码。
