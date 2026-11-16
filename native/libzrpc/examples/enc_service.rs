//! Dev helper: encrypt a `libzrpc_start_service` config the same way the
//! official background.js `encodeService` does (MT19937 seed 4728423 →
//! SHA-256 hex → key/iv slices → AES-256-GCM without tag → base64).
//!
//! Usage: cargo run --release --example enc_service -- '{"local_port":13599,...}'

fn main() {
    let json = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "{\"local_port\":13599,\"remote_port\":5055,\"type\":1,\"nas_id\":\"ZPROBE\"}".into());
    let key = zrpc::crypto::derive_key_js();
    let ct = zrpc::crypto::encrypt_gcm_no_tag(&key.key, &key.iv, json.as_bytes());
    use base64::Engine as _;
    println!("{}", base64::engine::general_purpose::STANDARD.encode(ct));
}
