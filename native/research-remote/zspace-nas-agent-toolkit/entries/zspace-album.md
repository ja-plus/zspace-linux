---
name: zspace-album
description: Browse ZSpace albums and timelines, run semantic, CLIP, or OCR searches, and delete album feeds after confirmation.
---

# 极相册

```bash
scripts/zspace-cli --profile <profile> album list --params '{}'
scripts/zspace-cli --profile <profile> album feeds --params '{"album_id":"<id>","start":0,"num":50}'
scripts/zspace-cli --profile <profile> album timeline --params '{"grouped":false,"start":0,"num":50}'
scripts/zspace-cli --profile <profile> album ai-search --params '{"mode":"clip","query_content":"海边日落"}'
```

`semantic` 返回相册级结果；`clip` 返回图片级语义结果；`ocr` 搜索图片文字。异步搜索超时会返回
`status:"running"` 和 `task_id`，把该 `task_id` 放入下一次相同调用继续查询。

`album delete` 会删除 feed，必须列出 `feed_ids` 对应对象并取得明确同意后传 `--yes`。

## 相册管理（阶段性功能验收版）

`album manage` 提供普通相册创建、详情、重命名、删除、导入与关联移除，以及来源目录和扫描管理。导入需先显式登记来源目录；移除关联不删除原图，详情和限制见 `../reference/function-expansion.md`。

`album manage`新增create_shared/configure_shared（家庭/宝宝/纪念册完整成员配置）、roles/role_add/role_delete、quit、baby_timeline与baby_growth_*、posting_*、comments_list/comment_create/comment_delete。生日YYYY-MM-DD，成长line_date为YYYYMMDD。修改成员需完整列表与expected_info；先清评论、再清内容、最后删相册。完整参数见[功能扩展](../reference/function-expansion.md)。

数据库：db_status/db_size/db_backup_info、db_backup_set、db_upgrade/db_migrate及进度；升级/迁移为全局操作。导出：export_create/export_tasks/export_task/export_errors/export_retry/export_ack；导出有同名改名和相同文件跳过策略，ack仅确认已结束任务，后端没有注册取消接口。详见function-expansion。

album feeds 的 album_id 支持列表返回的整数 ID 及字符串 ID，无需先强制转换；不接受额外 action 字段。

删除相册条目的 CLI 入口是 `album delete --params '{"feed_ids":["专用测试条目ID"]}' --yes`，不是 album delete_feeds。JSON字段名为 feed_ids（不带方括号）；支持字符串或正整数数组，不能为空。删除验收只使用自行创建的专用条目，不使用用户现有照片或假定大整数ID不存在。
