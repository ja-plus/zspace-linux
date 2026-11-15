# 0.2.1 验收报告确认与客户端 0.2.2 处置

依据用户提供的《zspace-nas-agent-toolkit-v0.2.1-验收报告-20261020.md》，逐项核对客户端、网页调用及 NAS 服务端源码。仅修复客户端；报告中的建议不作为修改服务端或既有业务对象的授权。原 0.2.1 包保留。

| 编号 | 确认结论 | 处理与验证 |
| --- | --- | --- |
| V15 | 存在，是前轮前置校验引入的回归。system path 在 docker::run 中单独处理，验证却只调用 HTTP request builder | 新增统一 Plan，校验与执行共用本地/HTTP 动作分发，恢复 system path。单测通过 run_named 的真实风险门禁执行私人/公共路径转换，确认免 --yes 且无业务 HTTP 请求；最终进程检查另用 docker system CLI 别名验证合法动作 |
| V16 | 存在。校验分支使用了不存在的内部工具名 download_manage，实际 catalog 名是 download_add | 绑定真实 download_add；带/不带 --yes 的未知 action 均 E_PARAM。单测核对 catalog 的 download/manage 映射；进程检查同时覆盖实际工具名和 CLI 别名 |
| V17 | 文档不够具体，但“资源字段均不支持”的推论不成立 | xnas-pc/src/components/docker/zContainer.vue 及 zdocker/model/container.go 确认资源在 config.HostConfig 下：NanoCpus、Memory、RestartPolicy.Name。补充准确层级、类型、单位及保留 Icon/Cron/其他资源值的说明；单测核对实际 JSON 请求，不宣称新做 NAS 资源写入验收 |
| V18 | 缺 host 的错误提示及默认端口显示存在体验问题；接受不带端口的 HTTP URL 本身符合 URL 语义，不是安全退化 | 单独降级标志返回 E_PARAM，明确要求本次 --host 并给出 --clear-host status 恢复方式。规范化地址显式保留有效端口，包括默认 :80/:443，:5055 等非默认端口不变。不允许只凭已保存的 host 加一个开关复用，继续每次明确指定目标；回归核对未保存降级许可、无许可整体拒绝及清除配置 |
| V19 | 存在 | 清理 zspace-media 当前交付表述；_index 指向 release-0.2.2；acceptance.md 重写当前流程，旧 RC 和 2026-10-10 等证据明确为历史记录。修正当前验收中的旧工具数、不安全 LAN 探测指导和首页列表清理推断 |
| V20 | 存在措辞歧义 | ops vm 的 vm_id 明确填写列表返回的 VM name（如 zspace_xxx），不是数字 ID。旧数字输入的序列化兼容不代表后端支持数字 ID 查询；当前入口与工具目录不再引导尝试正整数 |

## 测试方法修正

前轮 action-validation-0.2.1.py 把 download_manage 当作测试工具名；CLI 对不存在的工具同样返回 E_PARAM，因此前轮该项结果不能证明实际 download manage 的 action 已前置校验。此处是测试缺口，当前脚本已纠正为 download_add，并在测试前断言所有测试工具都属于真实 catalog。

当前进程回归包含 54 个真实工具 × 2 种 --yes 状态的未知 action 检查，再加 download manage 的 2 条 CLI 别名检查，以及 docker system path 的 2 条合法只读路径检查，共 112 条。拒绝测试之外补合法动作，防止白名单缺项被“全拒绝”掩盖。

清理核验应使用完整分页或精确 search/任务查询；不以首页 50 条 grep 判断不存在。报告指出的历史夹具由测试团队处理，本轮没有删除或改动其 NAS 既有对象。

## 保留范围

Compose env_save（V3）继续是已确认的 NAS 服务端缺陷，不修改、不冒称修复；客户端仍提供明确失败提示。授权码继续在当前交互隐藏输入，不另开终端。

最终构建、测试数量、平台边界见 client-validation-0.2.2.json。本轮没有重跑用户报告的约 620 次业务真机调用，不把协议模拟测试或 NAS 上的隔离 CLI 进程测试算作 NAS 业务端到端验收。
