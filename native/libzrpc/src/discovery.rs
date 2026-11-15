//! LAN device discovery (UDP broadcast, port 5577).
//!
//! Recovered protocol (REPORT Phase 0, mac `discover_session::boardcast` ->
//! `discover_utils::make_discover_request` -> `make_cipher_key` + `aes::encrypt`
//! -> `udp_socket::broadcast`):
//!   request  = hex( AES-256-GCM( JSON {"key":"tdxnassearch"} ) )
//!   response = hex( AES-256-GCM( JSON {id/nas_id, ip, hostname, ...} ) )
//!   key/iv   = MT19937(4728423) derivation (same as JS service-config).
//!
//! NOTE: wire framing is inferred from static analysis; real-NAS interop is
//! confirmed by packet capture (task #12) before M4.

use std::net::UdpSocket;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::crypto;

pub const DISCOVERY_PORT: u16 = 5577;
pub const DISCOVERY_MAGIC: &str = "tdxnassearch";

/// Cache of discovered devices so start_service can resolve nas_id -> ip.
static CACHE: OnceLock<Mutex<Vec<Device>>> = OnceLock::new();

fn cache() -> &'static Mutex<Vec<Device>> {
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn remember(devices: &[Device]) {
    let mut c = cache().lock().unwrap();
    for d in devices {
        if !c.iter().any(|x| x.nas_id == d.nas_id) {
            c.push(d.clone());
        }
    }
}

/// Resolve a nas_id to an IP. A nas_id that already looks like an IP is returned
/// as-is (the JS sometimes passes the IP in the nas_id field).
pub fn lookup_ip(nas_id: &str) -> Option<String> {
    if nas_id.contains('.') || nas_id.contains(':') {
        return Some(nas_id.to_string());
    }
    cache()
        .lock()
        .unwrap()
        .iter()
        .find(|d| d.nas_id == nas_id)
        .map(|d| d.ip.clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub nas_id: String,
    pub ip: String,
    pub hostname: String,
    pub port: u16,
}

/// Build the hex-encoded, AES-256-GCM-encrypted discovery request.
pub fn make_request() -> String {
    let key = crypto::derive_key_js();
    let payload = json!({ "key": DISCOVERY_MAGIC }).to_string();
    let ct = crypto::encrypt_gcm(&key, payload.as_bytes());
    crypto::to_hex(&ct)
}

/// Parse a hex-encoded, encrypted discovery response into a Device.
pub fn parse_response(hex_payload: &str) -> Option<Device> {
    let key = crypto::derive_key_js();
    let ct = hex::decode(hex_payload.trim()).ok()?;
    let pt = crypto::decrypt_gcm(&key, &ct).ok()?;
    let v: Value = serde_json::from_slice(&pt).ok()?;
    let nas_id = v
        .get("nas_id")
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_str())?
        .to_string();
    let ip = v.get("ip").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let hostname = v
        .get("hostname")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let port = v.get("port").and_then(|x| x.as_u64()).unwrap_or(5055) as u16;
    Some(Device { nas_id, ip, hostname, port })
}

/// Encode a Device into an encrypted response (used by tests / mock NAS).
pub fn make_response(d: &Device) -> String {
    let key = crypto::derive_key_js();
    let payload = json!({
        "key": DISCOVERY_MAGIC,
        "nas_id": d.nas_id,
        "id": d.nas_id,
        "ip": d.ip,
        "hostname": d.hostname,
        "port": d.port,
    })
    .to_string();
    let ct = crypto::encrypt_gcm(&key, payload.as_bytes());
    crypto::to_hex(&ct)
}

/// Broadcast a discovery request and collect responses until `timeout`.
pub fn discover(timeout: Duration) -> std::io::Result<Vec<Device>> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_broadcast(true)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let req = make_request();
    sock.send_to(req.as_bytes(), ("255.255.255.255", DISCOVERY_PORT))?;

    let mut devices: Vec<Device> = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 65535];
    while Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, addr)) => {
                if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                    if let Some(mut d) = parse_response(s) {
                        if d.ip.is_empty() {
                            d.ip = addr.ip().to_string();
                        }
                        if !devices.iter().any(|x| x.nas_id == d.nas_id) {
                            devices.push(d);
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => break,
        }
    }
    remember(&devices);
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let req = make_request();
        // hex-decode + decrypt must yield the magic JSON.
        let ct = hex::decode(&req).unwrap();
        let pt = crypto::decrypt_gcm(&crypto::derive_key_js(), &ct).unwrap();
        let v: Value = serde_json::from_slice(&pt).unwrap();
        assert_eq!(v["key"], DISCOVERY_MAGIC);
    }

    #[test]
    fn response_parse() {
        let d = Device {
            nas_id: "Z04A123456".into(),
            ip: "192.168.1.50".into(),
            hostname: "zspace-nas".into(),
            port: 5055,
        };
        let wire = make_response(&d);
        let parsed = parse_response(&wire).expect("parse");
        assert_eq!(parsed, d);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_response("not-hex").is_none());
        assert!(parse_response("00112233").is_none());
    }
}