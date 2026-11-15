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
use std::os::raw::{c_char, c_int};
use std::sync::{Mutex, OnceLock};

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

fn log(msg: &str) {
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
                    "code": 200,
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
            // App expects code:200 and data.host="ip:port", data.port, data.id.
            ret_string(json!({
                "code": 200,
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
pub extern "C" fn libzrpc_check_ports(arg: *const c_char) -> *const c_char {
    log(&format!("check_ports {:?}", unsafe { arg_str(arg) }));
    ret_string(r#"{"code":0,"data":[]}"#.to_string())
}

// ---- discovery / status (M2 fills real behavior) -------------------------

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
    log(&format!("detect_nas {:?} {flag}", unsafe { arg_str(arg) }));
    ret_string(r#"{"code":0,"data":{}}"#.to_string())
}

#[no_mangle]
pub extern "C" fn libzrpc_get_nas_status() -> *const c_char {
    log("get_nas_status");
    let st = state().lock().unwrap();
    // Fields consumed by background.js:
    //   nas_id, local_network, connected, conn_type
    ret_string(json!({
        "code": 200,
        "data": {
            "nas_id": st.nas_id,
            "local_network": !st.remote,
            "connected": st.started,
            "conn_type": if st.started { if st.remote { "tunnel" } else { "lan" } } else { "" },
            "online": true
        }
    }).to_string())
}

#[no_mangle]
pub extern "C" fn libzrpc_check_conn_ability(arg: *const c_char, mode: c_int) -> *const c_char {
    log(&format!("check_conn_ability {:?} {mode}", unsafe { arg_str(arg) }));
    ret_string(r#"{"code":200,"data":{"ipv6":false,"pub":true}}"#.to_string())
}

// ---- diagnostics (stub) --------------------------------------------------

#[no_mangle]
pub extern "C" fn libzrpc_network_diagnose(arg: *const c_char) -> *const c_char {
    log(&format!("network_diagnose {:?}", unsafe { arg_str(arg) }));
    ret_string(r#"{"code":0,"data":{}}"#.to_string())
}

#[no_mangle]
pub extern "C" fn libzrpc_link_diagnose(arg: *const c_char) -> *const c_char {
    log(&format!("link_diagnose {:?}", unsafe { arg_str(arg) }));
    ret_string(r#"{"code":0,"data":{}}"#.to_string())
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