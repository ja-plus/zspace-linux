# libzrpc 远程中继协议（尖峰逆向结论，2026-11-13）

来源：mac libzrpc.dylib 完整符号 + 内嵌 protobuf 描述符 + 桌面 background.js。
全部离线静态分析。物料：build/research-material/libzrpc-mac-x64.dylib。

## 核心结论：远程可用原版登录 token，登录体验与原版一致

桌面 background.js 实证：`c.ztoken = a.a.state.user.token`。
中继连接鉴权 token **就是用户正常账号密码登录后拿到的 user.token**，
无需浏览器 OAuth、无需额外授权。shim 复用同一 token 即可。

## 服务器与配置（真实官方端点）

- SDK 配置：`https://zrpl-ucloud.zenithspace.net/platform/config/zrp/zrpc/v16`
- 调度服务器（下发中继集群地址）：`dispatch-bsnew.zenithspace.net`、
  `dispatch-cs.zenithspace.net:3388`、`dispatch-yz.zenithspace.net`、
  `dispatch.ud.link:3388`（端口 3388）
- 中继服务器：zrpl-relay-server（地址由 guide 响应动态下发）
- NAS 本地鉴权：`http://NAS:5055/auth/token`
- 源码路径线索：`/data/dev/zrp/src/zsi/layer/transport/zrps/`

## 连接流程（强制中继 force_tunnel 模式）

1. **Guide（取中继地址）**
   请求 `msg.tp.zrpc_connect_guide{ nas_id }`
   响应 `msg.tp.zrpc_guide_response{ nas_server_id, real_addr, useful_addr }`
2. **Connect（建隧道，带鉴权）**
   `msg.tp.zrpc_connect_request{ nas_id, neigh_ip, request_uuid, zrpc_addr, ztoken }`
   其中 **ztoken = state.user.token**（桌面登录 token）
   另有 `msg.tp.zrps_connect_request{ internal_id, nas_id, token, version_id }`
3. **自定义 TLS 握手（msg.tls，公钥密码）**
   - client_hello{ client_rand }
   - server_hello{ server_pub_key, server_rand }
   - client_key_exchange{ pre_master_secret, client_verify_data }
   - server_finish{ server_verify_data }
   对应桌面版 cloudPubKey / cloudPubKeyId / nasPubKey（公钥交换）。
4. **Greet + 转发**
   `msg.tp.zrpl_greet_request_v2{ http_header }` /
   `msg.tp.public_connect_greet_request_v2{ http_header, src_nas_id }`
   隧道建立后按 nas_id 转发 HTTP 流量到 NAS。

## 其它消息

- `zrpc_notify_request{ nas_id }`、`zrpc_response{ useful_addr }`
- `p2p_connect_request{ nas_id, neigh_ip, src_addr, ztoken }`（P2P 模式，比中继复杂，先不做）
- `isp_addr{ addr }`
- 信令：`msg.signal_request`、`process_signal_request`、`do_signal_notify`、
  `signal_current_cluster_id`、`guide_ttl_seconds`、`update_tunnel_guide_info`
- tracker token：`zsi::sysutils::gen_tracker_token(string)`

## 关键函数（mac 符号，供反汇编）

- `zsi::transport::zrps::signal_connection::{connect,start,do_signal_new_connect}`
- `zsi::transport::zrpc::tunnel_connection::connect(bool)`
- `zsi::transport::zrps::tunnel_connection::start`
- `zsi::transport::public_connection::{connect,init,on_handshake_complete}`
- `guide_connect(bool)`、`process_handshake_request/response`、
  `write_handshake_response(msg::tp::RESULT_CODE,...)`、`skip_handshake`
- `zsi::tls::tls_connection::handshake_context`

## 可行性判定

**远程可行，登录体验与原版统一。** 协议为 protobuf + 自定义 TLS 握手，
鉴权用桌面登录 token。主要实现工作量：
1. dispatch 信令（signal_request → guide 取中继地址）
2. relay connect（带 user.token）
3. **自定义 TLS 握手（msg.tls 公钥密码层）——最主要的密码学工作**
4. 隧道 HTTP 转发（接进现有 shim 代理）

proto 完整 schema 可从 dylib 内嵌 FileDescriptorProto 提取（字段号/类型）。
下一步：提取完整 proto + 反汇编握手密码学，评估自定义 TLS 复刻难度。

---

## 2026-11-15 复核：Windows DLL 未 strip，schema 与握手密码学已全部坐实

**物料更正**：之前结论「Windows dll stripped」不成立。`libzrpc_x64.dll`
（13.2 MB，来自 `app-64.7z` 的 `resources/app.asar.unpacked/sdk/`）
导出表有 **30,586 个完整 C++ 符号**，含全部 `zsi::tls` / `zsi::transport` /
`zsi::crypto` / `msg::tp` / `msg::tls` 成员及地址。因此不需要再下 mac 包（394 MB）。

### 完整 proto schema

已从 DLL 内嵌 `FileDescriptorProto` 提取，落盘为
`native/proto/message.tp.proto`、`native/proto/message.tls.proto`
（提取脚本 `build/research-material/extract_proto.py`，dump 在
`build/research-material/proto-dump.txt`）。要点：

- `zrpc_connect_guide{ nas_id=1, guide_ver=2 }`
- `zrpc_guide_response{ result_code=1, real_addr=2, nas_isp=3, zrpc_isp=4,
  useful_addr=5, cluster_id=6, nas_node_addr_list=7(required), nas_server_id=8,
  guide_ttl_seconds=9 }`
- `zrpc_connect_request{ nas_id=1, remote_port=2, service_type=3,
  request_purpose=4, device_type=5, neigh_ip=6, ztoken=7, zrpc_addr=10,
  request_uuid=11, is_forward=12, isp=13 }`
- `zrpl_greet_request_v2{ http_header=1, greet_type=2 }`
- `data_segment_body{ size=1, data=2 }`、`transport_header{ action_code=1 }`
- 枚举：`ACTION_CODE{AC_NOP=0,AC_DATA_SEGMENT=1,AC_KEEP_ALIVE=2,
  AC_NOTIFY_IDLE=3,AC_NOTIFY_BUSY=4}`；`REQUEST_PURPOSE` 中
  `RP_WEB_PROXY=23`、`RP_REMOTE_NAS_ACCESS=10`；`DEVICE_TYPE.DEVT_PC=5`

### msg.tls 握手（`zsi::tls::tls_connection`，逐函数核对）

1. **client_hello**（`write_client_hello`）
   `proto_version = 1`，`client_rand = BN_rand(256)+BN_bn2hex`
   → **64 位大写十六进制**；`encode_pubkey = true`。
2. **server_hello**（`read_server_hello`）
   `{ result_code, server_rand, server_pub_key }`；`server_pub_key` 是
   **PEM `-----BEGIN PUBLIC KEY-----`**（`PEM_read_bio_RSA_PUBKEY`）。
3. **pre_master_secret** = 同 `generate_rand` 的 64 位大写 hex；
   `encrypt_pre_master_secret` = `RSA_public_encrypt(..., PKCS1_PADDING)`
   → **base64**，填入 `client_key_exchange.pre_master_secret`。
4. **master_secret**（`generate_master_secret`）=
   `zsi::crypto::sha256::calc( client_rand ‖ server_rand ‖ pre_master_secret )`。
   ⚠️ **该库的 sha256 输出不是标准 hex**：`sha256::result` 用 lower-case 表，
   但**先写低半字节再写高半字节**（每字节 nibble 交换）。必须原样复刻。
5. **client_verify_data**（`generate_client_verify_data`）=
   `base64( aes::encrypt(key=master_secret, data=server_rand) )`。
6. **server_finish**（`generate_server_verify_data`）=
   `base64( aes::encrypt(key=master_secret, data=client_rand) )`；
   客户端用 `decrypt_data` 后与自己的 `client_rand` 比较
   （`check_server_verify_data`）。
7. **aes::encrypt 细节**（`zsi::crypto::aes`）：
   `digest = sha256(secret)`（同上 nibble 交换 hex，64 字符）→
   `key = digest[0..32]`（**ASCII 字符**）、`iv = digest[0..16]`（ASCII），
   **AES-256-GCM，16 字节 IV，不追加 auth tag**（`EVP_EncryptFinal` 后直接截断）。
   与我们已实现的 `decrypt_gcm_no_tag` 同一构造，可直接复用。

### 线上帧格式（`zsi::transport`）

- `message_io::async_write_message_list`：每个 protobuf 消息前加
  **4 字节大端长度**（`htonl(ByteSize)`），可连续拼多个消息。
- **信令前必须发问候**：`zrpl_greet_request{ greet_type }`，
  guide 用 `ZRPLGT_ZRPC_GUIDE=4`，connect 用 `ZRPLGT_ZRPC_CONNECT=1`。
  问候与具体消息打成一批（问候帧 + 消息帧）发出。
  **实测**：只发 guide 会被直接断开；`greet(4)+guide` 能收到
  `08 08`（`zrpc_guide_response.result_code=8`，即 nas 不在线）。
- 握手阶段：上述消息直接走该帧格式，**不加密、无 data-segment 外壳**。
- 握手完成（`on_handshaked`）后 `base_extranet_connection::async_write_data`
  把内存流按 **0x4000 字节**切块，每块 `aes::encrypt` 后包成两帧：
  `transport_header{AC_DATA_SEGMENT=1}` +
  `data_segment_body{ size = 明文块长, data = 密文 }`。
  隧道里的 HTTP 是裸字节流，没有额外 greet。

### 平台配置与 dispatch 地址（实测 2026-11-15）

`GET https://zrpl-ucloud.zenithspace.net/platform/config/zrp/zrpc/v16` 返回：

```json
{"servers":{"tracker":{"addr":"https://proxy.zenithspace.net"},
            "zrpl":{"addr":"dispatch-yz.zenithspace.net:9091"},
            "confirm_server":{"addr":"dispatch-cs.zenithspace.net:3388"},
            "p2p_stun":{...}},
 "http_config":{"Host":"www.baidu.com","User-Agent":"Mozilla/5.0 ..."}}
```

备用 dispatch（DLL 内置）：`dispatch-bsnew/cs/yz.zenithspace.net`、
`dispatch.ud.link`（3388 端口）。注意配置下发的 zrpl 端口是 **9091**。

### 实现状态（2026-11-15）

- `native/libzrpc/src/relay.rs`：proto 编解码、帧编解码、握手状态机、
  **标准 lowercase hex sha256**、session cipher（`sha256(master)` 再取 hex 切片）、
  PKCS#1 RSA、guide/connect 信令、带 fallback 的 dispatch 选择、`Tunnel` 数据面（加密切块）。
- `proxy.rs`：远程 NAS 走 `start_tunnel`，每个本地连接建一条隧道并双向透传。
- `ffi.rs`：`start_service` 在无局域网 IP 或 `connect_mode=1`（ForceExternal.txt）
  时走中继；**主代理（simpleStartSerivce）不传 ztoken，远程路径不能以 token
  缺失为门槛**；`libzrpc_set_private_network_access` 的 IP 会覆盖发现结果，
  返回值按 app 约定（>1 为成功）返回 2/1；`get_nas_status` 补齐
  `connected` / `conn_type` 字段。
- `native/run-native.sh`：启动前自动构建/复用 `libzrpc.so`，并调用
  `patches/patch_app.py`（linux dll 分支 + 删 win32 .node + 装 sdk/libzrpc.so），
  再启动 Electron。
- ABI：app 的 18 项 ffi 表逐项核对，release `.so` 全部存在且可调用
  （ctypes 冒烟通过）；额外导出 `set_sdk_location` / `get_ipv6_priority` 不影响。
- 测试：20 项单测全过，含 mock zrpl 全链路与 **FFI 端到端**
  （加密 start_service 配置 → 本地代理 → 中继 → 回显）；`live_dispatch_guide_probe`
  对官方 dispatch 实测 `guide result_code=8`。
- 仍缺：用真实 `ztoken` + 远程 NAS 跑通 connect 与隧道（需用户登录态）。

### 复用检查（对照 `build/research-reuse/opensource-mapping.md`）

- 该矩阵结论为「连接层需自研」：LAN 发现/本地代理按 purpose 重写，远程中继重写。
  与实现一致，连接层没有可直接替换的开源实现（协议私有）。
- 已按矩阵复用官方资产：官方 `app.asar` 原样 + Electron 33.3.1 linux-x64；
  `zsviewer` WASM、`node_modules` prebuilds、图标字体均不动。
- 我们的代理按 `remote_port` 泛化，原生支持矩阵里的 purpose 7（同步 9001）、
  purpose 8（rclone 9001）与 web 代理 5055，无需为每个业务另写一份。



### 2026-11-15 实机补正：guide 端口 / 握手帧 / 公钥编码 / 当前卡点

- guide 的 `real_addr`/`useful_addr` 只有 IP（例 `223.111.129.15`），
  **不带端口**；官方客户端沿用当前 dispatch 端口（实测 9091）。
- msg.tls 的 4 条握手消息必须包在 data-segment 帧里：
  `transport_header{AC_DATA_SEGMENT}` + `data_segment_body{size,data}`，
  其中 `data` = `[4B BE len][protobuf]`，握手阶段不加密。
  裸 `[4B len][msg]` 会被服务端忽略。
- `server_hello.server_pub_key` 是 **base64(PEM 文本)**，不是裸 PEM。
- 实机握手能走到 `server_finish`，但服务端回 `result_code=2`
  （RC_VERIFY_DATA_FAILED）；rc=2 同时覆盖 RSA 解密失败与 verify 不匹配。
  已排除了 hex 风格/拼接顺序/切片/RSA padding/字段顺序/encode_pubkey 等变体。
  下一步需用官方客户端 + 自控 mock 中继抓真实握手反推公式。

### 2026-11-15 实机攻破：rc=2 的两处根因

1. **sha256 hex 是标准 lowercase hex**，不是静态误判的 nibble 交换。
   换标准 hex 后真实中继握手通过（server_finish rc=0）。
2. **pre_master_secret 是双 base64**：`rsa::encrypt` 内部先 base64 一次，
   `encrypt_pre_master_secret` 外层再编码一次；服务端 `decrypt_pre_master_secret`
   → `base64::decode` → `rsa::decrypt`（内部再 decode 一次）与之对称。

固化后已删除所有临时 hex/顺序/切片/padding 调试开关。
当时遗留：握手成功后写入任何数据都会被对端立即关闭（0 字节 EOF）。
后经 mock 抓包与 connect 参数对齐，已在 2026-11-16 真实中继复测通过（见下）。

### 2026-11-15 mock 中继实证：握手后首帧（结论）

**方法**：官方 Windows 客户端跑在 Wine 隔离副本（`/tmp/zspace-probe-app` +
临时 prefix），把 `plat_url` 指到本机 mock HTTPS 配置服务；配置里的 zrpl 指向
本机 TCP mock 中继（自控 RSA 私钥/自签证书）。未改 hosts、未改系统证书信任、
未使用账号凭据、未连真实 NAS。脚本：`build/research-material/mock_relay.py`。

**抓到的完整流程**（官方客户端 → mock）：

1. `greet(4) + zrpc_connect_guide{nas_id, guide_ver=2}` → guide 连接用后即关。
2. 新连接 `greet(1) + zrpc_connect_request`：
   `0a065a50524f424510bf2718012002280568ffffffffffffffffff01`
   = `nas_id=ZPROBE, remote_port=5055, service_type=1, request_purpose=2,
   device_type=5, isp=-1`。
   **注意：没有 zrpc_addr(10)、request_uuid(11)、is_forward(12)。**
3. msg.tls 握手（data-segment 包裹）：client_hello → server_hello →
   client_key_exchange → server_finish；mock 校验 `client_verify_data` 为 True。
4. **握手后没有任何 greet/注册/信令**。客户端的第一条数据就是：

```
00 00 00 02 | 08 01                                   <- transport_header{AC_DATA_SEGMENT}
00 00 00 66 | 08 62 | 12 62 | <98 B ciphertext>       <- data_segment_body{size=98,data}
```

`size` = 明文长度；`data` = 同长度密文（AES-256-GCM 无 tag，CTR+J0）。
解密后就是应用层原始字节：`GET /home/static/version HTTP/1.1\r\nHost: ...`。

**反向同样成立**：mock 用同样的 `transport_header + data_segment_body` 加密回
`HTTP/1.1 200 OK`，本地 `curl` 通过 SDK 的 13595 代理收到了完整响应（rc=0）。
即隧道是**裸字节透传**，没有额外握手/注册步骤。

**已按实证对齐的代码**：
- `ConnectParams::new` 的 `request_purpose` 由 10 改为 **2**（RP_MESSAGE，
  即官方 `tcp_specific`/web 代理隧道的真实值）；`isp` 由 0 改为 **-1**（int32，
  10 字节 varint）；`zrpc_addr`/`request_uuid`/`is_forward` 按官方行为不再发送。
- 新增单测 `connect_request_matches_official_wire`，逐字节比对上面抓到的请求。
- `Tunnel::write_raw/read_raw` 的帧形态与实证一致，无需改动。

**对「真实中继 0 字节 EOF」的解释**：帧形态本身没有问题，剩下的差异在
connect 请求参数（purpose/isp/可选字段）。真实中继复测需要用户 NAS 在线，
本轮按约定不碰真实 NAS，留待后续。

### 2026-11-16 真实中继复测：通过

- 条件：用户确认 NAS 在线；只用本机日志里已有的 nas_id 做协议探测，
  **未使用账号凭据、未修改 NAS 任何内容**，只请求公开接口。
- 命令：`ZRPC_NAS_ID=<local-log nas_id> cargo test live_relay_http_probe -- --ignored --nocapture`
- 结果：
  - guide：`dispatch-yz.zenithspace.net:9091` → 中继节点 `223.111.129.16:9091`；
  - `connect ok`（鉴权/信令通过）；
  - `/key/pub?webagent=v2&rnd=probe` → `HTTP/1.1 200 OK`，779 字节，
    body 为 NAS 公钥 JSON；
  - `/home/static/version` → `HTTP/1.1 200 OK`，325 字节（`Content-Length: 14`）；
  - 不再出现握手后 0 字节 EOF。
- 真实链路上的帧形态（临时 `ZRPC_WIRE_LOG` 抓取，验证后已移除该调试代码）：
  - 上行：`00000002 0801` + `00000047 0843 1243 <67B cipher>`
    （`transport_header{AC_DATA_SEGMENT}` + `data_segment_body{size=67,data}`）；
  - 下行：`00000002 0801` + `0000014b 08c502 12c502 <325B cipher>`。
  - 与官方客户端抓包（`00000002 0801` + `00000066 0862 1262 <98B>`）同构，
    `size` 均为明文长度、密文同长。
- 结论：远程中继路径（guide → connect → msg.tls → 加密切块隧道）端到端可用。


