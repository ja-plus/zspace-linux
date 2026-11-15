# 能力目录

所有参数都放入 `--params '<JSON>'`。下表的分组写法与 `tool <name>` 等价。

| 工具名 | 分组 action | 风险 | 关键参数 |
| --- | --- | --- | --- |
| `list_dir` | `file list` | read | `path,start,num,sortby,order,ftype` |
| `file_info` | `file info` | read | `path` |
| `user_space` | `file space` | read | 无 |
| `read_file` | `file read` | read | `path,offset,max_bytes,encoding` |
| `write_file` | `file write` | write | `path,content,encoding,rename,overwrite`；默认重名失败，rename:1改名、2跳过、3覆盖，返回实际path |
| `upload_file` | `file upload` | write | `local_path,dest_path,overwrite,rename,resume`；默认重名失败，`overwrite:true` 覆盖，`rename:1` 自动改名 |
| `download_file` | `file download` | write | `path,local_path,offset,overwrite` |
| `mkdir` | `file mkdir` | write | `parent,name,rename` |
| `rename` | `file rename` | write | `path,newname` |
| `move` | `file move` | write | `paths[],to,rename` |
| `copy` | `file copy` | write | `paths[],to,rename` |
| `delete` | `file delete` | destructive | `paths[],skip_bin,select_all` |
| `recyclebin` | `file recyclebin` | mixed (max destructive) | `action=list\|restore\|restore_all\|empty,paths（restore 必须非空）` |
| `search_files` | `search files` | read | `name,file_path,start,num,ftype` |
| `search_content` | `search content` | read | `name,doc_type,file_path,start,num` |
| `album_list` | `album list` | read | `types,include_dirs` |
| `album_feeds` | `album feeds` | read | `album_id,start,num,sort,order` |
| `album_feed_list` | `album timeline` | read | `grouped,start,num,dir_id` |
| `album_ai_search` | `album ai-search` | read | `mode,query_content,image_path,task_id,timeout_ms` |
| `album_delete_feeds` | `album delete` | destructive | `feed_ids[]` |
| `storage_info` | `storage info` | read | 无 |
| `smart_report` | `storage smart` | read | `sn,lang` |
| `system_users` | `system users` | mixed (max destructive) | 账号、权限与配额，详见 `../entries/zspace-system.md` |
| `system_info` | `system info` | read | `fields[]=basic\|cpu\|memory\|gpu\|fan\|network\|disk\|power,include_polling` |
| `disk_info` | `system disk` | read | `include[]` |
| `device_info` | `system device` | read | `include[],start,num` |
| `service_status` | `system services` | read | `include_tasks` |
| `app_status` | `app status` | read | `app_id` 或 `all:true`，可选 `source:appstore\|docker`，按动态目录查询 |
| `app_list` | `app list` | read | `update_now`（可选 bool，默认 false）；合并普通和 Docker 商店，返回 `sources/partial/full_success` |
| `app_manage` | `app manage` | mixed (max destructive) | `action,source=appstore\|docker,app_name,version_int,mp,with_data,task_id`，详见 `function-expansion.md` |
| `media_list` | `media list` | read | `library,start,num,classification_id` |
| `media_search` | `media search` | read | `library,query,start,num` |
| `media_detail` | `media detail` | read | `library,id\|detail_id,classification_id` |
| `media_convert` | `media convert` | mixed (max destructive) | `create` 必须传 `source_path`，可传 `target_format,out_path,quality`；其他 action 为 `list\|status\|update\|file\|cancel_file\|execute\|pause\|cancel\|wipe`，`wipe` 还要求 `all=true` |
| `download_tasks` | `download tasks` | read | `start,num` |
| `download_add` | `download manage` | write | `action,uri,dir,ids[]` |
| `backup_status` | `backup status` | read | `targets[]` |
| `share` | `file share` | mixed (max destructive) | `action,paths[],ids[],expire_days,state` |
| `file_labels` | `file labels` | mixed (max write) | `action,name,names[],new_name,paths[],task_id,labels[],top_labels[],scope,sortby,order,show_hidden` |
| `file_tasks` | `file tasks` | mixed (max write) | `action=list\|status\|statuses\|pause\|continue\|remove,task_id,task_ids[],start,num,state` |
| `file_archive` | `file archive` | mixed (max write) | `action=compress\|extract\|extract_dir\|set_password\|browse,paths[],path,to,pwd,charset,show_hidden,compress_level,parent,sortby,order`，详见 `function-expansion.md` |
| `file_notes` | `file notes` | mixed (max destructive) | 必填 `location:"1"\|"2"\|"3"`；action 与参数见 `function-expansion.md` |
| `file_devices` | `file devices` | mixed (max destructive) | V3 外部设备，详见 `../entries/zspace-file.md` |
| `file_user_share` | `file shared` | mixed (max write) | NAS 账号间共享，详见 `../entries/zspace-file.md` |
| `file_encrypt` | `file encrypt` | mixed (max destructive) | 轻度加密目录，详见 `../entries/zspace-safe.md` |
| `file_safe` | `file safe` | mixed (max destructive) | 保险箱状态、业务解锁、锁定和备份，详见 `../entries/zspace-safe.md` |
| `file_snapshots` | `file snapshots` | mixed (max destructive) | 快照、计划、保留与整池恢复，详见 `../entries/zspace-snapshot.md` |
| `file_metadata` | `file metadata` | mixed (max write) | hash/note/pin，详见 `../entries/zspace-file.md` |
| `file_history` | `file history` | mixed (max destructive) | recent/latest/visit/指定移除/显式清空，详见 `../entries/zspace-file.md` |
| `file_public` | `file public` | mixed (max destructive) | 团队分组、权限、配额，详见 `../entries/zspace-file.md` |
| `file_collect` | `file collect` | mixed (max destructive) | `action=create\|list\|info\|files\|stop\|clean\|clean_expired\|clean_stopped\|clean_done\|clean_all`；参数见 `function-expansion.md` |
| `vm_info` | `ops vm` | read | `vm_id=VM名称（列表的name）,start,num` |
| `iscsi_info` | `ops iscsi` | read | `include[]=info\|storage\|targets\|sessions\|luns\|groups` |
| `security_status` | `ops security` | read | `include[]` |
| `nas_query` | `raw` | read | `METHOD,path,params`；仅内置白名单 |

| `docker_containers` | `docker containers` | mixed (max destructive) | action 与字段见 `../entries/zspace-docker.md` |
| `docker_images` | `docker images` | mixed (max destructive) | 同上 |
| `docker_networks` | `docker networks` | mixed (max destructive) | 同上 |
| `docker_compose` | `docker compose` | mixed (max destructive) | 同上 |
| `docker_system` | `docker system` | mixed (max destructive) | 同上 |

混合工具的表中风险是最高风险。`recyclebin list`、`share list|statics`、`labels list|files`、
`file_tasks list|status|statuses`、`file archive browse`、`app manage info|tasks|disks|dependencies`、`media_convert list|status` 按 read 执行；`app manage uninstall` 和 `media_convert wipe` 按 destructive
执行，其余转码 action 按 write 执行；其他混合工具按对应入口的 action 说明执行。

Docker action 风险及协议见 `../entries/zspace-docker.md`；只读动作无需 --yes，所有状态变更需要 --yes。

| 工具名 | 分组 action | 风险 | 参数说明 |
| --- | --- | --- | --- |
| system_power | system power | destructive；提交前主账号预检 | 详见 `function-expansion.md` |
| system_ddns | system ddns | list read；create write；update/delete/refresh destructive | 详见 `function-expansion.md` |
| system_network | system network | 诊断 read；网络配置 destructive，当前主账号预检 | 详见 `function-expansion.md` |
| system_certificates | system certificates | list read；证书写入/重载 destructive | 详见 `function-expansion.md` |
| backup_sync | backup sync | 读取 read；创建/暂停/恢复/扫描/规则 write；删除 destructive | 详见 `function-expansion.md` |
| download_control | download control | 协议/状态/目录 read；单任务 write；删除/批量/引擎迁移 destructive | 详见 `function-expansion.md` |
| download_settings | download settings | get/basic read；set destructive | 详见 `function-expansion.md` |
| download_torrent | download torrent | 查询 read；预解析/生效/种子/tracker 配置 write | 详见 `function-expansion.md` |
| system_events | system events | list/login_log read；已读 write；删除 destructive | 详见 `function-expansion.md` |
| system_settings | system settings | get/timezones/power_plans read；其他 destructive，全局字段主账号预检 | 详见 `function-expansion.md` |
| storage_maintenance | storage maintenance | 状态 read；维护任务 write；缓存删除/忽略型号 destructive | 详见 `function-expansion.md` |
| backup_center | backup center | 查询 read；创建/规则/计划/运行/删除 destructive | 详见 `function-expansion.md` |
| backup_cloud | backup cloud | 查询 read；传输及选定任务控制 write；删除/全量控制/logout destructive | 详见 `function-expansion.md` |
| search_management | search manage | 状态/历史/结果 read；慢搜索/取消/重建 write；索引配置/历史删除 destructive | 详见 `function-expansion.md` |
| file_duplicates | file duplicates | 查询 read；扫描 write；替换结果/修改/清理 destructive | 详见 `function-expansion.md` |
| search_rag | search rag | 查询/检索 read；目录新增/模型任务 write；全局配置/清理 destructive | 详见 `function-expansion.md` |
| album_management | album manage | 查询 read；创建/导入/扫描 write；删除/重命名/目录移除/全量刷新 destructive | 详见 `function-expansion.md` |
| video_libraries | media video-libraries | 查询 read；创建/观看/记录 write；完整替换/路径变更/解散/精确删除 destructive | 详见 `function-expansion.md` |
| music_management | media music | 查询 read；创建/编辑/导入 write；删除/关联移除/乐库移除/取消全部任务 destructive | 详见 `function-expansion.md` |
| audiobooks_management | media audiobooks | 查询 read；创建/导入/记录/刷新 write；播单/目录/音频关联删除 destructive | 详见 `function-expansion.md` |
| `reader_management` | `media reader` | 按action区分 | 阅读分类、书架、书签、记录、MOBI/AZW3转换 |
| `comics_management` | `media comics` | 按action区分 | 漫画分类、目录、书架、进度、密码笔记与扫描 |
| `vm_management` | `ops vm-manage` | 按action区分 | 虚拟机创建、磁盘、快照、运行与VNC管理 |
| `iscsi_management` | `ops iscsi-manage` | 按action区分 | LUN/Target/组、映射、CHAP与设置管理 |
| `virus_management` | `ops virus` | 按action区分 | 查杀、记录、白名单、隔离恢复、计划和病毒库管理 |

| `firewall_management` | `ops firewall` | destructive（读取/预检为read） | 防火墙信息、规则预检、配置替换与全局维护 |

| `time_machine` | `backup time-machine` | destructive（状态查询为read） | 当前账号配置、精确备份删除、服务控制 |
| `storage_usb_mode` | `storage usb-mode` | destructive（状态查询为read） | USB盘创建、增量扩容、启停、重置与手机模式 |

| `browser_management` | `app browser` | destructive（读取为read、下载/启动为write） | NAS浏览器安装与生命周期 |
| `gpu_management` | `ops gpu` | destructive（查询为read） | GPU信息/AI任务与全局弹出 |

| `ai_models` | `media ai` | destructive（读取为read、生成为write） | 本地AI模型、引擎与SSE生成聚合 |

下载器联网写操作另需每次明确传 `--allow-nas-fetch`，并保留原有 `--yes`；默认拒绝。停止、删除与只读查询不受此额外开关限制。CLI 无法约束 NAS 最终 DNS、重定向及 BT peer，详见 `release-0.3.0-rc.1.md`。
