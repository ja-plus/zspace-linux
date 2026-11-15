# 客户端 0.2.1 发布说明

本次仅交付本地客户端包，不修改或部署 OAuth/NAS 服务端。授权码仍在当前交互隐藏输入。

## 平台范围

| 平台 | 发布定位 | 验证边界 |
| --- | --- | --- |
| macOS ARM64 | 正式支持 | 本机运行及安全进程检查 |
| macOS x64 | 兼容测试产物 | Apple Silicon Rosetta 检查；尚无 Intel 原生机器验收 |
| Linux x64 musl | 正式支持 | NAS 原生进程检查 |
| Linux ARM64 musl | 测试团队验收 | 交叉构建；待原生运行 |
| Windows x64 / ARM64 | 测试团队验收 | 交叉构建与 Clippy；待原生权限及运行验收 |

具体结果与产物哈希见 `client-validation-0.2.1.json` 和 `../release-manifest.json`。

## 升级变化

- 默认走可信 HTTPS 云代理；显式局域网 HTTP 需要每次传 `--host http://... --allow-insecure-http`。不关闭 HTTPS 证书检查。
- NAS 下载器可能发起联网的写操作默认拒绝，每次额外需要 `--allow-nas-fetch` 及原有 `--yes`。停止、删除等清理仍可用。此标志表示用户接受 NAS 执行连接的风险，不能保证服务端 DNS、重定向及 BT peer 被限制。
- raw 不接受令牌占位符或路径中的查询串；参数严格校验，NAS 路径拒绝编码穿越。
- 回收站全量恢复使用独立 `restore_all`；空 paths 不再代表恢复全部。
- 凭据使用唯一后端和会话权威标记，注销先失效再清理；请勿用旧版客户端混用本版 profile。
- 下载校验长度；分块续传需要强 ETag；不完整下载不覆盖原目标。
- 默认机密脱敏；`--show-secret` 不解除认证或已识别业务机密保护。

## 测试团队清单

1. 解压并校验 `bin/SHA256SUMS`，执行对应二进制 `--version`、`--help`、`catalog`。
2. 在隔离 profile 登录、doctor、文件上传/下载/比对、注销并重启确认失效。
3. Windows 用另一普通用户验证 profile/凭据不可读取；验证目录 reparse point、重命名竞态、已有 ACL 收紧及旧目标下载保护。运行源码 `scripts/check-security.sh` 的本机测试。
4. Linux ARM64 验证静态二进制、文件权限、符号链接拒绝和失败下载清理。
5. 检查下载写操作在缺少 `--allow-nas-fetch` 时返回 E_POLICY；读取、停止和删除不要求此开关。

测试团队完成并回传平台、版本、命令及脱敏结果后，才能扩大正式支持的平台。

## 限制

服务端令牌撤销、PKCE、服务端强制 state 和 NAS 网络隔离不属于本次客户端交付；客户端在换码前核对 HTTPS state 状态及 NAS 绑定，不宣称服务端已修复。
原生平台检查不等于全部业务端到端验收。Time Machine 实际备份、部分硬件和跨账号共享仍受测试设备/账号限制。
包未附平台代码签名或公证；SHA-256 用于完整性核对，不代表发布者身份认证。

依赖审计保留 `rsa 0.9.10 / RUSTSEC-2023-0071` 告警：受影响路径为私钥解密侧信道，生产代码仅执行公钥加密，私钥解密只在 cfg(test) 中使用。没有将此结果标为依赖零告警，后续仍需跟踪上游修复。

## 0.2.0 验收修复

HTTP host 在保存配置前校验，未显式允许时整体 E_POLICY 且不改变配置；已保存的不安全 host 不会再被 file space 的 warnings 掩盖。相册 ID 恢复 int|string。未知 action 在 --yes 之前校验。回收站 restore_all 按后端协议传 paths[]；ops vm vm_id 改为后端 name。
容器重建额外要求 recreate:true 并回显新旧 ID；info 增 app_id，未知对象 E_NOT_FOUND；stat 缓存缺失时返回明确 statistics_pending。doctor 只输出精简身份字段，省去头像。
Compose env_save 仍受 NAS 服务端缺陷影响，详见 acceptance-remediation-0.2.1.md；本版没有修改服务端，也没有声称该项已修复。
