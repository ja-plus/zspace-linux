---
name: zspace-docker
description: Manage ZSpace Docker containers, images, networks, Compose projects, and engine status.
---

# Docker 与 Compose

先读 `zspace-shared.md`。当前客户端候选版本为 `0.2.3-rc.2`，使用 `scripts/zspace-cli`，并显式选择 profile。所有命令格式为
`docker <业务域> --params '{"action":"..."}'`；默认 action 为 list。
写操作需要用户授权及 `--yes`。读取与状态变更按 action 区分，即使后端使用 GET 也不自动重试写入。

## 命令与参数

| 业务域 | action | 参数 |
| --- | --- | --- |
| containers | list | 可选 search 字符串 |
| containers | info | container_id 或 app_id（商店 App ID；兼容别名 app_name，不是容器名），二选一 |
| containers | stat/top/logs | container_id；logs 为后端最近 100 条快照 |
| containers | stats | 无，全部容器统计 |
| containers | create/update/hotupdate | create: 顶层 name+config.Image；update: container_id+顶层 name+config.Image+recreate:true（会重建，ID 改变）；hotupdate: container_id+config，原地资源调整 |
| containers | start/stop/force_stop/restart/pause/resume/remove/force_remove | container_id |
| containers | rename | container_id、new_name |
| containers | to_compose | container_id、dir（NAS 输出目录） |
| images | list | 可选 search 字符串 |
| images | common/pull_tasks/download_tasks | 无 |
| images | info/remove/force_remove | image_id |
| images | pull/search | image_name |
| images | hub_search | term，可选正整数 page |
| images | tags | repo_name，可选 is_official 布尔值 |
| images | import/download | image_file_name（NAS 路径） |
| images | cancel_pull/cancel_download | task_id |
| networks | list/ipv6 | 无 |
| networks | info/remove | network_id |
| networks | create | config 对象，必填 Name；可选 Driver/IPAM/Internal/Attachable/CheckDuplicate/Labels/EnableIPv6/Options |
| networks | attach/detach | network_id、container_id |
| networks | ipv6_set | enabled 布尔值；影响整个 Docker 引擎 |
| compose | list | 可选 search 字符串 |
| compose | create | project_name、path、config（YAML 字符串）；可选 env 字符串、rw:false、start:true、pull:false、cron_config 对象 |
| compose | file/containers/logs/remove/start/stop/restart | project_id |
| compose | save | project_id、config（原始 YAML 字符串） |
| compose | rebuild | project_id，可选 start:true、pull:false |
| compose | tasks/version | 无 |
| compose | task/cancel | task_id |
| compose | check_name/check_path | 分别 project_name/project_path |
| compose | cron/env | project_name |
| compose | cron_save/env_save | project_name 及 cron_config 对象/env_content 字符串 |
| system | list/status/info/engine_info/storage/disks/usage/copy_tasks/upgrade_tasks/check_upgrade/proxy/registry/private_hub | 无 |
| system | path | path，转换为 Docker 可用的 NAS 原生路径 |
| system | switch | status:"on"/"off" |
| system | restart/upgrade | 无；改变 Docker 引擎状态 |
| system | reset | all:true；高风险重置，未进行破坏性真机测试 |

cron_config 的字段是 CronStartHour/Minute/Dow、CronStopHour/Minute/Dow（字符串），
CronStartEnable、CronStopEnable（布尔值）。config 未知顶层字段会被拒绝；容器 config
采用 Docker 原生 Cmd、Env、HostConfig、Image、NetworkingConfig 等字段，不能当作任意 HTTP 请求使用。
空列表规范为 []；其他数据保留 NAS 原始对象。任务提交成功仅说明收到任务，需查询任务终态。

## 路径与安装

Compose create/check_path、容器 to_compose、镜像 import 和 system path 支持
`/sata1/my/...`、`/nvme1/my/...`、`/esata1/my/...` 及 public 路径，按照已验证的当前用户映射到
`/tmp/zfsv3/<pool>/<account>/...`。显式原生路径保持原样。先用 `file space` 查询实际存储池。
YAML 内卷挂载、HostConfig.Binds 中的路径不会自动改写，可先调用 system path 再显式填入配置。
Docker storage 返回的 /data_s001 等挂载点不能直接当作 Compose 个人目录。

商店应用使用 `app list` / `app manage`，详见 `../reference/function-expansion.md`。
普通商店和 Docker 商店来源必须显式区分。Docker 商店需要先初始化数据目录。
UI 应用先 preinstall，准备好独立目录、端口及应用自定义 install_options 后再 install。
业务口令可用 install_secrets 引用环境变量，禁止在命令参数中写入其值。
E_APP_INSTALL_UNCERTAIN 表示需查询任务和安装状态，不能直接重试。

## 协议与验证边界

路由前缀 `/zdocker/`，沿用 NAS OAuth 与会话 Cookie；不使用旧版 Docker 独立登录。
证据为 `docs/zspace-cli/zdocker.md`、Web `src/api/docker.js`、DockerSetupDialog 与 dockerAppConf，
服务端 `zdocker/server/router.go`、api 和 model 中对应 handler。
Compose create 使用 form，布尔编码为 true/false 字符串；save 使用 text/plain 原始 YAML。

真机已通过镜像拉取、独立容器及网络生命周期、Compose 文件往返和重建任务、思源商店应用完整安装卸载。
测试对象已清理。引擎 reset/upgrade/switch、现有用户容器修改、全局 IPv6 修改未经真机写入测试。
仍待补：终端与日志 WS、事件/进度流、镜像二进制导出、引擎配置写入及数据迁移。
后端未注册独立 volumes CRUD，不编造此能力。全量功能目标仍未完成。

## 0.2.2 容器与 Compose 注意事项

- `update` 是 NAS 完整重建流程，必须额外 `recreate:true`；不是原地更新。会停止、删除并重建旧容器，返回 `recreated`、`old_container_id`、新 `container_id`，后续请使用新 ID。不要自动迁移旧 ID。
- `hotupdate` 调用 NAS 原地资源更新，只调整该接口支持的 CPU/内存/重启策略及图标/定时字段；不支持原地变更 Image、环境变量或挂载。重建仍需单独获得用户授权。
- `info` 用容器 ID，或 `app_id`（商店 App ID；旧参数 app_name 仍兼容）。未知对象返回 E_NOT_FOUND，不再静默成功空对象。容器名称请先 list 查出 ID。
- `stat` 的 NAS 统计缓存缺失时会用 inspect 核对容器；存在则返回 available:false、reason:statistics_pending、state 与 metrics:null，不能把它当成零资源占用或不存在。
- `env_save` 仅发送 project_name/env_content。当前核对到的 NAS 服务端把项目 YAML 路径当目录写 .env，且读取端使用不同的 env 文件名；受影响固件无法通过客户端改传目录修复。CLI 会明确报保存失败，需升级/修复 NAS Docker 服务。不得改项目数据库或绕开网关写系统备份。

## hotupdate 的真实资源字段

CPU/内存/重启策略不是 config 顶层字段。与网页客户端 zContainer.vue 和 NAS HotUpdateContainer 对齐，真实位置如下：

| 字段 | 类型与单位 |
| --- | --- |
| config.HostConfig.NanoCpus | 整数；1 个 CPU 为 1000000000 |
| config.HostConfig.Memory | 整数；字节，例如 256 MiB 为 268435456 |
| config.HostConfig.RestartPolicy.Name | 字符串，如 no / always |
| config.Icon / config.Cron* | 保留现有图标与定时字段 |

结构示意（仅说明字段位置，**不要原样提交覆盖现有设置**）：

```json
{"HostConfig":{"NanoCpus":1000000000,"Memory":268435456,"RestartPolicy":{"Name":"no"}}}
```

先用 info 获取当前配置，从中选取 CLI 支持的 config 字段，保留当前 HostConfig 资源值、Icon、Cron*，再改目标字段并以 hotupdate + --yes 提交。NAS 对省略或过小的 CPU/内存会采用默认值，其他字段也可能被重置；不能把 hotupdate 当作任意字段的无副作用补丁。CpuNum/MemoryLimit/Restart 等猜测的 config 顶层字段不支持。
本版验证真实字段在 CLI 请求中的结构与传输，未声称重新完成 NAS CPU/内存写入验收。
