# 安全连接与选路

以下描述对应 2026-10-19 安全整改源码。

`auto` 和 `cloud` 默认使用 `https://oauth.zconnect.cn`，附带 NAS ID 与端口 5055 的代理路由头。不再向 tracker 返回或缓存的 HTTP 地址发送令牌，也不使用“HTTP 200 即可信设备”的探测。

局域网直连需显式指定可信 HTTPS `--host`。TLS 证书必须通过标准链与主机名验证，没有关闭证书验证的入口。固定 host 只允许私网、loopback、ULA 或链路本地 IP 的 origin，不接受用户信息、路径、query 或 fragment。仅指定 `--mode lan` 而未配置可信 host 时安全拒绝。

确需兼容 HTTP 内网设备时，每次调用都必须同时显式传入 `--host http://...` 和 `--allow-insecure-http`。该降级许可不持久化，也不因为缓存或 `--yes` 自动开启；这会暴露令牌给可观察链路的主体，应由用户明确决定。

`--host` 和 `--mode` 保存到 profile；`--clear-host` 清除固定地址。OAuth、刷新与 NAS API 均禁止自动重定向，3xx 目标不会收到第二次请求。请求路由只允许同源规范路径。

只读请求可对明确授权失效或瞬时权限错误重试；写入请求不会因为网络错误自动重放，避免重复执行。授权码输入继续沿用当前交互，不打开额外终端。

## HTTP 降级参数与端口

`--allow-insecure-http` 必须同时带本次明确的 `--host http://NAS地址:端口`，单独使用返回 E_PARAM 并提示补齐方式；保存在 profile 的 HTTP host 不构成以后调用的降级同意。
URL 不写端口时，HTTP 标准默认是 80，HTTPS 是 443，并非自动推断 NAS 的 5055。当前规范化结果显式显示有效端口（包括 :80/:443），非默认端口如 :5055 会保留。
继续 HTTP 访问时，每次传上述双参数；恢复默认 HTTPS 云连接使用 `scripts/zspace-cli --profile <profile> --clear-host status`。两个降级/联网许可标志均不落盘。
