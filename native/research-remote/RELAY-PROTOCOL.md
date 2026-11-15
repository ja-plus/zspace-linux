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