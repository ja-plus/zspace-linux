# 客户端 0.2.3-rc.4 登录机制复验候选版

本版修复rc.3登录机制验收的L1–L5，并明确L6/L7边界。Agent登录命令为：

```bash
scripts/zspace-cli --profile <name> login --no-open --auth-code-stdin
```

CLI在stdout输出结构化授权事件，Agent转交其中URL；用户完成网页授权并回复一次性码后，Agent只把码和换行写入当前进程stdin。进程等待180秒后自动退出；同profile并发login被profile锁拒绝；无控制台的默认隐藏输入模式在输出链接前给出具名错误。

OAuth协议、NAS业务接口和服务端均未修改。客户端不持久化授权码摘要；PKCE需要服务端协议支持，本版未宣称实现。逐项证据见 `acceptance-remediation-0.2.3-rc.4.md`。

0.2.3-rc.2的Windows本地写入、安全整改与业务验收边界继续有效。Windows原生W1/V24/V33及本轮stdin/超时/并发登录仍须测试团队复验；Linux ARM64和Intel Mac原生仍待验收。包无代码签名或公证，SHA-256只核对完整性。
