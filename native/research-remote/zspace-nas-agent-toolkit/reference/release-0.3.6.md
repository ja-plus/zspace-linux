# 0.3.6 验收修复

针对 0.3.5 Windows 原生验收报告：

- T1：去除 .NET Framework 不可靠的 ConsoleStream.CopyToAsync，子 CLI 直接继承原生 stdin 句柄。
- N1/N2：移除 Environment.Exit，脚本返回退出码并允许调用方 PowerShell 继续执行，消除该退出路径的额外开销。
- N3：等待子进程使用 finally 收尾，正常中断时尝试终止尚未结束的子进程。操作系统强杀宿主仍不能保证 finally 执行。
- N4：原子重命名对短暂访问拒绝（Windows error 5）使用现有最长三秒的有限重试。
- N5：更正 mkdir 文档；rename:0 阻止自动副本，部分固件对已有目录返回幂等成功。
- N7：删除预检文件时不再吞掉 Windows 删除句柄错误，失败会明确返回。
- N8：keyring 成功发布并删除旧文件凭据后，持久化 cleanup_pending:false，与登录输出一致。

N6：2.5 MiB 是 ZIP 大小要求；报告另提出解压后 3,000,000 字节容量建议。第三方许可证须完整保留，本版不删除法律声明。

以上是客户端实现修复，Windows PowerShell 5.1/CMD 原生 stdin、宿主继续执行、退出时延、写入及探针清理仍须验收复测。强杀宿主的进程树清理边界保持明确。NAS/OAuth 业务未变更。
