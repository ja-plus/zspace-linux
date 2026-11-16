# libzrpc FFI 行为对照（官方 DLL vs 本实现）

> 取证方式：在 Wine 隔离副本里启动官方 Windows 客户端，通过 Node inspector 直接调用
> `global._proxyDll` 暴露的 `libzrpc_*`；辅以 `libzrpc_x64.dll` 反汇编/字符串。
> 本机 mock：`/tmp/mock5055.py`（200 + `2.3.2026101601`）、`/tmp/mock5055_500.py`（500）、
> 本机监听端口（12345/5055）与不可达地址 `10.255.255.1`。
> 未使用账号凭据，未修改 NAS。

## get_nas_status

官方（无活动连接）：
```json
{"code":200,"data":{"conn_type":"none","connected":false,"local_network":false,"nas_id":""},"msg":"success"}
```
官方（本地代理已建隧道并保持一条连接）：
```json
{"code":200,"data":{"conn_type":"tunnel","connected":true,"local_network":false,"nas_id":"ZPROBE"},"msg":"success"}
```
本实现：同构；`conn_type` 取 `none` / `direct`（局域网）/ `tunnel`（中继），
**删除了此前自造的 `online` 字段**（官方没有）。`connected` 按真实活动连接计数，
不是“本地监听已启动”。

未实测：LAN `direct` 分支（静态字符串含 `direct`，未在真机抓到活动 LAN 连接）。

## detect_nas

官方成功（本机 mock 返回 200 + 版本体）：
```json
{"code":200,"data":{"is_v6":false,"nas_id":"2.3.2026101601","nas_ip":"127.0.0.1"},"msg":"success"}
```
官方失败（空输入 / 解析失败 / 连接拒绝 / HTTP 500）：
```json
{"code":404,"data":null,"msg":"failed: invalid str"}
```
行为：解析 host（按 is_v6 过滤地址族）→ `GET http://<ip>:5055/home/static/version`
（超时 5 s）→ 200 且响应体非空即成功，`nas_id` = 响应体。

本实现：`detect_nas_at(host, is_v6, 5055)`，输出与上表逐字段一致。
未实测：真实 NAS 的该接口（本机 mock 已覆盖）；`failed: can't connect to target ip ...`
分支（静态存在，未触发）。

## check_ports

输入 `{"ports":[...]}`，目标为 SDK 内部 `nas_ip`（由 `set_private_network_access`
或局域网发现提供）。

| 输入 | 官方 | 本实现 |
|---|---|---|
| 无 nas_ip | `{"allow":null,"deny":null,"msg":"failed","result":0}` | 同 |
| 全部拒绝 | `allow:[], deny:[...], msg:"partial", result:1`（反汇编；未真机抓） | 同 |
| 部分允许 | `{"allow":[12345],"deny":[5055,22],"msg":"partial","result":1}`（真机） | 同 |
| 全部允许 | `allow:[...], deny:[], msg:"success", result:2`（反汇编；未真机抓） | 同 |
| 空列表 | `allow:null, deny:null, msg:"failed", result:0` | 同 |

实现：对每个端口 `TcpStream::connect_timeout(800 ms)`。

## link_diagnose

输入 `{"hostnames":[...]}`；输出 `code` 是字符串 `"200"`。

| 场景 | 官方 | 本实现 |
|---|---|---|
| 成功 | `result:0, suggest:"成功"` | 同 |
| DNS 失败 | `result:10000, suggest:"无法解析域名，请稍后重试或是联系客服"` | 同 |
| TCP:80 失败 | `result:30000, suggest:"无法连接到路由器，请检查登录终端的相关设置"` | 同 |

`detail` 为人类可读分段文本，本实现按官方分段格式生成（内容不会导致结果码差异）。

## network_diagnose

输入 nas_id；输出 `code` 是字符串 `"200"`。

| 场景 | 官方 | 本实现 |
|---|---|---|
| 成功 | `result:0, reason:0, suggest:"成功"` | 同 |
| NAS 连接失败 | `result:3, reason:400, suggest:"内网IP连接失败，请重试或退出后重新登录"` | 同 |

`detail` 顺序：DNS(www.baidu.com) → TCP:80 → 中继连 NAS。

未实测：DNS 失败（`result:1, reason:100`）与公网不可达（`result:2, reason:200`）
两条分支的数值/文案。它们需要改主机解析或断网才能复现，本轮未改系统配置；
本实现按同一张官方 suggest 表填入，**数值待真机确认**。

## start_service

`libzrpc_start_service` 返回的 `code` 是**字符串** `"200"`，不是数字。
证据：`background.js` 的同步取端口逻辑 `Fe.startService()` 用严格比较
`if("200"===o.code) return parseInt(o.data.host.split(":")[1])`。
数字 200 会让它返回 undefined，同步设备地址写成
`tcp://127.0.0.1:undefined`（真机日志已复现）。
Web 代理路径用宽松比较 `200==h.code`，字符串同样兼容。

## 代码与测试

- `native/libzrpc/src/ffi.rs`：四个函数按上表实现；`get_nas_status` 去掉 `online`。
- `native/libzrpc/src/proxy.rs`：新增活动连接计数 `(lan, tunnel)`，供状态查询使用。
- 单测：`check_ports` 四种形态、`detect_nas` 200/500、`link_diagnose` 成功/DNS 失败、
  `get_nas_status` 字段集（断言无 `online`）。
- `cargo test`：30 passed / 2 ignored。
- 本机 ctypes 端到端（真实 `.so`）：上述官方输出逐字段复现；
  `network_diagnose` 对真实 NAS 走中继返回成功，对伪造 nas_id 返回 3/400。
