# 0.2.3-rc.1 验收报告核查与 rc.2 修复

依据用户提供的《zspace-nas-agent-toolkit-v0.2.3-rc.1-验收报告-20261020.md》。本轮仅改客户端并交付本地候选包，不修改服务端或报告中的已有资源。Windows 原生机器暂无，用户要求交付候选包和自检脚本供测试团队验收。

| 项目 | 核查结论与处理 |
| --- | --- |
| W1 / V33 | 确认 Windows 原生回归，报告提供同机 A/B，rc.1 的提前设置删除标记为明确新增路径。移除“创建后立即标记、安装前清除”的流程，保持临时文件可写，写完并同步后直接通过原句柄重命名；失败/退出时才通过原句柄清理。继续保留私有 DACL、reparse 拒绝及父目录句柄固定，没有改为关闭句柄后凭路径 MoveFileEx/删除。具体哪个 Win32 调用在该测试机返回 Access Denied 尚未原生跟踪，不能仅凭 A/B 冒称已精确定位系统调用。增加创建、同步、重命名的阶段/目标路径错误信息。最终是否恢复及清理是否可靠，必须运行 Windows 原生自检确认，不能把交叉构建视为修复验收 |
| V24 | 保留前缀完整比对与原目标保护，仍完整重传，不节省带宽。原生自检覆盖匹配成功、前缀不匹配保留旧目标、失败无目标和临时文件清理；本轮在 macOS/Linux 进程回归不代替 Windows 结果 |
| W3 | 确认文案退化。空 feed_ids 返回 E_PARAM 并明确“feed_ids 不能为空”；错误元素点名 feed_ids 并说明字符串/正整数类型，不回显值 |
| W4 | 报告的偶发 E_IO 有依据，但当前实现已有 profile 进程锁、3秒等待和25ms重试；不重复添加另一把锁。Windows 私有文件的只读句柄改为允许 FILE_SHARE_DELETE，使已有读取保留旧快照且允许原子替换；写/锁句柄和父目录固定仍不允许删除共享。句柄重命名仅对 ERROR_SHARING_VIOLATION/LOCK_VIOLATION（32/33）有上限重试；不把永久权限错误5无限重试或视为成功。补配置设置/清除循环、并发更新和 Windows 读取期间替换回归。偶发 E_NETWORK 无稳定复现，不掩盖为成功或扩大写请求重试 |
| 4.3 提示 | 直播缺字段使用具名 schema 提示；expected_draft 误传对象时明确必须为 live_draft.list 数组。补 m3u/txt 周期必填、single 可传0，以及 album delete + feed_ids 的准确入口 |
| W5 | 报告标注为既有契约/测试用法差异，本轮不改变 raw、恢复全量门禁或每次 --allow-nas-fetch 的要求。自检核对省略端口的 http host 仍显式规范化为 :80，且必须 --allow-insecure-http |
| V28 / 空目录 N001999 | 按报告，本测试机不具备所需能力，不纳入本轮客户端修复。此前现场已确认审阅依赖8235不可达、CLI删除失败来自前置 task/list，直接 project/delete 在自建夹具上成功；本轮不把 N001999 一概转换为“机型不支持”，也不绕过任务安全检查。须在支持对应能力的机型另行验收 |
| V3 | Compose env_save 保持服务端限制说明，不在本轮修改 |

Windows 文件共享语义参见 [Microsoft CreateFile 文档](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea)：删除共享也适用于重命名。文件句柄读取旧对象与替换后的新文件可以分别验证；这不意味着放开目录句柄的重命名保护。

## 给 Windows 测试团队

解压包，在普通用户 PowerShell 中执行（需要 Python 3.9+，不需要 NAS、OAuth 或现有 profile）：

```powershell
py -3 scripts/windows-write-selftest.py bin/zspace-cli-windows-x64.exe > windows-write-result.json
```

ARM64 原生机将二进制替换为 windows-arm64.exe。脚本只启动临时 127.0.0.1 HTTP 服务，使用合成凭据和独立临时 profile，退出清理测试目录。覆盖10组本地写入/进程验收：host 设置/清除各10次、默认端口、mode落盘、三目录各3次下载、覆盖保护、offset匹配与不匹配、网络截断失败清理、4进程各10次配置写入及注销。任何断言失败须保留异常信息，不能以生成了 JSON 文件即判断通过。

通过后再使用隔离 NAS profile 复验真实下载、覆盖、offset、host设置/清除。Windows 原生结果尚未取得时，本包继续是候选版。

## 数据与范围

没有通过业务账号登录或修改 NAS 业务对象；仅在已授权 Linux x64 验收机的唯一临时目录运行合成凭据进程自检并清理。没有处理报告中的历史审阅项目、直播源或相册。报告提到相册11/feed46已被验收操作删除，本轮不自动恢复、不触碰其他照片；今后删除验收只对专门创建的夹具对象执行，不能假定任意大整数 ID 永远不存在。

本轮完整结果见 client-validation-0.2.3-rc.2.json；旧 rc.1 运行包保留，便于原生 A/B。
