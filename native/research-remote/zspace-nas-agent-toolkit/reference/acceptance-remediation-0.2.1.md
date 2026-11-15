# 0.2.0 验收报告确认与客户端 0.2.1 处置

来源：用户提供的《zspace-nas-agent-toolkit-v0.2.0-验收报告-20261019.md》。报告为待核对证据，所附建议不等于执行服务端修改的授权。本次继续仅处理客户端，原 0.2.0 包保留。

| 编号 | 确认结论 | 0.2.1 处理与验证 |
| --- | --- | --- |
| V1 | 存在。main 保存 host 先于明文门禁，file space 还将策略拒绝作为部分查询 warnings | 保存配置前校验显式 host；已有 HTTP host 在执行 NAS 命令前整体拒绝；聚合读取不吞 E_POLICY。进程测试核对非零退出及 config 字节不变 |
| V2 | 参数文档缺失、未明确重建语义存在；保留 Image/挂载/环境变更同时保留容器 ID 不是 NAS 完整重建接口能提供的能力 | 继续调用已核对的服务端完整重建流程，但额外强制 recreate:true、顶层 name、config.Image；未确认重建直接 E_PARAM。返回旧/新 ID、recreated。原地资源配置用 hotupdate，不宣称完整配置可原地更新 |
| V3 | env_save 不通存在，但报告的客户端归因不成立 | CLI 只传 project_name/env_content。zdocker/api/compose_v2.go 的 SaveComposeEnv 把 project.FilePath 传给要求目录的 SaveComposeEnvToDisk；model/compose_project_v2.go 的读取还使用 env 而非 .env。客户端增加明确服务端失败提示，未改服务端，功能仍受影响；不能标为已修复 |
| V4 | 存在 | 在 --yes 门禁前按各域实际 serde action 枚举及扩展分发校验，旧式 match handler 使用明确 action 集合。未知 action 无论是否 --yes 均 E_PARAM，不发业务请求。真实写操作继续需要 --yes |
| V5 | 客户端参数行为与后端一致；文档分组易被误读 | downloader/doc/api.md 的 /tasks/details 无业务参数，只返回两队列长度。明确 details 只传 action；task_type/start/size 属于 directories，不为无分页接口伪造分页支持 |
| V6 | 存在文档漂移与不够具体的提示 | 云模式无 NAS 网页地址时 redirect_uri 明确必填，错误提示和文档提供填写格式，不要求为此配置 HTTP host |
| V7 | 非缺陷，报告的只读判断不成立 | time_sync 调用 /setting/time/sync 触发校时；lyrics_global(enabled) POST setting 修改 scrape_lyric。保留 --yes，补充与真正读取 get/timezones/settings 的区别；测试锁定写风险分类 |
| V8 | 未知字段错误分类存在；restore_all 实为协议错误，不仅是空仓库问题 | 未声明字段/类型错误改 E_PARAM，路径攻击和空 restore 仍按安全策略拒绝。restore_all 按 RecycleRestore handler 显式发送 paths[]=/.recycle/my，限定当前账号个人回收站，不把空表单当全量。模拟服务核对线协议 |
| V9 | 存在 | 当前入口与功能参考清除 rc.1 和安全整改搁置表述；当前版本 0.2.1，历史版本独立验收记录保留日期与版本上下文 |
| V10 | 存在 | entries/zspace-download.md 新增逐工具/action 的 --yes 与 --allow-nas-fetch 映射；示例补开关；明确不扩散到其他上传/备份工具 |
| V11 | 存在 | doctor 仅保留有限长度的身份字段，不返回完整 userinfo/头像/base64；单测核对裁剪 |
| V12 | 存在，安全 schema 误收紧 | album feeds / 对应 raw 路由的 album_id 接受整数或字符串，浮点与对象拒绝；模拟 HTTP 核对两种输入均正确发送 album_id=11 |
| V13 | 统计缓存缺失导致错误存在，是 NAS 统计缓存时序问题 | stat 遇到明确 not found 时再 inspect 核对；容器存在则返回 available:false、reason:statistics_pending、state、metrics:null，不伪造资源数字，不假称已完成资源采样。inspect 真实失败仍失败 |
| V14 | 存在参数名歧义及静默空结果 | 新增准确名称 app_id，保留 app_name 为商店 App ID 兼容别名；明确不接受容器名，名称先 list 换 ID。空对象返回 E_NOT_FOUND |

## 报告中的其他遗留项

- N1：确认后端 domainDetail 读取 name 而非 vm_id/id。ops vm 的 vm_id 现在映射 name，兼容非空字符串与正整数；模拟服务验证不再发送错误字段。
- N4：Windows 系统命令行长度上限非 CLI 能消除；文档说明大内容先写本地文件再 file upload，不声称 1 MiB JSON 上限可绕过 WinError 206。
- N10 / N15 / N16 / N24 / N25 / N26：分别对应 V8 / V3 / V2 / V4 / V7 / V5，按上表处置。
- N29：review_* 是 media video-libraries 的 action，并非独立命名工具；不为提高 catalog 数量添加重复入口，文档注明实际调用方式。catalog 仍为 91 项。
- N32 / N33：报告未复现或已修复，未据此引入无关修改。

## 验证边界

代码回归覆盖输入校验顺序、错误分类、配置不变性和请求/响应协议。平台构建与最终进程结果见 client-validation-0.2.1.json。没有把模拟测试写成新 NAS 真机写入验收，也未复跑报告全部约 660 次调用。
Windows x64 报告中的功能冒烟通过不等于第二用户 ACL/reparse 验收已完成；正式支持平台不因此扩大。

补充进程回归覆盖 54 个 action 型命名工具，分别带/不带 --yes，共 108 条未知 action 输入均返回 E_PARAM；这是输入门禁复测，不等于报告 403 条跨应用业务重跑。
