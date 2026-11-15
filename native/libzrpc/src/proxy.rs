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
            // No total timeout: 4K media streams run for minutes. Only bound
            // connect so a dead NAS still fails fast.
            .connect_timeout(Duration::from_secs(10))
            .timeout(None)
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

/// Start a local proxy whose upstream is a zrpl relay tunnel (remote NAS).
///
/// Each accepted local connection gets its own tunnel: the local bytes are
/// piped through the encrypted data segments unchanged, so HTTP semantics
/// (keep-alive, streaming, WebSocket) behave like a transparent TCP proxy.
pub fn start_tunnel(local_port: u16, params: crate::relay::ConnectParams) -> std::io::Result<()> {
    stop(local_port);
    let listener = TcpListener::bind(("127.0.0.1", local_port))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let params = Arc::new(params);
    let thread = std::thread::spawn(move || loop {
        if stop2.load(Ordering::Relaxed) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let params = params.clone();
                std::thread::spawn(move || {
                    let _ = handle_tunnel(stream, &params);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(30));
            }
            Err(_) => break,
        }
    });
    proxies().lock().unwrap().insert(local_port, Proxy { stop, _thread: thread });
    Ok(())
}

fn handle_tunnel(local: TcpStream, params: &crate::relay::ConnectParams) -> std::io::Result<()> {
    let mut up = match crate::relay::open_tunnel(None, params) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[libzrpc-native] relay tunnel failed: {e}");
            return Err(e);
        }
    };
    let mut down = up.try_clone()?;
    let mut local_read = local.try_clone()?;
    let mut local_write = local;

    let writer = std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        loop {
            match local_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if up.write_raw(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        up.shutdown();
    });

    loop {
        match down.read_raw() {
            Ok(data) => {
                if data.is_empty() {
                    continue;
                }
                if local_write.write_all(&data).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = local_write.shutdown(std::net::Shutdown::Both);
    let _ = writer.join();
    Ok(())
}

pub fn stop(local_port: u16) {    if let Some(p) = proxies().lock().unwrap().remove(&local_port) {
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

    // `/splay/` is the media stream: the official header templates do not
    // inject webagent/rnd there, and extra query params may break seeking.
    let path = if raw_path.starts_with("/splay/") {
        raw_path.clone()
    } else {
        normalize_path(&raw_path)
    };
    let url = format!("{target}{path}");
    let is_head = method.eq_ignore_ascii_case("HEAD");
    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let resp = client.request(m, &url).headers(headers).body(body).send();

    match resp {
        Ok(mut resp) => {
            let status = resp.status();
            let mut head = format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("OK")
            );
            // Preserve status (including 206), Content-Range, ETag,
            // Last-Modified ... but rewrite framing headers ourselves so the
            // body can be streamed without ever buffering it.
            for (k, v) in resp.headers() {
                if k == reqwest::header::TRANSFER_ENCODING
                    || k == reqwest::header::CONNECTION
                    || k == reqwest::header::CONTENT_LENGTH
                {
                    continue;
                }
                if let Ok(vs) = v.to_str() {
                    head += &format!("{}: {}\r\n", k.as_str(), vs);
                }
            }
            let upstream_len = resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let chunked = upstream_len.is_none();
            if let Some(len) = &upstream_len {
                head += &format!("content-length: {len}\r\n");
            } else {
                head += "transfer-encoding: chunked\r\n";
            }
            head += "connection: close\r\n\r\n";
            writer.write_all(head.as_bytes())?;
            writer.flush()?;

            if !is_head {
                // Stream the body in 64 KiB slices; never hold it in memory.
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = resp.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    if chunked {
                        writer.write_all(format!("{n:x}\r\n").as_bytes())?;
                        writer.write_all(&buf[..n])?;
                        writer.write_all(b"\r\n")?;
                    } else {
                        writer.write_all(&buf[..n])?;
                    }
                    writer.flush()?;
                }
                if chunked {
                    writer.write_all(b"0\r\n\r\n")?;
                }
            }
        }
        Err(e) => {
            let msg = format!("{{\"code\":502,\"msg\":\"native proxy upstream error: {e}\"}}");
            let head = format!(
                "HTTP/1.1 502 Bad Gateway\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
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

    /// Read an HTTP head byte-by-byte until CRLFCRLF.
    fn read_head(s: &mut TcpStream) -> String {
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        loop {
            s.read_exact(&mut b).unwrap();
            head.push(b[0]);
            if head.ends_with(b"\r\n\r\n") {
                return String::from_utf8_lossy(&head).to_string();
            }
        }
    }

    fn free_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    }

    /// 206 must keep Content-Range/ETag/Content-Length and stream (no buffering).
    #[test]
    fn streams_206_and_preserves_range_headers() {
        use std::time::Instant;
        let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
        let uport = upstream.local_addr().unwrap().port();
        let total = 4 * 1024 * 1024usize;
        let first = 64 * 1024usize;

        let srv = std::thread::spawn(move || {
            let (mut s, _) = upstream.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut req = line.clone();
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                    break;
                }
                req.push_str(&h);
            }
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{}\r\nContent-Length: {}\r\nETag: \"v1\"\r\nLast-Modified: Wed, 01 Jan 2026 00:00:00 GMT\r\n\r\n",
                total - 1,
                11586518326u64,
                total
            );
            s.write_all(head.as_bytes()).unwrap();
            s.write_all(&vec![0x41u8; first]).unwrap();
            s.flush().unwrap();
            // Hold the rest back: if the proxy buffered, the client cannot get
            // the first chunk before this delay elapses.
            std::thread::sleep(Duration::from_millis(400));
            s.write_all(&vec![0x42u8; total - first]).unwrap();
            s.flush().unwrap();
            req
        });

        let lport = free_port();
        start(lport, format!("http://127.0.0.1:{uport}")).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let mut c = TcpStream::connect(("127.0.0.1", lport)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(b"GET /splay/movie.mkv HTTP/1.1\r\nHost: nas\r\nRange: bytes=0-99\r\n\r\n")
            .unwrap();

        let head = read_head(&mut c).to_lowercase();
        assert!(head.starts_with("http/1.1 206"), "{head}");
        assert!(head.contains("content-range: bytes 0-4194303/11586518326"), "{head}");
        assert!(head.contains("etag: \"v1\""));
        assert!(head.contains("last-modified:"));
        assert!(head.contains("content-length: 4194304"));
        assert!(head.contains("connection: close"));

        let t0 = Instant::now();
        let mut head_chunk = vec![0u8; first];
        c.read_exact(&mut head_chunk).unwrap();
        assert!(
            t0.elapsed() < Duration::from_millis(300),
            "first chunk was buffered: {:?}",
            t0.elapsed()
        );
        assert!(head_chunk.iter().all(|&b| b == 0x41));

        let mut rest = vec![0u8; total - first];
        c.read_exact(&mut rest).unwrap();
        assert!(rest.iter().all(|&b| b == 0x42));

        let req = srv.join().unwrap().to_lowercase();
        assert!(req.contains("range: bytes=0-99"), "{req}");
        // /splay/ must not be decorated with webagent/rnd
        assert!(req.contains("/splay/movie.mkv"));
        assert!(!req.contains("webagent="), "{req}");
        assert!(!req.contains("rnd="), "{req}");
        stop(lport);
    }

    /// Chunked upstream (no Content-Length) stays streamable: we re-chunk.
    #[test]
    fn streams_chunked_response() {
        let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
        let uport = upstream.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let (mut s, _) = upstream.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                    break;
                }
            }
            s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\n\r\n\
                  5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
            )
            .unwrap();
        });

        let lport = free_port();
        start(lport, format!("http://127.0.0.1:{uport}")).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let mut c = TcpStream::connect(("127.0.0.1", lport)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(b"GET /v2/file/list HTTP/1.1\r\nHost: nas\r\n\r\n").unwrap();

        let head = read_head(&mut c).to_lowercase();
        assert!(head.contains("transfer-encoding: chunked"), "{head}");
        assert!(!head.contains("content-length"), "{head}");

        let mut body = Vec::new();
        c.read_to_end(&mut body).unwrap();
        let body = String::from_utf8_lossy(&body).to_string();
        assert!(body.ends_with("0\r\n\r\n"), "{body:?}");
        assert!(body.contains("hello world"), "{body:?}");

        let _ = srv.join();
        stop(lport);
    }

    /// HEAD must not hang or error.
    #[test]
    fn head_request_has_no_body() {
        let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
        let uport = upstream.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let (mut s, _) = upstream.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                    break;
                }
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 123\r\n\r\n")
                .unwrap();
        });

        let lport = free_port();
        start(lport, format!("http://127.0.0.1:{uport}")).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let mut c = TcpStream::connect(("127.0.0.1", lport)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(b"HEAD /v2/file/list HTTP/1.1\r\nHost: nas\r\n\r\n")
            .unwrap();
        let head = read_head(&mut c).to_lowercase();
        assert!(head.starts_with("http/1.1 200"), "{head}");
        assert!(head.contains("content-length: 123"));
        let mut rest = Vec::new();
        c.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty(), "HEAD must not carry a body: {rest:?}");

        let _ = srv.join();
        stop(lport);
    }
}