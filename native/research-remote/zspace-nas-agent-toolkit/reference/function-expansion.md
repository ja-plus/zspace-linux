# 功能扩展开发记录

> 历史功能验收记录：其中业务测试日期与结论对应当时设备环境。当前客户端 0.3.0-rc.1 的安全处置、平台支持和限制以 `release-0.3.0-rc.1.md` 为准。
日期：2026-10-15。目标为按 `docs/zspace-cli/README.md` 顺序补齐全部非 UI 业务能力。
当前客户端版本为 0.3.0-rc.1，业务功能不宣称全部完成；安全与平台范围以 release-0.3.0-rc.1.md 为准。

## 第一批命令契约

`app list` 接受可选 `update_now` 布尔值（默认 false）。分别 POST form 到
`/AppStore/v1/list` 和 `/zdocker/AppStore/v1/list`，普通商店将刷新标志编码为 `1/0`。
结果 `list` 中每项为 `{source,app}`，同名跨来源应用不会合并。`sources` 保留成功状态和数量；
一个来源失败时 `partial:true`；两个来源都失败时返回非零错误。响应缺少 `list` 数组按来源失败处理。
这是一份商店目录，条目可能未安装，安装状态由原始应用数据说明。

`app manage` 必填 `action` 和 `source`（`appstore` 或 `docker`）：

| action | 参数 | 风险/行为 |
| --- | --- | --- |
| info | app_name | 只读详情 |
| tasks | 无 | 只读当前来源任务，保留原始任务状态 |
| disks | 无，仅 appstore | 只读可安装卷 |
| dependencies | app_name，可选 version_int | 只读依赖检查 |
| install | app_name、正整数 version_int、mp | 写入，安装或升级；mp 从 disks 选择 |
| start/stop/restart | app_name | 写入，改变应用运行状态 |
| uninstall | app_name；普通应用可选 with_data（bool） | 高风险；with_data:true 会同时删除数据 |
| cancel | app_name | 写入，取消安装任务 |
| remove_task | app_name；普通应用还必须传 task_id | 写入，移除任务记录 |

所有应用动作使用所属商店 `/AppStore/v1/pkg/*` 的 POST form 接口，非统一 JSON。
普通应用管理部分接口需要管理员；以 NAS 权限响应为准。安装提交成功不表示安装结束，
之后使用相同 source 的 tasks 查询；超时后不自动重新提交安装。Docker 安装可传 install_options（字符串键值表），不能覆盖身份或核心动作字段。
install_secrets 为“业务字段名 → 环境变量名”的字符串映射，值只在进程内读取；禁止把 OAuth 凭据放入其中。
UI 应用必须提供 appstore_port（50000～65535）；默认 appstore_restart_policy 为 no。
install 会先检查依赖；非空 sys_ver/app_ver/apps 返回 E_APP_DEPENDENCY，不提交任务。
Docker preinstall 使用 app_name、正整数 version_int 准备商店包，需要 --yes。
安装提交后遇到非权限错误返回 E_APP_INSTALL_UNCERTAIN：先查询 tasks，不能自动重复安装。

`file archive` 必填 action：

| action | 参数 | API |
| --- | --- | --- |
| compress | 非空 paths 字符串数组；可选 to、pwd、compress_level（0/1/2）、show_hidden（bool） | POST form `/v2/compression/create` |
| extract | 非空 paths 字符串数组；可选 to、pwd、charset、show_hidden（bool） | POST form `/v2/file/decompress/make` |
| browse | 非空 path；可选 parent、pwd、charset、sortby、order | POST form `/v2/compression/browser` |

charset 支持 auto/utf8/gbk/gb2312；sortby 支持 name/size/mtime，order 支持 asc/desc。
show_hidden 编码为 1/0，paths 编码为重复 `paths[]`。compress/extract 要求 `--yes`。
压缩、解压返回 NAS 原始异步任务对象，浏览也可能返回尚未完成的服务端浏览状态；不要把 HTTP 成功等同于任务完成。
压缩另支持 exfile、exdir 路径数组；解压支持 select_all 布尔值和 exfile 路径数组。
select_all:true 时 paths 表示要遍历的目录，不再表示单个压缩包；默认 false。
extract_dir 必填 path，可选 to、pwd、charset、show_hidden、exfile，递归处理目录内压缩包；to 不能位于源目录内。
set_password 必填正整数字符串 task_id、非负整数 index（decompress_sources 的零起索引），
提供非空 pwd 或 skip:true 二选一，可选 forall:false。发送 id/index/pwd/skip/forall 至 /v2/file/decompress/setpwd。
提交前精确查询任务，确认对应源处于 pwd_wait/pwd_err 且 is_current=1，任务整体处于 pwd_checking；
终态或越界索引在本地拒绝。递归任务不进入交互式密码等待，密码错误记录后跳过，需检查 failures。
尚未支持压缩包文件二进制导出和字符集自动重试。

`file tasks status` 按 ID 精确查询，`statuses` 按非空 task_ids 数组批量查询。
POST `/v2/file/taskbyids` 使用重复 `ids[]`。暂停、继续、单任务移除分别调用
`/v2/file/task/pause`、`/v2/file/task/continue`、`/v2/file/taskdel`，表单参数为 `id`。
list 支持 start、num、state。未返回任务只能说明当前可见任务集中没有匹配，不能推断成功终态。

## 协议证据

- Web：`NasServices/xnas-pc/src/api/appStore.js`、`filev2.js`，压缩对话框 `views/fileManage/components/CompressDialog.vue`。
- 后端：`appstore/router/router.go`、`api/apps.go`、`api/install.go`。
- Docker 商店：`zdocker/server/router.go`、`api/appstore.go`、`api/appstore_v3.go`。
- 文件：`zfilev2/server/router.go`、`api/task.go`、`api/task_compress.go`、`api/task_decom.go`、`api/api_cb.go`。
- 笔记：Web `src/api/notebook.js`、`views/notebook/memo.vue`、`notelist.vue`，后端 `zfilev2/api/notepad.go` 与 `model/notepad.go`。
- 收集与标签：Web `src/api/fileCollect.js`、`labels.js`，后端 `zfilev2/api/receiver.go`、`api/file_label.go`、`model/receiver.go`。

## 笔记命令契约

`file notes` 的所有动作都必须显式指定字符串 `location:"1"|"2"|"3"`，不隐式使用后端默认值。
1 对应保险箱备忘录（受 NAS 解锁条件限制），Web 普通笔记使用 2；3 按后端支持的原始位置值透传，尚未真机验收其业务用途。
可选 `safe_dir` 是字符串，后端只在支持的读接口中使用；写接口可能拒绝非空 safe_dir。
以下字段之外的参数会在本地拒绝。

| action | 参数 | 行为 |
| --- | --- | --- |
| list（默认） | start,num,classify_id,label_id,note_type,sortby | 分页，sortby 1～6 对应修改时间新/旧、创建时间新/旧、标题正/倒序 |
| info | id | 详情 |
| create | 必填 title；可选 body,classify_id,note_type,in_brief | 新建，写入 |
| update | id；title/body 至少一个非空；可选 note_type,in_brief | 更新，写入 |
| delete | id 或非空 ids 字符串数组，二选一 | 删除笔记及相关内容，高风险 |
| pin | id，可选 cancel:0/1 | 置顶或取消置顶，写入 |
| search | keyword，可选 start,num,classify_id | 搜索 |
| classes | start,num,parent_id | 分类分页 |
| all_classes | 无 | 全部分类 |
| class_create | name，可选 parent_id | 新建分类，写入 |
| class_update | id,name，可选 parent_id | 修改分类，写入 |
| class_delete | id | 删除分类，高风险 |
| move | 非空 ids 字符串数组,classify_id | 移动笔记分类，写入 |
| label | id,label_id,label_action:1/2 | 添加/移除标签，写入；label_action 映射为服务端 action |
| config | scope,config_key（可选） | 配置查询 |
| config_set | config_key,config_value（均非空字符串），可选 scope | 配置写入 |
| history | notepad_id，可选 start,num | 历史分页 |
| history_info | id（历史记录 ID） | 历史详情 |
| size | 无 | 用量 |

ID 和分页参数接受非负整数或其字符串，id/notepad_id 必须大于零。批量 ids 必须为正整数字符串数组。
title/body/name/keyword/config_key/config_value/in_brief/note_type/safe_dir 使用字符串。
已补 attachment_upload/attachment_download、TXT/DOCX export、class_order 及 trash/restore/purge。
文件传输必须提供 local_path，受 --local-root 和显式覆盖约束；单个附件与导出上限 16 MiB，当前不提供流式大文件附件。
分类树使用 id/child 数组，CLI 推导 parent_id，最多两级且 ID 不能重复；未改动用户现有分类树排序。
trash 映射 classify_id:-1；restore/purge 逐个预检对象仍在笔记回收站，purge 另需 permanent:true。
NAS purge 是逻辑删除标记，不保证物理擦除底层历史和附件。详细 schema 见 `../entries/zspace-file.md`。
class_create 的 NAS 原始成功数据为分类 ID 标量；create 的原始成功数据为笔记对象，不能混用。

## 收集与标签扩展契约

`file collect` 按动作区分风险：

| action | 参数 | 风险/说明 |
| --- | --- | --- |
| create | task_name、receive_path；正整数 valid_hours 或 expire_time 二选一 | 写入，expire_time 为 Unix 秒时间戳 |
| list | start:0、num:20、status:0 默认值 | 只读；status 0 全部、1 进行中、2 已过期、3 已停止、4 已完成 |
| info/files | 非空字符串 id | 只读详情/已收集文件 |
| stop | 非空 ids 字符串数组 | 写入，停止指定任务 |
| clean | 非空 ids 字符串数组 | 高风险，移除指定任务记录 |
| clean_expired/clean_stopped/clean_done/clean_all | all:true | 高风险，清理对应范围记录；不会替调用者推断全量意图 |

create 还接受 task_note（字符串）、paths（附件路径字符串数组，最多 5 项）、subdirs（字符串数组）、
accepted_types（字符串数组）、max_file_size（字节数）、max_num（文件数量）、add_prefix_flag（bool，默认 true）。
max_file_size/max_num 默认 0，按 NAS 语义不额外限制。路径及外部可访问收集入口应由调用者明确确认。
写/清理动作要求 `--yes`。请求使用 POST form `/v2/receiver/create|list|info|files|action`；
stop/clean 发送重复 ids[]，清理范围映射为 clean_expired/clean_stop/clean_done/clean。
外部访客上传/下载协议尚未作为 CLI 子命令实现。

标签 assign 支持 names 数组或兼容单一 name，必须有非空 paths；发送 label_names[]、filepaths[]。
它设置文件标签集合，不承诺在旧集合上追加。files 新增 scope、sortby、order、show_hidden 筛选。
rename-status 是只读 `/v2/labels/renamestatus`，字段 task_id。
order 写入 `/v2/labels/savelabelorder`，必须显式 labels 和 top_labels 数组；空数组会清空对应顺序。
真实用户排序没有被测试改动；排序请求编码由 mock 验证。

## 本地验收进度

首批应用/归档/任务/笔记/收集/标签：120 项单元测试和 2 项 CLI 契约测试通过；最终格式和静态检查见验收记录。
SSH 只读连接测试通过（x86_64）。临时 OAuth profile 已完成真机功能验收，证据见 `acceptance-functions-2026-10-15.json`。
LAN 动态应用发现返回普通商店 55 项、Docker 商店 15 项，总计 70 项；cloud 动态发现也成功。
压缩与解压均验证任务 done，按实际输出目录找到文件并核对文本一致；普通应用任务读取成功。
笔记及分类创建、笔记详情/修改/搜索/历史/置顶、收集创建/详情/文件/停止、标签创建/多标签设置/筛选/改名/进度均通过。
专用笔记移入笔记回收站，分类、收集记录、标签均已清理；专用文件目录移入回收站，没有清空回收站。
未在真机安装、卸载或启停用户现有应用，也未修改用户现有标签排序。
Docker 商店最初返回“读取数据目录配置失败”；用户手动初始化后，任务接口和引擎正常。
已完成 Busybox 拉取、容器生命周期/日志/进程/暂停恢复/改名、独立网络连接与断开、Compose 创建/文件读取修改/任务终态/启停重启/重建/删除。
独立思源应用完成预安装、安装到终态、状态检查、停止/启动/重启、卸载到终态；使用独立目录、端口和临时业务口令。
测试容器、网络、项目、应用任务、包缓存、两份新增镜像已清理，测试目录移入回收站。
Docker 命令契约及尚缺能力见 `../entries/zspace-docker.md`。
六平台二进制和 ZIP 仍为旧发行包，未将源码变更冒充为已发布功能。

## 后续范围（均未据此宣称完成）

文件全部选项/批量/分页与流式传输、外接设备高级配置、访客收集/分享、保险箱与快照剩余授权/验收、完整搜索、
系统账号网络存储管理、应用高级配置、下载与备份同步、各家网盘、相册与媒体工作流、
Docker/Compose/WS、虚拟机/iSCSI、安全应用管理、监控/恢复/邮件/亲情/课堂/微信、
本地 AI、Lightbox、USB 盘模式/Time Machine、浏览器与 GPU 均继续保留在全量目标中。
涉及缺失的服务端源码、第三方认证和平台测试时需补充协议或用户输入，不能编造接口。

团队空间分组、权限和配额命令已实现；真机独立分组验收与清理通过，详见 `../entries/zspace-file.md`。配额写入、全局开关尚未真机写入验收。

归档补密码续传与递归解压均已完成真机内容往返核对；文件摘要、备注写入清除、个人目录置顶取消和访问历史扩展已验收，测试对象已清理。

快照命令已接入任务/列表、创建编辑删除、计划与保留、初始化和恢复登记；独立快照生命周期真机通过，初始化/恢复与加密池交互仍未真机验证，详情见 `../entries/zspace-snapshot.md`。

保险箱状态/备份只读真机通过，密码授权请求经 NAS 公钥加密并有 HTTP mock 往返测试；
当前授权账号保险箱初始化、解锁、目录读取、笔记读写清理和最终锁定已真机通过。轻度加密目录全部管理动作已真机验收，解除后内容一致，测试目录已清理。
文件分享扩展已真机验收创建/有效期/取消恢复/指定删除；账号间目录共享已接入协议，写入验收待明确接收账号。
新引入 rsa/rand 依赖用于业务凭据公钥加密，生产代码不进行私钥操作；安全整改与依赖告警适用性以 release-0.2.3-rc.2.md 为准。

笔记 TXT/DOCX 与附件二进制往返、回收站查看/恢复/指定逻辑移除已真机通过。回收站定时清理配置新增命令仅只读验收，不改变用户清理计划。全量业务覆盖矩阵见 `coverage-functions-2026-10-15.md`。

app status 已取消静态应用表，真机动态 70 项查询全部通过；system users 接入账号列表、昵称备注、权限与池配额、局域网限制，读取通过，现有账号未改动。

## 系统电源

`system power` 接受 `action:reboot|shutdown`，需要 `--yes`，提交前重新读取账号并确认当前为主账号。返回 submitted 不代表动作完成；连接中断返回 E_POWER_RESULT_UNKNOWN，需先检查设备状态，避免重发。仅本地模拟验证，未对验收机执行真实重启或关机。

## 网络诊断与 DDNS

`system network` 当前提供 info、mode、interfaces/interface(name)、bonds、bridges/bridge(name)、wireless、wireless_advanced、wireless_config、hardware、ipv6_monitor、public4/public6、public4_detect/public6_detect、interface_public4/interface_public6(name)。这是服务端原始网络信息，未套用 Web 对无线网卡的显示过滤。公网检测可能较慢，可增加全局 timeout。

`system ddns` action 为 list/create/update/delete/refresh。create/update 使用 config 对象：provider(aliyun/tencent/cloudflare/duckdns)、ipv4_enable、ipv6_enable、record4/record6 完整域名数组、ipv4_detect_method/ipv6_detect_method(zspace 默认或 interface)、对应 ipv4_interface/ipv6_interface 和 ipv6_address_index(0～9)。至少启用一个 IP 版本，每个启用版本需 1～10 个域名。两个检测方式均发送，符合服务端即使禁用版本也校验方式的行为。

服务商凭证仅从 access_key_env/access_id_env 读取；create 必须有 access_key_env，aliyun/tencent 还需 access_id_env。update 是整份配置替换，省略凭证引用时由服务端保留原凭证，其他字段不按补丁合并。delete 以 provider 为单位删除整个配置；refresh 必须 all:true，会刷新全部配置，可能更新真实 DNS。写入需 --yes；未用现有用户 DNS 进行写入验收。

依据 xnas-pc/src/api/network.js、ddns.js、views/ddns/index.vue 与 zddns/ddns.go/routes.go。真机 DDNS list 和网络版本/模式/网卡/绑定/无线基本列表/硬件/IPv6 监测读取通过；网桥列表经服务端 routes.go 核对为 GET（客户端写作 POST），修正后读取通过；无线高级和已保存配置在当前设备返回 API 错误，当前 go-base-set 路由未提供，未当作空列表成功。


网络配置 action 另有 mode_set(mode 0～6)、ipv6_monitor_set(enabled)、hardware_set(advanced)、interface_update(name,config)、bond_create/bond_update(mode 1/4/6)、bond_delete(all:true)、bridge_create(name,interfaces 名称数组)、bridge_delete(name)。均需 --yes 和当前主账号预检；自定义 Bond/Bridge 要求现有网络模式为 0，不会自动切换。interface_update 的 config 是显式补丁，支持 ipv4_method/address/netmask/gateway、ipv4_dns_method/dns1/dns2、mtu；static 地址必须包含地址/连续掩码/同网段网关，静态 DNS 需要 dns1，MTU 为 1280～9000。连接失败返回 E_NETWORK_RESULT_UNKNOWN，须从设备控制台确认。未对现网做配置写入验收。

## TLS 证书

`system certificates`：list、use/delete/regenerate(id 字符串)、backup(id,backup_path)、reload、upload(server_path,private_path,intermediate_path 可选,remark,replace:false,use_after_upload:false)。list 输出配置存在标志，不输出 DNS/EAB 凭证值。上传使用客户端相同的自定义槽位 2；已存在时需 replace:true。上传本地文件受 --local-root 限制，每个最多 1 MiB；multipart 字段为 id/remark/status/server/private/intermediate。切换和当前证书重新生成会返回 ssl_reload_required，重载用单独 reload 命令。拒绝删除设备默认证书与当前启用证书。写入均需 --yes。

当前已真机验证证书读取、空闲槽位上传未启用测试证书、NAS 目录备份、删除与清理；当前 HTTPS 证书未切换。证书 ZIP 导出已真机验证字节一致性与 Unix 0600 文件权限；Windows ACL 尚未跨平台验收。ACME 申请入口已编码，未使用真实域名申请。依据 zbasic/api/cert.go 与 xnas-pc/src/views/setting/certManagement.vue。

## NAS 文档同步（Syncthing）

`backup sync` 通过 /syncApi/rest 调用原生 JSON，兼容空成功响应；使用 xnas-pc/src/views/docSync/api/request.js 相同的 NAS ID 派生 API key。MD5 只用于既有协议兼容，不是密码加密。没有向本机安装 Syncthing，也不将 NAS 端配置登记等同于跨设备同步已完成。

读取 action：status、folders、devices、connections、device_stats、config_sync、folder_status(folder)、completion(folder,device)、need/errors(folder,page:1,perpage:10)、ignores(folder)。folders/devices 仅返回对应配置列表，不返回含 GUI/API key 的整份配置。

写入 action：create(path,label 可选,folder_type:sendreceive 默认/sendonly/receiveonly,devices:[] 默认,paused:true 默认)、pause/resume(folder)、scan(folder)、set_ignores(folder,ignore 单行规则数组)、delete(folder,scope 必填 nas/all_devices)。create 需要已存在的独立个人/团队子目录，转换 NAS 原生路径，使用服务 defaults.folder；默认仅当前 NAS，无对端。devices 只接受已配置的其他设备 ID，不自动添加或邀请设备。配置修改先重读当前配置，只修改所选任务；delete nas 只移除 NAS 配置，all_devices 使用客户端同步删除协议。文件保持不删，返回 submitted 不代表所有远端已确认。写入需 --yes。

用户安装启动文档同步后，真机读取全部通过，独立 NAS 端任务创建/恢复/忽略规则写读/扫描/状态/待同步/错误/暂停/删除保留文件与夹具清理通过。跨电脑配对、版本恢复/清理、完整同步计划与冲突处理仍待补齐。


证书另支持 export(id,local_path,overwrite:false) 与 apply(config,secrets,replace:false)。apply 固定槽位 2；config 为 ca_type:1(Let’s Encrypt)/2(ZeroSSL)、dns_type:1(阿里)/2(腾讯)/3(Cloudflare)、email、private_key_type:EC256/EC384/RSA2048/RSA3072/RSA4096/RSA8192、domains、remark 可选、use_after_apply:false。secrets 为对应 eab_kid_env/eab_key_env（ZeroSSL）、dns_access_id_env/dns_access_secret_env（阿里/腾讯）或 dns_access_token_env（Cloudflare），值从环境读取。成功仅表示异步申请已提交，使用 list 读取 status/fail_reason；use_after_apply 可能切换服务证书，需明确用户要求。

## 下载器扩展

保留基线 `download tasks` 列表及 `download manage` add/stop/start/delete，新增 `download control`、`download settings`、`download torrent`；使用 catalog 可区分命名工具，不覆盖旧命令。

control：protocols/details/directories(task_type:all/loading/complete,start:0,size:50)/engine_status 为读取；start/stop/verify/pin/unpin(id 正整数) 为单任务写入；delete(id,remove_files:false) 默认保留文件。start_all/stop_all/start_seeding_all/stop_seeding_all(all:true)、delete_all(all:true,task_type:loading/complete/fail/seeding-fail/seeding-stopped,remove_files:false) 显式批量范围。engine_switch(bt_engine_type:1 Transmission/2 qBittorrent,force:false)、engine_confirm、storage_move(pool)、storage_confirm 为主账号高风险命令。引擎切换默认保留服务端迁移检查，不自动 force；异步命令返回 submitted/completed:false，状态由列表和 engine_status 查询。

settings action=get/basic/set，set 的 config 为显式补丁：seed_ratio_limit_enable/seed_ratio_limit（百分之一单位，100=1.0）、seed_time_limit_enable/seed_time_limit（分钟）、speed_limit_up_enable/speed_limit_up、speed_limit_down_enable/speed_limit_down（KB/s）、peer_limit_torrent_max/peer_limit_global_max、peer_port、dht_enabled/pex_enabled/lpd_enabled、download_dir/download_dir_clear、torrent_watch_dir/torrent_watch_dir_clear、download_queue_size、tracker URL 数组。空 tracker 数组明确清空默认 tracker；clear 字段清空目录配置，不是删除目录文件。set 可能影响所有下载任务，不在验收中改动现有全局设置。

torrent：generic(id)、generic_set(id,config 使用服务端 camelCase：uploadLimitEnable/uploadLimit/downloadLimitEnable/downloadLimit/seedRatioLimitEnable/seedRatioLimit/seedTimeLimitEnable/seedTimeLimit)、trackers/peers/files(id,start:0,size:50)、file_set(id,file_id 含 0,option:high/normal/low/ignore)、tracker_add(id,trackers)、tracker_delete(id,tracker_ids)。单任务 seedRatioLimit 为浮点比值，与全局 seed_ratio_limit 的百分之一单位不同。

种子入口 parse_upload(local_path，8 MiB 限制)、parse_nas(path)、cancel_parse(hash)、effect(hash,dir,ids 叶节点 ID 数组,checked:wanted/unwanted,name 可选)、add_upload(local_path,dir)、add_nas(path,dir)。预解析是写缓存操作，需要 --yes；调用 effect 后应查询任务确认登记，不能因无任务 ID 就重复提交。未提供客户端使用但当前服务端会拒绝的 -100 全选哨兵；全选应提交解析树中的实际叶节点 ID。

读取设置/基本设置/协议/目录/队列/引擎状态通过。使用一字节自有私有种子和保留 .invalid tracker，已验证本地 multipart 与 NAS 解析 hash、取消解析、文件选择生效、精确任务 hash、文件/peer/tracker 读取、单任务上传限速回读、优先级、置顶取消/校验、删除默认保留文件与最终清理。曾遇到后台校验忙碌 N201032，已等待并清理确切任务；现映射为 E_DOWNLOAD_BUSY，需先查询状态，避免把异步 ACK 当作已完成。

依据 downloader/internal/webapi/handlers 与 xnas-pc/src/api/download.js，内部 control/local/pprof 路由不开放。批量动作、全局设置、真实 tracker 修改、引擎切换与存储迁移仅本地验证/待专用环境，未改变用户全局任务状态。

## 事件日志

`system events` action=list(event_type:""/notify/log,group,event_action,start_id:0,is_new:-1/0/1,levels 数组,num:50)、login_log(start_id,num)、mark_read/delete(ids)、mark_all_read(all:true)、delete_all(all:true,event_type)。ID 是逗号字符串，当前账号范围由服务端强制；批量删除需显式范围。没有开放消息发布入口。写入需 --yes，用户现有日志未做写入验收。


## 系统设置

`system settings` action=get/set(config)、timezones/timezone_set(timezone)、time_sync、power_plans/power_plan_set(option:on/off,enabled,times:[{weekday:0～6,time:HH:mm:ss}])、fan_set(mode:auto/auto_plus/cool)。电源计划时刻按设备时区执行；这是设置计划，不是立即执行电源动作。timezone_set 必须匹配设备 timezones 列表。主机校时、电源计划、风扇和全局字段均先校验当前主账号，写入需要 --yes。未修改当前设备时钟、风扇或电源计划。

set config 为非空显式补丁，支持 show_hidden、share_number/share_expire/share_has_pass/share_advanced_mode、flash_number/flash_expire/flash_has_pass、file_sort_by/file_sort_order、file_hash_check/file_hash_check_option(simple/complete)、rename_copy/rename_upload(0～4)、rename_copy_tip/rename_upload_tip、wps_version_num；设备/分享全局可见性 usb_show/sata_show/esata_show/can_share 为 master/all，trust_local/trust_anyway/boot_on_power_off 为 yes/no，fs_evt_monitor 为布尔，disk_stats_sec 至少 3600。share/flash expire 单位秒。桌面壁纸等纯 UI 设置未纳入业务命令。

真机 get/timezones/power_plans 与当前账号 show_hidden 切换回读及恢复原值通过。依据 zbasic/api/setting.go/fan.go；其他设置变更及计划执行待专用验收。

## 存储维护

`storage maintenance`：info/hardware/polling/capability；smart_report/sn、smart_real/sn、smart_vendor/dev；smart_start/status/cancel(pid)、smart_schedule(pid)、smart_schedule_set(pid,days:[1～7] 或 [-1] 禁用,hour,min)；ihm_report(sn)、ihm_start/status/cancel(pid)；balance_analyze/analyze_status(pid)、balance_start(pid,gids 非空组数组)、balance_status/cancel(pid)；scrub_start/status/cancel(pid)；cache_hits(pid)、cache_delete(pid,confirm_cache_delete:true)、smr_ignore(sn)。所有 pid 都必须显式提供，0 是有效 ID；SN/dev 不能空。smr_ignore 在服务端按磁盘型号记录忽略，不只是单块盘。

读取不需要 --yes；负载操作、计划/取消需要 --yes、主账号及池存在性预检。cache_delete 是缓存解绑删除，不是清空命中统计；不能为检查命中率而调用。写入只返回 submitted/completed:false，需要对应状态查询。没有实现内部 shutdown/pprof/local 接口。

真机池信息/硬件/轮询/能力、SMART 状态与计划、IHM、碎片分析状态/任务、scrub 状态通过。当前池无缓存，cache_hits 返回 N300121，现映射为 E_NOT_FOUND；没有启动磁盘负载或删缓存。依据 zspoolv3/internal/api/router.go 与 handlers。Web 的 balance/task/timer 不在当前后端路由内，未假装为受支持功能。

## 备份中心

`backup center`：count(all_tasks:false)、list(all_tasks:false,basic_type:1/2 可选,source_type/target_type 可选,owner 可选且需 all_tasks)、info(task_id)、failures(task_id,start:0,num:50)、start/pause/resume/plan_stop/plan_start/delete(task_id)。all_tasks 需要当前主账号预检，写操作先重读确认准确任务 ID。delete 删除任务配置/日志，不删除备份输出。

本地创建 create_local(task)、完整替换 update_local(task_id,task)：task 为 name、sources 非空逻辑目录数组、target 逻辑目录、plan、rules、confirm_mirror:false。源和目标以及多个源不得相同或互相包含。默认 plan={kind:manual}；interval 使用 hours:1～199；fixed 使用 every:day/week/month、days 数组（每天为空，每周 1～7、每月 1～31）与 time:HH:mm。创建只登记配置，不自动执行；返回 task_id。创建网络中断或成功响应缺少 ID 返回 E_BACKUP_CREATE_UNCERTAIN，先查询 list 进行核对。

rules：increment_type 默认 overwrite，可为 new/new_diff；ignore_ext_list、ignore_hidden:false、ignore_disk_sleep:false、local_compare_first:true、file_check_method:simple/complete、mirror:false、keep_version_count:0。镜像会删除目标多余文件，必须 confirm_mirror:true；keep_version_count=0 不清理历史版本。当前本地任务不弹出设备、不清空源卡、不启用加密；外部设备、协议/网盘、加密及恢复流程仍待扩展。输出中的 task_source_ext_info/task_target_ext_info 含协议连接资料，提供 configured 标志而不直接输出其内容。

真机独立同 NAS 源/目标目录验证 count/list、创建手动任务、更新名称、执行终态成功、内容往返、失败文件查询、计划停用/启用、删除配置后输出仍保留，以及夹具清理通过。依据 backupcenter/zbakcenterv2/service/dto/task_req_params.go、api/task.go/task_check.go 与客户端 Step4Confirm.vue。

## 网盘供应商

`backup cloud` provider: aliyun/baidu/onedrive/tianyi/yun115/yun123/yun139/kuake/gdrive/dropbox。check 在没有固定 NAS 地址/缓存地址的云端模式下必须传 redirect_uri，否则从 NAS 地址生成 /home/newNetdisk。user/space/flow_quota/drives/benefits/mount_info/tasks/sync_status/sync_folders/auto_backup_info 按供应商实际能力开放；不支持的组合直接拒绝。

files(query) 采用供应商独立分页。upload(query) 必须 paths，夸克/115/123 另需 folder_id，阿里需 drive_id/drive_type/remote_dir；百度可选 save_path。OneDrive/天翼/Google/移动的上传目标由服务端决定，拒绝无法生效的目标字段。download(query) 指定 NAS save_path、file_ids/folder_ids，阿里需 drive_id/drive_type；移动使用 download139(save_path,file_id_map,folder_id_map)，映射值为云盘完整路径。Dropbox 不提供这些传输入口。

task_action(method:pause/resume/delete) 必须 task_id 或 all:true 二选一，映射 clean/clean_all/pause_all/resume_all。failures(task_id,page,limit) 查失败文件。sync_enable/disable/delete 需 all:true，logout 需 confirm:true。写入需 --yes 和当前登录预检，submitted 不代表完成。任务删除不删除云盘文件。

十家登录检查通过。夸克容量释放后已完成49字节上传/下载内容比对、精确任务清理及NAS目录回收；云端唯一夹具目录由用户删除后已核验不存在。分享转存和同步目录真机验收尚待补齐。当前客户端安全处置以 release-0.2.3-rc.2.md 为准。

`sync_set(save_path,items:[{folder_ids:[...],drive_id?}],confirm_replace:true)` 配置云端目录到 NAS 的自动备份目录。阿里每组需要 drive_id；115/123/夸克/Google 使用 JSON items；百度、天翼、OneDrive 仅单组并采用表单，百度字段为 file_ids。拒绝空目录集合和重复项；配置可能替换已有列表，按 destructive 处理，不自动开启同步。Dropbox/移动没有此入口。供应商请求编码及替换确认已单元验证，未在含用户文件的云端目录执行。

## 搜索管理

`search manage`：index_dirs、index_failures(path)、index_set(paths,mode:add/remove/pause/resume)、reindex(all_configured:true)、history(search_type:1文件名/2内容)、history_delete(search_type,names)。索引配置按当前账号和选定路径更新，不删除文件；重建作用于当前账号全部配置目录，需明确范围。历史删除必须显式 names，不提供隐式全删。

slow_start(kind:files/content,name,file_path,show_hidden:false,shared_only:0) 返回 taskid；slow_result(kind,task_id,start:0,num:100) 查询；slow_stop(kind,task_id) 取消。shared_only:0全部、1共享、2本人、3本人和文件广场、4文件广场，file_path 显式指定扫描目录。内容搜索另可传 doc_type:0..5、mtime/brtime天数；文件搜索可传 ftypes 整数数组，不接受服务端未使用的时间/尺寸过滤。内容慢搜索只扫描与当前账号已开启内容索引目录相交的范围；没有配置可能正常返回零结果。状态 1运行、2停止、3完成；服务端返回原 taskid，CLI 接口字段名为 task_id。

独立 TXT 目录验证文件/内容慢搜索找到夹具、完成与停止、内容索引新增/暂停/恢复/失败列表、精确索引移除和历史清理及目录回收通过。全局重建未执行；AI/RAG 和重复相似文件工作流仍待补。依据 file-search controllers/content_index.go/content_search.go/file_search.go、services/search_task.go 与 xnas-pc bigSearch.js。

## 重复与相似文件

`file duplicates`：state、scan(paths,kind:file/same_image/similar_image/empty_dir/empty_file,replace_previous:false,include_sync:false,include_bdmv:false)、groups(start:0,num:100,file_type:100..107可选)、group_files(group_id,start,num)、modify(scan_id,state:scan_done/partial_done/clean_done/expire/cancel)、clean(scan_id,file_ids,recycle:true,confirm_permanent:false)。替换先前结果需要明确 replace_previous；修改与清理要求准确 scan_id，清理必须显式非空唯一 file_ids，不回收需 confirm_permanent:true。当前命名入口不开放按类别或全扫描隐式批量删除。

清理前读取当前扫描状态3与全部结果组/文件，逐项确认 ID 归属，再确认扫描未切换后提交。异步提交不代表完成，需查询 state。扫描状态1初始、2运行、3扫描完成、4清理中、5清理完成、7过期、8部分完成、9取消。modify 是后端状态更新入口，不能把状态写为 scan_done 当成实际扫描已完成。

真机当前无已有扫描时，用独立目录两份相同文本验证扫描、结果组/文件分页、指定其中一份回收且另一份保留、标记本次扫描过期及目录回收通过。相似图像/空目录扫描与更多中断分支待专项验收。依据 zfilev2/api/fdd.go、output/fdd.go 和 xnas-pc fileClear.js。

## 相册文件 RAG

`search rag` 按 xnas-pc 的 /v2/album/rag 调用实现，区别于独立 zaispace 应用。读取 config/pools/model_check/model_info/datapath_info/dirs/flush_info/index_state；search(query_content,top_limit:1..1000,file_types:[1]) 使用 input_modal:1，file_types:0文档/1图片/2视频，空数组表示全部。

配置 enable(enabled)、run_period(start:HH:mm,end:HH:mm) 或 clear:true；模型 model_create(save_path,retry:false)、model_pause/resume(task_id)、model_clean(all:true)；索引 datapath_set(save_path,confirm_reconfigure:true)、datapath_clean(all:true)、dirs_add(paths)、dirs_delete(dir_ids)、flush_all(all:true)。当前没有确认参数的 dirs/flush 和 flush/clean 写入没有臆测实现。模型和存储配置需当前主账号，启用前确认模型已下载成功，暂停/恢复校验当前模型任务ID。变更前必须读到配置 rag_open；调整存储或清除要求先显式关闭 RAG，不自动变更开关；删目录校验当前目录ID。

当前验收机相册配置可读但没有 rag_open，七个 RAG 状态路由返回404；没有下载模型、启用或调整相册设置。记录为源码调用已编码、当前固件不支持真机功能验收；模拟测试确认此固件不会收到任何 RAG 配置写入。独立 zaispace 接口仍需单独核对。

## 相册生命周期与导入

`album manage`：home/dirs/scan_info/flush_info；info(album_id)、create(name,description:"",feed_ids:[]) 创建类型10的普通自建相册，不含外部成员；rename(album_id,name,description可选) 仅无其他成员的普通相册，省略描述时保留原描述及已有显示/分享设置；delete(album_id) 删除相册关联，不删除原图。

feeds(album_id,start:0,num:100)、add_feeds/remove_feeds(album_id,feed_ids)、import(album_id,paths,monitor:false)。remove_feeds 只移除相册关联，引用内容的二次确认未自动同意。import 路径必须已在当前相册来源目录登记，可用 dirs_add(paths,scan:false) 单独登记而不自动扫描；dirs_add 默认 scan:true。dirs_delete/dirs_flush(dir_ids) 先读当前目录列表确认归属，scan_new、flush_all(all:true) 为异步入口，提交成功不等于完成。

真机通过空相册创建/详情/重命名保留描述/删除并确认不存在；独立静态图片上传、来源目录登记不扫描、导入、只移除关联且原图保留、来源登记及相册和NAS夹具清理。scan_info 当前固件404，flush_info可用；未运行全局刷新。家庭/宝宝/纪念册、共享及人物地点等完整管理仍待扩展。依据 zalbumv2/api/album.go/page.go、model/album.go 与 xnas-pc albumv2.js。

## 影视分类与扫描

`media video-libraries`：list、dirs、create(name,paths:[],skip_scrape:false)、rename(library_id,name)、delete(library_id)、add_dirs(library_id,paths)、remove_dir(library_id,path)、scan_status/scrape_status(library_id)、rescan(library_id,paths非空)、rescan_result(task_id)、scrape(library_id)、scrape_result(task_id)。每个 library_id 先与当前账号分类列表顶层 id 比对，不能用嵌套成员ID冒充分类。create 不传 share_users，不创建跨账号分享。异步响应 completed:false，需要后续查询确认。删除分类/移除目录仅删除影视记录和关联，原文件保留。create 返回 result 为当前分类列表，按准确名称定位本次新ID；不盲目重试。

真机独立空分类、skip_scrape:true、空目录，验证读取、创建/重命名、扫描/刮削状态、目录新增、扫描任务终态2、目录引用移除、分类删除并核验不存在以及夹具回收通过。未操作用户现有分类或执行真实媒体刮削。字幕/音轨、元数据编辑、播放记录、直播与审阅等完整影视工作流仍待补齐。依据 zvideo_v2/routers/api/v2/classification.go/task.go、models/classification.go 与 xnas-pc video.js。

## 音乐管理

`media music`：playlists、playlist_info(playlist_id)、playlist_create(name)、playlist_rename(playlist_id,name)、playlist_delete(playlist_ids)、playlist_songs(playlist_id,page:1,page_size:100)、playlist_add(playlist_id,kind:song/file/album/artist,values)、playlist_remove(playlist_id,kind:song,values)、dirs、dirs_add/dirs_refresh/dirs_remove(paths)、task_status、task_cancel(all_current:true)、services。按实际 GET/POST 编码；创建歌单使用 playlist/add，不能把客户端名 createPlayList 的 play/list 播放入口当成新建歌单。删除歌单先确认每个准确ID，删除关联与记录，不删除原音频；乐库目录移除可能同时清理相关索引和收藏/记录，按 destructive 处理。task_cancel 是当前账号全部添加与刷新任务，不能伪装成按 ID 取消。

真机独立空歌单与目录生命周期通过。独立一秒静音 WAV 上传、索引、歌单目录关联、按歌曲 ID 移除关联及原音频保留、乐库目录移除、歌单和夹具清理通过，详见 acceptance JSON。当前服务端按目录移除歌单关联返回500，源码 playlist_file.go:145 删除条件缺少 playlist_id，:164 SQL末尾多余AND；CLI明确拒绝 playlist_remove kind:file，没有悄悄扩大操作或以删歌曲冒充解绑目录。服务端源码未改、未部署。歌词/记录/收藏/Subsonic、有声读物、阅读与漫画已继续补齐，见下文；播放资源/设备、数据库迁移重建等仍待开发。

## 2026-10-16 音乐补全

`media music` 新增 song_info(song_id)、lyrics(song_id,allow_scrape:false)、lyrics_search(title,artist,album)、lyrics_scrape(song_id)、lyrics_save(song_id,lyrics)、lyrics_embed(song_id,lyrics,confirm_audio_write:true)、lyrics_auto(song_id,enabled)、settings、lyrics_global(enabled)。默认歌词读取明确 scrape=0；allow_scrape:true 可能联网匹配，按 write 要求 --yes。lyrics_save 保存服务端歌词缓存并恢复该歌曲默认匹配策略，空字符串清空缓存；lyrics_embed 写入原音频标签，需要明确确认。

`tags_set(song_id,tags,confirm_audio_write:true)` 为字段补丁，支持 song_title/artist/album/album_artist/song_genre/song_date/song_disc/song_track/song_lyrics；提交前读取准确歌曲，逐一填回未指定原值，不清空其他标签；不支持的字段直接拒绝，不隐式改封面。

favorites(kind:song/album/artist/artist_id/playlist,page:1,page_size:100)、favorite_add/delete(kind,ids非空唯一)；history(kind:song/album/playlist,page,page_size)、history_latest(playlist_id或album_id可选)、history_add(song_id,progress_ms,playlist_id或album_id可选)、history_delete(kind,ids非空)、history_clear(kind,all:true)。播放进度单位毫秒，未提供上下文不会自动写入其他歌单记录；没有运行全量清空。

subsonic 为只读；subsonic_set(username可选,enabled,password_env可选) 通过环境变量传入密码。首次配置需要用户名及密码环境变量；先读取当前服务能力，读取失败不提交写入。当前没有账号删除接口；经用户授权，真机创建、启用、禁用及最终 enabled=0 回读通过，账号资料保留于仓库外私有文件。用户名为3..20位字母开头的字母/数字/下划线；密码为8..16位，字母/数字/允许符号中至少两类。

独立一秒静音 WAV 验证歌词缓存保存/清空、内嵌写入及本地回读、标题补丁保留艺术家和歌词、收藏增删、500毫秒播放记录回读及精确删除通过。账号设置/历史/收藏、未配置 Subsonic 读取通过。未修改全局歌词偏好、未执行真实联网歌词刮削；全部测试歌单、乐库登记和NAS夹具已清理。

## 有声读物

`media audiobooks`：list/info/create/rename/delete（name、playlist_id）；dirs_add/dirs_delete(playlist_id,paths非空)；tracks(playlist_id,page:1,page_size:100,descending:false)、random(playlist_id)、tracks_delete(playlist_id,track_ids非空唯一)、track_info(track_id)、cue(track_id或path二选一，path是对应音频文件路径)、position(playlist_id,track_id,descending:false)、external_status/add_status/refresh_status/refresh(playlist_id)、history_latest(playlist_id)、history_add(playlist_id,track_id,progress_ms:0)。CUE 使用当前后端 /track/parse/cue 而非旧客户端 /song/parse/cue；历史通过 POST 表单而非 query 提交，排序字段保留后端拼写 sort_filed。

安装状态先通过应用商店查询，未安装返回 E_APP_NOT_INSTALLED；变更前校验准确播单，拒绝删除默认播单。导入与刷新返回 submitted/completed:false，需要查询任务终态。目录和音频移除仅解除本播单的索引/关联，不删除原音频。真机独立 WAV+CUE 完成播单生命周期、目录导入、位置/随机/外挂状态、CUE解析、500毫秒记录回读、刷新、指定音频移除保留原音频、目录和播单清理通过。


## 阅读（2026-10-16）

`media reader`：categories/category_info/category_create/category_rename/category_delete/category_sort；files_add/files_delete(category_id,paths)、manage_categories/manage_files；books(category_id可选,page:1,page_size:100)、search(keyword)、shelf/recent；shelf_add/shelf_delete/books_delete(books:[{category_id,book_id}])；shelf_file(path)、record(path)、folder/folder_info/folder_remove(category_id,path)；read_set(book,patch)、read_status(book,status整数)、read_clear(book)、pin(book,enabled)、book_update(book,title,words,chapters)、bookmarks/bookmark_add(book,bookmark)/bookmark_delete(book,bookmark_ids)；add_status/refresh_status/refresh(category_id,paths)/cancel(category_id)。

read_set 支持 read_chapter/read_chapter_id/read_page/read_progress/read_finished(0/1)/text_position/font_size/read_progress_ext；未指定字段从当前准确书籍补回，不隐式清空进度。read_clear 只将后端 have_read 置0，非清空所有进度。批量书籍引用以表单 data JSON字符串编码。任务0无任务、1等待、2关联中、3扫描结束、4关联完成；取消也可能显示4，应结合内容验证。

convert(path,local_path,confirm_replace_conversion:true,overwrite:false) 仅支持 MOBI/AZW3 -> EPUB，会取消同账号旧转换，按 destructive 处理；下载受 local-root 限定及16MiB上限，普通EPUB使用 file download。真机34项EPUB分类/导入/书架/进度补丁保留/书签/元数据/下载内容/刷新/移除保留原文及清理通过；另6项自有 legacy MOBI转换/EPUB正文验证/原文保留/目录及精确缓存清理通过。AZW3尚未独立验收。

用户授权修复验收机 Calibre 缺库后，从设备既有软件源下载 libopengl0 1.7.0-1build1，提取 libOpenGL.so.0.0.0 和其符号链接至 `/tmp/zfsv3/sata11/internal/cache/zreader/calibre-6.6.1-x86_64/lib/`；未安装全局系统包。文件SHA256为9a0a6024499300f918ef1b42d581427cdb20bbc17a7d8239a4b7434833a98d4a。回退仅移除上述本次新增文件/链接；应用更新可能覆盖此修复。该裁剪包缺QtSvg，EPUB生成MOBI夹具不可用，MOBI转EPUB已通过；未以此宣称所有Calibre格式均可用。

## 漫画（2026-10-16）

`media comics`：categories/category_create(name)/category_rename(category_id,name)/category_delete(category_id)/category_sort(category_ids全部分类顺序)；dirs(category_id可选)/dirs_add(category_id,paths)/dirs_delete(dir_ids)；list(category_id/dir_id可选,shelf_only/edited_only/progressed_only/without_masked:false,start:0,limit:100且最大1000)；info(path,dir_id可选)、infos(comics:[{dir_id,path}])、search(name,start,limit)；create(dir_id,path,name,cover可选,on_shelf:false,folder_as_comic:false)、edit(comic_id,name/cover/folder_as_comic可选，至少一项)、shelf(comic_ids,enabled)、progress(comic_id,progress非空原生字符串)、progress_clear/delete(comic_ids)。edit 使用 `_` 显式清除名称或封面，省略值保留后端原值。删除记录/目录/分类不删除源图片。

notebooks/notebook_create(title,secret_env可选,path可选)/notebook_edit(note_id,title/secret_env/path至少一项)/notebooks_delete(note_ids)。密码只接受环境引用，列表以secret_configured标志代替密码；路径单独修改会补回原笔记以满足后端校验。后端不支持空值清除这些字段，需删除笔记重建。

flush_info/flush(dir_ids非空或all:true二选一)/flush_cancel(all_current:true)/flush_clean(all_current:true)。无效dir_ids在后端可能退回全量扫描，CLI先核验全部目录ID归属再提交；分类排序必须包含全部当前分类以免后端重置遗漏分类。提交不代表完成，查询state及details.result。真机30项独立PNG目录及漫画创建/批量详情/搜索/编辑/书架/进度清除/密码笔记保留与脱敏/指定目录扫描成功/记录删除保留源图及全部夹具清理通过；没有取消用户已有任务、调整已有分类排序。


## 虚拟机管理（2026-10-16）

`ops vm-manage`：environment/service/host/limits/pools/list/usb/pci/sriov；info/base_info/disks/pause_reason/network/timers/vnc_enabled/snapshots/snapshot_state(name)。读取空列表统一为空数组。service_start(confirm_service_restart:true)实际重启虚拟机服务；service_stop(all_vms:true)停用全局服务，均destructive，未真机执行。

create(title,pool,cpu_count,memory_gib,os_type:win/linux/other/qemu-img,firmware:bios/efi,machine:q35/i440fx,disks:[{capacity_gib,bus:ide/sata/scsi/virtio,index,import_path可选}],networks:[{mode:nat/bridge,hub,driver:virtio/e1000,mac可选}],iso/virtio_iso/vnc_port可选,start:false)。pool必须是pools实际mountpoint；磁盘index非空唯一且含1；创建不自动启动。backend生成name，提交后按唯一title查list核对，不得盲目重试。name不是显示title。

update(name,patch)支持title/cpu_count/memory_gib/iso/virtio_iso/start_policy(字符串1保留上次状态/2开机启动/3不处理)/video_type。先读准确详情，省略字段填回原值，保留VNC端口、USB和显卡配置；镜像显式空字符串卸载。state(name,state:start/shutdown/suspend/resume/reboot/destroy/save/restore)，destroy需confirm_force:true。所有写入先核验当前主账号、虚拟机及磁盘/快照归属。

delete(name,confirm_disk_delete:true)会永久删除磁盘和快照。后端删除最后虚拟机会额外清理全部池的虚拟机专用目录，因此CLI在最后一台时还要求confirm_last_vm_cleanup:true；必须先核对历史遗留磁盘。disk_add(name,capacity_gib,bus,index,import_path可选)、disk_delete(name,target,confirm_disk_delete:true)、disk_resize(name,target,capacity_gib,index)、disk_modify(name,target,capacity_gib/bus可选至少一项)。target来自disks.list[].name，校验index，容量只允许扩容。

snapshot_create(name,remark)/snapshot_delete(name,snapshot)/snapshot_revert(name,snapshot,confirm_restore:true)；clone(name,title,start:false,reset_mac:true)使用后端true/false和yes/no分别编码；migrate(name,pool,confirm_move:true)；export(name,path)导出到NAS目录而非本地下载；vnc_password(name,password_env)/vnc_disable(name,confirm_no_password:true)/vnc_port(name,port10000..65535)。VNC密码1..8字节可打印ASCII。写入均返回submitted/completed:false，应结合info.domainTaskState=0、errorCode与实际内容验证。

真机9项环境资源/服务/硬件只读通过；原虚拟机为空且全部池专用VM目录不存在时，独立1CPU/1GiB内存/1GiB稀疏磁盘完成41项：标题补丁保留其余字段、网络读取、VNC配置、第二盘新增/扩容/删除、快照创建回滚删除、克隆停止状态及删除、原生导出、运行/暂停/恢复/强制停止状态回读、导出目录回收及最后测试机清理。没有安装客体系统，没有操作现有虚拟机。NAT网络配置/移除、禁用定时配置、原生及OVF导出/解析/导入均已补验；迁移、磁盘bus变更、PCI/USB/GPU写入与控制台仍待完善或验收。


## iSCSI 管理（2026-10-16）

`ops iscsi-manage`：info/sessions/pools/settings/luns/targets/groups/lun_preinfo/target_preinfo/group_preinfo，lun_info/target_info/group_info(uuid)。GET写入仍按write/destructive要求--yes，不能因HTTP方法判定只读。写入先核验当前主账号、准确UUID、关联对象与可用存储池。

lun_create(name,pool,capacity_gib,sparse:true,description:"",target_ids:[],groups:[{uuid,mode:deny/read_write/read_only}],allow_all:false)默认自定义空访问集合、不自动创建Target。lun_update(uuid,name/description/capacity_gib至少一项)读取原配置保留映射/访问组，容量只允许扩容；lun_map(uuid,target_ids,groups,allow_all:false,confirm_replace:true)显式完整替换映射。lun_delete(uuid,confirm_data_delete:true)永久删除LUN数据，准确携带原mountpoint，不删除关联Target。

target_create(name,description:"",enabled:false,lun_ids:[])采用预配置WWN与默认协议设置；target_map(uuid,lun_ids,confirm_replace:true)，空数组明确解除全部本Target映射，不删除LUN；target_rename(uuid,name,description可选)保留认证/映射/协议配置；target_enable/target_disable/target_delete(uuid)，删除Target明确lundelete=no，保留LUN。

group_create(name,system,initiators:[IQN/EUI],description:"")；group_update(uuid,name/description/system/initiators可选至少一项,confirm_replace:true)保留省略字段；group_delete(uuid)。组变更会调整发起端访问，不对原数据进行格式化或挂载。

target_configure(uuid,config,password_env/mutual_password_env可选)配置补丁支持wwn、global_chap/enable_chap/enable_mutual_chap/enable_head_crc/enable_data_crc/multi_session布尔、userid/mutual_userid、max_recv_datasegment_length/max_xmit_datasegment_length(4096/8192/65536/262144)、max_connections。密码只从环境读取，CHAP用户名1..12位字母数字、密码12..16位字母数字；未提供配置及密码填回原值，响应脱敏。

settings_set(config,confirm_global:true,password_env/mutual_password_env可选)支持target_port(1024..65535)、io_queue_length(1/16/64/128)、target_prefix_wwn、enable_chap/enable_mutual_chap、userid/mutual_userid；settings_reset(confirm_global:true)恢复全局默认，可能影响全部Target连接。均destructive，未在现网执行。

真机10项只读通过，19项独立对象验收通过：组创建/详情/重命名保留IQN，禁用Target创建，自定义无映射1GiB稀疏LUN创建/改名/扩2GiB/只读测试组映射，独立CHAP环境密码与脱敏回读、Target改名保留认证和映射、启停、解除/重建映射、删除Target保留LUN，以及LUN数据/组删除与列表不存在核验。未从真实initiator登录或格式化LUN，没有修改用户全局配置；LUN迁移、克隆/导入与更多协议选项仍需继续扩展。


虚拟机后续新增 network_set(name,networks,confirm_replace:true)：停机后完整替换客体网卡，修改时每张卡必须明确mac；空数组明确移除全部客体网卡，不创建/改变宿主机桥接。timer_set(name,operation:start/shutdown,plan:once/weekly/monthly,days:[],hour,minute,force_shutdown:false,wait_seconds:0,confirm_replace:true)保留原启用状态，新计划默认禁用；weekly为0..6，monthly为1..31，once不传days，对应后端dayly但实际为单次。timer_enable(name,operation,enabled)单独启停该动作计划。

import_check(path)校验原生导出索引；import(title,pool,path,confirm_reset_devices:true)仅接受.index，以停止状态导入，后端重置外接设备/网卡，导出会清除VNC密码，启动前需核对端口及密码。ovf_export(name,path,include_iso:false)、ovf_parse(path,virtual_system_id可选)、ovf_import(path,virtual_system_id,verify_manifest必填布尔,title,pool,cpu_count,memory_gib,os_type,firmware,machine,disks:[{disk_id,capacity_gib,bus,index}],networks:[])；OVF/OVA导入最多26盘，先解析核验准确系统及全部disk_id/源虚拟容量，目标不得缩小；verify_manifest=true时要求manifest存在。OVF网络编码使用netMode[]/netHub[]/netDriver[]/netMacAddr[]，不沿用普通创建的net[]。

## 病毒查杀（2026-10-16）

`ops virus`：info/settings/database/engine/realtime/data_info/pools/quarantine/quarantine_detail/schedules/schedule/summary；records(page:1,page_size:100,statuses:[],kinds:[])、record(record_id)、files(record_id或realtime:true二选一,page,page_size,virus_names:[],statuses:[],file_ids:[])、quarantine_files(page,page_size,virus_names:[])、whitelist(page,page_size)。记录和文件过滤通过POST JSON，分页通过query，均read。record从当前可见记录中匹配UUID，避开当前源码/scanrecord/info的非空UUID反向校验缺陷。

scan(paths非空,file_types:[],default_action:none)仅扫描明确目录；scan_all(kind:full/sys,all_configured:true,default_action:none)明确全范围。file_types为video/image/audio/doc/compress/other，空数组全部；默认只报告，不采用用户可能设为隔离的默认策略，quarantine显式选择按destructive。cancel/suspend/resume/finish(record_id)先核验info中当前UUID；后端暂停/恢复/取消不接受ID，CLI预检仍不是服务端原子条件。

records_delete(record_ids非空唯一)拒绝空数组，避免后端解释为全清。files_action(record_id或realtime:true,file_ids非空,operation:delete/recover/trust/quarantine,confirm_file_change:true)读取范围内全部分页并核验文件ID。quarantine_action(files:[{quarantine_id,file_id}],operation:delete/recover/trust,confirm_file_change:true)核验完整隔离区/文件ID对；旧默认隔离区可能空ID，也必须与列表准确匹配。删除永久移除文件；trust会调整信任名单，未对用户威胁文件执行。

whitelist_add(paths)接受NAS逻辑路径；whitelist_delete(paths)必须使用whitelist返回的准确服务端路径，并核验归属。whitelist_clean_invalid(all_users:true)按后端真实语义影响所有用户，需主账号；未执行全局清理。

schedule_set(kind:custom/full/sys,paths:[],file_types:[],default_action:none,plan:daily/weekly/monthly,days:[],hour,minute,enabled:false,confirm_replace:true,all_configured:false)完整替换当前账号唯一计划，full/sys须all_configured；weekly0..6、monthly1..31、daily不传days。schedule_enable/schedule_disable(schedule_ids非空)核验准确计划ID。后端没有注册计划删除路由，因此未创建无法无痕撤销的新测试计划，计划写入仅本地验证。

realtime_set(enabled,confirm_global:true)、database_update/upload(paths)/confirm(confirm_global:true)、data_set(pool,confirm_reconfigure:true)、quarantine_master(pool,confirm_reconfigure:true)需主账号；data_set需匹配pools逻辑mount_point。settings_set(config,confirm_settings:true)为局部补丁，支持notify_when_find_virus_file/notify_when_scan_finish布尔、default_action:none/quarantine、default_scan_filetype数组、scanrecord_keep:no_limit/six_hours/one_month/three_month/six_month/one_year/three_years。six_hours对应后端one_day，其实现实际为6小时，不能称作一天。主账号的设置可能作用于全局，未改变通知/保留/实时防护设置。

用户明确同意初始化并保留病毒库，进一步确认使用应用实际暴露的逻辑池sata11（/tmp/zfsv3/sata11）。初始化自动下载期间另一次更新返回N005001，现映射E_VIRUS_BUSY，提示查询info而不重复启动；最终数据库ready且实时防护no。15项状态读取、5项独立TXT白名单生命周期及16项普通文本/EICAR报告、隔离、恢复、精确永久清理/扫描记录清理与目录回收通过。EICAR来自服务源码pkg/clamd/clamd.go，与[官方非恶意测试文件说明](https://www.eicar.org/download-anti-malware-testfile/)一致；不使用真实恶意程序。库按授权保留，未处理用户现有隔离文件。应用业务验收与当前客户端安全整改分别记录，参见 release-0.2.3-rc.2.md。

## 防火墙（2026-10-16）

`ops firewall --params '{"action":"info"}'`，对应 firewall_management。info/interfaces/countries/services 读取配置、可设网卡、国家代码和服务；precheck(rules)以POST JSON执行只读预检。规则沿用服务端字段，包括拼写 desctiption；必须显式提供protocol(all/tcp/udp/icmp/igmp)、ip_version(all/ipv4/ipv6)、action(allow/deny)、is_active/is_new布尔与priority整数。

apply(rules,interfaces:[{name,default_action:allow/deny}],expected_info,confirm_replace:true)完整替换规则并更新指定网卡默认策略；expected_info必须是最近info完整原值。主账号检查、网卡可配置检查、预检、二次配置比较均通过才写入；201客户端封锁警告作为错误阻断，不提供绕过开关。配置比较不能提供服务端原子锁，其他管理员仍可能在最后一次检查后修改。预检仅检查规则，不能保证网卡默认deny或其他策略变更不会断开连接。提交超时后先查询状态，不能盲目重试。

enable(enabled,expected_info,confirm_global:true)切换全局状态，启用前预检当前规则；同状态返回unchanged。reinsert(confirm_global:true)、services_sync(service或all_services:true,confirm_global:true)、countries_update(confirm_global:true)分别重插链、同步服务白名单和更新地区数据库，均需要主账号并按破坏性动作处理，即使后端使用GET。未暴露内部日志/配置重载接口、未注册的策略占位接口与有参数校验缺陷的单国家查询。

真机5项只读与空规则预检通过，设备support=yes、enable=no、已有1条规则；未修改用户防火墙。HTTP模拟验证配置变化及预检201均不产生apply请求。规则写入、启停与全局维护尚未真机验收。

## Time Machine（2026-10-16）

`backup time-machine`（time_machine）：status读取当前OAuth账号备份信息；service/limit由同一现代status响应提取状态与limit_size/size，避开当前固件不支持的旧/settings。configure(pool,limit_gib,expected_status,confirm_reconfigure:true)要求V3逻辑池、当前status完整匹配，目标固定到当前账号`/tmp/zfsv3/<pool>/TimeMachine/<username>/`，0表示无限制。变更池时后端update=true，可能影响既有备份；不自动改双备份策略。delete(path,confirm_data_delete:true)仅接受当前账号status.dir中精确匹配路径，不能删除账号根目录或其他账号。现代接口以form type=7和data JSON字符串发送。service_set(enabled,confirm_global:true)为客户端仍保留的旧/tm/open或/tm/close全局服务动作，主账号限定，当前固件未真机验证；不虚构现代enable接口。

3项只读真机通过；当前已有配置、没有备份，未改配额/存储或开关。实际Mac备份与精确备份删除待Mac配合；HTTP模拟验证未列出的路径不会发起删除。

## USB盘模式（2026-10-16）

`storage usb-mode`（storage_usb_mode）：status；create(pid,gid,size_gib,filesystem,expected_status,confirm_create:true)；expand(additional_gib,expected_status,confirm_expand:true)；enable/disable(expected_status,confirm_device_change:true)；cancel_error(expected_status,confirm_device_change:true)；reset(expected_status,confirm_data_delete:true)；mobile_set(enabled,expected_status,confirm_device_change:true)。每次变更需主账号、完整status比较与--yes；创建仅允许none状态及现存pid/gid、support_fs格式。扩容只允许NAS端正常on的NTFS，additional_gib为新增容量，不是最终总量；不提供缩容。disable默认不重启，reboot:true还需confirm_reboot:true，客户端不会自动发重启命令，后端仅设置下次重启后关闭。reset永久移除镜像，拒绝电脑模式和运行中的创建/扩容；设备二级验证要求由NAS正常执行，不绕过。

9项真机生命周期通过：初始none、1GiB NTFS创建/回读、增量1GiB扩容/容量核对、无重启关闭、NAS端开启、精确自有镜像重置及恢复none。未测试真实电脑连接、手机模式、失败恢复与重启分支；不能用镜像测试宣称USB硬件传输已验收。

## 外接NVIDIA GPU（2026-10-16）

`ops gpu`（gpu_management）：info；tasks(process_name:zllamas/zaispace)；unload(all_gpus:true,expected_info,confirm_eject:true)。xnas-pc注释声称有gpu_id，但实际页面调用unloadGpu()不带参数，缺少后端源码证明单GPU定位，因此CLI不提供虚假的单卡弹出能力；采用显式全局确认、主账号与最新非空GPU列表一致性检查。三项真机读取接口均返回错误，记录为接口不可用，未伪装成空列表或验收通过；未进行弹出。

## 本地AI模型与生成（2026-10-16）

`media ai`（ai_models）对应deepseekmgr与xnas-pc DeepSeek：disks/models/state/download_state；download(mountpoint,code,expected_state,confirm_replace_all_models:true)校验设备目录、精确mp与至少两倍模型空间。注意服务端下载新模型会停止引擎并删除全部旧模型文件，绝非普通追加下载。cancel_download(expected_download,confirm_cancel:true)比较任务信息（忽略持续递增的progress）；start(code,device:cpu/gpu,expected_state,confirm_engine_change:true)仅接受完整安装模型；stop(expected_state,confirm_engine_change:true)全局停止；clean(code,confirm_model_delete:true)要求引擎已停止且目标已安装。后端全部管理路由均需主账号，CLI同样检查。内部GET /llama/clean实际映射stop，未作为“清理全部模型”暴露。

chat(messages:[{role:system/user/assistant,content}])要求引擎running及已核对的url_prefix，以stream:true调用生成接口，收齐SSE后输出content/reasoning_content/usage/completed的JSON；当前不逐字打印、不保存历史，也不向外部AI上报聊天。事件流必须收到[DONE]，中断不会伪装完整成功；最多64条消息、正文总1MiB、响应解析8MiB。可按模型耗时增大CLI全局timeout；超时不自动重发生成。

4项真机只读通过，当前4模型均未安装、引擎stop；尚未下载/启动或真实生成。已询问用户选择下载验收后清理或保留；等待期间继续其他功能。SSE Unicode/usage/中断/引擎错误本地测试通过。AI空间zaispace、知识库与检索尚未由此覆盖。

## NAS浏览器（2026-10-16）

`app browser`（browser_management）：open保留设备app_open业务error和元数据（0就绪、3未安装）；info/state仅用于已安装实例，未安装时后端报错不能冒充空成功；check_port(port)；download按设备元数据file下载，进度/取消沿用docker images download_tasks/cancel_download。install(username,password_env,host_port,nas_path,chrome_sleep:false,confirm_install:true)要求未安装、端口可用和已完成镜像；从镜像Config继承启动默认值，替换KASM_USER_NAME/VNC_PW，Chrome可配置空闲暂停60秒；挂载设备指定唯一目录、桥接网络、1GiB共享内存，不自动启动。密码通过环境变量引用，元数据和响应中的env统一脱敏。NAS路径支持逻辑my/public转换；安装服务会chmod绑定目录，应选专用目录。start/stop；remove(expected_container_id,confirm_remove:true,retain_image:true)默认保留镜像，仅允许精确当前容器。

14项独立真机验收通过（卸载请求retain_image:false后NAS仍保留镜像，补用docker images remove普通精确删除并核验，未force）：镜像下载导入后安装、身份回读、启停和重启、open就绪、卸载与未安装恢复、测试目录清理、镜像不存在及本次下载任务清理。密码未写入参数或报告；未修改已有浏览器，因为起始未安装。浏览器高级代理/GPU/资源/网络/容器配置及浏览器内容操作尚未由此覆盖；无GUI自动化或代用户访问网页。

## 家庭/宝宝/纪念册、成长记录与评论（2026-10-16）

以下复用`album manage`，不新增命名工具：

- create_shared(kind:family/baby/memorial,config,feed_ids可选)。config为完整对象：name、description、permission:read_only/read_write、members:[{username,role可选}]、allow_sharing、hide_timeline；宝宝还需owner_role、gender:M/F、birthday:YYYY-MM-DD。空members明确只有当前账号；非空成员先查真实账号，不对未知账号静默跳过。
- configure_shared(album_id,config,expected_info,confirm_replace_members:true)替换全体成员和基本设置，expected_info为最近完整info。必须完整带回原成员；服务端会移除省略的成员。普通/备份/家庭/宝宝/纪念册可用；AI系统类型拒绝。后端不接受空desc清空，CLI拒绝这种伪成功。
- quit(album_id,confirm_leave:true)成员退出，所有者拒绝；roles/role_add(role)/role_delete(role_id)管理当前账号自定义关系。
- baby_timeline(album_id,milestones_only:false)；baby_growth_info(album_id,line_date)；baby_growth_save(album_id,line_date,description,location:{latitude,longitude}或null,visible_user_ids可选,confirm_replace:true)。line_date为YYYYMMDD，生日为YYYY-MM-DD，两者不可混用。save完整替换描述/位置，null或省略location清位置；省略visible_user_ids保持原可见范围，提供列表仅允许相册所有者、列表需精确当前相册成员ID。描述最多500字符。
- baby_growth_like(album_id,line_date,liked)、baby_growth_milestone(album_id,line_date,enabled)、baby_growth_delete(album_id,line_date,confirm_metadata_delete:true)。删除区块不删除原照片；仍有评论时先清评论。服务端无照片空区块时间线省略likeBit，点赞通过详情位图核验，未伪造时间线点赞人数。
- posting_list(album_id,start:0,num:100)、posting_info(album_id,posting_id)、posting_create(album_id,description,published_at,feed_ids或paths)、posting_replace(album_id,posting_id,description,published_at,feed_ids,paths可选,expected_info,confirm_replace:true)、posting_like(album_id,posting_id,liked)、posting_delete(album_id,posting_ids,confirm_metadata_delete:true)。published_at为正Unix秒；描述500字符、图片/视频合计99个。首条内容需要图片/视频，后续可纯文字；替换保留帖子身份、点赞与评论。精确核对posting_info.album_id，拒绝错父相册。feed_ids需已在指定纪念册内（先add_feeds/import），paths需在当前已登记相册来源目录内。后端路由拼写为posting/modity。
- comments_list(target,start:0,num:100)、comment_create(target,content,reply_to_id可选)、comment_delete(target,comment_id,confirm_delete:true)。target为{kind:posting,album_id,posting_id}或{kind:baby,album_id,line_date}；宝宝成长区块应先save创建。评论最多50字符；回复目标先在该内容分页列表精确查找，作者ID由服务器结果填入，不能伪造回复接收账号；删除同样限定精确父内容。评论删除会按服务端实现连带删除该评论的后续回复。

服务端直接删除成长记录/纪念内容或父相册不会完整清理独立评论/成长元数据。CLI拒绝删除仍有评论的内容，以及仍有成长区块/纪念内容的父相册，要求依次先清评论、再清内容、最后删除相册。

真机证据：三类相册及自定义关系18项；成长记录及评论回复18项；纪念图文及评论回复20项，均仅当前账号，不邀请其他成员。已验证完整替换、点赞/取消、大事记、可见性保留、精确删除、原图保留与自有目录清理；当前账号间分享/退出及真正多成员权限尚未验收。两次早期中断测试因后端父删除未级联而遗留2条孤立评论和2条成长行，已按精确测试身份/分类/行ID/内容/时间核对，私有备份后清理；未修改服务端业务代码。来源目录删除受后台扫描限制，等待扫描完成后清理，没有取消用户扫描。

人物/地理与全部元数据、自动备份/共享访客、数据库和导出等相册业务仍待继续。

## 相册数据库与导出（2026-10-16）

`album manage`新增db_status/db_size/db_backup_info/db_upgrade_progress/db_migrate_progress。db_backup_set(frequency_days/retain_num可选，至少一项,expected_info,confirm_global:true)仅提交所选补丁，允许值以设备supportedFrequencyDays/supportedRetainNums为准；比较expected_info的frequencyDays/retainNum，保留省略字段。db_upgrade(save_path,expected_status,confirm_database_change:true)仅SQLite转PG；db_migrate同参数仅PG迁移。两者需主账号、当前状态一致、无运行升级/迁移和扫描、设备实际物理挂载根目录；不将“升级”误做普通刷新。

数据库真机8项通过：5项只读；备份频率短暂设为30天、核对保留份数5未变、恢复原2天并回读。当前数据库为PG，未做真实升级/迁移。

export_create(album_id,destination,layout:flat/month,include_descriptions:false,liked_only:false,extensions:[]可选image/video/gif,user_ids:[],after_date可选YYYYMMDD,confirm_replace_tasks:true)。导出到已存在NAS目录内的相册子目录；平铺或按拍摄月份保存。后端新任务会确认同相册旧完成任务，CLI要求显式确认且拒绝相册已有运行任务。宝宝可导出成长描述；纪念册后端忽略点赞/媒体/用户过滤，CLI拒绝这些伪过滤请求。相同文件按大小和修改时间跳过，冲突文件自动改名，不覆盖已有不同文件。

export_tasks(album_id)列未确认导出；export_task/export_errors/export_retry/export_ack均需album_id与task_id，按当前账号及父相册精确核对。retry仅失败且已退出工作容器；ack另需confirm_clear_result:true，仅成功/失败且已结束任务，清除结果展示而非取消执行，底层正常保留confirmed任务历史。后端没有注册导出取消接口，CLI不编造该入口。

17项真机导出验收通过：自有图片导入、导出任务/错误读取、同名哨兵保持内容、新文件改名后字节比对、完成确认、再次导出跳过相同文件、任务确认及自有相册/来源/输入输出清理。HTTP模拟证明运行任务不会提交确认请求。月份分组、宝宝描述文本导出和失败重试尚未单独真机验收。

## 相册地点、属性与原文件元数据（2026-10-16）

复用 `album manage`，不增加命名工具。新增 `position(latitude,longitude)`、`positions(locations:[{latitude,longitude}])`，有限坐标、批量1..500项，分别映射 `/v2/album/position` 与 `positionList` 的 `lat/lng` 与数组字段。

`attributes(album_id,expected_info,hidden?,cover_feed_id?,person_role?)` 至少一项属性；完整 `info` 比较防止基于旧信息修改。人物关系仅type100，值0/10/20/30/40；封面必须正ID且由服务端核对属于该相册。仅提交所选字段，不提交成员/目录列表。`timeline_visible(album_ids,visible)` 批量相册时间线显隐，逐一info预检。

真机9项通过：单点/批量地点查询、自建图片导入、隐藏/恢复核验、封面ID核验及来源/相册/目录清理；另独立空相册时间线隐藏和恢复，`feed_hide` 实际1/0，随后清理。未修改用户现有相册、人物关系或封面。

`exif_info(path)`、`exif_shared(encrypted_path)` 按xnas-pc封装；`exif_set(paths,orientation?,original_datetime?,location?,confirm_original_write:true)` 支持1..500唯一路径。日期/GPS写原图；orientation为NAS展示旋转标记（1/3/6/8），不保证改原始JPEG方向标签。日期为有效`YYYY-MM-DD HH:mm:ss`、经纬度有限。逐一文件权限校验，逐文件统计成功/失败；部分失败不标记全成功。空字符串清除GPS在真机被拒绝且客户端无确认协议，未提供clear_location入口。

**请求兼容已修复：**用户确认网页端EXIF/音轨正常，排查定位到CLI缺少版本与app Cookie；Api仅对`/transcode/`补入`version=2.3.2026101201; app=file`（版本来自xnas-pc生产配置），保留CLI设备标识和其他接口Cookie。独立图片EXIF读取、展示方向mori=6、拍摄日期/GPS读回与清理通过；日期/GPS下载原图内容改变，方向原始JPEG不变。共享EXIF仍待验收。

Time Machine：用户确认没有专用Mac，实际备份与删除待验收。本地AI已按授权完成ds1d5 CPU真实生成与停止/模型清理，7项通过。

### 跨相册图片复制与动态引用

`copy_album(album_id,source_album_id,expected_info,expected_source_info,confirm_copy_all_photos:true)` 从来源复制图片关联到目标，保留原图及来源，不复制成长记录/评论。目标类型10/40/50/60；来源10/40/100/110/120/130/150/160。动态 `link_album(...,confirm_follow_source:true)`、`unlink_album(...,confirm_unlink:true)` 来源限系统40及上述AI分类，分别调用 `feed/add` 的 `monit=1` 与 `unbind`。不能把普通相册当作动态来源。每次比较两份info，禁止同源同目标；非事务比较，不保证并发扫描期间完全一致。

独立相册复制10项通过，目标同feed ID，来源保留、删除目标后原图仍在；夹具已清理。动态关联/解除关联协议模拟通过，未对用户已有系统或AI相册做真机关联。

### 相册外部分享管理

仍经 `album manage`：`shares(start=0,num=100)`；`share_create(album_id,expected_info,expires_in_seconds,max_visits,require_passcode,confirm_dynamic_share:true)` 为整相册动态分享，**后续加入的图片也可能被访问**，有效期60..2592000秒，次数1..10000。提取码由服务器生成，结果包含分享能力信息，应仅交付预期接收方。此入口不自动发送消息或打开访客链接，不处理纪念图文专用分享。

`share_stop(share_ids,confirm_stop:true)` 将次数置零且无法原地恢复；固件列表又因次数用尽把状态5改为3（过期），应结合remain=0判断。`share_delete(share_ids,confirm_delete:true)` 只删指定分享记录，不删原图。先分页确认ID属于当前账号；拒绝空ID和未知ID，避免服务端空列表等于全部清理的语义。无全量/过期分享批量清理入口。

### 子相册组织（地点/人物）

`sub_info(scope)`，scope为`{kind:album,album_id}`、`{kind:year,year}`或`{kind:directory,path}`。`sub_configure(scope,expected_info,geo,face,confirm_replace:true)` 的两种维度模式为`off/manual/automatic`；完整替换这两项模式。`sub_status(scope,dimension)`、`sub_list(scope,dimension,include_hidden_people=false)`，dimension为`geo/face`。配置ID均从所选scope解析，避免只传裸ID跨范围读取。

`sub_refresh(scope,dimension,expected_info,confirm_refresh:true)` 手动刷新；`sub_clean(scope,dimension?,expected_info,confirm_remove_generated:true)` 停用并清理指定维度的生成子相册/关联与记录，省略dimension还删除整个配置。保留原图。配置变更/清理前两维任务均不得运行；比较expected_info。目录配置写入/清理实际影响整个登记根目录，其完整范围接口仍待补，因此目前目录scope只读，写入使用相册或年份scope；目录写入不能称为完成。

独立空相册11项通过：原始未配置、手动地点配置、两维状态及列表、地点刷新完成、单维停用/清理、完整配置清理、父相册删除。刷新遇到NAS扫描时等待既有扫描结束，未取消用户扫描。没有真实人脸/地点识别质量证据。

### 相册AI设置与分类管理

`photo_ai_config`、`photo_ai_options`、`photo_ai_clean_status`、`photo_ai_rescene_status` 四项读取通过。无任务时后端N003588规范化为`{active:false,task:null}`，不掩盖其他错误。

`photo_ai_configure(patch,expected_config,confirm_ai_change:true)` 仅提交明确字段，沿用后端遗漏字段保留语义。保存字段与读取字段大小写不同：保存用`ai_search_open/ai_ocr_open/ai_main_open/ai_sr_open/video_open`；另支持`open/scene_open/ai_pet_open/ai_pet_face_open/history_today_open/history_today_person/history_today_group/history_today_location/ai_recommend_open`，开关为整数0/1；数量`show_min_num/show_pet_min_num`；阈值`laplacian_threshold/video_laplacian_threshold/scene_threshold/ai_similarity`。不接收任意KV或RAG配置，不自动下载安装缺失AI组件。保存字段映射及越界/未知字段模拟验证通过，未修改真机现有AI配置。

`photo_ai_progress(types,expected_config,confirm_requeue:true)` 类型为100/110/140/150/1000/1200非空唯一列表。**该查询会在满足条件时TouchFeedForReRun，将旧照片标记待重算，因此风险为write，需要显式确认**。初始缺types预检返回N001212后已依后端源码修正；没有以修正后的接口触碰用户照片。

`photo_ai_rebuild(operation,expected_config,confirm_rebuild_metadata:true)` 的`run/clean/recluster/rescene`均按破坏性元数据操作处理；尤其`run`对应`/ai/run`，先清旧AI相册再运行，不等同普通无副作用唤醒。仅编码，未对用户AI分类运行。

`ai_remove_feeds(album_id,feed_ids,expected_info,confirm_remove_classification:true)` 从人物/场景/宠物分类剔除图片；`person_move_feeds(album_id,to_album_id,feed_ids,expected_info,expected_target_info,confirm_move_classification:true)` 仅人物type100；`person_merge(source_album_ids,to_album_id,expected_sources,expected_target_info,confirm_merge_classifications:true)` 仅同类人物100或宠物面孔160。比较每份info，验证图片确在来源分类，拒绝空/重复/未知/跨类输入。修改分类关联，保留原图。未对现有分类做真机写入，HTTP模拟证明无关图片不会触发剔除。

`feed_infos(paths)` 按1..500个唯一路径查已登记照片元数据；`feed_like(path,expected_feed,liked)` 查询路径唯一对应的照片，完整比较expected_feed再设置当前查看者喜欢状态，不猜裸ID。独立照片6项读取/喜欢/取消及来源、相册、目录清理通过。

### 本地AI真实生成与最终清理

用户2026-10-16选择验收后清理模型。`ds1d5`（1,894,532,192字节）下载到`/data_s001/ai_models`，CPU引擎启动/运行、固定提示`Reply exactly OK.`的真实SSE完整生成、引擎停止、单模型清理及最终全模型未安装共7项通过。生成内容165字符且usage存在，仅用于验证完整生成协议，不以模型严格服从短答案要求作为质量结论。已核验独立模型文件及CPU进程不存在；没有保留权重。

首次验收脚本将state.models字典误当作列表，下载后进入finally清理，未启动引擎。修正为models.list并第二次完整下载后完成上述7项；首次未被算作生成通过。磁盘选择配置和应用自身运行日志可保留，没有删除应用或其他用户数据。该证明覆盖CPU单模型路径；GPU推理、取消下载、生成中断/超时等真实失败流程仍待验收。

## 影视自定义专辑与元数据（2026-10-16）

复用 `media video-libraries`，新增 `collection_info/collection_people/collection_videos(library_id,collection_id)`；先核对当前账号分类及可读专辑ID。详情中的classification_id由服务端按入参回显，不能用它证明专辑只属于该分类；操作对象按当前账号实际collection_id与完整expected_info核对。

`collection_create(library_id,metadata)` 创建空自定义专辑；`collection_replace(library_id,collection_id,metadata,expected_info,confirm_replace_metadata:true)` 是完整替换，**所有metadata字段必须明确提供**，防止后端遗漏字段清空：`title,other_title,regions,languages,company,genres,mins,release_date,release_year,score,season,episode,type,storyline,directors,writers,actors,cover,cover_type,backdrop,backdrop_type`。type为100/200/300；score为0..10；release_year为0或1900..9999；演职员数组逐项有name，发送时转换为后端要求的director/writer/actor JSON字符串表单。海报/背景只接受空或无查询串的`/zvideo_static/`引用，不接受任意NAS文件或网络URL；实际图片上传/下载与常规刮削元数据修正另待补齐。空字符串和空数组在完整替换中表示明确清空。


metadata精确类型如下，21字段全部必填（不能直接复制collection_info充当metadata）：

| 类型 | 字段 |
| --- | --- |
| 字符串 | title、other_title、regions、languages、company、genres、mins、release_date、storyline、cover、backdrop |
| 非负整数 | release_year、season、episode、type、cover_type、backdrop_type |
| 数字 | score |
| 对象数组，每项含name字符串 | directors、writers、actors；不设置时也必须显式传[] |

可直接使用的创建示例（library_id请替换为list返回的分类ID，另加--yes）：

```json
{"action":"collection_create","library_id":"<从list取得>","metadata":{"title":"测试专辑","other_title":"","regions":"美国","languages":"英语","company":"","genres":"喜剧 / 动画","mins":"95","release_date":"","release_year":2024,"score":0,"season":0,"episode":0,"type":100,"storyline":"","directors":[],"writers":[],"actors":[],"cover":"","cover_type":0,"backdrop":"","backdrop_type":0}}
```

regions/languages/genres不是数组，mins不是数字，cover_type/backdrop_type不是字符串。
collection_info不含三个演职员数组；替换时若需保留演职员，先读取collection_people并映射为directors/writers/actors，不能默认为[]。
参数诊断会列出缺失或类型错误的metadata字段，不回显其内容。

`collection_seen(library_id,collection_id,seen,expected_info)` 标记已看/未看；`collection_dismiss(...,expected_info,confirm_dismiss_and_rescrape:true)` 解散自定义专辑，**后端会对其中视频重新刮削**，不是删除视频文件，也不等同只删除空元数据。服务器限制可解散类型，不提供force入口。

真机独立空分类/专辑10项通过：创建、基础元数据、演职员/视频读取、已看/未看核验、标题/简介/评分/年份完整替换、解散和分类删除。没有关联实际视频，没有重刮现有影片；实际媒体关联/海报/字幕/播放及非空解散仍待验收。完整元数据缺字段拒绝、演职员表单编码及原始文件路径不能冒充海报已本地验证。

### 影视专辑路径关联、手动片头片尾与音轨读取

`collection_paths(library_id,collection_id,start=1,num=100)` 的start为从1起的**行偏移**，不是页码；num1..500，空列表归一化为[]。

`collection_add_paths(library_id,collection_id,paths,expected_info,confirm_scan_selected_paths:true)` 先验证路径可访问，再按所选目录/文件扫描后加入专辑；目录包含递归扫描语义。`collection_remove_paths(...,confirm_unmatch:true)` 要求每个路径都在专辑直接路径关联列表，防止传入任意路径影响其他专辑；后端把移出的媒体转成未匹配，保留原片。嵌套目录内单个文件不等于直接目录关联，不能用该入口绕过直接路径检查。

本机FFmpeg生成39,918字节、2秒、两音轨（英/中）和自制字幕MP4，独立NAS目录/分类/专辑的10项上传、关联、资源、移除、原片保留及清理通过。`video_streams(library_id,path)` 在文件权限预检后GET `/transcode/video/streams`；补齐转码版本与app Cookie后真实读出2音轨/1字幕，已核对资源回传file_path与上传路径一致。初始T000001属于已修复的CLI请求兼容问题。

`collection_skip_manual(library_id,collection_id,expected_info,intro_start_seconds,intro_end_seconds,outro_start_from_end_seconds,outro_end_from_end_seconds,confirm_replace_skip:true)` 完整替换当前专辑手动片头/片尾设置。片头从开头计秒、起点<=终点；片尾为距结束的倒数秒、起点>=终点；映射previous/start/end/preview，selected固定1（手动），不冒充selected2的智能识别。新空专辑12项生命周期中，手动0..1秒、结束前1..0秒读取一致，归零及清理通过。尚无播放器实际跳过证明；各视频片长/一半片长约束及智能跳过任务仍待进一步实现/验收。

### 播放格式与外挂字幕

仍复用 `media video-libraries`，`video_format_menu(library_id,path,play_env,video_codecs,audio_codecs)` 与 `video_formats(...,dest_format?)`，play_env为1局域网/2远程，编解码器显式数组（如video_codecs:[h264,mpeg4,hevc]、audio_codecs:[aac,mp4a]），不假定CLI具有浏览器播放能力。读取服务端可用格式列表，不启动播放。

`video_subtitles(library_id,path)` 发现外挂字幕；`video_add_subtitles(library_id,path,subtitle_paths,confirm_attach:true)` 解析/预处理指定1..100个唯一本地NAS字幕文件，验证视频与每个字幕文件均可访问。该GET存在缓存预处理副作用，按write执行且不以read自动重试；不承诺跨播放器会话永久绑定字幕。未提供任意远程URL或全局字幕扫描。

独立短片/SRT8项上传、私有分类、清晰度菜单、可用格式、字幕发现、指定字幕解析及分类/目录清理通过。HTTP模拟验证字幕文件预检失败时不会调用转码接口。实际播放器切换/渲染与远程网络仍待验收。

### 影视播放记录

`history_list(library_id,start=1,num=100)`，start为从1起的行偏移；`history_info(library_id,collection_id)`；`history_set(library_id,collection_id,video_id,position_seconds,expected_history,settings?)` 从所选专辑资源验证video_id和真实片长，位置不能超过取整后的时长，不接受任意外部视频ID/自报片长。settings仅支持chapter/audio_index/subtitle_index/subtitle_hash/subtitle_setting/subtitle_language/subtitle_codex/subtitle_title的字符串补丁；同一视频保留未指定项，换视频默认清空旧轨道选择，显式空字符串可清空。比较当前完整记录再提交。

服务端播放进度异步入库，因此history_set返回submitted:true、completed:false，需轮询history_info核对，不能凭HTTP成功声称写入已完成。`history_delete(library_id,collection_id,expected_history,confirm_delete:true)` 精确删除单个当前账号分类/专辑记录并读回确认；已不存在时幂等返回，不清理其他记录，也无全量清空入口。

包含实际短片资源的独立18项生命周期通过：等待自建分类扫描/刮削空闲、文件关联、真实音轨字幕、初始无记录、1秒及0秒进度异步写入、保留未指定音轨/字幕/章节、列表、精确删除和重复删除、原片保留及全部夹具清理。首次脚本在初始扫描尚未结束时读到无视频，未写记录且完成清理；等待空闲复跑成功。

路径加入/移除和解散自定义专辑现增加分类扫描/刮削空闲预检，避免进行中的旧任务覆盖手动关联；HTTP模拟证明扫描中不提交关联写入，带预检的18项真机流程已重跑通过。检查与写入不是服务端事务，不保证外部并发任务无法在检查后启动。

### 播放地址与码流控制

`play_start(library_id,path,play_env,video_codecs,audio_codecs,dest_format?,audio_index?,subtitle_index?)` 通过`/transcode/play`创建播放地址；该GET可能启动解码，风险为write，不按read自动重试。当前账号文件预检、分类可见性和所选轨道index必须有效（字幕-1表示关闭）。编解码器由调用方明确声明，不能把CLI当作浏览器自动探测能力。结果含URI等临时播放能力信息，不写入公开验收记录。

`play_switch(library_id,path,play_addr,video_codecs,audio_codecs,dest_format,audio_index?,subtitle_index?,start_time_seconds,confirm_affects_shared_stream:true)` 切换码流/音轨/字幕；`play_stop(play_addr,confirm_affects_shared_stream:true)` 停止该地址的解码。地址可能被多播放器共用，不提供停止全部入口；不得把地址本身等同独占会话身份。仅接收当前NAS接口源的初始媒体地址或相对地址（/play、/splay的媒体资源或/v2/file/download），不接收控制接口地址。其HTTP行为继续继承现有基础层，未接入延期的HTTP安全整改草稿。

`play_probe(play_addr)` 发送0..65535字节Range请求，返回状态/实际收到字节数/Content-Type/是否HLS，不输出资源内容。实际返回大小受服务端Range行为及既有API限制；API JSON错误页或HTML页不能被当作媒体。此功能证明资源可取，不证明播放器画面、声音或字幕渲染质量。

独立2秒双音轨/字幕短片14项通过：上传及独立分类、轨道校验、COP拷贝码流、HLS清单读取、第二音轨切换、内嵌字幕切换及各资源读取，3个本次播放地址停止确认，分类删除与目录回收。当前无待停止的本次播放地址。没有向其他人发送播放地址，未中断其他影片播放；实际渲染、远程播放与STRM仍待覆盖。

影视图片引用约束（2026-10-16）：`collection_create` 的 cover/backdrop 当前须为空；`collection_replace` 只接受保留当前引用（去掉显示查询串）或显式清空。非空引用须符合应用生成的15位小写十六进制独立缓存目录布局。原生解散会移除封面父目录，因此旧版不符合该布局的封面需先清空再解散。专用图片上传、NAS选图及URL下载仍待实现。

## 2026-10-19 后续源码：影视图片、筛选和收藏

当前 0.2.3-rc.2 ZIP及六平台bin均从本次修复源码重建；各业务的历史验收时间仍按对应段落记录
`target/debug/zspace-cli`。14项安全整改暂缓。

### 图片准备与应用

`media video-libraries --params` action=`collection_artwork_apply`，必填library_id、collection_id、
expected_info（最新collection_info完整值）、metadata（已有21字段完整替换契约）、slot（cover/backdrop）、
source及confirm_replace_metadata:true，另需--yes。source三选一且不接受混合字段：

- local：`{kind:"local",local_path:"fixture.jpg"}`。按--local-root读取JPEG/PNG内容，cover最多3MiB、backdrop最多6MiB。原生multipart字段为cover/backdrop。
- nas：`{kind:"nas",path:"/sata11/my/data/fixture.jpeg"}`。预检可访问普通文件，支持jpg/jpeg/png，由NAS复制到独立图片缓存。
- url：`{kind:"url",url:"https://example.com/image.jpg",confirm_nas_fetch:true}`。原生NAS下载HTTP(S)图片，拒绝内嵌凭据/片段并明确确认NAS发起请求；服务端既有连接/TLS策略未修改，不视为安全整改。

应用只接入自定义/未刮削专辑edit，常规已刮削专辑仍需另接correct。其他图片只能保留当前引用或清空，不能借用另一专辑的引用；独立缓存布局新增支持.jpeg。
准备完成后再次读取专辑，变化则停止编辑，返回partial:true、applied:false和candidate。
edit发出后失败/结果未知则返回partial:true、applied:null、edit_submitted:true；成功响应为applied:true。
必须检查这些字段，不能只看外层ok。候选引用返回供核对；原生接口无单独缓存删除入口，失败候选、被替换旧图与backdrop不能声称自动清理。解散只清理当前cover目录；准备请求本身网络失败时是否生成缓存也可能未知。

### 列表与收藏

| action | 参数 |
| --- | --- |
| collection_filters | 无，读取原生filter/v2的筛选ID |
| collection_list | library_id；start默认1（行偏移）、num默认100且1..500；field默认created_at，可video_updated/release_year/title/score；sort为asc/desc默认desc；filters默认全0，仅system/regions/genres/hdr/audio/resolution/year/seen/collection_type整数 |
| favorite_list | start/num同上；field仅created_at/release_year；sort默认desc或asc；返回当前账号PC可见收藏 |
| favorite_set | library_id、collection_id、expected_item、favorite布尔值，另需--yes |

收藏添加的expected_item取自collection_list(filters.system=95)完整行；取消取自favorite_list完整行。
写前重新读取列表并精确核对分类、专辑及完整行，拒绝跨分类同ID、未知目标和陈旧数据；最多100批各500行。
添加已收藏对象时不重复写入；取消一个已不在收藏列表的对象会拒绝，需先刷新。当前添加支持可见单个专辑（含合集内单片），合集本身、跨账号共享写入待补，不提供全量清空。
空count/list归一化为[]。协议依据zvideo_v2 collection.go、favorite.go、models/file.go、collection_list.go及xnas-pc video调用。

本批真机验收完成12项：筛选读取、独立JPEG/短片上传与扫描、关联、收藏添加/取消回读、本地封面/NAS背景/URL封面回读、专辑解散、分类删除及目录回收。URL使用临时回环HTTP服务与SSH反向转发，均已关闭。3个唯一候选图片目录中当前cover由原生解散清理，余2个经目录内容/路径/无符号链接/夹具SHA-256核对后逐文件删除并rmdir；无全局缓存清理。PNG真实写入、普通互联网URL、并发故障真机分支尚未验收，勿扩大结论。

## 2026-10-19 后续：影视合集、匹配、智能跳过、分享与直播

下列动作均复用 `media video-libraries --params`。写操作仍需 `--yes`；expected_* 为相应读取动作返回的完整最新对象，陈旧快照会拒绝写入。源码持续开发，旧阶段包没有这些动作。

### 合集与云匹配

- `series_list(library_id,start=1,num=100)` 使用现代video/v2/list的system=7；`series_info(library_id,series_id)`、`series_contents(library_id,series_id,start=1,num=100)`读取详情和成员。
- `series_create(library_id,name,description)` 创建手动合集。`series_replace(...,expected_info,name,description,confirm_replace_metadata:true,use_member_cover=false)`完整替换名称/说明，true恢复成员封面。
- `series_add_collections(...,expected_info,expected_items,confirm_group_collections:true)`核对system=95的完整专辑行，按JSON字符串数组提交成员；`series_remove_collections(...,expected_info,collection_ids,confirm_ungroup_collections:true)`仅移除已核对成员。
- `series_detach(...,expected_info,expected_collection_ids,confirm_remove_from_library:true)`核对完整成员集合再移除当前分类关系；原片保留，原生全局合集元数据仍保留，返回backend_metadata_retained:true。不能视为物理删除元数据。
- `series_artwork_apply(...,expected_info,name,description,source,confirm_replace_metadata:true)`仅封面，复用local/nas/url图片来源及并发检查；`series_favorite(...,expected_info,favorite)`收藏/取消。其他写操作暂拒绝auto_series_id非空的自动合集，避免隐藏的黑名单副作用。
- `cloud_search(keyword,source=douban/tmdb/all,year?,category?,region?)`读取匹配候选。上游无明确结果结构时返回E_VIDEO_SEARCH_INDETERMINATE，不能将其报告为无结果。
- `collection_match(library_id,collection_id,expected_info,expected_item,candidate,confirm_cloud_rematch:true,confirm_may_relocate_or_merge:true)`调用原生correct，可能合并专辑或移往另一分类；必须检查返回的新ID及partial。真机仅云搜索通过，重匹配写入仅HTTP模拟。

手动合集16项真机通过，包含两专辑分组/移除、替换名称/封面、收藏及移除关系。所有可见临时对象与唯一图片缓存已清理；保留原生detach的全局元数据语义。

### 智能片头片尾

- `skip_tasks(library_id,task_status=-1,start=1,num=100)`，-1全部电视专辑、0未计算、1运行、2排队、3取消、4成功、5失败、6部分成功；task_id=0表示尚无任务。`skip_task_info`和`skip_results`再带collection_id。
- `skip_set_enabled(library_id,expected_library,enabled,confirm_requeue_or_cancel_classification:true)`会影响整个分类，开启可能排队，关闭会取消待运行任务。
- `skip_calculate_classification(library_id,expected_library,confirm_recalculate_all_collections:true)`原生会复用已有任务/结果，不保证强制重算；`skip_calculate_collection(library_id,collection_id,expected_item,expected_info,confirm_compute:true)`需电视专辑、分类已开启且扫描空闲。
- `skip_task_action(library_id,collection_id,expected_task,method=cancel/retry/clean,confirm_change:true)`只接受当前精确任务；cancel限运行/排队，retry/clean限终态，retry保留已有结果；clean删除任务和结果。
- `skip_edit_result(library_id,collection_id,video_id,expected_result,timings,confirm_replace_result:true)`，timings四字段intro_start_seconds、intro_end_seconds、outro_start_from_end_seconds、outro_end_from_end_seconds。片尾为距结束的相对秒数；数值非负且小于片长一半，片头start<=end、片尾start_from_end>=end_from_end。
- `skip_use_smart(library_id,collection_id,expected_info,confirm_replace_skip:true)`选择智能结果并保留现有手动四个时间值。

两份自制90秒音视频真机19项通过：原生算法成功产出两集结果、编辑回读、选择智能、重试、清理、分类计算、取消真实排队任务、关闭及全部夹具清理。此结果不代表真实影视识别准确率。

### 对外分享与跨账号共享

`shares(start=0,num=100)`列出本人分享。`share_create(library_id,collection_id,expected_item,expected_info,expires_in_seconds,max_visits,require_passcode,confirm_dynamic_share:true)`分享整个动态专辑；有效期60..2592000秒、次数1..10000。创建响应不含ID，CLI按返回code回查，返回share_created、share_id、partial；回查失败不重复创建。分享码/提取码及地址为敏感能力信息，勿放公开日志。

`share_stop(share_ids,confirm_stop:true)`耗尽剩余次数；`share_delete(share_ids,confirm_delete:true)`删除指定记录。ID列表须1..500个唯一正整数，先核对本人列表，拒绝原生空ID触发全量删除。7项真机通过并清理全部临时分享、空专辑和分类；没有打开访客链接。

`received_libraries(state=0)`读取收到的分类，0全部/1显示/2隐藏；`received_library_state(library_ids,expected_libraries,hidden,confirm_change:true)`按顺序核对完整快照再隐藏/恢复。
`received_collection_info(library_id,collection_id)`和`received_collection_videos(...)`先核对接收授权。
`library_share_replace(library_id,expected_library,recipients,confirm_replace_recipients:true,confirm_removed_recipient_history_deleted:true)`是完整ACL替换，空列表显式清空。接收者须为account user list中的准确username，拒绝自己/重复/不存在账号；移除接收者会删除其该共享分类的收藏和播放历史。用户暂无专用接收账号，跨账号写入仅HTTP模拟，不发送真实共享通知。`received_collections(library_id,start=1,num=100,field="created_at",sort="desc",filters={})`使用与collection_list一致的过滤字段；`received_series_contents(library_id,series_id,start=1,num=100)`读取合集成员。两者先核对当前接收授权；仍待跨账号真机验收。访客会话仍待补。

### 直播源与频道

- `live_sources(start=0,num=50)`、`live_channels(source_id=0,start=0,num=50,group_title="",keyword="",favorite_only=false,field="created_at",sort="desc")`，num1..200，start为0基行偏移；favorite_only=false为全部（原生不支持仅未收藏）。排序字段created_at/name/group_title/number/updated_at/check_status/is_fav/fav_at。
- `live_groups(source_id=0)`、`live_channel_info(channel_id)`、`live_draft(source_id)`读取分组、频道及草稿。频道详情可能包含播放地址，勿公开。
- `live_import_file(local_path,name,confirm_import:true)`按local-root导入非空m3u/m3u8/txt，最大10MiB，原生multipart字段file；创建草稿，尚未保存。
- `live_import_url(url,name,mode=m3u/txt/single,auto_update,update_interval_minutes,confirm_nas_fetch:true)`明确远程访问及周期；仅HTTP(S)，无内嵌凭据/片段。周期60/180/360/720分钟，single不允许自动更新或伪装成列表扩展名。服务端连接限制保持原生行为。
- `live_draft_patch(source_id,expected_source,expected_draft,name,channels,delete_ids,confirm_replace:true)`完整草稿快照；channels包含id及可选name/group_title/number，草稿不支持收藏、多频道草稿不能改分组。更新和删除目标不得重叠。
- `live_source_action(source_id,expected_source,method=save/cancel/delete/refresh,confirm_change:true)`，save/cancel仅草稿，cancel和delete删除源及频道/导入缓存；refresh仅已保存远程列表，会访问网络，超时须刷新列表确认结果而非盲目重复。
- `live_source_patch(source_id,expected_source,name,auto_update,update_interval_minutes,confirm_replace:true)`替换源配置。
- `live_channel_patch(channels,expected_channels,confirm_replace:true)`、`live_channel_delete(channel_ids,expected_channels,confirm_delete:true)`只作用于逐个核对的已保存频道，快照取live_channel_info并按目标顺序排列；收藏is_fav仅0/1，禁止空删除。

13项真机通过：本地列表导入/草稿编辑保存、详情、收藏、改名、精确频道删除、源删除、single URL草稿取消。仅使用example.invalid占位流，未播放外部媒体；所有测试源已删除。远程列表抓取/周期更新、logo导出和实际直播播放尚未真机验收。

### 视频审阅首批

`review_projects`读取本人项目（返回的视频路径/分享码勿写公开报告）；`review_project_create(name,description,admin_name)`要求明确管理员展示名，名称/管理员各1..255字节、说明最多1024字节。`review_videos(project_id)`、`review_users(project_id)`和`review_shares(project_id)`先核对当前项目归属。

`review_add_videos(project_id,expected_project,paths,confirm_transcode:true)`预检最多500条唯一普通文件路径，添加可能异步转码，返回submitted:true/completed:false；批量服务端部分成功仍需刷新，不能当作原子事务。尚未真机写入。

`review_project_delete(project_id,expected_project,confirm_delete_project_and_shares:true)`删除项目、视频关系、用户、批注和关联分享；当前拒绝仍有任何转码任务记录的项目，避免原生删记录但不停止工作进程。检查转码任务列表本身可能同步进度到数据库，不能当成纯读。后续待接入任务生命周期及输出文件保留策略。

独立空审阅项目6项真机通过并删除，无媒体/分享/真实通知。批注、回复、审阅分享配置、访客会话和转码控制仍待补齐。

审阅续增：`review_tasks(project_id,confirm_refresh_state:true)`与`review_progress(confirm_refresh_state:true)`需--yes，查询可能同步数据库/推进任务；`review_task_action(project_id,task_id,expected_task,method=cancel/resume,confirm_change:true)`先核对任务当前完整值和project_id。cancel限queued/running，resume限canceled/failed/not_started；仅提交，不承诺转码已停止或完成。任务控制仅模拟验收。

`review_annotations(project_id,video_id,keyword?,status?,creator_id=0)`读取/搜索；status仅0待解决/1已完成，默认全部，包含同组回复；keyword存在则走search，不能空白。
`review_annotation_create(project_id,video_id,expected_video,content,draw_data,time_ms,expected_parent?,confirm_post:true)`以项目创建者的唯一管理员身份发言；expected_video取review_videos完整对象。draw_data为JSON对象/数组或null，最多1MiB；content最多64KiB，内容与绘图不可同时空。time_ms为非负i32毫秒；暂未按视频实际时长校验。expected_parent为当前批注完整对象，CLI推导parent/group及被回复者，不接受任意伪造审阅身份。
`review_annotation_update(project_id,video_id,expected_annotation,content?,draw_data?,status?,confirm_change:true)`不支持空字符串清除原生忽略的字段；绘图可显式空数组。
`review_annotation_delete(project_id,video_id,expected_annotation,confirm_delete:true)`必须先删回复，拒绝使其悬空；每次重新核对项目、视频、批注和管理员。当前批注写入/回复仅模拟，待有媒体项目的真机验收；审阅分享创建/配置、访客会话、转码输出清理策略仍待补。

## 当前交付版本

2026-10-19：本文件所述当前源码已纳入0.2.0-rc.2六平台测试构建。此前“需源码构建/旧包未包含”的文字描述rc.1时期，不适用于rc.2。各功能的真机与模拟范围、待完成事项仍按各段说明；版本更新不等于新增验收结论。


## rc.2 验收报告修正契约（2026-10-19）

本节覆盖此前段落中与下列行为不一致的历史说明；修复源码将纳入rc.3，旧rc.2 ZIP不变。

- `file write`与upload共用/v2/file/upload冲突协议，默认rename:0（fail）；1改名、2跳过、3覆盖，overwrite:true等同3。目标已存在时默认E_EXISTS，不静默改名。返回path为实际路径，requested_path为原目标；skip可能返回原文件，size为实际文件大小、requested_size为本次内容大小，outcome不会把skip误称覆盖成功。分片写入只在完整结果回读后返回成功。空文件可使用file upload；file write沿用content非空契约。
- `slow_start`新增规范字段task_id（整数），保留taskid兼容旧调用方。slow_result/slow_stop支持task_id或taskid、正整数或十进制字符串；同一次调用不能同时传两个名字。
- `review_project_create`新增project_id，保留原id；审阅项目操作也接受id别名（项目ID仍为整数）。这不表示所有业务的ID都应改为同一种类型：UUID/数字ID各按原业务契约。
- `ops vm`只接受vm_id/start/num，`ops iscsi`只接受include，不支持action；未知字段返回E_PARAM，不能悄悄执行默认查询。
- `file archive extract`的to是输出父目录，NAS按压缩包名创建子目录（同名可能自动改名），不是平铺解压；返回destination_layout=archive_subdirectory及说明。browse.parent是以/开始的包内路径，如/pack，不是NAS目录；错误提示已区分。
- 业务对象不存在用E_NOT_FOUND、未接收共享授权用E_PERMISSION、快照变化用E_CONFLICT；参数结构/格式错误继续E_PARAM。旧接口未在本轮全面改造，不能据此假设所有历史API的E_PARAM都已细分。
- 本人分享等输出默认遮罩分享码、提取码及含能力参数的链接，保留share_id/id便于停止和删除。确需交付链接时显式加全局--show-secret；仍不能输出OAuth Token/password/cookie等凭证。不要把显式秘密输出写入公开日志。
- 图片local/nas/url三来源都会真实解码JPEG/PNG，拒绝仅魔数、截断/非法图像及超出尺寸/分配限制的图像。NAS选图先读取并验证，再上传这些确切字节，避免复制未验证数据；图片种类标记保持local=1/nas=2/url=4。
- 图片URL由CLI抓取后上传，不再调用NAS的collection/download接口；仅允许公网HTTP(S)，DNS所有答案均需公网并固定到实际连接，手动逐跳重定向校验，禁用环境代理，不转发NAS凭证，DNS/连接/读取总时限8秒，大小仍为cover3MiB/backdrop6MiB。该路径不提供私网逃生开关。
- live_import_url的m3u/txt模式同样由CLI抓取并以静态快照导入，返回import_mode=static_snapshot/remote_auto_update_supported=false；auto_update须false。single模式仅保存公网URL且关闭自动更新，不启动播放。CLI拒绝NAS后台自动更新、NAS代抓refresh，以及自动更新仍开启的旧远程草稿save；请先关闭其更新。已有源可读取、关闭更新和删除。此限制用于避免将CLI的一次DNS检查误当成NAS未来连接的保证。
- 上述出站修复覆盖报告中的影视图片和直播URL入口，不代表其余14项CLI安全整改全部完成，也不修改NAS固件或其他客户端的行为。

### 完整快照来源

不能自行挑选字段或拼凑expected_*；固件会增加字段，固定字段清单不能替代完整最新对象。

| 写入动作 | 期望快照来源 |
| --- | --- |
| collection_replace/seen/dismiss、图片/路径/手动跳过等的expected_info | collection_info原样完整返回 |
| favorite_set添加的expected_item | collection_list(filters.system=95)中classification_id、collection_id均匹配的完整行，含所有未知字段 |
| favorite_set取消的expected_item | favorite_list中的目标完整行 |
| series_replace/成员/封面/移除/收藏的expected_info | series_info完整值；成员expected_items来自system=95完整行 |
| collection_match | collection_info和system=95完整行，加cloud_search完整候选 |
| skip_set_enabled/分类计算、library_share_replace | video-libraries list中目标分类完整行 |
| skip_task_action / skip_edit_result | skip_task_info完整对象 / skip_results目标video_id完整行 |
| received_library_state | received_libraries(state=0)完整目标行，按library_ids顺序排列 |
| live_source_action/patch、draft_patch.expected_source | live_sources目标完整行 |
| live_draft_patch.expected_draft | live_draft.list完整数组 |
| live_channel_patch/delete.expected_channels | 对各目标调用live_channel_info所得完整对象，按目标顺序排列 |
| review_project_delete/add_videos.expected_project | review_projects目标完整行，而不是创建响应 |
| review_task_action.expected_task | review_tasks目标完整行（查询本身可能同步状态） |
| review_annotation_create.expected_video / expected_parent | review_videos目标完整对象 / review_annotations目标完整行 |
| review_annotation_update/delete.expected_annotation | review_annotations目标完整行 |

favorite_set示意：读取collection_list返回v，选择`row = v.list.find(r => r.classification_id == library_id && r.collection_id == collection_id)`，将row原样赋给expected_item。不要仅构造{collection_id,title}；取消收藏时改从favorite_list取行。

URL参数中的confirm_nas_fetch沿用历史拼写以兼容，rc.3实际由CLI抓取；若网络仅能经代理或DNS返回私有地址，请先可信下载文件再使用local/NAS来源，不会自动退回不受控的NAS URL抓取。

如果读取的完整快照中含[REDACTED]（例如带能力参数的URL），不要手工拼接或把遮罩当真实值；确需此类完整快照时显式--show-secret读取并仅在本地传回，避免公开日志。OAuth/密码/Cookie等凭证的既有脱敏不会因此解除。

## 0.2.2 验收勘误

`download control details` 只传 action，无 task_type/start/size；这些字段属于 directories。
`system settings time_sync` 会触发设备校时，`media music lyrics_global(enabled)` 会改变全局歌词抓取设置，二者都是写操作，继续需要 --yes；读取分别使用 get/timezones 与 settings。
`backup cloud check` 在默认云模式且无 NAS 网页地址时必须显式传 redirect_uri，例如 `{"action":"check","provider":"baidu","redirect_uri":"https://你的NAS网页地址/home/newNetdisk"}`。它是授权回调地址，不是把 CLI 连接降级为 HTTP；请勿为此改写 profile host。
