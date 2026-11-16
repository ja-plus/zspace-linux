//! C ABI surface matching the official libzrpc exports.
//!
//! Signatures transcribed from background.js `new ffi.Library(le, {...})`
//! (REPORT §2.1). ffi-napi maps: "string" -> char*, "int" -> int32,
//! "bool" -> 1-byte C bool, "void" -> no return.
//!
//! M1: lifecycle + state + call logging are real; network behaviors are stubs
//! filled in M2 (discovery/proxy). Every call is logged to aid integration.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::os::raw::{c_char, c_int};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::crypto;

#[derive(Default)]
struct State {
    initialized: bool,
    data_dir: String,
    log_path: String,
    device_type: String,
    sdk_location: String,
    force_tunnel: bool,
    ipv6_priority: i32,
    nas_id: String,
    remote: bool,
    /// True while a start_service proxy is running (get_nas_status.connected).
    started: bool,
    /// IP pinned by `libzrpc_set_private_network_access` (IP-direct login).
    private_ip: String,
    private_port: i32,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

thread_local! {
    static RET: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Keep the returned C string alive until the next string-returning call on
/// this thread (ffi-napi reads it synchronously).
fn ret_string(s: String) -> *const c_char {
    let cs = CString::new(s).unwrap_or_default();
    let ptr = cs.as_ptr();
    RET.with(|r| *r.borrow_mut() = Some(cs));
    ptr
}

unsafe fn arg_str(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

pub(crate) fn log(msg: &str) {
    let path = state().lock().unwrap().log_path.clone();
    let line = format!("[libzrpc-native] {msg}\n");
    if !path.is_empty() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
    eprint!("{line}");
}

/// Minimal JSON string field extractor (avoids a serde dependency for M1).
fn json_str_field(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let idx = json.find(&pat)?;
    let rest = &json[idx + pat.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ---- lifecycle -----------------------------------------------------------

#[no_mangle]
pub extern "C" fn libzrpc_init(config: *const c_char) -> c_int {
    let cfg = unsafe { arg_str(config) };
    let mut st = state().lock().unwrap();
    st.data_dir = json_str_field(&cfg, "data_dir").unwrap_or_default();
    st.log_path = json_str_field(&cfg, "filepath").unwrap_or_default();
    st.initialized = true;
    drop(st);
    log(&format!("init data_dir={:?}", json_str_field(&cfg, "data_dir")));
    // Touch the crypto module so the key is derived once at startup (M2 uses it).
    let _ = crypto::derive_key_js();
    0
}

#[no_mangle]
pub extern "C" fn libzrpc_uninit() {
    log("uninit");
    state().lock().unwrap().initialized = false;
}

#[no_mangle]
pub extern "C" fn libzrpc_refresh() -> bool {
    log("refresh");
    true
}

// ---- service / proxy (M2 fills real behavior) ----------------------------

#[no_mangle]
pub extern "C" fn libzrpc_start_service(enc_cfg: *const c_char) -> *const c_char {
    let b64 = unsafe { arg_str(enc_cfg) };
    log(&format!("start_service (encrypted, {} bytes)", b64.len()));
    // DEBUG: dump the exact input for offline decryption analysis (M4).
    let _ = std::fs::write("/tmp/zrpc-input.b64", &b64);
    let key = crypto::derive_key_js();
    let decoded = match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        Ok(d) => d,
        Err(e) => {
            log(&format!("start_service: bad base64: {e}"));
            return ret_string(json!({"code":1,"msg":"bad base64"}).to_string());
        }
    };
    // The app's encodeService omits the GCM auth tag (concat([update,final])
    // only), so decrypt as tag-less GCM (CTR + J0). Fall back to tagged GCM.
    let pt = crypto::decrypt_gcm_no_tag(&key.key, &key.iv, &decoded);
    let cfg: Value = match serde_json::from_slice(&pt) {
        Ok(v) => v,
        Err(_) => match crypto::decrypt_gcm(&key, &decoded) {
            Ok(p) => serde_json::from_slice(&p).unwrap_or(Value::Null),
            Err(_) => {
                log("start_service: decrypt failed");
                return ret_string(json!({"code":1,"msg":"decrypt failed"}).to_string());
            }
        },
    };
    let local_port = cfg["local_port"].as_u64().unwrap_or(13579) as u16;
    let remote_port = cfg["remote_port"].as_u64().unwrap_or(5055) as u16;
    let nas_id = cfg["nas_id"].as_str().unwrap_or("").to_string();

    // IP pinned by libzrpc_set_private_network_access wins over discovery
    // (the app uses it for IP-direct login). Empty => auto/LAN discovery.
    let (pinned_ip, device_type) = {
        let st = state().lock().unwrap();
        (st.private_ip.clone(), st.device_type.clone())
    };
    let discovered = crate::discovery::lookup_ip(&nas_id).unwrap_or_default();
    let ip = if !pinned_ip.is_empty() {
        pinned_ip
    } else {
        discovered
    };

    // Remote NAS (behind NAT) or ForceExternal.txt (connect_mode=1): the
    // official client goes through the zrpl relay keyed by nas_id. A ztoken is
    // only present for the remote-access/cloud-phone helpers; the main web
    // proxy sends none, so its absence must not block the relay path.
    let forced_external = cfg["connect_mode"].as_u64().unwrap_or(0) == 1;
    if forced_external || ip.is_empty() {
        let params = crate::relay::ConnectParams {
            nas_id: nas_id.clone(),
            ztoken: cfg["ztoken"].as_str().unwrap_or("").to_string(),
            remote_port: remote_port as u32,
            service_type: cfg["type"].as_u64().unwrap_or(1) as u32,
            // The JS start_service config omits request_purpose for tcp_specific
            // services; the official SDK then sends RP_MESSAGE (2). Only sync
            // (7) and rclone (8) pass an explicit value.
            request_purpose: cfg["request_purpose"].as_u64().unwrap_or(2) as u32,
            device_type: if device_type == "mac" { 4 } else { 5 },
            neigh_ip: cfg["neigh_ip"].as_str().unwrap_or("").to_string(),
        };
        {
            let mut st = state().lock().unwrap();
            st.nas_id = nas_id.clone();
            st.remote = true;
        }
        return match crate::proxy::start_tunnel(local_port, params) {
            Ok(_) => {
                state().lock().unwrap().started = true;
                log(&format!(
                    "start_service: relay tunnel 127.0.0.1:{local_port} -> nas {nas_id:?}"
                ));
                ret_string(json!({
                    // Official returns the code as a string here: the sync path
                    // checks `"200"===o.code` (strict) before parsing data.host.
                    "code": "200",
                    "data": {
                        "host": format!("127.0.0.1:{local_port}"),
                        "port": local_port,
                        "id": format!("native-{local_port}"),
                    }
                }).to_string())
            }
            Err(e) => {
                log(&format!("start_service: relay bind failed: {e}"));
                ret_string(json!({"code":1,"msg":format!("bind failed: {e}")}).to_string())
            }
        };
    }

    let scheme = cfg["scheme"].as_str().unwrap_or("https");
    let target = format!("{scheme}://{ip}:{remote_port}");
    {
        let mut st = state().lock().unwrap();
        st.nas_id = nas_id.clone();
        st.remote = false;
    }
    match crate::proxy::start(local_port, target.clone()) {
        Ok(_) => {
            state().lock().unwrap().started = true;
            log(&format!("proxy 127.0.0.1:{local_port} -> {target}"));
            // App expects code:"200" and data.host="ip:port", data.port, data.id.
            ret_string(json!({
                "code": "200",
                "data": {
                    "host": format!("127.0.0.1:{local_port}"),
                    "port": local_port,
                    "id": format!("native-{local_port}"),
                }
            }).to_string())
        }
        Err(e) => {
            log(&format!("start_service: bind failed: {e}"));
            ret_string(json!({"code":1,"msg":format!("bind failed: {e}")}).to_string())
        }
    }
}

#[no_mangle]
pub extern "C" fn libzrpc_stop_service(id: *const c_char) {
    let v = unsafe { arg_str(id) };
    log(&format!("stop_service {v:?}"));
    if let Ok(port) = v.parse::<u16>() {
        crate::proxy::stop(port);
    }
    state().lock().unwrap().started = false;
}

#[no_mangle]
pub extern "C" fn libzrpc_stop_all_service() {
    log("stop_all_service");
    crate::proxy::stop_all();
    state().lock().unwrap().started = false;
}

#[no_mangle]
pub extern "C" fn libzrpc_get_nas_status() -> *const c_char {
    log("get_nas_status");
    ret_string(nas_status_json().to_string())
}

/// Official shape (captured from libzrpc_x64.dll):
///   {"code":200,"data":{"conn_type":"none|direct|tunnel","connected":bool,
///    "local_network":bool,"nas_id":"..."},"msg":"success"}
/// `connected`/`conn_type` reflect an established NAS connection, not just a
/// started local listener; there is no `online` field.
fn nas_status_json() -> Value {
    let (lan_active, tunnel_active) = crate::proxy::active_counts();
    let st = state().lock().unwrap();
    let (conn_type, connected, local_network, nas_id) = if tunnel_active > 0 {
        ("tunnel", true, false, st.nas_id.clone())
    } else if lan_active > 0 {
        ("direct", true, true, st.nas_id.clone())
    } else {
        ("none", false, false, String::new())
    };
    json!({
        "code": 200,
        "data": {
            "conn_type": conn_type,
            "connected": connected,
            "local_network": local_network,
            "nas_id": nas_id
        },
        "msg": "success"
    })
}

#[no_mangle]
pub extern "C" fn libzrpc_check_conn_ability(arg: *const c_char, mode: c_int) -> *const c_char {
    log(&format!("check_conn_ability {:?} {mode}", unsafe { arg_str(arg) }));
    ret_string(r#"{"code":200,"data":{"ipv6":false,"pub":true}}"#.to_string())
}

// ---- probes / diagnostics (official-compatible) --------------------------
//
// Return shapes below were captured by calling the official Windows
// libzrpc_x64.dll through the Electron main-process inspector:
//   detect_nas       : resolve host -> GET http://<ip>:5055/home/static/version
//   check_ports      : TCP connect to the SDK's current nas_ip
//   link_diagnose    : resolve -> TCP :80 -> optional ttl check
//   network_diagnose : DNS(www.baidu.com) -> TCP :80 -> NAS relay connect

const DETECT_PATH: &str = "/home/static/version";

fn resolve_all(host: &str, port: u16) -> Vec<IpAddr> {
    let mut out = Vec::new();
    if let Ok(iter) = (host, port).to_socket_addrs() {
        for sa in iter {
            if !out.contains(&sa.ip()) {
                out.push(sa.ip());
            }
        }
    }
    out
}

fn pick_addr(addrs: &[IpAddr], v6: Option<bool>) -> Option<IpAddr> {
    addrs
        .iter()
        .find(|ip| v6.map(|want| ip.is_ipv6() == want).unwrap_or(true))
        .copied()
}

fn tcp_ok(ip: IpAddr, port: u16, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&SocketAddr::new(ip, port), timeout).is_ok()
}

fn ip_literal(ip: IpAddr) -> String {
    match ip {
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

fn http_get(url: &str, timeout: Duration) -> Option<(u16, String)> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()
        .ok()?;
    let resp = client.get(url).send().ok()?;
    let status = resp.status().as_u16();
    let body = resp.text().unwrap_or_default();
    Some((status, body))
}

/// `libzrpc_detect_nas(host, is_v6)`.
///
/// Official success: `{"code":200,"data":{"is_v6":b,"nas_id":"<body>",
/// "nas_ip":"<ip>"},"msg":"success"}`. Any failure observed (empty input,
/// resolve failure, connection refused, HTTP != 200) returns
/// `{"code":404,"data":null,"msg":"failed: invalid str"}`.
fn detect_nas_at(host: &str, want_v6: bool, port: u16) -> Value {
    let host = host.trim();
    if host.is_empty() {
        return json!({"code":404,"data":null,"msg":"failed: invalid str"});
    }
    let addrs = resolve_all(host, port);
    for ip in addrs {
        if ip.is_ipv6() != want_v6 {
            continue;
        }
        let url = format!("http://{}:{port}{DETECT_PATH}", ip_literal(ip));
        if let Some((status, body)) = http_get(&url, Duration::from_secs(5)) {
            let body = body.trim();
            if status == 200 && !body.is_empty() {
                return json!({
                    "code": 200,
                    "data": {"is_v6": ip.is_ipv6(), "nas_id": body, "nas_ip": ip.to_string()},
                    "msg": "success"
                });
            }
        }
    }
    json!({"code":404,"data":null,"msg":"failed: invalid str"})
}

/// Official `check_ports` result:
///   both empty -> {allow:null,deny:null,msg:"failed",result:0}
///   any denied -> {allow:[...],deny:[...],msg:"partial",result:1}
///   all allowed-> {allow:[...],deny:[],msg:"success",result:2}
fn check_ports_impl(nas_ip: &str, ports: &[u16]) -> Value {
    if nas_ip.is_empty() {
        return json!({"allow": Value::Null, "deny": Value::Null, "msg": "failed", "result": 0});
    }
    let mut addrs = resolve_all(nas_ip, 5055);
    if addrs.is_empty() {
        if let Ok(ip) = nas_ip.parse::<IpAddr>() {
            addrs.push(ip);
        }
    }
    let mut allow: Vec<u16> = Vec::new();
    let mut deny: Vec<u16> = Vec::new();
    for &p in ports {
        if addrs.iter().any(|ip| tcp_ok(*ip, p, Duration::from_millis(800))) {
            allow.push(p);
        } else {
            deny.push(p);
        }
    }
    let (msg, result) = if allow.is_empty() && deny.is_empty() {
        ("failed", 0)
    } else if !deny.is_empty() {
        ("partial", 1)
    } else {
        ("success", 2)
    };
    let (allow, deny) = if allow.is_empty() && deny.is_empty() {
        (Value::Null, Value::Null)
    } else {
        (json!(allow), json!(deny))
    };
    json!({"allow": allow, "deny": deny, "msg": msg, "result": result})
}

/// Official `link_diagnose` result (code is a string "200"):
///   success : result 0,     suggest "成功"
///   DNS fail: result 10000, suggest "无法解析域名，请稍后重试或是联系客服"
///   TCP fail: result 30000, suggest "无法连接到路由器，请检查登录终端的相关设置"
fn link_diagnose_impl(hostnames: &[String], port: u16) -> Value {
    let mut detail = String::new();
    let mut items = Vec::new();
    for host in hostnames {
        detail.push_str("-----------------------------\r\nchecking domain resolving ...\r\n");
        let addrs = resolve_all(host, port);
        if addrs.is_empty() {
            detail.push_str(&format!(
                "resolving {host} ...\r\nfailed to resolve {host}: no address\r\n"
            ));
            items.push(json!({
                "hostname": host, "ip": "", "result": 10000,
                "suggest": "无法解析域名，请稍后重试或是联系客服"
            }));
            continue;
        }
        detail.push_str(&format!("resolving {host} ...\r\nresolved {host}:\r\n"));
        for ip in &addrs {
            detail.push_str(&format!("  {ip}\r\n"));
        }
        let ip = pick_addr(&addrs, Some(false))
            .or_else(|| pick_addr(&addrs, Some(true)))
            .unwrap();
        detail.push_str(
            "------------------------------\r\nchecking if the target endpoint is accessible...\r\n",
        );
        detail.push_str(&format!("connecting {ip}:{port} ...\r\n"));
        if tcp_ok(ip, port, Duration::from_secs(3)) {
            detail.push_str(&format!("connected to {ip}...\r\n"));
            items.push(json!({
                "hostname": host, "ip": ip.to_string(), "result": 0, "suggest": "成功"
            }));
        } else {
            detail.push_str(&format!("failed to connect ...{ip}\r\n"));
            detail.push_str(
                "-----------------------------\r\nchecking if the ttl is accessible...\r\n",
            );
            detail.push_str(&format!(
                "link diagnose target {host} ttl...\r\nfailed to connect, timeout..., ttl=2\r\n"
            ));
            items.push(json!({
                "hostname": host, "ip": ip.to_string(), "result": 30000,
                "suggest": "无法连接到路由器，请检查登录终端的相关设置"
            }));
        }
    }
    json!({"code":"200","data":{"detail":detail,"hostnames":items},"msg":"success"})
}

/// Official `network_diagnose` result (code is a string "200"):
///   success    : result 0, reason 0,   suggest "成功"
///   NAS failed : result 3, reason 400, suggest "内网IP连接失败，请重试或退出后重新登录"
///
/// The DNS (reason 100) and internet (reason 200) failure branches are wired to
/// the same suggest table but were not reproducible without changing the host
/// resolver/network, so their numeric codes are unverified.
fn network_diagnose_impl(nas_id: &str) -> Value {
    let mut detail = String::from("--------\r\nchecking domain resolution...\r\n");
    let addrs = resolve_all("www.baidu.com", 80);
    if addrs.is_empty() {
        detail.push_str("failed to resolve www.baidu.com\r\n");
        return json!({
            "code": "200",
            "data": {"detail": detail, "reason": 100, "result": 1,
                     "suggest": "无法解析域名，请检查后重试"},
            "msg": "success"
        });
    }
    detail.push_str("resolving www.baidu.com ...\r\nresolved www.baidu.com:\r\n");
    for ip in &addrs {
        detail.push_str(&format!("  {ip}\r\n"));
    }
    detail.push_str("--------\r\nchecking if the internet is accessible...\r\n");
    let probe_ip = pick_addr(&addrs, Some(false)).unwrap_or(addrs[0]);
    detail.push_str("connecting www.baidu.com:80 ...\r\n");
    if !tcp_ok(probe_ip, 80, Duration::from_secs(3)) {
        detail.push_str("failed to connect www.baidu.com\r\n");
        return json!({
            "code": "200",
            "data": {"detail": detail, "reason": 200, "result": 2,
                     "suggest": "无法连接网络，请检查终端配置或稍后重试"},
            "msg": "success"
        });
    }
    detail.push_str("connected www.baidu.com\r\n");
    detail.push_str("--------\r\nchecking if the nas is connectable...\r\n");
    detail.push_str(&format!(
        "connecting to nas [{}:5055] (connect_mode: 0, zrpl: dispatch-yz.zenithspace.net) ...\r\n",
        nas_id
    ));
    let params = crate::relay::ConnectParams::new(nas_id.to_string(), "");
    match crate::relay::open_tunnel(None, &params) {
        Ok(t) => {
            t.shutdown();
            detail.push_str(
                "connected nas via [dispatch-yz.zenithspace.net].\r\nconnect to the nas successfully.\r\n",
            );
            json!({
                "code": "200",
                "data": {"detail": detail, "reason": 0, "result": 0, "suggest": "成功"},
                "msg": "success"
            })
        }
        Err(e) => {
            detail.push_str(&format!(
                "failed to connect nas via [dispatch-yz.zenithspace.net]: {e}\r\n"
            ));
            json!({
                "code": "200",
                "data": {"detail": detail, "reason": 400, "result": 3,
                         "suggest": "内网IP连接失败，请重试或退出后重新登录"},
                "msg": "success"
            })
        }
    }
}

#[no_mangle]
pub extern "C" fn libzrpc_check_ports(arg: *const c_char) -> *const c_char {
    let raw = unsafe { arg_str(arg) };
    log(&format!("check_ports {raw:?}"));
    let ports: Vec<u16> = serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|v| v.get("ports").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|p| p.as_u64())
        .map(|p| p as u16)
        .collect();
    let (pinned, nas_id) = {
        let st = state().lock().unwrap();
        (st.private_ip.clone(), st.nas_id.clone())
    };
    let nas_ip = if !pinned.is_empty() {
        pinned
    } else {
        crate::discovery::lookup_ip(&nas_id).unwrap_or_default()
    };
    ret_string(check_ports_impl(&nas_ip, &ports).to_string())
}

// ---- discovery / status --------------------------------------------------

#[no_mangle]
pub extern "C" fn libzrpc_search_device() -> *const c_char {
    log("search_device");
    let devices =
        crate::discovery::discover(std::time::Duration::from_secs(2)).unwrap_or_default();
    let data: Vec<serde_json::Value> = devices
        .iter()
        .map(|d| {
            serde_json::json!({
                "nas_id": d.nas_id, "ip": d.ip, "hostname": d.hostname, "port": d.port,
            })
        })
        .collect();
    log(&format!("search_device found {} device(s)", data.len()));
    ret_string(serde_json::json!({ "code": 0, "data": data }).to_string())
}

#[no_mangle]
pub extern "C" fn libzrpc_detect_nas(arg: *const c_char, flag: bool) -> *const c_char {
    let host = unsafe { arg_str(arg) };
    log(&format!("detect_nas {host:?} {flag}"));
    ret_string(detect_nas_at(&host, flag, 5055).to_string())
}

// ---- diagnostics ---------------------------------------------------------

#[no_mangle]
pub extern "C" fn libzrpc_network_diagnose(arg: *const c_char) -> *const c_char {
    let nas_id = unsafe { arg_str(arg) };
    log(&format!("network_diagnose {nas_id:?}"));
    ret_string(network_diagnose_impl(&nas_id).to_string())
}

#[no_mangle]
pub extern "C" fn libzrpc_link_diagnose(arg: *const c_char) -> *const c_char {
    let raw = unsafe { arg_str(arg) };
    log(&format!("link_diagnose {raw:?}"));
    let hostnames: Vec<String> = serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|v| v.get("hostnames").and_then(|h| h.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|h| h.as_str().map(|s| s.to_string()))
        .collect();
    ret_string(link_diagnose_impl(&hostnames, 80).to_string())
}

// ---- settings ------------------------------------------------------------

#[no_mangle]
pub extern "C" fn libzrpc_set_device_type(t: *const c_char) {
    let v = unsafe { arg_str(t) };
    log(&format!("set_device_type {v:?}"));
    state().lock().unwrap().device_type = v;
}

#[no_mangle]
pub extern "C" fn libzrpc_set_sdk_location(loc: *const c_char) {
    let v = unsafe { arg_str(loc) };
    log(&format!("set_sdk_location {v:?}"));
    state().lock().unwrap().sdk_location = v;
}

#[no_mangle]
pub extern "C" fn libzrpc_set_private_network_access(
    a: *const c_char,
    b: c_int,
    c: bool,
    d: bool,
) -> c_int {
    let ip = unsafe { arg_str(a) };
    let has_ip = !ip.is_empty();
    log(&format!("set_private_network_access {ip:?} {b} {c} {d}"));
    {
        let mut st = state().lock().unwrap();
        st.private_ip = ip;
        st.private_port = b;
    }
    // The app treats `> 1` as success (`setPrivateNetworkAccess ... i > 1`).
    if has_ip {
        2
    } else {
        1
    }
}

#[no_mangle]
pub extern "C" fn libzrpc_set_ipv6_priority(v: c_int) {
    log(&format!("set_ipv6_priority {v}"));
    state().lock().unwrap().ipv6_priority = v;
}

#[no_mangle]
pub extern "C" fn libzrpc_get_ipv6_priority() -> c_int {
    state().lock().unwrap().ipv6_priority
}

#[no_mangle]
pub extern "C" fn libzrpc_get_force_tunnel() -> bool {
    state().lock().unwrap().force_tunnel
}

#[no_mangle]
pub extern "C" fn libzrpc_set_force_tunnel(v: bool) {
    log(&format!("set_force_tunnel {v}"));
    state().lock().unwrap().force_tunnel = v;
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn closed_port() -> u16 {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    }

    fn http_once(status: u16, body: &'static str) -> (u16, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        (port, h)
    }

    #[test]
    fn check_ports_matches_official_shapes() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let open = l.local_addr().unwrap().port();
        let closed = closed_port();

        let all = check_ports_impl("127.0.0.1", &[open]);
        assert_eq!(all["result"], 2);
        assert_eq!(all["msg"], "success");
        assert_eq!(all["deny"].as_array().unwrap().len(), 0);

        let partial = check_ports_impl("127.0.0.1", &[open, closed]);
        assert_eq!(partial["result"], 1);
        assert_eq!(partial["msg"], "partial");
        assert_eq!(partial["allow"].as_array().unwrap().len(), 1);
        assert_eq!(partial["deny"].as_array().unwrap().len(), 1);

        let none = check_ports_impl("127.0.0.1", &[]);
        assert_eq!(none["result"], 0);
        assert_eq!(none["msg"], "failed");
        assert!(none["allow"].is_null());

        let no_ip = check_ports_impl("", &[open]);
        assert_eq!(no_ip["result"], 0);
        assert!(no_ip["allow"].is_null());
        drop(l);
    }

    #[test]
    fn detect_nas_reads_version_body() {
        let (port, h) = http_once(200, "2.3.2026101601");
        let v = detect_nas_at("127.0.0.1", false, port);
        h.join().unwrap();
        assert_eq!(v["code"], 200);
        assert_eq!(v["data"]["nas_id"], "2.3.2026101601");
        assert_eq!(v["data"]["nas_ip"], "127.0.0.1");
        assert_eq!(v["data"]["is_v6"], false);
        assert_eq!(v["msg"], "success");
    }

    #[test]
    fn detect_nas_rejects_non_200() {
        let (port, h) = http_once(500, "error");
        let v = detect_nas_at("127.0.0.1", false, port);
        h.join().unwrap();
        assert_eq!(v["code"], 404);
        assert_eq!(v["msg"], "failed: invalid str");
    }

    #[test]
    fn nas_status_has_official_shape() {
        let st = nas_status_json();
        assert_eq!(st["code"], 200);
        assert_eq!(st["msg"], "success");
        assert_eq!(st["data"]["conn_type"], "none");
        assert_eq!(st["data"]["connected"], false);
        assert_eq!(st["data"]["local_network"], false);
        assert_eq!(st["data"]["nas_id"], "");
        // The official DLL has no `online` field.
        assert!(st["data"].get("online").is_none());
    }

    #[test]
    fn link_diagnose_shapes() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let ok = link_diagnose_impl(&["127.0.0.1".to_string()], port);
        assert_eq!(ok["code"], "200");
        assert_eq!(ok["data"]["hostnames"][0]["result"], 0);
        assert_eq!(ok["data"]["hostnames"][0]["suggest"], "成功");
        drop(l);

        let bad = link_diagnose_impl(&["no-such-host.invalid".to_string()], 80);
        assert_eq!(bad["data"]["hostnames"][0]["result"], 10000);
        assert_eq!(
            bad["data"]["hostnames"][0]["suggest"],
            "无法解析域名，请稍后重试或是联系客服"
        );
    }
}
