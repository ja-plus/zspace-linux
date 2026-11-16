//! Temporary research probe: fetch a public path through a real relay tunnel.
//! Usage: ZRPC_NAS_ID=<id> cargo run --example fetch_tunnel -- /key/pub

use std::time::Duration;

use zrpc::relay::{open_tunnel, ConnectParams};

fn main() {
    let nas = std::env::var("ZRPC_NAS_ID").expect("ZRPC_NAS_ID");
    let path = std::env::args().nth(1).unwrap_or_else(|| "/key/pub".into());
    let dispatch = std::env::var("ZRPC_DISPATCH")
        .unwrap_or_else(|_| "dispatch-yz.zenithspace.net:9091".into());
    let params = ConnectParams::new(nas, "");
    let mut t = open_tunnel(Some(&dispatch), &params).expect("open_tunnel");
    t.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: nas\r\nConnection: close\r\n\r\n");
    t.write_raw(req.as_bytes()).unwrap();
    let mut got = Vec::new();
    loop {
        match t.read_raw() {
            Ok(d) if d.is_empty() => continue,
            Ok(d) => got.extend_from_slice(&d),
            Err(e) => {
                eprintln!("read end: {e}");
                break;
            }
        }
    }
    println!("=== {} bytes ===", got.len());
    println!("{}", String::from_utf8_lossy(&got));
    t.shutdown();
}
