# 0.2.0-rc.3 验收报告修复候选版

> 历史功能验收记录：其中阶段版本和“安全整改搁置”等描述仅对应当时状态。当前客户端 0.2.0 的安全处置、平台支持和限制以 `release-0.2.0.md` 为准。
2026-10-19（Asia/Shanghai）。本版专门处理rc.2验收报告中确认的问题，保留原rc.2包。
逐项核对结论与证据见[报告问题处理](rc2-report-remediation-2026-10-19.md)。
这仍是测试候选版，业务尚未全量完成，也不代表14项安全整改整体完成。

## 安装与验证

解压后保留完整zspace-nas-agent-toolkit目录；macOS/Linux使用scripts/zspace-cli，
Windows使用scripts/zspace-cli.ps1或.cmd。含macOS/Linux/Windows的ARM64与x64六种二进制。

```sh
scripts/zspace-cli --version
# zspace-cli 0.2.0-rc.3
(cd bin && shasum -a 256 -c SHA256SUMS)
scripts/zspace-cli catalog
```

ZIP外附SHA-256，bin内附六平台SHA256SUMS；哈希只保证完整性，不是发布者签名。
Windows可用Get-FileHash比对。没有代码签名/公证。
release-manifest.json记录源码快照、版本和架构；package-validation-0.2.0-rc.3.json记录实际运行范围。

## 复测重点

1. 按function-expansion.md的21字段示例创建独立专辑；缺少演职员/传错字段类型时检查诊断字段名。
2. 在独立目录重复file write：默认应E_EXISTS；rename:1返回实际改名路径；rename:2保留原内容；rename:3/overwrite:true覆盖，并读回内容。
3. slow_start返回task_id和taskid；slow_result/slow_stop接受其中任一名字及数字/十进制字符串。
4. ops vm/iscsi未知字段或伪造action应E_PARAM；业务快照变化应E_CONFLICT、对象不存在E_NOT_FOUND、接收共享未授权E_PERMISSION（新影视接口范围）。
5. share_create/shares默认不显示code/pass/能力链接，share_id仍可管理；显式--show-secret才能查看分享能力，OAuth凭证仍遮罩。测试输出不要公开。
6. 8字节伪造PNG、非法JPEG应在提交图片前失败；有效PNG/JPEG、本地/NAS来源仍应可用。
7. 图片URL只由CLI抓取并上传；私网/回环/链路本地、混合DNS/重定向私网都应拒绝。总时限8秒，cover最大3MiB/backdrop6MiB。
8. live_import_url的远程M3U/TXT必须auto_update:false，返回静态快照；single仅存公网URL且不播放。
   NAS代抓refresh、开启后台auto_update和保存仍开启自动更新的旧远程草稿会被拒绝。
   仍可读取、关闭更新、删除已有源。这是防止NAS重解析/重定向绕过的明确限制，不是“所有固件自动更新已安全”。
9. archive.extract的to是父目录，输出下按包名建子目录；返回destination_layout，browse.parent错误提示应标为包内路径。
10. review_project_create同时返回project_id/id，项目动作接受id别名；expected_*仍须从完整读取对象取得。

所有写入使用自己的临时对象和明确确认；异步操作查询终态，partial不能视为完成。
测试结束只清理本次对象，不全局清回收站或缓存；独立profile退出后检查logged_in=false。

## 验证与限制

本批192项单元测试、2项CLI契约测试及Clippy通过。真机33项修复复验见
acceptance-report-fixes-2026-10-19.json，包含失败与正常流程，全部临时对象和已知图片缓存清理完成。
公网重定向图片与远程片单使用公开HTTP测试资源，未启动外部媒体播放；未向外发送NAS凭证。
发布二进制冒烟另见package-validation-0.2.0-rc.3.json，不把既有业务证据冒充全部新包真实写入重跑。

外部报告已证明rc.2在Windows x64运行，但rc.3仍需Windows两架构及Linux ARM64原生回归。
macOS两架构与NAS Linux x64的发布二进制按实际包验证记录标注。

其余业务与14项历史安全待办继续按coverage-functions-2026-10-15.md和审计清单管理。
本轮不修改NAS固件，也不保证其他客户端或历史下载器具备本次影视URL限制。
Windows乱码历史文件仅为报告观察，未有本机可核对文件，未盲删凭证数据。
