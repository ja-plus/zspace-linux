//! Local reverse proxy: 127.0.0.1:<local_port> -> NAS:<remote_port> (HTTPS).
//!
//! The official client's renderer talks HTTP to the local proxy started by
//! libzrpc_start_service; the proxy forwards to the NAS API. Recovered request
//! conventions (mac http_header_generator): `webagent=v2` marker and
//! `rnd=<ts>_<rand>` cache-buster. Upstream uses standard TLS (nas_confirm uses
//! boost.asio.ssl https_client); self-signed NAS certs are accepted.
//!
//! M2: blocking HTTP/1.1 forward with full-body buffering (adequate for the
//! file/browse API). Streaming/WebSocket (video/events) is upgraded in M5.
//! Real-NAS header/auth behavior is confirmed by capture (task #12).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Proxy {
    stop: Arc<AtomicBool>,
    _thread: JoinHandle<()>,
}

static PROXIES: OnceLock<Mutex<HashMap<u16, Proxy>>> = OnceLock::new();

fn proxies() -> &'static Mutex<HashMap<u16, Proxy>> {
    PROXIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start (or replace) a proxy on 127.0.0.1:local_port forwarding to target.
/// `target` is like "https://192.168.1.50:5055".
pub fn start(local_port: u16, target: String) -> std::io::Result<()> {
    stop(local_port);
    let listener = TcpListener::bind(("127.0.0.1", local_port))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let thread = std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        loop {
            if stop2.load(Ordering::Relaxed) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let client = client.clone();
                    let target = target.clone();
                    std::thread::spawn(move || {
                        let _ = handle(stream, &client, &target);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(30));
                }
                Err(_) => break,
            }
        }
    });
    proxies().lock().unwrap().insert(local_port, Proxy { stop, _thread: thread });
    Ok(())
}

pub fn stop(local_port: u16) {
    if let Some(p) = proxies().lock().unwrap().remove(&local_port) {
        p.stop.store(true, Ordering::Relaxed);
    }
}

pub fn stop_all() {
    let mut map = proxies().lock().unwrap();
    for (_, p) in map.drain() {
        p.stop.store(true, Ordering::Relaxed);
    }
}

fn rnd_param() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let r = (ms % 10000) as u32;
    format!("{ms}_{r}")
}

/// Ensure the request path carries webagent=v2 and a rnd cache-buster.
fn normalize_path(path: &str) -> String {
    let mut p = path.to_string();
    if !p.contains("webagent=") {
        p += if p.contains('?') { "&" } else { "?" };
        p += "webagent=v2";
    }
    if !p.contains("rnd=") {
        p += if p.contains('?') { "&" } else { "?" };
        p += &format!("rnd={}", rnd_param());
    }
    p
}

fn handle(stream: TcpStream, client: &reqwest::blocking::Client, target: &str) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();

    let mut headers = reqwest::header::HeaderMap::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            }
            // Host is rewritten by reqwest for the upstream; skip hop-by-hop.
            if k.eq_ignore_ascii_case("host")
                || k.eq_ignore_ascii_case("connection")
                || k.eq_ignore_ascii_case("proxy-connection")
            {
                continue;
            }
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(v),
            ) {
                headers.append(name, val);
            }
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    let path = normalize_path(&raw_path);
    let url = format!("{target}{path}");
    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let resp = client.request(m, &url).headers(headers).body(body).send();

    match resp {
        Ok(resp) => {
            let status = resp.status();
            let mut head = format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("OK")
            );
            for (k, v) in resp.headers() {
                if k == reqwest::header::TRANSFER_ENCODING || k == reqwest::header::CONNECTION {
                    continue;
                }
                if let Ok(vs) = v.to_str() {
                    head += &format!("{}: {}\r\n", k.as_str(), vs);
                }
            }
            let bytes = resp.bytes().unwrap_or_default();
            head += &format!("content-length: {}\r\n\r\n", bytes.len());
            writer.write_all(head.as_bytes())?;
            writer.write_all(&bytes)?;
        }
        Err(e) => {
            let msg = format!("{{\"code\":502,\"msg\":\"native proxy upstream error: {e}\"}}");
            let head = format!(
                "HTTP/1.1 502 Bad Gateway\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                msg.len()
            );
            writer.write_all(head.as_bytes())?;
            writer.write_all(msg.as_bytes())?;
        }
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_params() {
        let p = normalize_path("/system/recent");
        assert!(p.contains("webagent=v2"));
        assert!(p.contains("rnd="));
        // idempotent-ish: existing params preserved
        let p2 = normalize_path("/x?webagent=v2&rnd=1");
        assert_eq!(p2.matches("webagent=").count(), 1);
        assert_eq!(p2.matches("rnd=").count(), 1);
    }

    /// End-to-end proxy test against a local mock upstream (no NAS involved).
    #[test]
    fn proxy_forwards_to_mock() {
        // mock upstream HTTP server
        let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
        let uport = upstream.local_addr().unwrap().port();
        let uh = std::thread::spawn(move || {
            if let Ok((mut s, _)) = upstream.accept() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                let _ = r.read_line(&mut line);
                // drain headers
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                        break;
                    }
                }
                let body = b"{\"ok\":true}";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
                let _ = s.write_all(body);
            }
        });

        let target = format!("http://127.0.0.1:{uport}");
        // pick a free local port
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        let lport = probe.local_addr().unwrap().port();
        drop(probe);
        start(lport, target).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let resp = reqwest::blocking::Client::new()
            .get(format!("http://127.0.0.1:{lport}/system/recent"))
            .send()
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().unwrap(), "{\"ok\":true}");

        stop(lport);
        let _ = uh.join();
    }
}