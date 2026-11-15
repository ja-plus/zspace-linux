---
name: zspace-system
description: Inspect ZSpace system health, disks, SMART, storage pools, services, processes, devices, events, and application status.
---

# 系统与存储

```bash
scripts/zspace-cli --profile <profile> system info --params '{"fields":["basic","cpu","memory","disk"]}'
scripts/zspace-cli --profile <profile> system disk --params '{"include":["mounts","statics"]}'
scripts/zspace-cli --profile <profile> storage info --params '{}'
scripts/zspace-cli --profile <profile> storage smart --params '{"sn":"<disk-serial>","lang":"zh"}'
scripts/zspace-cli --profile <profile> system services --params '{}'
scripts/zspace-cli --profile <profile> app status --params '{"all":true}'
scripts/zspace-cli --profile <profile> app list --params '{}'
scripts/zspace-cli --profile <profile> app manage --params '{"action":"info","source":"appstore","app_name":"video"}'
```

`app list` 动态读取两个应用商店，返回来源和部分失败状态。
`app status all:true` 已改为遍历动态目录，分别使用普通商店 OpenInfo 和 Docker 商店 info；
同名应用保留来源。app_id 单查默认 source:appstore，也可显式 source:docker；不允许 app_id 与 all:true 同用。
不传 app_id/all 时返回动态应用名和查询提示；可用 source 限定输出。结果保留 sources/partial/full_success，不能忽略单应用错误。
应用安装、启停、卸载、取消与移除任务使用 `app manage`，写操作必须先获得用户确认并传 `--yes`。
参数及异步语义见 `reference/function-expansion.md`。主机电源通过 `system power --params '{"action":"reboot"}' --yes` 或 shutdown 提交，须用户明确要求并验证主账号；不自动执行。磁盘格式化、存储池删除或 RAID 重建尚未提供。
SMART 的 `sn` 来自 `system disk` 返回的磁盘序列号。
`system info fields` 只接受 `basic/cpu/memory/gpu/fan/network/disk/power`，非法值在本地返回 `E_PARAM`，
不会透传成 NAS 内部错误码。

## 账号与存储权限（阶段性功能验收版）

`system users --params '{"action":"..."}'`：

| action | 参数 | 行为 |
| --- | --- | --- |
| list | 可选 all:true、include_quota:false | 读取账号及 sp_perms；include_quota 对应 cq:1，可能增加磁盘查询耗时 |
| nickname | nickname | 修改当前账号昵称，2～16 个 Unicode 字符且不超过 60 字节，write |
| remark | id、remark | 主账号修改普通子账号备注，同昵称长度限制，write |
| permission | 非空 ids 整数数组、permission:rw/ro/none | 主账号修改普通子账号整体访问权限，destructive |
| pool_permissions | pool、非空 entries:[{id,permission}] | 主账号按池修改普通子账号权限，destructive |
| pool_quota | id、pool、quota_kib:0～107374182400 | 主账号按池设配额，KiB，0 取消限制，destructive |
| lan_only | id、enabled 布尔值 | 主账号设置目标账号仅局域网访问；可作用于主账号本身并影响云端访问，destructive |

所有写入需用户授权和 --yes。ID 使用 list 返回的数值 ID；子账号管理拒绝 ID 1，
提交前重新读取账号列表，拒绝不存在或已提升为管理员的目标，避免 NAS 静默跳过管理员而误报成功。
lan_only 明确允许管理员目标；不会根据用户名猜测 ID。pool 为 sataN/nvmeN/esataN。
permission 使用逗号分隔 id 字段；pool_permissions 的 ids/perms 是一一对应的逗号分隔字段，不能互换或只传一半。
权限变化会影响文件访问；配额取决于 NAS 是否支持。写入后使用 list/include_quota 读回目标的实际状态。

命令使用 POST form `/account/user/list|modnickname|modremark|modperm|pool/perms|pool/quota|lanonly`。
它们与 Web `/auth/*` 经 nginx rewrite `/account/*` 的 handler 相同，沿用 OAuth，不调用 /local 内部接口。
证据：Web user.js、systemV2.js、userCenter 页面；后端 zbasic/server/router.go、api/account.go、output/user.go，
网关 srv/zbasic.conf。各 handler 继续执行账号权限校验。
真机 list 和 include_quota:true 查询通过；现有账号没有修改。创建/删除/禁用/账号改密等二级授权流程仍待接入，
不会用 OAuth 登录替代后端要求的临时验证凭证，也不会要求 Agent 获取账号密码。

## 网络与 DDNS（阶段性功能验收版）

使用 `system network --params '{"action":"interfaces"}'` 检查网卡；`system ddns --params '{"action":"list"}'` 查询 DDNS。完整 action/schema、凭证环境引用、更新替换与全局刷新语义见 `reference/function-expansion.md`。不得将读取成功理解为网络重配已验收。

`system network` 还提供主账号网络配置命令，完整 schema 与结果未知处理见扩展参考。`system certificates` 管理证书读取、上传、切换、重新生成、备份、删除和重载；不得为验收自动切换用户当前证书。

`system settings` 管理文件默认规则、主机时间与电源计划等；`system events` 查询当前账号消息和登录日志并管理已读/删除。`storage maintenance` 查询和管理 SMART/IHM、碎片整理、scrub 与缓存。具体范围与参数见功能扩展参考，现有盘池不作破坏性验收。
