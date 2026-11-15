# 0.2.2 验收报告核查与客户端修复

依据用户提供的《zspace-nas-agent-toolkit-v0.2.2-验收报告-20261020.md》。仅处理客户端，不修改 NAS/OAuth 服务端，不删除报告列出的历史测试对象。授权码继续在当前交互隐藏输入。

| 编号 | 核查与处置 | 验证边界 |
| --- | --- | --- |
| V21 | 确认。通用路径递归校验误伤完整快照。仅豁免顶层明确列举的 expected_info/item/items/history/library/libraries/project/task/annotation/parent/video/source/channels/draft/result；快照仍做机密输入校验，显式及嵌套业务路径仍检查穿越 | 真机创建/已看/替换/解散通过；原样空路径、加密字符串、结构化 paths 快照通过；实际路径及嵌套伪快照越界仍拒绝 |
| V22 | 确认。设备目录和用户空间两项主请求都失败时改为 E_NETWORK，非零退出；仅部分失败保留 warnings | 全失败与部分成功的 HTTP 回归 |
| V23 | 确认上传公共层回归。Body::Bytes 在已有 multipart Content-Type 后再次添加 octet-stream，破坏请求体解释；改为仅在没有显式 Content-Type 时提供默认值。URL 片单导入也经此公共上传层 | HTTP 接收端断言只有一个正确 Content-Type，直播文件导入→草稿→保存→删除正向流程通过；NAS 真机文件导入→草稿→保存→删除通过；远程 URL 片单未单独重跑 |
| V24 | 确认 offset 原为死参数。现支持本地文件长度等于 offset、overwrite:true 的前缀校验：重新抓取完整远端内容并逐字节比较本地前缀，匹配后原子安装完整文件；不匹配 E_CONFLICT 且保留原目标 | 真机匹配前缀下载内容一致；本地匹配/不匹配及临时文件清理回归。此实现完整重传，不节省带宽；多块仍要求一致强 ETag |
| V25 | 同 V23，专用种子上传受重复 Content-Type 影响；保留 downloader 的 file 字段和文件名 | parse_upload/add_upload 的实际 HTTP multipart 接收测试；NAS 真机 parse_upload、add_upload、任务回读及删除均通过 |
| V26 | 确认。schema 错误现在指出具体未知/缺失字段名，不回显参数值，不再一律声称完整元数据缺失 | collection_videos 的 start、review_projects 的 num 报 E_PARAM 并指出字段 |
| V27 | 确认。feed_ids 接受非空字符串或正整数数组，统一转换字符串后发送；格式错误 E_PARAM | 整数 ID 74/75 发送为 74,75 |
| V28 | NAS 返回的失败尚未解决。客户端 project_id、paths[] 与 zfilev2/api/review.go 一致；空项目删除和添加媒体的 N001999 均可能来自数据库/事务，不能仅凭错误码认定转码服务不可用 | 保留原始 E_API/N001999，并提供接口与回读排查提示；已获授权只读查看失败设备日志：仅见查询未找到记录，不能证明根因；源码日志使用 UDP 127.0.0.1:514 local5，而设备 rsyslog 的 UDP 接收配置被注释，配置的 local5 文件也不存在。未改日志配置，不增加绕过网关或数据库删除兜底。仍为发布阻断项 |
| V29 | 确认两个根因：expected_info.paths 为结构体被误判；confirm_scan_selected_paths:true 也因 _paths 后缀被误当路径数组。已分别豁免受比较的顶层快照和此明确确认字段，真正 paths 仍严格校验 | 创建→读取→已看→替换→关联→解除关联→解散的完整 CLI 分发及 HTTP 回归；type:300 自定义专辑真机关联/回读/解除通过 |
| V30 | 确认字段不一致。列表保留数组及 id，补 project_id 别名；快照比较兼容旧列表且拒绝别名与 id 矛盾。接口本身无分页，文档明确；多余参数指出具体字段 | 现有审阅完整快照回归及 schema 测试 |
| V31 | 确认。single 模式跳过更新周期检查，发送 0，仍要求 auto_update:false | 单频道 import/url 的实际请求检查 |
| V32 | 确认文档缺项。补充 encrypted:1 锁定、2 解锁、空字符串非加密目录 | 以报告真实设备结果及现有协议为依据，本轮未改加密逻辑 |
| V33 | 确认 Windows 失败清理存在缺少错误检查的风险。创建临时文件后先检查并设置句柄删除标记；成功安装前清除标记，再句柄重命名；失败关闭句柄清理 | macOS 空临时文件/下载失败回归；Windows 交叉检查，原生故障场景仍待测试团队复验，不能视为已原生验收 |
| V34 | 同 V23，图片上传使用同一 Body::Bytes 分支，修复重复 Content-Type；图片解码和尺寸限制保留 | 共有实际传输头回归及已有图片协议测试；NAS 200×200 PNG 合集封面写入及回读通过 |

Windows 删除标记依据 [Microsoft FileDispositionInfo 文档](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info)。标记设置失败会明确返回错误；不以 Drop 中忽略错误作为唯一清理方式。尚不保证所有 Windows 文件系统与权限失败场景均通过原生验收。

## 回归与交付范围

本轮新增 9 项正向/边界回归，完整 229 项单元测试和 4 项 CLI 测试通过，原生 Clippy 无告警。测试通过真实 HTTP 模拟接收端验证请求及业务分发，不等同于报告中约 1,050 次 NAS 真机调用。之前测试只检查首个 Content-Type，未识别重复头；新测试检查完整头集合。

V3 Compose env_save 保持服务端缺陷说明；V28 未获真实失败日志，仍阻断正式发布。当前修复用于候选版本复验，不能宣称所有问题已修复或全业务已验收。Windows 原生清理验证和远程 URL 片单等未覆盖场景需继续完成。历史发布包保留。

## 本轮 NAS 业务复验

使用用户重新授权的隔离 profile，对 NAS 10.172.10.114 的本轮唯一前缀对象执行：专辑创建/已看/替换/解散、自定义专辑关联/回读/解除、合集 PNG 封面写入回读、直播文件导入/草稿/保存/删除、10字节前缀下载与20字节完整内容比对、私有无 tracker 种子解析/添加/回读/删除。没有操作报告历史残留、原有智能跳过开关或既有媒体。

路径管理回读适用于 type:300 自定义专辑；电影 type:100 的媒体关联并不生成自定义目录管理行。首轮夹具用 type:100 导致路径回读不满足预期，修正夹具类型后完成真机闭环；没有将首轮请求成功当作端到端通过。
