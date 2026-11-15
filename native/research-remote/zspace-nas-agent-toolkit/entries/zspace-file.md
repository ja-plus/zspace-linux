---
name: zspace-file
description: Browse, search, read, write, upload, download, copy, move, delete, share, label, and inspect file tasks on a ZSpace NAS.
---

# 文件与搜索

`file copy` / `file move` 的 `to` 必须是已存在的目标目录，不是目标文件名；改名使用 `file rename`。

优先使用分组命令；完整参数见 `reference/tool-catalog.md`。

```bash
scripts/zspace-cli --profile <profile> file space --params '{}'
scripts/zspace-cli --profile <profile> file list --params '{"path":"/<pool>/my/data","start":0,"num":50}'
scripts/zspace-cli --profile <profile> file info --params '{"path":"/<pool>/my/data/a.txt"}'
scripts/zspace-cli --profile <profile> file read --params '{"path":"/<pool>/my/data/a.txt","max_bytes":65536}'
scripts/zspace-cli --profile <profile> search files --params '{"name":"report","file_path":"/<pool>/my/data"}'
```

先用 `file space` 发现真实路径。部分固件的 `device_dirs.list` 为空；CLI 会从存储池推导
`/<pool>/my/data`、逐个调用目录接口验证，并把可用结果放在 `personal_spaces`。

写操作先确认，再传 `--yes`：

```bash
scripts/zspace-cli --profile <profile> file mkdir --params '{"parent":"/<pool>/my/data","name":"docs"}' --yes
scripts/zspace-cli --profile <profile> --local-root "$PWD" file upload --params '{"local_path":"a.txt","dest_path":"/<pool>/my/data/a.txt"}' --yes
```

上传默认在目标存在时返回 `E_EXISTS`。确认需要覆盖后传 `overwrite:true`；只有明确希望保留两份文件时
才传 `rename:1`，并使用返回的 `data.path` 作为实际 NAS 路径。空文件同样会真实创建，父目录不存在
或发生冲突时不会返回假成功。断点续传只从完整的 8 MiB 分片边界恢复，可检查
`remote_partial_size`、`resumed_from` 与 `resume_reason`。

读取大文件用 `offset`/`next_offset` 分段；二进制内容用 `encoding:"base64"`。下载到已存在的本地文件时，
必须显式 `overwrite:true`；续传时 `offset` 必须等于本地文件大小。相对 `local_path` 以
`--local-root` 为基准；失败下载不会留下 0 字节目标。

`delete` 默认进回收站；`skip_bin:"1"` 为永久删除。`recyclebin list` 是只读，不需
`--yes`；恢复、永久删除、清空回收站、撤销分享前必须单独确认。

用 `file tasks --params '{"action":"status","task_id":"123"}'` 按 ID 精确查询文件任务。
返回 `found:false` 仅表示当前账号可见的活动任务集合未返回该 ID，不能据此判断任务已成功完成。
批量精确查询使用 `action:statuses,task_ids:["123","456"]`；暂停、继续、移除使用
`action:pause|continue|remove,task_id:"123"` 并在用户确认后传 `--yes`。移除正在运行的任务可能中止其处理。

压缩包浏览使用 `file archive --params '{"action":"browse","path":"/<pool>/my/data/a.zip"}'`。
创建压缩或解压任务使用 `action:compress|extract,paths:[...],to:"<目标目录>"` 并传 `--yes`。
提交后用返回的任务 ID 查询 `file tasks`；具体参数见 `reference/function-expansion.md`。

笔记用 `file notes --params '{"action":"list","location":"2","start":0,"num":20}'`。
支持内容 CRUD、分类、移动、标签、置顶、搜索、历史和配置；创建/修改/删除前确认并传 `--yes`。
保险箱备忘录使用 `location:"1"`，必须先满足设备端解锁条件。不得把普通文件写入当作笔记保存。

文件收集用 `file collect`；创建要求 `task_name`、`receive_path` 及 `valid_hours`/`expire_time` 二选一。
创建会开放收集入口，应先确认接收位置、有效期和文件限制。停止及清理指定任务传非空 `ids`；
全量范围的清理动作必须显式 `all:true` 和 `--yes`。参数见 `reference/function-expansion.md`。

标签 `assign` 支持 `names` 数组（兼容旧 `name`）；这会设置指定文件的标签集合。
`rename-status` 按 `task_id` 查询改名进度；`order` 必须显式提供 `labels`、`top_labels` 数组并确认执行。

## 团队空间（阶段性功能验收版）

`file public --params '{"action":"..."}'` 使用 NAS OAuth、POST form `/v2/public/*`。

| action | 参数 | 行为 |
| --- | --- | --- |
| list | 可选 start:0、num:50（正整数） | 按名称升序分页列出 /public 下可见分组 |
| permission | dir_name（例如 /public） | 当前用户权限与全局开关；保留后端 is_mater 原始拼写 |
| permissions | dir_name（单级分组名） | 管理员读取分组的普通用户权限 |
| user_groups | username | 管理员查看该用户各分组权限；后端最多返回 100 项 |
| create | dir_name、location（sataN/nvmeN/esataN） | 管理员创建分组，默认子账号只读；拒绝路径作为名称 |
| delete | 非空 dir_names 字符串数组 | 管理员删除分组及其文件，按名字作用于所有存储池，永久删除，高风险 |
| switch | enabled 布尔值 | 管理员修改全局团队空间开关，高风险 |
| set_permissions | 非空 entries 对象数组 | 管理员逐项设置权限；每项 user_name、dir_name、permission（1 读写/2 只读/3 禁止） |
| quotas | 非空 paths 字符串数组 | 管理员读取分组配额及当前用量 |
| quota_set | path、quota_kib（0～107374182400 整数） | 管理员设置配额，单位 KiB，0 取消限制；受 NAS 配额支持条件约束 |

配额路径必须为 `/<pool>/public/<group>`。set_permissions 发送重复 param[]，每项为一个 JSON 字符串；
读写/只读条目总数不能超过 60，不允许重复用户/分组组合。只更新给出的组合，不隐式生成全员权限。
全部写动作需 `--yes`；权限、配额、全局开关和删除按 destructive 处理，create 按 write 处理。
删除可能返回异步任务，使用 file tasks status 查询实际完成状态。不要因超时重发删除或创建。

协议证据：Web filev2.js、userCenter/components/public_list.vue、public/permssions.vue、
fileManage/myFiles.vue 与 actions.vue；后端 zfilev2/server/router.go、api/pub_permission.go、
api/dir.go、model/pub_permission.go。路由受 StorageOK/AuthRequired 保护，各管理 handler 再校验管理员。
真机已验证独立空分组创建、详情、权限读写、用户分组查询、配额查询、删除及列表无残留。
未改变现有分组、全局开关或现有配额；配额设置仅有请求契约测试，尚未真机写入验收。

归档补充动作：`file archive extract_dir` 接受 path、可选 to/pwd/charset/show_hidden/exfile，
递归解压；目标不能位于源目录内。`set_password` 接受 task_id（正整数字符串）、index（零起），
以及 pwd 或 skip:true 二选一，可选 forall:false。先查询 file tasks status 中 decompress_sources，
根据该源的 index/state/is_current 选择，不要把任务整体 pwd_checking 当作完成或失败。
compress 支持 exfile/exdir；extract 支持 select_all（bool，默认 false）和 exfile。
完整字段与尚缺能力见 `../reference/function-expansion.md`。

## 文件元数据与历史（阶段性功能验收版）

`file metadata`：hash 必填非空 paths 数组，只读；note 必填 paths 和 note 字符串，
note 最长 100 个 UTF-8 字节，空字符串清除备注；pin 必填 path，可选 cancel:false，
仅支持个人空间目录。note/pin 需要 `--yes`。
Hash 是 NAS 的抽样摘要：SHA-1 中包含文件长度及抽样字节，不是整文件 SHA-1 校验和；目录可能返回空摘要。
小文件验收按服务端算法计算并比对。note 使用 filepaths[]/file_note；hash 使用 paths[]；pin 的 cancel 为 1/0。
协议见 Web filev2.js、fileManage/actions.vue，后端 zfilev2/api/file.go、dir.go、share.go 及 util/util.go。

`file history`：recent/latest 读取最近访问/上传列表；可选 start:0、num:50（正整数）、scope:0
（0 全部/1 个人/2 团队）、ftype 字符串数组、extensions 字符串数组、show_hidden:false。
ftype 支持 100 文件、101 图片、102 视频、103 音频、104 种子、105 应用、106 归档、107 文档、10 目录。
CLI 将 ftype/extensions 数组编码为逗号分隔的单一表单字段；后端 recent 当前不使用 show_hidden，
latest 使用该标志。返回原始分页，空 data 规范为 `{total:0,list:[]}`；过滤发生于部分分页之后，
不要将单页的条数等同于全部匹配数。
visit 必填 path，上报访问，write；remove_recent/remove_latest 必填非空 paths 数组，仅移除指定记录；
clear_recent/clear_latest 必填 all:true，清理整个对应范围。移除/清理按 destructive 处理，均需 `--yes`。
移除记录不删除文件。clear_latest 的后端同时涉及个人及团队上传记录，不能当作仅个人记录清理。
协议见 Web filev2.js、后端 api/recent.go、latest.go、param_val.go 及 model/recent.go。
真机已验证独立文件 visit/list/扩展名过滤、指定访问记录移除、latest 读取；未清空用户历史。

## 文件分享与账号间共享（阶段性功能验收版）

`file share` 保留原有 list/statics/create/setexpire/delete/modify，并扩展：

- list 可选 type:0（全部），按分享类型过滤，start:0、num:50；statics 可选 stype:0（全部）。
- create 必填非空 paths；type:1 默认闪电传，type:2 文件分享；with_password:true 默认生成提取码。
  可选正整数 max_downloads（NAS 默认 1000 次）、expire_seconds/expire_days（未指定时 NAS 默认 2 天），
  show_hidden:false、exfile/exdir 路径数组。advanced_mode:true 要求 type:2 且只有一个目录，提交前预检目录。
- setexpire 使用正整数 ID 字符串数组 ids 或单一 share_id，二选一；expire_seconds/expire_days 按较大值计算，
  单位秒/天。有效期相对于原分享 created_at，不是修改时刻。
- modify 使用 ids/share_id 和字符串 state：0 取消、1 有效、2 审核不通过、5 清除次数上限。
- delete 仅移除显式指定 ids/share_id 的分享记录，不删除源文件；空 ID 集合被拒绝。
- modify_by/delete_by 必须显式 all:true、stype:0 非闪电传/1 闪电传、state:0 取消/1 有效/2 审核不通过/3 已过期/4 全部。
  modify_by 另需 newstate:0/1；后端更新会排除审核不通过的记录。这两个按 destructive 处理，不用于指定 ID 的小范围操作。

写操作均需 --yes。分享列表返回原始分页，空 data 规范为 `{list:[],total:0}`；只读动作不需 --yes。
协议为 POST form `/v2/share/*`，写状态按业务语义分级。证据：Web filev2.js、后端 api/share.go、model/share.go。
真机已验证唯一空目录的 type:2 高级分享创建、有效期读回、取消恢复、统计、指定记录删除；没有执行全量范围变更。
提取码和链接只返回给调用者，测试没有把链接发送给他人。外部访客授权/文件管理和视频审阅分享仍是后续范围。

`file shared` 是 NAS 账号间目录共享：incoming（共享给我）、mine（我共享）、info（path）、
create（path 及非空 users 字符串数组，或显式 all:true，二选一）、cancel（path）。
incoming 可选 sortby:name、order:asc、extensions 字符串数组；后端最多返回 2000 项，不支持客户端假分页。
create 会替换该目录已有接收者集合；空 users 不会隐式变成全员共享，重复、无效或当前账号会被拒绝。
共享给全部账号必须显式 all:true；create/cancel 为 write，需要 --yes。
传输字段 for 是逗号分隔的用户名；其他接口分别为 POST `/v2/nshare/forme|list|info|make|discard`。
权限、同名目录冲突及用户可见范围以 NAS 响应为准；不要把路径详情当作接收者已成功访问的证明。
证据：Web components/dialogs/shareSetting.vue、fileManage/share.vue、filev2.js，后端 api/nshare.go。
当前仅接入协议和本地接收者校验；写入真机验收待明确测试接收账号，不修改用户已有目录共享。


## 笔记附件、导出和回收站补充

所有动作仍须显式 location 字符串 1/2/3。新增动作：

| action | 参数 | 行为 |
| --- | --- | --- |
| export | id、local_path，可选 format:"txt"/"docx"（默认 txt）、overwrite:false、safe_dir | 预检笔记存在后导出到 --local-root 内，需 --yes |
| attachment_upload | id、local_path | 预检笔记存在，发送本地原始字节；返回 file_id 和不含凭据的 resource_path，需 --yes |
| attachment_download | file_id、local_path，可选 overwrite:false、safe_dir | 下载原始附件字节，需 --yes |
| class_order | 非空 tree 数组 | 节点为 {id,child:[...]}，ID 正整数且不重复，最多两级；顺序按数组定义，需 --yes |
| trash | 可选 start、num、sortby、note_type、safe_dir | 只读笔记回收站 |
| restore | 非空 ids 字符串数组，可选 classify_id:0 | 逐个确认仍在笔记回收站后恢复到指定分类，需 --yes |
| purge | 非空 ids 字符串数组、permanent:true | 逐个确认仍在笔记回收站后从回收站逻辑移除，高风险，需 --yes |

笔记删除一次进入回收站，同一 NAS delete 接口对回收站对象再次调用会设置逻辑删除标记；
CLI 为该动作提供显式 purge。逻辑移除不保证底层内容、历史及附件物理擦除。
restore/purge 的多对象预检通过后才提交，不接受当前仍在普通列表的对象。
class_order 转换为 classify_tree JSON 字符串，自动推导 parent_id；只更新给出的节点，完整排序应先读取 all_classes 构造完整树。

附件和导出当前单次最大 16 MiB；超出限制失败，目标文件不会被半成品替换。下载采用临时文件写入并完成后安装；
已有目标默认 E_EXISTS，只有显式 overwrite:true 才覆盖。错误 JSON 不会作为附件保存；
带附件头的合法 JSON 文件按原始字节保存。附件上传不传 appname，始终使用当前账号笔记命名空间。
上传不会自动改笔记正文，可把 resource_path 写入正文 HTML/Markdown 引用；后端保存的下载文件名不等同于原始本地名。

协议：POST 原始 octet-stream `/v2/file/notepad/uploadfile`，headers 为 notepadid/location；
GET `/v2/file/notepad/downloadfile?file_id=...&location=...`；TXT/DOCX 分别 GET downloadt/downloadocx。
证据：Web notebook.js、components/editor/index.vue/mdEditor.vue，后端 api/notepad.go、model/notepad.go。
真机已验证附件原始字节/正文引用、TXT 文本、DOCX 内文档 XML、默认不覆盖，以及回收/恢复/逻辑移除。

## 外部设备与回收站配置（V3）

`file devices` 仅适配 zspool V3：list、details 为无参数读取；info 必填 sn，mount_info 必填 mnt；
refresh 必填 sn，write；mount 必填 sn 和 type（USB/SATA/ESATA/NVME/SDCARD/CFE/TYPE-C），write；
unmount 必须且只能指定 sn/mnt，可选 lazy:false，destructive；rename 必填 sn/new_name，可选 part_seq，write。
所有写入需 --yes。本地盘改的是分区别名，part_seq 必须原样使用 info/list 返回的 partnum，不能用槽位 pos 代替；
new_name:"" 配合显式 part_seq 可清除该分区别名。网络盘改名不需要 part_seq。
路由 POST form `/zspool/external/list|details|info|mnt/details|refresh|mount|umount|rename`，
new_name 映射 nfilename，lazy 为 1/0。不会隐式调用格式化、擦除或外接存储池删除。
证据：Web filev2.js、components/file/sidebar.vue/publicSidebar.vue；后端 zspoolv3 的 extn*.go handlers。
真机 list/details 读取通过；当前无专用外接测试盘，挂载/卸载/改别名未真机写入验收。
网络挂载创建、PTP、外接盘高级设置、SMART/平衡任务及 V2 存储服务兼容仍待补齐。

`file recyclebin config` 只读自动清理配置；config_set 至少提供 my_cycle/public_cycle 之一，
值为 -1 禁用或 1/3/7/30/90 天。config_set 按 destructive 处理并需 --yes：保存可能立即触发清理历史回收站内容。
public_cycle 仅主账号生效；命令会读回实际配置，返回 requested、actual、full_success，必须检查 full_success。
未指定周期不重置为默认值。对应 POST `/v2/file/recycle/config/get|save`，证据为 Web recycle/setting.vue、
后端 api/task_remove.go 与 service/recycle.go。没有真机更改用户清理计划。

## 搜索管理（阶段性功能验收版）

`search manage` 支持慢搜索、内容索引和搜索历史，参数与索引前置条件见 `../reference/function-expansion.md`。开始慢搜索和索引修改需要 --yes。

`file duplicates` 提供重复/相似/空文件目录扫描和指定结果清理；清理必须 scan_id 和 file_ids，默认回收。完整参数见功能扩展参考。

## 安全整改源码的传输与恢复约束

`recyclebin restore` 必须提供非空 `paths`；全量恢复独立使用 `action:restore_all` 与 `--yes`，不接受同时提供 paths。下载默认拒绝覆盖已存在文件；确需覆盖时显式 `overwrite:true`。缺少完整长度、范围不匹配或文件版本变化时保留原目标；多块下载需要强 ETag。`offset>0` 时本地文件长度必须等于 offset，且显式 `overwrite:true`：客户端重新下载完整远端并逐字节核对本地前缀，不一致返回 E_CONFLICT 并保留原文件；此过程不节省带宽。以上对应 2026-10-19 源码，旧 rc.3 二进制需重新构建后才具备这些行为。

Windows 命令行存在系统长度限制，大 JSON 不应放在 `--params` 命令参数中。二进制或超大内容请先保存为本地文件，再用 file upload（local_path/dest_path）。restore_all 只恢复当前账号个人回收站，后端异步任务结果仍需查询任务状态。

需要用 `file write` 写入较大文本时，可把完整 JSON 对象保存到 `--local-root` 内的 UTF-8 文件，再使用
`--params-file params.json`；该通道上限 16 MiB，并受相同 schema、路径、确认和敏感信息检查。授权码与 token
不得写入参数文件。普通二进制或更大文件仍应使用 `file upload`。

## 文件字段与状态补充

- `file rename` 使用 `{"path":"/sata11/my/data/old.txt","newname":"new.txt"}`。
- `file copy` / `file move` 使用 `{"paths":["/sata11/my/data/a.txt"],"to":"/sata11/my/data/target"}`；JSON 字段为 `paths`，客户端负责编码为线上 `paths[]`，不要在 JSON 中使用 `paths[]` 别名。写操作须 `--yes`。
- 父目录列表中的 `encrypted`：`1` 表示加密目录已锁定，`2` 表示已解锁，空字符串表示非加密目录。
- 下载 `offset` 表示待核对的本地前缀长度，当前实现完整重传并验证，不提供节省带宽的续传；响应 `verified_prefix_bytes` 为已核对长度，`transferred_bytes` 为本次完整传输量。
