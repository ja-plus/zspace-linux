//! Remote relay (force-tunnel) handshake and transport framing.
//!
//! Everything here is verified against the official Windows `libzrpc_x64.dll`
//! (2026-11-15). The DLL exports ~30.6k C++ symbols, so the recovered behavior
//! is read off the real code paths instead of guessed:
//!
//! * `tls_connection::write_client_hello` — `client_hello{ proto_version = 1,
//!   client_rand = 64-char random hex, encode_pubkey = true }`
//! * `tls_connection::generate_rand` — OpenSSL `BN_rand(…, 256, -1, 1)` +
//!   `BN_bn2hex` (UPPERCASE hex, 64 chars)
//! * `tls_connection::generate_master_secret` — `zsi::crypto::sha256::calc(
//!   client_rand || server_rand || pre_master_secret )`, hex string
//! * `tls_connection::generate_client_verify_data` —
//!   `base64( aes::encrypt(master_secret, server_rand) )`
//! * `tls_connection::generate_server_verify_data` —
//!   `base64( aes::encrypt(master_secret, client_rand) )`
//! * `rsa::load_public_key_from_str` — `PEM_read_bio_RSA_PUBKEY`
//!   (`-----BEGIN PUBLIC KEY-----`)
//! * `rsa::encrypt` — `RSA_public_encrypt(..., RSA_PKCS1_PADDING)` + base64
//! * `zsi::crypto::sha256::calc` — standard lowercase hex. (Static reading
//!   once suggested nibble-swapped output; the live relay rejected that and
//!   accepted standard hex, so [`handshake_sha256_hex`] is authoritative.)
//! * `tls_connection::encrypt_data` — `aes::encrypt(key = master_secret, …)`
//!   and `aes::encrypt` = SHA-256(master_secret) → key = hex_ascii[0..32],
//!   iv = hex_ascii[0..16], AES-256-GCM, no auth tag.
//! * `message_io::async_write_message_list` — each protobuf message is framed
//!   with a 4-byte big-endian length (`htonl(ByteSize)`).
//! * `base_extranet_connection::async_write_data` — payload is chunked into
//!   ≤ 0x4000-byte segments, each wrapped as
//!   `transport_header{action_code = AC_DATA_SEGMENT}` +
//!   `data_segment_body{ size = plaintext_len, data = encrypted_chunk }`.
//!
//! The handshake messages themselves use the same framing but are not encrypted
//! until `on_handshaked` installs the data-segment encoder.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use base64::Engine as _;
use rand_core::OsRng;
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};

use crate::crypto::{self, CipherKey};

/// `msg.tls.client_hello.proto_version` observed on the wire.
pub const TLS_PROTO_VERSION: u32 = 1;
/// `msg.tp.ACTION_CODE.AC_DATA_SEGMENT`
pub const AC_DATA_SEGMENT: u32 = 1;
/// Data segments are split at 0x4000 bytes of plaintext.
pub const MAX_PLAINTEXT_SEGMENT: usize = 0x4000;

// ---------------------------------------------------------------------------
// Minimal protobuf wire codec (only what these messages need).
// ---------------------------------------------------------------------------

pub fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn put_key(out: &mut Vec<u8>, field: u32, wire: u8) {
    put_varint(out, ((field as u64) << 3) | wire as u64);
}

pub fn put_uint32(out: &mut Vec<u8>, field: u32, v: u32) {
    put_key(out, field, 0);
    put_varint(out, v as u64);
}

pub fn put_bool(out: &mut Vec<u8>, field: u32, v: bool) {
    put_uint32(out, field, v as u32);
}

pub fn put_bytes(out: &mut Vec<u8>, field: u32, b: &[u8]) {
    put_key(out, field, 2);
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

pub fn put_str(out: &mut Vec<u8>, field: u32, s: &str) {
    put_bytes(out, field, s.as_bytes());
}

enum PbValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

struct PbReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> PbReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut shift = 0;
        let mut v = 0u64;
        loop {
            let b = *self.buf.get(self.pos)?;
            self.pos += 1;
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }

    fn next(&mut self) -> Option<(u32, PbValue<'a>)> {
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        match key & 7 {
            0 => Some((field, PbValue::Varint(self.varint()?))),
            2 => {
                let len = self.varint()? as usize;
                let end = self.pos.checked_add(len)?;
                let bytes = self.buf.get(self.pos..end)?;
                self.pos = end;
                Some((field, PbValue::Bytes(bytes)))
            }
            1 => {
                self.pos += 8;
                Some((field, PbValue::Varint(0)))
            }
            5 => {
                self.pos += 4;
                Some((field, PbValue::Varint(0)))
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// msg.tls messages
// ---------------------------------------------------------------------------

pub fn encode_client_hello(client_rand: &str) -> Vec<u8> {
    let mut out = Vec::new();
    put_uint32(&mut out, 1, TLS_PROTO_VERSION);
    put_str(&mut out, 2, client_rand);
    put_bool(&mut out, 3, true); // encode_pubkey
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ServerHello {
    pub result_code: u32,
    pub server_rand: String,
    pub server_pub_key: String,
}

pub fn parse_server_hello(buf: &[u8]) -> Result<ServerHello, String> {
    let mut r = PbReader::new(buf);
    let mut msg = ServerHello::default();
    while let Some((field, value)) = r.next() {
        match (field, value) {
            (1, PbValue::Varint(v)) => msg.result_code = v as u32,
            (2, PbValue::Bytes(b)) => msg.server_rand = String::from_utf8_lossy(b).into_owned(),
            (3, PbValue::Bytes(b)) => msg.server_pub_key = String::from_utf8_lossy(b).into_owned(),
            _ => {}
        }
    }
    Ok(msg)
}

pub fn encode_client_key_exchange(
    result_code: u32,
    pre_master_secret_b64: &str,
    client_verify_data_b64: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    // The official client omits result_code when it is RC_SUCCESS (proto2 default).
    if result_code != 0 {
        put_uint32(&mut out, 1, result_code);
    }
    put_str(&mut out, 2, pre_master_secret_b64);
    put_str(&mut out, 3, client_verify_data_b64);
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ServerFinish {
    pub result_code: u32,
    pub server_verify_data: String,
}

pub fn parse_server_finish(buf: &[u8]) -> Result<ServerFinish, String> {
    let mut r = PbReader::new(buf);
    let mut msg = ServerFinish::default();
    while let Some((field, value)) = r.next() {
        match (field, value) {
            (1, PbValue::Varint(v)) => msg.result_code = v as u32,
            (2, PbValue::Bytes(b)) => msg.server_verify_data = String::from_utf8_lossy(b).into_owned(),
            _ => {}
        }
    }
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Transport framing (msg.tp)
// ---------------------------------------------------------------------------

/// Prefix a serialized protobuf message with its 4-byte big-endian length.
pub fn encode_frame(msg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + msg.len());
    out.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    out.extend_from_slice(msg);
    out
}

/// Incremental decoder for `[u32 BE len][protobuf]` frames.
#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Pop one complete message, if available.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        if self.buf.len() < 4 {
            return None;
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if self.buf.len() < 4 + len {
            return None;
        }
        let msg = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Some(msg)
    }
}

pub fn encode_transport_header(action_code: u32) -> Vec<u8> {
    let mut out = Vec::new();
    put_uint32(&mut out, 1, action_code);
    out
}

pub fn parse_transport_header(buf: &[u8]) -> u32 {
    let mut r = PbReader::new(buf);
    while let Some((field, value)) = r.next() {
        if field == 1 {
            if let PbValue::Varint(v) = value {
                return v as u32;
            }
        }
    }
    0
}

pub fn encode_data_segment_body(size: u32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put_uint32(&mut out, 1, size);
    put_bytes(&mut out, 2, data);
    out
}

pub fn parse_data_segment_body(buf: &[u8]) -> Result<(u32, Vec<u8>), String> {
    let mut r = PbReader::new(buf);
    let mut size = 0u32;
    let mut data = Vec::new();
    while let Some((field, value)) = r.next() {
        match (field, value) {
            (1, PbValue::Varint(v)) => size = v as u32,
            (2, PbValue::Bytes(b)) => data = b.to_vec(),
            _ => {}
        }
    }
    Ok((size, data))
}

/// Wrap one payload chunk into `transport_header` + `data_segment_body` frames.
///
/// `cipher = None` sends plaintext (handshake phase, before `on_handshaked`).
pub fn encode_data_segment(plaintext: &[u8], cipher: Option<&CipherKey>) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in plaintext.chunks(MAX_PLAINTEXT_SEGMENT) {
        let body = match cipher {
            Some(k) => crypto::encrypt_gcm_no_tag(&k.key, &k.iv, chunk),
            None => chunk.to_vec(),
        };
        out.extend_from_slice(&encode_frame(&encode_transport_header(AC_DATA_SEGMENT)));
        out.extend_from_slice(&encode_frame(&encode_data_segment_body(
            chunk.len() as u32,
            &body,
        )));
    }
    out
}

/// Decrypt one `data_segment_body.data` payload.
pub fn decode_data_segment(data: &[u8], cipher: Option<&CipherKey>) -> Vec<u8> {
    match cipher {
        Some(k) => crypto::decrypt_gcm_no_tag(&k.key, &k.iv, data),
        None => data.to_vec(),
    }
}

// ---------------------------------------------------------------------------
// Handshake crypto
// ---------------------------------------------------------------------------

/// `zsi::crypto::sha256::result` emits each digest byte **low nibble first**
/// (lowercase table). This is not standard `hex::encode`, but both peers use
/// the same routine, so we must reproduce the quirk byte-for-byte.
/// `zsi::crypto::sha256::calc` output as used by the handshake. Verified
/// against the live relay: the server accepts **standard lowercase hex**
/// (`hex::encode`), not a nibble-swapped variant.
pub fn handshake_sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

/// Key/iv derivation used by `zsi::crypto::aes::encrypt`:
/// sha256(secret) → hex → key = ascii hex[0..32], iv = ascii hex[0..16].
pub fn session_cipher(secret: &str) -> CipherKey {
    let hexstr = handshake_sha256_hex(secret.as_bytes());
    let mut key = [0u8; 32];
    let mut iv = [0u8; 16];
    key.copy_from_slice(&hexstr.as_bytes()[0..32]);
    iv.copy_from_slice(&hexstr.as_bytes()[0..16]);
    CipherKey { key, iv }
}

/// `generate_master_secret`: sha256(client_rand || server_rand || pre_master)
/// as the swapped hex string.
pub fn master_secret(client_rand: &str, server_rand: &str, pre_master_secret: &str) -> String {
    let mut concat = String::with_capacity(
        client_rand.len() + server_rand.len() + pre_master_secret.len(),
    );
    concat.push_str(client_rand);
    concat.push_str(server_rand);
    concat.push_str(pre_master_secret);
    handshake_sha256_hex(concat.as_bytes())
}

/// `generate_{client,server}_verify_data`: base64(AES-GCM(master_secret, data)).
pub fn verify_data(master_secret: &str, data: &str) -> String {
    let k = session_cipher(master_secret);
    let ct = crypto::encrypt_gcm_no_tag(&k.key, &k.iv, data.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(ct)
}

/// `rsa::encrypt`: base64(RSA_PKCS1(server_pub_key, pre_master_secret)).
///
/// The official `encrypt_pre_master_secret` encodes **twice**: `rsa::encrypt`
/// already returns base64(ciphertext), then the caller base64-encodes that
/// string again. The server mirrors it (base64::decode → rsa::decrypt, which
/// base64-decodes once more). Sending a single base64 makes the server's RSA
/// decrypt fail and it answers `server_finish.result_code = 2`.
pub fn encrypt_pre_master(
    server_pub_key_pem: &str,
    pre_master_secret: &str,
) -> Result<String, String> {
    let key =
        RsaPublicKey::from_public_key_pem(server_pub_key_pem.trim()).map_err(|e| e.to_string())?;
    let ct = key
        .encrypt(&mut OsRng, Pkcs1v15Encrypt, pre_master_secret.as_bytes())
        .map_err(|e| e.to_string())?;
    let once = base64::engine::general_purpose::STANDARD.encode(ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(once.as_bytes()))
}

/// `server_pub_key` arrives base64-encoded (base64 of the PEM text). Accept
/// both forms so a future server change does not break us.
pub fn normalize_server_pub_key(key: &str) -> String {
    let k = key.trim();
    if k.contains("BEGIN") {
        return k.to_string();
    }
    match base64::engine::general_purpose::STANDARD.decode(k) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => k.to_string(),
    }
}

/// `tls_connection::generate_rand`: BN_rand(256) + BN_bn2hex → 64 upper-case
/// hex chars.
pub fn rand_hex_256() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02X}")).collect())
}

/// Client side of the msg.tls handshake.
pub struct Handshake {
    pub client_rand: String,
    pub pre_master_secret: String,
    pub server_rand: String,
    pub server_pub_key: String,
    pub master_secret: String,
    pub cipher: CipherKey,
}

impl Handshake {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            client_rand: rand_hex_256()?,
            pre_master_secret: rand_hex_256()?,
            server_rand: String::new(),
            server_pub_key: String::new(),
            master_secret: String::new(),
            cipher: CipherKey {
                key: [0u8; 32],
                iv: [0u8; 16],
            },
        })
    }

    pub fn client_hello(&self) -> Vec<u8> {
        encode_client_hello(&self.client_rand)
    }

    /// Consume `server_hello`; derives the master secret and session cipher.
    pub fn accept_server_hello(&mut self, msg: &[u8]) -> Result<(), String> {
        let sh = parse_server_hello(msg)?;
        if sh.result_code != 0 {
            return Err(format!("server_hello result_code={}", sh.result_code));
        }
        if sh.server_rand.is_empty() || sh.server_pub_key.is_empty() {
            return Err("server_hello missing rand/pubkey".into());
        }
        self.server_rand = sh.server_rand;
        self.server_pub_key = normalize_server_pub_key(&sh.server_pub_key);
        self.master_secret = master_secret(
            &self.client_rand,
            &self.server_rand,
            &self.pre_master_secret,
        );
        self.cipher = session_cipher(&self.master_secret);
        Ok(())
    }

    pub fn client_key_exchange(&self) -> Result<Vec<u8>, String> {
        let pre_master_b64 = encrypt_pre_master(&self.server_pub_key, &self.pre_master_secret)?;
        let verify = verify_data(&self.master_secret, &self.server_rand);
        Ok(encode_client_key_exchange(0, &pre_master_b64, &verify))
    }

    /// Verify `server_finish.server_verify_data` == AES-GCM(master, client_rand).
    pub fn verify_server_finish(&self, msg: &[u8]) -> Result<bool, String> {
        let sf = parse_server_finish(msg)?;
        if sf.result_code != 0 {
            return Err(format!("server_finish result_code={}", sf.result_code));
        }
        let raw = base64::engine::general_purpose::STANDARD
            .decode(sf.server_verify_data.trim())
            .map_err(|e| e.to_string())?;
        let pt = crypto::decrypt_gcm_no_tag(&self.cipher.key, &self.cipher.iv, &raw);
        Ok(pt == self.client_rand.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// msg.tp signaling: guide -> connect
// ---------------------------------------------------------------------------

/// Platform config endpoint that hands out the zrpl (dispatch/relay) address.
pub const PLATFORM_CONFIG_URL: &str =
    "https://zrpl-ucloud.zenithspace.net/platform/config/zrp/zrpc/v16";
/// Fallback dispatch nodes baked into the official client.
pub const DISPATCH_FALLBACKS: &[&str] = &[
    "dispatch-yz.zenithspace.net",
    "dispatch-bsnew.zenithspace.net",
    "dispatch-cs.zenithspace.net:3388",
    "dispatch.ud.link:3388",
];

/// `zrpc_connect_guide{ nas_id = 1, guide_ver = 2 }`
pub fn encode_connect_guide(nas_id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    put_str(&mut out, 1, nas_id);
    put_uint32(&mut out, 2, 2); // guide_ver = 2 (zrpc_connect_guide.guide_ver)
    out
}

/// `zrpl_greet_request{ greet_type = 1 }` — the relay expects this short
/// greeting before each signaling message (verified against the official
/// dispatch: guide without the greeting is dropped, with it answers
/// `08 08` = RC_ZRPC2ZRPL_GUIDE_ERROR for an unknown nas_id).
pub const ZRPLGT_ZRPC_CONNECT: u32 = 1;
pub const ZRPLGT_ZRPC_GUIDE: u32 = 4;

pub fn encode_greet(greet_type: u32) -> Vec<u8> {
    let mut out = Vec::new();
    put_uint32(&mut out, 1, greet_type);
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct GuideResponse {
    pub result_code: u32,
    pub real_addr: String,
    pub useful_addr: String,
    pub cluster_id: i32,
    pub nas_server_id: String,
    pub guide_ttl_seconds: u32,
    /// `nas_node_addr_list` entries: (isp, addr)
    pub node_addrs: Vec<(i32, String)>,
}

pub fn parse_guide_response(buf: &[u8]) -> GuideResponse {
    let mut r = PbReader::new(buf);
    let mut msg = GuideResponse::default();
    while let Some((field, value)) = r.next() {
        match (field, value) {
            (1, PbValue::Varint(v)) => msg.result_code = v as u32,
            (2, PbValue::Bytes(b)) => msg.real_addr = String::from_utf8_lossy(b).into_owned(),
            (5, PbValue::Bytes(b)) => msg.useful_addr = String::from_utf8_lossy(b).into_owned(),
            (6, PbValue::Varint(v)) => msg.cluster_id = v as i32,
            (7, PbValue::Bytes(b)) => {
                let mut ir = PbReader::new(b);
                let mut isp = 0i32;
                let mut addr = String::new();
                while let Some((f, v)) = ir.next() {
                    match (f, v) {
                        (1, PbValue::Varint(x)) => isp = x as i32,
                        (2, PbValue::Bytes(x)) => addr = String::from_utf8_lossy(x).into_owned(),
                        _ => {}
                    }
                }
                msg.node_addrs.push((isp, addr));
            }
            (8, PbValue::Bytes(b)) => msg.nas_server_id = String::from_utf8_lossy(b).into_owned(),
            (9, PbValue::Varint(v)) => msg.guide_ttl_seconds = v as u32,
            _ => {}
        }
    }
    msg
}

/// Parameters for one tunnel to the NAS (mirrors the JS `start_service` config).
#[derive(Clone, Debug, Default)]
pub struct ConnectParams {
    pub nas_id: String,
    pub ztoken: String,
    /// 5055 for the NAS HTTP API.
    pub remote_port: u32,
    pub service_type: u32,
    pub request_purpose: u32,
    pub device_type: u32,
    pub neigh_ip: String,
}

impl ConnectParams {
    pub fn new(nas_id: impl Into<String>, ztoken: impl Into<String>) -> Self {
        Self {
            nas_id: nas_id.into(),
            ztoken: ztoken.into(),
            remote_port: 5055,
            service_type: 1,
            // Captured from the official client's tcp_specific service
            // (the local web-proxy tunnel): request_purpose = RP_MESSAGE.
            request_purpose: 2,
            device_type: 5, // DEVT_PC
            neigh_ip: String::new(),
        }
    }
}

/// `zrpc_connect_request` — every field the client sets, in proto order.
///
/// Wire-identical to the official `tcp_specific` service request captured
/// against a local mock relay:
/// `0a065a50524f424510bf2718012002280568ffffffffffffffffff01`
/// (nas_id, remote_port=5055, service_type=1, request_purpose=2,
/// device_type=5, isp=-1; no zrpc_addr/request_uuid/is_forward).
pub fn encode_connect_request(p: &ConnectParams, zrpc_addr: &str, request_uuid: &str) -> Vec<u8> {
    let mut out = Vec::new();
    put_str(&mut out, 1, &p.nas_id);
    put_uint32(&mut out, 2, p.remote_port);
    put_uint32(&mut out, 3, p.service_type);
    put_uint32(&mut out, 4, p.request_purpose);
    put_uint32(&mut out, 5, p.device_type);
    if !p.neigh_ip.is_empty() {
        put_str(&mut out, 6, &p.neigh_ip);
    }
    if !p.ztoken.is_empty() {
        put_str(&mut out, 7, &p.ztoken);
    }
    if !zrpc_addr.is_empty() {
        put_str(&mut out, 10, zrpc_addr);
    }
    if !request_uuid.is_empty() {
        put_str(&mut out, 11, request_uuid);
    }
    // isp is int32 and the official client sends -1 (10-byte varint).
    put_key(&mut out, 13, 0);
    put_varint(&mut out, u64::MAX);
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ZrpcResponse {
    pub result_code: u32,
    pub useful_addr: String,
    pub cluster_id: i32,
}

pub fn parse_zrpc_response(buf: &[u8]) -> ZrpcResponse {
    let mut r = PbReader::new(buf);
    let mut msg = ZrpcResponse::default();
    while let Some((field, value)) = r.next() {
        match (field, value) {
            (1, PbValue::Varint(v)) => msg.result_code = v as u32,
            (2, PbValue::Bytes(b)) => msg.useful_addr = String::from_utf8_lossy(b).into_owned(),
            (3, PbValue::Varint(v)) => msg.cluster_id = v as i32,
            _ => {}
        }
    }
    msg
}

/// Fetch the zrpl dispatch address from the official platform config.
///
/// Response shape (verified 2026-11-15):
/// `{"servers":{"zrpl":{"addr":"dispatch-yz.zenithspace.net:9091"},...}}`
pub fn fetch_dispatch_addr() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(false)
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let text = client.get(PLATFORM_CONFIG_URL).send().ok()?.text().ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let addr = v["servers"]["zrpl"]["addr"].as_str()?.trim();
    if addr.is_empty() {
        None
    } else {
        Some(addr.to_string())
    }
}

/// Cached dispatch address (one platform-config fetch per process).
pub fn dispatch_addr() -> Option<String> {
    static CACHE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| fetch_dispatch_addr().or_else(|| DISPATCH_FALLBACKS.first().map(|s| s.to_string())))
        .clone()
}

/// Guide responses carry only an IP (`real_addr`/`useful_addr` = "1.2.3.4").
/// The official client connects to the assigned relay on the **same port as
/// the dispatch it is talking to** (observed: 9091). Keep an explicit port when
/// present, otherwise append `default_port`.
pub fn ensure_port(addr: &str, default_port: u16) -> String {
    let a = addr.trim();
    if a.parse::<std::net::SocketAddr>().is_ok() {
        return a.to_string();
    }
    // host:port (IPv4/hostname) — keep when the suffix is a valid port
    if let Some((host, port)) = a.rsplit_once(':') {
        if !host.is_empty() && port.parse::<u16>().is_ok() {
            return a.to_string();
        }
    }
    format!("{a}:{default_port}")
}

fn port_of(addr: &str) -> Option<u16> {
    addr.trim()
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
}

fn connect_tcp(addr: &str, timeout: Duration) -> std::io::Result<TcpStream> {
    let mut last_err = None;    for sa in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&sa, timeout) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty address")
    }))
}

fn write_frame(stream: &mut TcpStream, msg: &[u8]) -> std::io::Result<()> {
    stream.write_all(&encode_frame(msg))
}

fn read_frame(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let n = u32::from_be_bytes(len) as usize;
    if n > 8 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// Handshake messages travel **inside data-segment frames** (verified: a bare
/// `[len][msg]` frame gets no reply from the relay). Before the session cipher
/// exists the data segment payload is plaintext.
fn send_handshake_message(stream: &mut TcpStream, msg: &[u8]) -> std::io::Result<()> {
    let payload = encode_frame(msg);
    stream.write_all(&encode_data_segment(&payload, None))
}

fn read_handshake_message(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    loop {
        let header = read_frame(stream)?;
        let body = read_frame(stream)?;
        match parse_transport_header(&header) {
            AC_DATA_SEGMENT => {
                let (_size, data) = parse_data_segment_body(&body)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let payload = decode_data_segment(&data, None);
                let mut dec = FrameDecoder::default();
                dec.push(&payload);
                if let Some(msg) = dec.next_frame() {
                    return Ok(msg);
                }
            }
            _ => continue,
        }
    }
}

/// Random UUID v4 (the official client sends `request_uuid` per tunnel).
pub fn rand_uuid() -> std::io::Result<String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    ))
}

/// An established remote tunnel: after the msg.tls handshake every byte is
/// carried in encrypted `data_segment_body` frames.
pub struct Tunnel {
    stream: TcpStream,
    cipher: CipherKey,
}

impl Tunnel {
    /// Write one protobuf message after the handshake: `[4B len][msg]` inside
    /// an encrypted data segment (the official message_io path).
    pub fn write_message(&mut self, msg: &[u8]) -> std::io::Result<()> {
        let payload = encode_frame(msg);
        let wire = encode_data_segment(&payload, Some(&self.cipher));
        self.stream.write_all(&wire)
    }

    /// Write raw tunnel bytes (already framed as one or more data segments).
    pub fn write_raw(&mut self, data: &[u8]) -> std::io::Result<()> {
        let wire = encode_data_segment(data, Some(&self.cipher));
        self.stream.write_all(&wire)
    }

    /// Blocking read of the next decrypted data segment payload.
    pub fn read_raw(&mut self) -> std::io::Result<Vec<u8>> {
        loop {
            let header = read_frame(&mut self.stream)?;
            let body = read_frame(&mut self.stream)?;
            match parse_transport_header(&header) {
                AC_DATA_SEGMENT => {
                    let (_size, data) = parse_data_segment_body(&body).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                    })?;
                    return Ok(decode_data_segment(&data, Some(&self.cipher)));
                }
                // keep_alive / idle / busy control frames: ignored for now.
                _ => continue,
            }
        }
    }

    pub fn shutdown(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    pub fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        self.stream.set_read_timeout(d)
    }

    /// A second handle on the same TCP stream (for a concurrent writer thread).
    pub fn try_clone(&self) -> std::io::Result<Tunnel> {
        Ok(Tunnel {
            stream: self.stream.try_clone()?,
            cipher: self.cipher,
        })
    }
}

/// Open a client tunnel: guide -> connect -> msg.tls handshake.
///
/// `dispatch` is the zrpl address (e.g. `dispatch-yz.zenithspace.net:9091`);
/// when empty the platform config / fallback list is used.
pub fn open_tunnel(dispatch: Option<&str>, params: &ConnectParams) -> std::io::Result<Tunnel> {
    let candidates: Vec<String> = match dispatch {
        Some(a) if !a.trim().is_empty() => vec![a.trim().to_string()],
        _ => {
            // Dev/test override so the native path can be exercised against a
            // local mock relay. Unset in production.
            if let Ok(a) = std::env::var("ZRPC_DISPATCH_ADDR") {
                if !a.trim().is_empty() {
                    return open_tunnel(Some(a.trim()), params);
                }
            }
            let mut list: Vec<String> = Vec::new();
            if let Some(a) = dispatch_addr() {
                list.push(a);
            }
            for f in DISPATCH_FALLBACKS {
                if !list.iter().any(|x| x == f) {
                    list.push((*f).to_string());
                }
            }
            list
        }
    };
    let timeout = Duration::from_secs(10);

    // 1) greet + guide (a bare guide request is dropped by the relay).
    //    Try each dispatch candidate; a framed reply is authoritative even
    //    when it carries an error code, so only transport errors fall through.
    let mut last_err: Option<std::io::Error> = None;
    let mut conn: Option<(TcpStream, GuideResponse, String)> = None;
    for addr in &candidates {
        let mut stream = match connect_tcp(addr, timeout) {
            Ok(s) => s,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        let mut batch = encode_frame(&encode_greet(ZRPLGT_ZRPC_GUIDE));
        batch.extend_from_slice(&encode_frame(&encode_connect_guide(&params.nas_id)));
        if let Err(e) = stream.write_all(&batch) {
            last_err = Some(e);
            continue;
        }
        match read_frame(&mut stream) {
            Ok(buf) => {
                let guide = parse_guide_response(&buf);
                conn = Some((stream, guide, addr.clone()));
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let (mut stream, guide, current) = conn.ok_or_else(|| {
        last_err.unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no dispatch reachable")
        })
    })?;
    if guide.result_code != 0 {
        eprintln!("[libzrpc-native] guide failed: rc={}", guide.result_code);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("guide result_code={}", guide.result_code),
        ));
    }

    // 2) switch to the assigned relay node when the guide points elsewhere.
    //    The response only contains an IP (no port): the official client keeps
    //    using the dispatch port (observed 9091).
    let raw_target = if !guide.useful_addr.is_empty() {
        guide.useful_addr.clone()
    } else {
        guide.real_addr.clone()
    };
    let default_port = port_of(&current).unwrap_or(9091);
    let target = ensure_port(&raw_target, default_port);
    let mut current = current;
    if !raw_target.is_empty() && target != current {
        eprintln!("[libzrpc-native] relay switch {current} -> {target}");
        stream = connect_tcp(&target, timeout)?;
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        current = target;
    }

    // 3) greet + connect (ztoken auth). The official client omits
    //    zrpc_addr/request_uuid/is_forward for the tcp_specific service.
    let mut batch = encode_frame(&encode_greet(ZRPLGT_ZRPC_CONNECT));
    batch.extend_from_slice(&encode_frame(&encode_connect_request(params, "", "")));
    stream.write_all(&batch)?;
    let resp = parse_zrpc_response(&read_frame(&mut stream)?);
    if resp.result_code != 0 {
        eprintln!(
            "[libzrpc-native] connect failed: rc={} (addr {current})",
            resp.result_code
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("connect result_code={}", resp.result_code),
        ));
    }
    eprintln!("[libzrpc-native] connect ok via {current}");

    // 4) custom TLS handshake (messages are wrapped in data-segment frames)
    let mut hs = Handshake::new()?;
    send_handshake_message(&mut stream, &hs.client_hello())?;
    hs.accept_server_hello(&read_handshake_message(&mut stream)?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let cke = hs
        .client_key_exchange()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    send_handshake_message(&mut stream, &cke)?;
    let ok = hs
        .verify_server_finish(&read_handshake_message(&mut stream)?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if !ok {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "server_finish verify failed",
        ));
    }

    stream.set_read_timeout(None)?;
    Ok(Tunnel {
        stream,
        cipher: hs.cipher,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::LineEnding;

    #[test]
    fn varint_multibyte() {
        let mut out = Vec::new();
        put_varint(&mut out, 300);
        assert_eq!(out, vec![0xac, 0x02]);
    }

    /// Wire-identical to the official tcp_specific connect request captured
    /// from the real Windows client against the local mock relay.
    #[test]
    fn connect_request_matches_official_wire() {
        let p = ConnectParams::new("ZPROBE", "");
        assert_eq!(
            hex::encode(encode_connect_request(&p, "", "")),
            "0a065a50524f424510bf2718012002280568ffffffffffffffffff01"
        );
    }

    #[test]
    fn client_hello_roundtrip() {
        let bytes = encode_client_hello("A1B2C3D4");
        let mut r = PbReader::new(&bytes);
        let mut version = 0;
        let mut rand = String::new();
        let mut enc = false;
        while let Some((f, v)) = r.next() {
            match (f, v) {
                (1, PbValue::Varint(x)) => version = x as u32,
                (2, PbValue::Bytes(b)) => rand = String::from_utf8_lossy(b).into_owned(),
                (3, PbValue::Varint(x)) => enc = x != 0,
                _ => {}
            }
        }
        assert_eq!(version, TLS_PROTO_VERSION);
        assert_eq!(rand, "A1B2C3D4");
        assert!(enc);
    }

    #[test]
    fn sha256_hex_is_standard() {
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223...
        assert!(handshake_sha256_hex(b"abc").starts_with("ba7816bf"));
    }

    #[test]
    fn master_secret_and_verify_data() {
        let h = Handshake {
            client_rand: "A".repeat(64),
            pre_master_secret: "C".repeat(64),
            server_rand: "B".repeat(64),
            server_pub_key: String::new(),
            master_secret: String::new(),
            cipher: CipherKey {
                key: [0u8; 32],
                iv: [0u8; 16],
            },
        };
        let master = master_secret(&h.client_rand, &h.server_rand, &h.pre_master_secret);
        assert_eq!(master.len(), 64);
        let cipher = session_cipher(&master);

        // client_verify_data decrypts back to server_rand
        let cvd = verify_data(&master, &h.server_rand);
        let raw = base64::engine::general_purpose::STANDARD
            .decode(cvd.as_bytes())
            .unwrap();
        assert_eq!(
            crypto::decrypt_gcm_no_tag(&cipher.key, &cipher.iv, &raw),
            h.server_rand.as_bytes()
        );

        // server_verify_data decrypts back to client_rand
        let svd = verify_data(&master, &h.client_rand);
        let raw = base64::engine::general_purpose::STANDARD
            .decode(svd.as_bytes())
            .unwrap();
        assert_eq!(
            crypto::decrypt_gcm_no_tag(&cipher.key, &cipher.iv, &raw),
            h.client_rand.as_bytes()
        );
    }

    #[test]
    fn frame_decoder_handles_partial_input() {
        let f1 = encode_frame(b"hello");
        let f2 = encode_frame(b"world!");
        let mut d = FrameDecoder::default();
        d.push(&f1[..3]);
        assert!(d.next_frame().is_none());
        d.push(&f1[3..]);
        d.push(&f2);
        assert_eq!(d.next_frame().unwrap(), b"hello");
        assert_eq!(d.next_frame().unwrap(), b"world!");
        assert!(d.next_frame().is_none());
    }

    #[test]
    fn data_segment_roundtrip_encrypted() {
        let cipher = session_cipher(&"D".repeat(64));
        let payload = vec![7u8; MAX_PLAINTEXT_SEGMENT + 10];
        let wire = encode_data_segment(&payload, Some(&cipher));

        let mut d = FrameDecoder::default();
        d.push(&wire);
        let mut out = Vec::new();
        let mut sizes = Vec::new();
        while let Some(header) = d.next_frame() {
            assert_eq!(parse_transport_header(&header), AC_DATA_SEGMENT);
            let body = d.next_frame().expect("body frame");
            let (size, data) = parse_data_segment_body(&body).unwrap();
            out.extend_from_slice(&decode_data_segment(&data, Some(&cipher)));
            sizes.push(size as usize);
        }
        assert_eq!(sizes, vec![MAX_PLAINTEXT_SEGMENT, 10]);
        assert_eq!(out, payload);
    }

    #[test]
    fn rsa_pre_master_roundtrip() {
        use rsa::pkcs8::EncodePublicKey;
        use rsa::RsaPrivateKey;
        let mut rng = OsRng;
        let private = RsaPrivateKey::new(&mut rng, 1024).expect("keygen");
        let pem = private
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("pem");

        let pre = "C".repeat(64);
        let b64 = encrypt_pre_master(&pem, &pre).expect("encrypt");
        let inner = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .unwrap();
        let ct = base64::engine::general_purpose::STANDARD
            .decode(inner)
            .unwrap();
        let pt = private.decrypt(Pkcs1v15Encrypt, &ct).expect("decrypt");
        assert_eq!(pt, pre.as_bytes());
    }

    /// Mock zrpl relay: greet+guide, greet+connect, msg.tls, then an encrypted
    /// echo service that stops after echoing `bye`. Returns (port, handle).
    fn spawn_mock_relay(
        private: rsa::RsaPrivateKey,
        server_pem: String,
        expect_token: Option<String>,
    ) -> (u16, std::thread::JoinHandle<()>) {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();

            // greet(4) + guide
            let greet = read_frame(&mut s).unwrap();
            let mut gr = PbReader::new(&greet);
            let mut greet_type = 0u32;
            while let Some((f, v)) = gr.next() {
                if let (1, PbValue::Varint(x)) = (f, v) {
                    greet_type = x as u32;
                }
            }
            assert_eq!(greet_type, ZRPLGT_ZRPC_GUIDE);

            let guide = read_frame(&mut s).unwrap();
            let mut r = PbReader::new(&guide);
            let mut nas_id = String::new();
            let mut guide_ver = 0u32;
            while let Some((f, v)) = r.next() {
                match (f, v) {
                    (1, PbValue::Bytes(b)) => nas_id = String::from_utf8_lossy(b).into_owned(),
                    (2, PbValue::Varint(x)) => guide_ver = x as u32,
                    _ => {}
                }
            }
            assert_eq!(nas_id, "NAS-TEST");
            assert_eq!(guide_ver, 2);
            s.write_all(&encode_frame(&[])).unwrap(); // result_code=0, no addrs

            // greet(1) + connect request
            let greet2 = read_frame(&mut s).unwrap();
            let mut gr = PbReader::new(&greet2);
            let mut greet_type = 0u32;
            while let Some((f, v)) = gr.next() {
                if let (1, PbValue::Varint(x)) = (f, v) {
                    greet_type = x as u32;
                }
            }
            assert_eq!(greet_type, ZRPLGT_ZRPC_CONNECT);

            let req = read_frame(&mut s).unwrap();
            let mut r = PbReader::new(&req);
            let mut got_token = String::new();
            let mut got_port = 0u32;
            let mut got_nas = String::new();
            while let Some((f, v)) = r.next() {
                match (f, v) {
                    (1, PbValue::Bytes(b)) => got_nas = String::from_utf8_lossy(b).into_owned(),
                    (2, PbValue::Varint(x)) => got_port = x as u32,
                    (7, PbValue::Bytes(b)) => got_token = String::from_utf8_lossy(b).into_owned(),
                    _ => {}
                }
            }
            assert_eq!(got_nas, "NAS-TEST");
            assert_eq!(got_port, 5055);
            assert_eq!(got_token, expect_token.unwrap_or_default());
            s.write_all(&encode_frame(&[])).unwrap(); // zrpc_response result_code=0

            // msg.tls handshake (data-segment wrapped; pubkey is base64 PEM)
            let ch = read_handshake_message(&mut s).unwrap();
            let mut r = PbReader::new(&ch);
            let mut client_rand = String::new();
            while let Some((f, v)) = r.next() {
                if let (2, PbValue::Bytes(b)) = (f, v) {
                    client_rand = String::from_utf8_lossy(b).into_owned();
                }
            }
            assert_eq!(client_rand.len(), 64);

            let server_rand = "B".repeat(64);
            let pem_b64 = base64::engine::general_purpose::STANDARD.encode(server_pem.as_bytes());
            let mut sh = Vec::new();
            put_str(&mut sh, 2, &server_rand);
            put_str(&mut sh, 3, &pem_b64);
            send_handshake_message(&mut s, &sh).unwrap();

            let cke = read_handshake_message(&mut s).unwrap();
            let mut r = PbReader::new(&cke);
            let mut pre_b64 = String::new();
            let mut verify_b64 = String::new();
            while let Some((f, v)) = r.next() {
                match (f, v) {
                    (2, PbValue::Bytes(b)) => pre_b64 = String::from_utf8_lossy(b).into_owned(),
                    (3, PbValue::Bytes(b)) => verify_b64 = String::from_utf8_lossy(b).into_owned(),
                    _ => {}
                }
            }
            let ct = base64::engine::general_purpose::STANDARD
                .decode(pre_b64.as_bytes())
                .unwrap();
            // The client sends base64(base64(rsa_ciphertext)).
            let ct = base64::engine::general_purpose::STANDARD
                .decode(ct)
                .unwrap();
            let pre = private.decrypt(Pkcs1v15Encrypt, &ct).expect("rsa decrypt");
            let pre = String::from_utf8(pre).unwrap();
            let master = master_secret(&client_rand, &server_rand, &pre);
            let cipher = session_cipher(&master);

            let raw = base64::engine::general_purpose::STANDARD
                .decode(verify_b64.as_bytes())
                .unwrap();
            let pt = crypto::decrypt_gcm_no_tag(&cipher.key, &cipher.iv, &raw);
            assert_eq!(pt, server_rand.as_bytes());

            let mut sf = Vec::new();
            put_str(&mut sf, 2, &verify_data(&master, &client_rand));
            send_handshake_message(&mut s, &sf).unwrap();

            // encrypted echo service
            loop {
                let header = read_frame(&mut s).unwrap();
                let body = read_frame(&mut s).unwrap();
                assert_eq!(parse_transport_header(&header), AC_DATA_SEGMENT);
                let (_, data) = parse_data_segment_body(&body).unwrap();
                let pt = decode_data_segment(&data, Some(&cipher));
                s.write_all(&encode_data_segment(&pt, Some(&cipher))).unwrap();
                if pt == b"bye" {
                    break;
                }
            }
            let _ = s.shutdown(std::net::Shutdown::Both);
        });
        (port, handle)
    }

    /// End-to-end: mock zrpl server speaks guide -> connect -> msg.tls -> echo.
    #[test]
    fn tunnel_roundtrip_against_mock_relay() {
        use rsa::pkcs8::EncodePublicKey;
        use rsa::RsaPrivateKey;

        let private = RsaPrivateKey::new(&mut OsRng, 1024).expect("keygen");
        let pem = private
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("pem");
        let (port, server) = spawn_mock_relay(private, pem, Some("user-token".to_string()));

        let params = ConnectParams::new("NAS-TEST", "user-token");
        let mut tunnel =
            open_tunnel(Some(&format!("127.0.0.1:{port}")), &params).expect("tunnel");
        tunnel.write_raw(b"ping").unwrap();
        assert_eq!(tunnel.read_raw().unwrap(), b"ping");
        tunnel.write_raw(b"bye").unwrap();
        assert_eq!(tunnel.read_raw().unwrap(), b"bye");
        tunnel.shutdown();
        server.join().unwrap();
    }

    /// FFI + proxy wiring: start_service with no LAN IP must bind the local
    /// port and route each connection through the relay (no ztoken needed).
    #[test]
    fn ffi_start_service_routes_remote_through_relay() {
        use rsa::pkcs8::EncodePublicKey;
        use rsa::RsaPrivateKey;
        use std::ffi::{CStr, CString};
        use std::net::TcpListener;

        let private = RsaPrivateKey::new(&mut OsRng, 1024).expect("keygen");
        let pem = private
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("pem");
        let (relay_port, server) = spawn_mock_relay(private, pem, None);
        std::env::set_var("ZRPC_DISPATCH_ADDR", format!("127.0.0.1:{relay_port}"));

        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        let local_port = probe.local_addr().unwrap().port();
        drop(probe);

        let cfg = format!(
            r#"{{"local_port":{local_port},"remote_port":5055,"type":1,"nas_id":"NAS-TEST"}}"#
        );
        let k = crypto::derive_key_js();
        let ct = crypto::encrypt_gcm_no_tag(&k.key, &k.iv, cfg.as_bytes());
        let b64 = base64::engine::general_purpose::STANDARD.encode(ct);
        let c = CString::new(b64).unwrap();
        let out = crate::ffi::libzrpc_start_service(c.as_ptr());
        let res = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        assert!(res.contains("\"code\":\"200\""), "start_service -> {res}");

        let mut s = TcpStream::connect(("127.0.0.1", local_port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s.write_all(b"ping").unwrap();
        let mut buf = [0u8; 4];
        s.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        s.write_all(b"bye").unwrap();
        let mut b2 = [0u8; 3];
        s.read_exact(&mut b2).unwrap();
        assert_eq!(&b2, b"bye");
        drop(s);

        crate::ffi::libzrpc_stop_all_service();
        std::env::remove_var("ZRPC_DISPATCH_ADDR");
        server.join().unwrap();
    }

    #[test]
    fn uuid_shape() {
        let u = rand_uuid().unwrap();
        assert_eq!(u.len(), 36);
        assert_eq!(u.as_bytes()[14], b'4');
    }

    #[test]
    fn guide_addr_gets_dispatch_port() {
        assert_eq!(ensure_port("223.111.129.15", 9091), "223.111.129.15:9091");
        assert_eq!(ensure_port("223.111.129.15:9091", 9091), "223.111.129.15:9091");
        assert_eq!(ensure_port(" relay.example.com ", 3388), "relay.example.com:3388");
        assert_eq!(port_of("dispatch-yz.zenithspace.net:9091"), Some(9091));
        assert_eq!(port_of("223.111.129.15"), None);
    }

    /// Live end-to-end probe: real guide/connect/TLS/data through the official
    /// relay. Run with a NAS id:
    ///   ZRPC_NAS_ID=... cargo test live_relay_http_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_relay_http_probe() {
        let nas = match std::env::var("ZRPC_NAS_ID") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => {
                eprintln!("skip: ZRPC_NAS_ID not set");
                return;
            }
        };
        let dispatch =
            std::env::var("ZRPC_DISPATCH").unwrap_or_else(|_| "dispatch-yz.zenithspace.net:9091".into());
        let params = ConnectParams::new(nas, "");
        let mut t = open_tunnel(Some(&dispatch), &params).expect("open_tunnel");
        let _ = t.set_read_timeout(Some(Duration::from_secs(15)));
        // Optional post-handshake greeting (debug): the relay may expect the
        // HTTP header via zrpl_greet_request_v2 before forwarding data.
        if let Ok(v) = std::env::var("ZRPC_GREET_V2") {
            if v == "1" || v == "0" {
                let gt: u32 = v.parse().unwrap();
                let mut g = Vec::new();
                put_str(&mut g, 1, "GET /key/pub HTTP/1.1\r\nHost: nas\r\n\r\n");
                put_uint32(&mut g, 2, gt);
                eprintln!("[libzrpc-native] sending greet_v2 type={gt}");
                t.write_message(&g).unwrap();
            }
        }
        if std::env::var("ZRPC_GREET_PUB").ok().as_deref() == Some("1") {
            let mut g = Vec::new();
            put_str(&mut g, 1, "GET /key/pub HTTP/1.1\r\nHost: nas\r\n\r\n");
            put_bool(&mut g, 2, false);
            put_str(&mut g, 3, &params.nas_id);
            put_bool(&mut g, 4, false);
            eprintln!("[libzrpc-native] sending public_connect_greet_request_v2");
            t.write_message(&g).unwrap();
        }
        t.write_raw(b"GET /key/pub?webagent=v2&rnd=probe HTTP/1.1\r\nHost: nas\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut got = Vec::new();
        for _ in 0..8 {
            match t.read_raw() {
                Ok(chunk) if !chunk.is_empty() => {
                    got.extend_from_slice(&chunk);
                    if got.windows(4).any(|w| w == b"\r\n\r\n") || got.len() > 512 {
                        break;
                    }
                }
                Ok(_) => continue,
                Err(e) => {
                    eprintln!("read err after {} bytes: {e}", got.len());
                    break;
                }
            }
        }
        let head = String::from_utf8_lossy(&got[..got.len().min(300)]).to_string();
        eprintln!("relay http response ({} bytes):\n{head}", got.len());
        assert!(head.starts_with("HTTP/1."), "no HTTP response: {head:?}");
        t.shutdown();
    }

    /// Live probe against the official dispatch (network). Run with
    /// `cargo test live_dispatch -- --ignored --nocapture`.
    ///
    /// A fabricated nas_id must be answered with `RC_ZRPC2ZRPL_GUIDE_ERROR`
    /// (8). Getting that framed reply proves greeting + guide framing match
    /// the real server.
    #[test]
    #[ignore]
    fn live_dispatch_guide_probe() {
        let params = ConnectParams::new("zspace-native-probe", "");
        let err = open_tunnel(Some("dispatch-yz.zenithspace.net:9091"), &params)
            .err()
            .expect("bogus nas_id must fail");
        eprintln!("live guide probe -> {err}");
        assert!(err.to_string().contains("guide result_code=8"), "{err}");
    }
}
