# 客户端 0.2.3-rc.2 复验候选版

本轮针对 rc.1 Windows 本地写入回归 W1：撤回临时文件提前设置删除标记的流程，保留句柄原子安装和失败清理；改善并发配置替换、具体输入提示与使用文档。前轮 multipart 和快照修复保留。

尚无 Windows 原生运行结果，因此不能声称 W1/V24/V33 已通过 Windows 验收。用户已安排将本地候选产物和自检脚本交给测试团队；详见 [逐项处置与运行步骤](acceptance-remediation-0.2.3-rc.2.md)。

## 本地自检

```powershell
py -3 scripts/windows-write-selftest.py bin/zspace-cli-windows-x64.exe > windows-write-result.json
```

需要 Python 3.9+；不需要 NAS 或授权码，使用本机 HTTP 夹具、合成凭据和隔离 profile。适用于普通用户 Windows NTFS；应在实际测试目录和干净目录分别执行。如失败请回传脱敏异常、Windows版本和目录文件系统类型。

原生开发环境可运行 cargo test，覆盖新建/覆盖/失败安装、失败清理、profile循环与并发更新；Windows 另有保持旧读句柄期间安装新配置的回归。

## 平台和业务边界

六个平台都提供二进制。macOS ARM64、Linux x64 检查具体结果见 client-validation-0.2.3-rc.2.json；macOS x64 仅 Rosetta，Intel 原生待验收。Windows x64/ARM64、Linux ARM64 继续交付测试团队原生验收，不以交叉编译替代。

按本轮外部验收报告，测试机的审阅能力受限；不在本轮修改 NAS 服务端，不将 N001999 全局当作机型不支持，也不绕过任务删除门禁。Compose env_save（V3）继续是既有服务端限制。未修改报告中的现有项目、照片或其他资源。

本轮没有重跑外部约430次业务调用。依赖集合未变，沿用2026-10-19 RustSec快照及 rsa 0.9.10 / RUSTSEC-2023-0071 的适用性说明，未宣称更新漏洞库。

授权码仍保留当前交互隐藏输入，不另开终端。包无签名/公证，SHA-256只核对完整性，不证明发布者身份。原 rc.1 包保留。
