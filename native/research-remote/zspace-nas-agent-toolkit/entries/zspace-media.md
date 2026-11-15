---
name: zspace-media
description: Browse and search ZSpace video, music, books, audiobooks, comics, and manage media conversion tasks.
---

# 媒体库

`library` 可为 `video`、`music`、`book`、`audio`、`comic`。

```bash
scripts/zspace-cli --profile <profile> media list --params '{"library":"video","start":0,"num":30}'
scripts/zspace-cli --profile <profile> media search --params '{"library":"music","query":"夜曲"}'
scripts/zspace-cli --profile <profile> media detail --params '{"library":"comic","id":"/漫画/示例"}'
```

`media list/search` 始终返回数组 `list`（空库为 `[]`），并在能够与详情接口对应时为条目补充
`detail_id` 和 `detail_id_field`。将 `detail_id` 传给 `media detail` 的 `id` 或 `detail_id` 参数；极影视还
应沿用列表顶层的 `classification_id`。详情不存在时返回 `E_NOT_FOUND`，不再以成功空对象表示。

有声读物没有搜索接口，先用 `media list` 获取播放列表；只有带 `track_id/detail_id` 的曲目可直接查详情。
转码 action 支持 `list/status/create/update/file/cancel_file/execute/pause/cancel/wipe`。其中
`list/status` 只读，常规任务操作会写入 NAS；`wipe` 会清空全部转码任务历史，属于不可逆操作，必须
同时传 `{"action":"wipe","all":true}` 和 `--yes`。后端不支持按单个 `task_id` 删除；不要用 `wipe`
代替单任务删除。

`create` 是 Agent 友好的两阶段操作：必须提供 NAS 上已存在文件的 `source_path`，可选
`target_format`（默认 `mkv`）、`quality`（1～3，默认 2）及已存在目录 `out_path`。CLI 会先验证路径，
再创建任务并立即挂载源文件，但不会自动开始转码；成功响应给出 `task_id` 和下一步 `execute`。若挂载
源文件失败，返回 `E_CONVERTER_ADD_FILE_FAILED` 与已经创建的 `task_id`，绝不把空任务报告为成功，
也不会自动执行全量 `wipe`。

```bash
scripts/zspace-cli --profile <profile> media convert --params '{"action":"create","source_path":"/<pool>/my/data/a.mp4","target_format":"mkv"}' --yes
```

## 影视分类（阶段性功能验收版）

`media video-libraries` 提供影视分类生命周期、目录关联和扫描/刮削异步任务。所有写入需 --yes，状态查询不需要；完整 schema 与验收限制见 `../reference/function-expansion.md`。

## 音乐补全与有声读物（2026-10-16 源码）

`media music` 新增歌词、标签补丁、收藏、播放记录和Subsonic配置；`media audiobooks` 提供播单生命周期、目录导入、音频关联、CUE、记录及刷新任务。参数见功能扩展参考。歌词默认本地读取；写音频标签需要 confirm_audio_write:true；音频关联移除不删除原文件。

2026-10-16新增 `media reader` 和 `media comics`，完整动作参数见 [功能扩展](../reference/function-expansion.md)。阅读转换仅MOBI/AZW3，需confirm_replace_conversion；漫画infos采用comics:[{dir_id,path}]；扫描必须显式dir_ids或all:true，密码笔记只接受secret_env。

本地AI：`media ai --params '{"action":"state"}'`。模型下载会清掉旧模型，需confirm_replace_all_models:true与expected_state；生成chat收齐SSE后输出JSON，需运行中的模型。详见function-expansion。

## 2026-10-19 后续源码动作

`media video-libraries` 新增collection_artwork_apply（本地/NAS/URL封面及背景）、collection_filters、
collection_list、favorite_list、favorite_set。参数和部分完成处理见功能扩展记录最新章节。
当前 0.2.3-rc.2 包已包含这些动作。写入需--yes及最新expected_info/expected_item；
不要忽略partial或applied:null。

2026-10-19源码续增：影视手动合集、云匹配、智能片头片尾任务、对外专辑分享、跨账号分类共享、直播播放列表/草稿/源/频道管理。动作与完整快照、动态分享及批量副作用确认参数见[功能扩展](../reference/function-expansion.md)。这些动作已包含在当前六平台构建；业务验收边界仍按功能扩展记录。

当前交付为候选版 0.2.3-rc.2，发布范围与限制见 [发布说明](../reference/release-0.2.3-rc.2.md)；旧 RC 验收说明是历史记录。

rc.3修正：分享能力默认脱敏，明确--show-secret才显示；图片真实解码，URL由受限CLI抓取上传，远程片单仅静态导入。NAS后台远程自动更新/代抓刷新暂拒绝。完整快照来源矩阵、ID兼容和错误码见function-expansion.md末节。

## 0.2.2 验收后的接口说明

`review_projects` 不接收 start/num：后端返回完整数组，客户端保留 `id` 并补充同值 `project_id`，可将完整列表项回传为 `expected_project`。未知参数返回 E_PARAM 并指出字段。审阅 N001999 需要查失败设备日志；客户端没有数据库清理兜底，不能把通用错误码直接解释为转码服务未启动。

`live_import_url` 的 `mode:"single"` 使用 `auto_update:false`，`update_interval_minutes:0` 可用，线上统一发送 0。m3u/txt 远程片单继续经 CLI 校验后作为静态快照导入。完整快照应原样回传，不要删除空路径字段。

修复及尚未完成的复验见 `reference/acceptance-remediation-0.2.3-rc.2.md`。

`collection_paths` 是 type:300 自定义专辑的直接目录/文件管理列表。电影 type:100 的关联媒体请通过 collection_videos 核对，不应由空的目录管理列表推断关联失败。

直播参数：`live_import_url` 必须提供 `update_interval_minutes`；m3u/txt 为60/180/360/720，single 为0且 auto_update:false。`live_draft_patch.expected_draft` 应取 `live_draft.list` 的完整数组，不能传整个响应对象。
