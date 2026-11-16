//! Linux 版 ZSpaceMountMonitor。
//!
//! 官方 Windows/macOS 客户端（V2.46，`supportMountV2=true`）在挂载时并不调用
//! 旧的 `mounts3.dll`，而是 spawn `bin/ZSpaceMountMonitor` 这个 Go CLI，由它：
//!   1. 用 `config --add` 写出一份 rclone 配置（webdav 指向本地 13600→NAS:9001 代理）；
//!   2. 用 `mount`（根命令）拉起 `ZSpaceMount`（官方内嵌 rclone v1.70.2）；
//!   3. 通过 `rc --rc-addr=.../mount/status` 与 `/shutdown` 查询/停止挂载；
//!   4. 提供 `check` / `process` / `disk` / `fuse` 等子命令供主进程调用。
//!
//! 本程序按同样的命令行契约实现 Linux 侧行为，挂载实体换成上游 rclone + FUSE3。
//! 行为取证见同目录 README.md 与 research/。

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
    fn geteuid() -> u32;
    fn kill(pid: i32, sig: i32) -> i32;
}

extern "C" fn on_signal(_sig: i32) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

const MONITOR_VERSION: &str = "1.1.1-linux";

const COMMANDS: &[&str] = &[
    "check",
    "check-port-free",
    "completion",
    "config",
    "disk",
    "disk-free-space",
    "drivers",
    "fuse",
    "help",
    "network",
    "only-check-port-free",
    "port",
    "process",
    "random-free-port",
    "rc",
    "sys",
    "version",
];

const VALUE_FLAGS: &[&str] = &[
    "program",
    "home-dir",
    "cache-dir",
    "cache-max-size",
    "volume",
    "volume-label",
    "config-section",
    "host-name",
    "rc-o",
    "rc-addr",
    "name",
    "pid",
    "dir",
    "install",
    "uninstall",
    "add-hosts-host",
    "add-hosts-ip",
    "tmp-log",
    "check",
    "file-info",
    "file-sha256",
    "file-version",
    "dll-locksmith",
    "system-service",
    "pass",
    "user",
    "url",
    "config",
];

const BOOL_FLAGS: &[&str] = &[
    "add",
    "obscure",
    "elevate",
    "status",
    "list",
    "kill",
    "free",
    "only",
    "show-hidden",
    "use-filter",
    "network-mode",
    "no-fuse-mode",
    "run-as-normal-user",
    "check-cloud",
    "check-free",
    "check-fs",
    "check-mount",
    "check-rw",
    "check-sparse",
    "empty",
    "force-umount",
    "size",
    "sys",
    "walk",
    "remove",
    "hosts",
    "proxy",
    "eth",
    "is-root",
    "close",
    "check-sha2",
    "sync-time",
    "regsvr32",
    "regsvr32-check",
];

#[derive(Default)]
struct Args {
    cmd: Option<String>,
    flags: HashMap<String, Vec<String>>,
    bools: HashSet<String>,
    positional: Vec<String>,
}

impl Args {
    fn parse() -> Args {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut a = Args::default();
        let mut i = 0usize;
        while i < raw.len() {
            let t = &raw[i];
            if let Some(body) = t.strip_prefix("--") {
                let (key, inline) = match body.split_once('=') {
                    Some((k, v)) => (k.to_string(), Some(v.to_string())),
                    None => (body.to_string(), None),
                };
                if BOOL_FLAGS.contains(&key.as_str()) {
                    a.bools.insert(key);
                } else if VALUE_FLAGS.contains(&key.as_str()) {
                    let v = match inline {
                        Some(v) => v,
                        None => {
                            i += 1;
                            raw.get(i).cloned().unwrap_or_default()
                        }
                    };
                    a.flags.entry(key).or_default().push(v);
                } else if inline.is_none() && VALUE_FLAGS.contains(&key.as_str()) {
                    // 理论到不了，保留分支以便将来加 flag 时兼容。
                    a.bools.insert(key);
                } else {
                    a.flags.entry(key).or_default().push(inline.unwrap_or_default());
                }
            } else if t.starts_with('-') {
                // 官方只用到 --xxx；短选项（如 -h）一律当布尔开关忽略。
            } else if a.cmd.is_none() && COMMANDS.contains(&t.as_str()) {
                a.cmd = Some(t.clone());
            } else {
                a.positional.push(t.clone());
            }
            i += 1;
        }
        a
    }

    fn get(&self, k: &str) -> Option<String> {
        self.flags.get(k).and_then(|v| v.last()).cloned()
    }

    fn get_all(&self, k: &str) -> Vec<String> {
        self.flags.get(k).cloned().unwrap_or_default()
    }

    fn has(&self, k: &str) -> bool {
        self.bools.contains(k)
    }
}

// ---------------------------------------------------------------------------
// 输出
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn emit_ok(data: &str) -> ! {
    println!("{{\"status\":200,\"data\":{},\"error\":\"\"}}", data);
    std::process::exit(0)
}

fn emit_err(msg: &str) -> ! {
    println!(
        "{{\"status\":-1,\"data\":\"\",\"error\":\"{}\"}}",
        esc(msg)
    );
    std::process::exit(1)
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// epoch -> "YYYY-MM-DD HH:MM:SS"（UTC），避免引入 chrono。
fn fmt_time(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant civil_from_days
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, mo, d, h, m, s
    )
}

struct Logger {
    file: Option<File>,
}

impl Logger {
    fn new(path: &Path) -> Logger {
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Logger { file }
    }
    fn log(&mut self, level: &str, msg: &str) {
        let line = format!("{} [{}] {}\n", fmt_time(now_secs()), level, msg);
        if let Some(f) = self.file.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
        eprintln!("{}", line.trim_end());
    }
}

fn mkdirs(p: &Path) -> Result<(), String> {
    fs::create_dir_all(p).map_err(|e| format!("mkdir {}: {}", p.display(), e))
}

fn write_file(p: &Path, s: &str) -> Result<(), String> {
    fs::write(p, s).map_err(|e| format!("write {}: {}", p.display(), e))
}

fn read_file(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok()
}

fn free_port() -> Result<u16, String> {
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let p = l.local_addr().map_err(|e| e.to_string())?.port();
    drop(l);
    Ok(p)
}

fn port_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn cmd_output(program: &str, args: &[String]) -> Result<std::process::Output, String> {
    Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("exec {}: {}", program, e))
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(p) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

/// 解析 --program；找不到时回退到同目录的 rclone / ZSpaceMount。
fn resolve_program(args: &Args) -> Result<PathBuf, String> {
    if let Some(p) = args.get("program") {
        let pb = PathBuf::from(&p);
        if executable(&pb) {
            return Ok(pb);
        }
    }
    if let Ok(p) = std::env::var("ZSPACE_RCLONE") {
        let pb = PathBuf::from(p);
        if executable(&pb) {
            return Ok(pb);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["rclone", "ZSpaceMount"] {
                let pb = dir.join(name);
                if executable(&pb) {
                    return Ok(pb);
                }
            }
        }
    }
    if let Some(p) = which("rclone") {
        return Ok(p);
    }
    Err("rclone program not found (use --program=...)".into())
}

// ---------------------------------------------------------------------------
// 极简 HTTP 客户端（只访问 127.0.0.1 的 rclone rc）
// ---------------------------------------------------------------------------

fn http_request(method: &str, url: &str, body: Option<&str>) -> Result<(u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("unsupported url: {}", url))?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = if hostport.contains(':') {
        hostport.to_string()
    } else {
        format!("{}:80", hostport)
    };
    let sock: std::net::SocketAddr = addr
        .parse()
        .map_err(|_| format!("bad address: {}", addr))?;
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_secs(2))
        .map_err(|e| format!("connect {}: {}", addr, e))?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let body = body.unwrap_or("");
    let req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        method,
        path,
        hostport,
        body.len(),
        body
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = match text.find("\r\n\r\n") {
        Some(i) => (&text[..i], &text[i + 4..]),
        None => return Err(format!("bad http response: {}", &text[..text.len().min(120)])),
    };
    let code = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "bad status line".to_string())?;
    let chunked = head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    let body = if chunked {
        decode_chunked(body)
    } else {
        body.to_string()
    };
    Ok((code, body))
}

fn decode_chunked(mut s: &str) -> String {
    let mut out = String::new();
    loop {
        let i = match s.find("\r\n") {
            Some(i) => i,
            None => break,
        };
        let size = usize::from_str_radix(s[..i].trim(), 16).unwrap_or(0);
        s = &s[i + 2..];
        if size == 0 {
            break;
        }
        if s.len() < size {
            out.push_str(s);
            break;
        }
        out.push_str(&s[..size]);
        s = &s[(size + 2).min(s.len())..];
    }
    out
}

// ---------------------------------------------------------------------------
// 子命令：version / fuse / config / check / process / disk / port / rc
// ---------------------------------------------------------------------------

fn cmd_version() -> ! {
    println!("===== Version Info =====");
    println!("Version:               {}", MONITOR_VERSION);
    println!("Tag:                   Linux");
    println!("Build OS:              Linux");
    println!("Build Arch:            AMD64");
    println!("===== Runtime Info =====");
    println!("OS:                    linux");
    println!("Platform:              Linux");
    println!("Kernel Arch:           x86_64");
    println!("===== Version Info =====");
    std::process::exit(0)
}

fn fuse_installed() -> bool {
    Path::new("/dev/fuse").exists() && which("fusermount3").is_some()
}

fn fuse_status_json() -> String {
    if fuse_installed() {
        // 官方在 Windows 返回 [{name,guid,is_installed}]，mac 返回 fuse-t/macFUSE。
        // Linux 对应的实体是内核 FUSE + fuse3 工具，返回一项即可让主进程判定“已安装”。
        "[{\"name\":\"fuse3\",\"is_installed\":true}]".to_string()
    } else {
        "[]".to_string()
    }
}

fn cmd_fuse(args: &Args) -> ! {
    if args.has("status") {
        let arr = fuse_status_json();
        if arr == "[]" {
            println!(
                "{{\"status\":200,\"data\":[],\"error\":\"FUSE not installed\"}}"
            );
        } else {
            emit_ok(&arr);
        }
        std::process::exit(0)
    }
    if args.get("install").is_some() {
        emit_err("install FUSE is handled by the distro package manager (fuse3)")
    }
    if args.get("uninstall").is_some() {
        emit_err("FUSE cannot uninstall")
    }
    emit_ok("null")
}

fn config_section(args: &Args) -> String {
    args.get("config-section")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "uzmount".to_string())
}

fn cmd_config(args: &Args) -> ! {
    let home = match args.get("home-dir") {
        Some(h) if !h.is_empty() => PathBuf::from(h),
        _ => emit_err("home-dir is required"),
    };
    if !args.has("add") {
        // 官方没有 query 形态；保持简洁，直接返回当前配置文件内容。
        let conf = home.join("mount.conf");
        match read_file(&conf) {
            Some(s) => emit_ok(&format!("\"{}\"", esc(&s))),
            None => emit_err(&format!("config not found: {}", conf.display())),
        }
    }
    let section = config_section(args);
    let url = args.get("url").unwrap_or_default();
    let user = args.get("user").unwrap_or_default();
    let pass = args.get("pass").unwrap_or_default();
    let program = match resolve_program(args) {
        Ok(p) => p,
        Err(e) => emit_err(&e),
    };
    if let Err(e) = mkdirs(&home) {
        emit_err(&e);
    }
    let conf = home.join("mount.conf");
    let stored_pass = if args.has("obscure") {
        match cmd_output(
            program.to_string_lossy().as_ref(),
            &["obscure".to_string(), pass.clone()],
        ) {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Ok(o) => emit_err(&format!(
                "failed to obscure pass: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => emit_err(&format!("failed to obscure pass: {}", e)),
        }
    } else {
        pass
    };
    let content = format!(
        "[{}]\ntype = webdav\nurl = {}\nvendor = other\nuser = {}\npass = {}\n\n",
        section, url, user, stored_pass
    );
    if let Err(e) = write_file(&conf, &content) {
        emit_err(&e);
    }
    let cmdline = format!(
        "{} --config={} config create {} webdav url={} vendor=other user={} pass=***",
        program.display(),
        conf.display(),
        section,
        url,
        user
    );
    let out = format!(
        "[{}]\ntype = webdav\nurl = {}\nvendor = other\nuser = {}\npass = *** ENCRYPTED ***\n",
        section, url, user
    );
    emit_ok(&format!(
        "{{\"cmd\":\"{}\",\"out\":\"{}\"}}",
        esc(&cmdline),
        esc(&out)
    ))
}

fn fs_type(dir: &Path) -> String {
    match Command::new("stat")
        .arg("-f")
        .arg("-c")
        .arg("%T")
        .arg(dir)
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    }
}

fn disk_free(dir: &Path) -> u64 {
    match Command::new("df")
        .arg("-P")
        .arg("-B1")
        .arg(dir)
        .output()
    {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .nth(1)
                .and_then(|l| l.split_whitespace().nth(3))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
        }
        _ => 0,
    }
}

/// 解析 rclone SizeSuffix（10G / 500M / 1.5Gi ...）。
fn parse_size(s: &str) -> u64 {
    let t = s.trim();
    let mut num = String::new();
    let mut suffix = String::new();
    for c in t.chars() {
        if c.is_ascii_digit() || c == '.' {
            if suffix.is_empty() {
                num.push(c);
            }
        } else {
            suffix.push(c);
        }
    }
    let v: f64 = num.parse().unwrap_or(0.0);
    let mult: f64 = match suffix.to_ascii_lowercase().as_str() {
        "k" | "ki" | "kib" => 1024.0,
        "m" | "mi" | "mib" => 1024.0 * 1024.0,
        "g" | "gi" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "ti" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (v * mult) as u64
}

/// 官方行为：缓存上限不能超过可用空间；不足时取可用空间的一半。
fn effective_cache_max_size(requested: &str, cache: &Path) -> String {
    let want = parse_size(requested);
    let free = disk_free(cache);
    if want > 0 && free > 0 && free < want {
        let half = free / 2;
        format!("{:.3}Gi", half as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        requested.to_string()
    }
}

fn is_mountpoint(p: &Path) -> bool {    let target = match p.canonicalize() {
        Ok(t) => t,
        Err(_) => return false,
    };
    let info = match read_file(Path::new("/proc/self/mountinfo")) {
        Some(s) => s,
        None => return false,
    };
    for line in info.lines() {
        let mut it = line.split_whitespace();
        let _id = it.next();
        let _parent = it.next();
        let _majmin = it.next();
        let _root = it.next();
        if let Some(mp) = it.next() {
            if Path::new(mp) == target {
                return true;
            }
        }
    }
    false
}

fn mountpoint_for(args: &Args) -> Result<PathBuf, String> {
    // Linux 分支对齐官方 mac 分支的路径推导：挂载点 = <cache-dir>/<volume-label>。
    // 官方 Windows 分支的 volume 是盘符，mac 分支的清理路径同样落在 cache-dir 下。
    let cache = args
        .get("cache-dir")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "cache-dir is required".to_string())?;
    let label = args
        .get("volume-label")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "极空间-个人".to_string());
    Ok(PathBuf::from(cache).join(label))
}

fn cmd_check(args: &Args) -> ! {
    let cache = match args.get("cache-dir").filter(|s| !s.is_empty()) {
        Some(c) => PathBuf::from(c),
        None => emit_err("cache-dir is required"),
    };
    let volume = args.get("volume").unwrap_or_default();
    let created = mkdirs(&cache).is_ok();
    let mut can_read = false;
    let mut can_write = false;
    if created {
        can_read = fs::read_dir(&cache).is_ok();
        let test = cache.join(".rw_test.tmp");
        if fs::write(&test, b"zspace").is_ok() {
            can_write = true;
            let _ = fs::remove_file(&test);
        }
    }
    let fstype = fs_type(&cache);
    let lower = fstype.to_lowercase();
    let sparse = !(lower.contains("vfat")
        || lower.contains("fat")
        || lower.contains("msdos")
        || lower.contains("exfat")
        || lower.contains("iso9660"));
    let cloud = lower.contains("fuse") || lower.contains("nfs") || lower.contains("cifs");
    let mp = mountpoint_for(args).unwrap_or_else(|_| cache.clone());
    let mp_used = is_mountpoint(&mp);
    let volume_valid = created
        && !volume.is_empty()
        && !mp_used
        && (can_write || fs::metadata(&mp).is_ok());
    let free = disk_free(&cache);
    let root = unsafe { geteuid() == 0 };
    let fuse = fuse_status_json();
    let fuse_running = fuse_installed();
    let data = format!(
        "{{\"can_read\":{},\"can_write\":{},\"disk_free_space\":{},\"drive_letter_used\":[],\"is_mount_point_used\":{},\"is_volume_valid\":{},\"disk_file_system\":\"{}\",\"drive_type\":\"Fixed\",\"is_support_sparse\":{},\"is_support_sha2\":true,\"fuse_status\":{},\"fuse_service_running\":{},\"is_cloud\":{},\"is_root\":{}}}",
        can_read,
        can_write,
        free,
        mp_used,
        volume_valid,
        esc(&fstype),
        sparse,
        fuse,
        fuse_running,
        cloud,
        root
    );
    emit_ok(&data)
}

fn proc_name(pid: u32) -> Option<String> {
    read_file(Path::new(&format!("/proc/{}/comm", pid))).map(|s| s.trim().to_string())
}

fn cmd_process(args: &Args) -> ! {
    if args.has("list") {
        let raw = args.get("name").unwrap_or_default();
        let prefix = raw.trim_end_matches('*');
        let mut items = Vec::new();
        if let Ok(rd) = fs::read_dir("/proc") {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if let Ok(pid) = name.parse::<u32>() {
                    if let Some(c) = proc_name(pid) {
                        if c.starts_with(prefix) {
                            items.push(format!("{{\"pid\":{},\"name\":\"{}\"}}", pid, esc(&c)));
                        }
                    }
                }
            }
        }
        emit_ok(&format!("[{}]", items.join(",")))
    }
    if args.has("kill") {
        let pid = args
            .get("pid")
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(-1);
        if pid <= 0 {
            emit_err("pid is required");
        }
        let r = unsafe { kill(pid, SIGTERM) };
        if r == 0 {
            emit_ok("null")
        } else {
            emit_err(&format!("kill {} failed", pid))
        }
    }
    emit_ok("[]")
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    let md = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if md.is_file() {
        return md.len();
    }
    if md.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                total += dir_size(&e.path());
            }
        }
    }
    total
}

fn cmd_disk(args: &Args) -> ! {
    if args.has("size") {
        let dir = args.get("dir").unwrap_or_default();
        let n = dir_size(Path::new(&dir));
        emit_ok(&n.to_string())
    }
    if args.has("list") {
        emit_ok("[]")
    }
    emit_ok("null")
}

fn cmd_port(args: &Args) -> ! {
    if args.has("free") {
        match free_port() {
            Ok(p) => emit_ok(&p.to_string()),
            Err(e) => emit_err(&e),
        }
    }
    emit_ok("null")
}

fn cmd_check_port(args: &Args) -> ! {
    let p: u16 = args
        .get("check")
        .or_else(|| args.positional.first().cloned())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let (old, new) = if p == 0 {
        (0u16, free_port().unwrap_or(0))
    } else if port_free(p) {
        (p, p)
    } else {
        (p, free_port().unwrap_or(0))
    };
    emit_ok(&format!(
        "{{\"new_port\":{},\"old_port\":{}}}",
        new, old
    ))
}

fn rc_url_parts(url: &str) -> (String, String) {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    match rest.find('/') {
        Some(i) => (rest[..i].to_string(), rest[i..].to_string()),
        None => (rest.to_string(), "/".to_string()),
    }
}

fn cmd_rc(args: &Args) -> ! {
    let url = match args.get("rc-addr").filter(|s| !s.is_empty()) {
        Some(u) => u,
        None => emit_err("rc-addr is required"),
    };
    let (hostport, path) = rc_url_parts(&url);
    if path == "/mount/status" {
        // 官方 rclone fork 自带 /mount/status；上游 rclone 没有。
        // rclone rc 只接受 POST，这里用 TCP 连通性探活：能连上即视为挂载进程在运行。
        let addr = if hostport.contains(':') {
            hostport.clone()
        } else {
            format!("{}:80", hostport)
        };
        match addr.parse::<std::net::SocketAddr>() {
            Ok(sock) => match TcpStream::connect_timeout(&sock, Duration::from_secs(2)) {
                Ok(_) => emit_ok("{\"is_running\":true,\"is_mounting\":true}"),
                Err(e) => emit_err(&format!("connect {}: {}", addr, e)),
            },
            Err(_) => emit_err(&format!("bad address: {}", addr)),
        }
    }
    if path == "/shutdown" {
        let quit = format!("http://{}/core/quit", hostport);
        match http_request("POST", &quit, Some("{}")) {
            Ok((code, body)) if (200..300).contains(&code) => {
                emit_ok(&format!("\"{}\"", esc(body.trim())))
            }
            Ok((code, body)) => emit_err(&format!("shutdown {}: {}", code, body)),
            Err(e) => emit_err(&e),
        }
    }
    if path == "/mount/restart" {
        emit_ok("{\"is_running\":true}")
    }
    // 其它 rc 路径：rclone 只认 POST，body 用空 JSON。
    match http_request("POST", &url, Some("{}")) {
        Ok((code, body)) if (200..300).contains(&code) => {
            let b = body.trim();
            if b.starts_with('{') || b.starts_with('[') {
                emit_ok(b)
            } else {
                emit_ok(&format!("\"{}\"", esc(b)))
            }
        }
        Ok((code, body)) => emit_err(&format!("request failed {}: {}", code, body)),
        Err(e) => emit_err(&e),
    }
}

fn cmd_network(args: &Args) -> ! {
    let _ = args;
    // 官方仅 mac 分支使用（/etc/hosts 指向 极空间.local）；Linux 不做系统级改动。
    emit_ok("null")
}

fn cmd_sys() -> ! {
    emit_ok("{}")
}

fn cmd_drivers() -> ! {
    emit_ok("[]")
}

// ---------------------------------------------------------------------------
// 根命令：挂载
// ---------------------------------------------------------------------------

fn mount_args(
    conf: &Path,
    section: &str,
    mountpoint: &Path,
    cache: &Path,
    max_size: &str,
    label: &str,
    rc_port: u16,
    filter: Option<&Path>,
    use_filter: bool,
    show_hidden: bool,
    rc_opts: &[String],
) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    a.push(format!("--config={}", conf.display()));
    a.push("mount".to_string());
    a.push(format!("{}:", section));
    a.push(mountpoint.display().to_string());
    a.push(format!("--cache-dir={}", cache.display()));
    a.push("--vfs-cache-mode=full".to_string());
    a.push(format!("--vfs-cache-max-size={}", max_size));
    a.push("--vfs-cache-max-age=10m".to_string());
    a.push("--attr-timeout=5m".to_string());
    a.push("--file-perms=0777".to_string());
    a.push("--dir-cache-time=1m".to_string());
    a.push("--vfs-write-back=1s".to_string());
    a.push("--stats=5s".to_string());
    a.push(format!("--volname={}", label));
    a.push("--rc".to_string());
    a.push(format!("--rc-addr=127.0.0.1:{}", rc_port));
    a.push("--rc-no-auth".to_string());
    // 与官方 monitor 的内置过滤保持一致：
    //   --use-filter 时用 home/filter.txt；否则默认隐藏所有点开头条目；
    //   --show-hidden 时两条内置过滤都不加。
    if use_filter {
        if let Some(f) = filter {
            a.push(format!("--filter-from={}", f.display()));
        }
    } else if !show_hidden {
        a.push("--filter=- .*/".to_string());
        a.push("--filter=- .*".to_string());
    }
    // 官方 ZSpaceMount 的 --rc-o 是“传给 rclone 的额外参数”（如 -vv）。
    for o in rc_opts {
        if !o.is_empty() {
            a.push(o.clone());
        }
    }
    a
}

fn spawn_mount(
    program: &Path,
    margs: &[String],
    log: &Path,
) -> Result<Child, String> {
    let out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("open {}: {}", log.display(), e))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    Command::new(program)
        .args(margs)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("spawn {}: {}", program.display(), e))
}

fn cleanup(home: &Path, mountpoint: &Path, logger: &mut Logger) {
    // 依次尝试 fusermount3 -u -z / umount -l，避免残留 FUSE 挂载点。
    let mp = mountpoint.to_string_lossy().to_string();
    if is_mountpoint(mountpoint) {
        let _ = Command::new("fusermount3")
            .args(["-u", "-z", &mp])
            .status();
        if is_mountpoint(mountpoint) {
            let _ = Command::new("umount").args(["-l", &mp]).status();
        }
    }
    let removed = fs::remove_file(home.join("mount.port")).is_ok();
    let _ = fs::remove_file(home.join("mount.lock"));
    logger.log("Info", &format!("cleanup done (port file removed={})", removed));
}

fn cmd_mount(args: &Args) -> ! {
    unsafe {
        signal(SIGINT, on_signal as *const () as usize);
        signal(SIGTERM, on_signal as *const () as usize);
    }
    let home = match args.get("home-dir").filter(|s| !s.is_empty()) {
        Some(h) => PathBuf::from(h),
        None => emit_err("home-dir is required"),
    };
    let cache = match args.get("cache-dir").filter(|s| !s.is_empty()) {
        Some(c) => PathBuf::from(c),
        None => emit_err("cache-dir is required"),
    };
    if let Err(e) = mkdirs(&home) {
        emit_err(&e);
    }
    if let Err(e) = mkdirs(&cache) {
        emit_err(&e);
    }
    let mut logger = Logger::new(&home.join("mount_monitor.log"));
    logger.log("Info", "===== Start ====");
    let cmdline: Vec<String> = std::env::args().collect();
    logger.log("Info", &cmdline.join(" "));

    let program = match resolve_program(args) {
        Ok(p) => p,
        Err(e) => {
            logger.log("Fatal", &e);
            emit_err(&e)
        }
    };
    let conf = home.join("mount.conf");
    if !conf.is_file() {
        let e = format!("config not found: {}", conf.display());
        logger.log("Fatal", &e);
        emit_err(&e);
    }
    let label = args
        .get("volume-label")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "极空间-个人".to_string());
    let mountpoint = match mountpoint_for(args) {
        Ok(p) => p,
        Err(e) => {
            logger.log("Fatal", &e);
            emit_err(&e)
        }
    };
    if let Err(e) = mkdirs(&mountpoint) {
        logger.log("Fatal", &e);
        emit_err(&e);
    }
    if is_mountpoint(&mountpoint) {
        let e = format!("mount point is in use: {}", mountpoint.display());
        logger.log("Fatal", &e);
        emit_err(&e);
    }
    // 官方用 mount.lock 表示已有监控进程；这里写入 pid，便于排查。
    if let Some(old) = read_file(&home.join("mount.lock")) {
        if let Ok(pid) = old.trim().parse::<i32>() {
            if pid > 0 && Path::new(&format!("/proc/{}", pid)).exists() {
                let e = format!("another mount monitor is running: pid {}", pid);
                logger.log("Fatal", &e);
                emit_err(&e);
            }
        }
    }
    let _ = write_file(&home.join("mount.lock"), &format!("{}\n", std::process::id()));

    let rc_port = match free_port() {
        Ok(p) => p,
        Err(e) => {
            logger.log("Fatal", &e);
            emit_err(&e)
        }
    };
    let _ = write_file(&home.join("mount.port"), &format!("{}\n", rc_port));

    let filter = home.join("filter.txt");
    let filter = if filter.is_file() { Some(filter) } else { None };
    let max_size = args
        .get("cache-max-size")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "10G".to_string());
    let max_size = effective_cache_max_size(&max_size, &cache);
    logger.log("Info", &format!("cache max size: {}", max_size));
    let section = config_section(args);
    let margs = mount_args(
        &conf,
        &section,
        &mountpoint,
        &cache,
        &max_size,
        &label,
        rc_port,
        filter.as_deref(),
        args.has("use-filter"),
        args.has("show-hidden"),
        &args.get_all("rc-o"),
    );
    let child_log = home.join("mount.log");
    let mut child = match spawn_mount(&program, &margs, &child_log) {
        Ok(c) => c,
        Err(e) => {
            logger.log("Fatal", &e);
            let _ = fs::remove_file(home.join("mount.port"));
            let _ = fs::remove_file(home.join("mount.lock"));
            emit_err(&e)
        }
    };
    logger.log(
        "Info",
        &format!(
            "[MOUNT] start mount, PID: {}, command: {} {}",
            child.id(),
            program.display(),
            margs.join(" ")
        ),
    );

    const MAX_RETRY: u32 = 3;
    let mut retries = 0u32;
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            logger.log("Info", "signal received, shutting down");
            let quit = format!("http://127.0.0.1:{}/core/quit", rc_port);
            let _ = http_request("POST", &quit, Some(""));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    _ if Instant::now() >= deadline => {
                        let _ = child.kill();
                        break;
                    }
                    _ => sleep(Duration::from_millis(100)),
                }
            }
            cleanup(&home, &mountpoint, &mut logger);
            logger.log("Info", "===== Exit ====");
            std::process::exit(0);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                logger.log(
                    "Warning",
                    &format!("[MOUNT] mount exited ({})", status),
                );
                if status.success() {
                    // 官方 /shutdown 或外部 fusermount -u 会让 rclone 正常退出。
                    logger.log("Info", "[MOUNT] mount stopped normally");
                    cleanup(&home, &mountpoint, &mut logger);
                    logger.log("Info", "===== Exit ====");
                    std::process::exit(0);
                }
                if retries >= MAX_RETRY {
                    logger.log("Error", "[MOUNT] mount crashed too many times, exited");
                    cleanup(&home, &mountpoint, &mut logger);
                    logger.log("Info", "===== Exit ====");
                    std::process::exit(1);
                }
                retries += 1;
                let delay = Duration::from_secs(2);
                logger.log(
                    "Warning",
                    &format!(
                        "[MOUNT] mount crashed, retry {}/{} after {:?}",
                        retries, MAX_RETRY, delay
                    ),
                );
                sleep(delay);
                match spawn_mount(&program, &margs, &child_log) {
                    Ok(c) => {
                        child = c;
                        logger.log(
                            "Info",
                            &format!("[MOUNT] restart mount, PID: {}", child.id()),
                        );
                    }
                    Err(e) => {
                        logger.log("Error", &format!("[MOUNT] start mount error when restart: {}", e));
                        cleanup(&home, &mountpoint, &mut logger);
                        logger.log("Info", "===== Exit ====");
                        std::process::exit(1);
                    }
                }
            }
            Ok(None) => {
                // 首次成功启动后把重试计数归零，避免偶发重启累计。
                if retries > 0 && child.id() > 0 {
                    retries = 0;
                }
                sleep(Duration::from_millis(300));
            }
            Err(e) => {
                logger.log("Error", &format!("wait mount error: {}", e));
                sleep(Duration::from_millis(500));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_suffixes() {
        assert_eq!(parse_size("10G"), 10 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1.5Gi"), 3 * 1024 * 1024 * 1024 / 2);
        assert_eq!(parse_size("500M"), 500 * 1024 * 1024);
        assert_eq!(parse_size("1024"), 1024);
    }

    fn base_args(show_hidden: bool) -> Vec<String> {
        mount_args(
            Path::new("/tmp/home/mount.conf"),
            "uzmount",
            Path::new("/tmp/cache/极空间-个人"),
            Path::new("/tmp/cache"),
            "10G",
            "极空间-个人",
            12345,
            None,
            false,
            show_hidden,
            &[],
        )
    }

    #[test]
    fn default_filters_hide_dotfiles() {
        let a = base_args(false);
        assert!(a.iter().any(|x| x == "--filter=- .*/"));
        assert!(a.iter().any(|x| x == "--filter=- .*"));
    }

    #[test]
    fn show_hidden_drops_default_filters() {
        let a = base_args(true);
        assert!(!a.iter().any(|x| x.starts_with("--filter")));
    }

    #[test]
    fn mount_shape_matches_official() {
        let a = base_args(false);
        assert_eq!(a[0], "--config=/tmp/home/mount.conf");
        assert_eq!(a[1], "mount");
        assert_eq!(a[2], "uzmount:");
        assert_eq!(a[3], "/tmp/cache/极空间-个人");
        assert!(a.iter().any(|x| x == "--vfs-cache-mode=full"));
        assert!(a.iter().any(|x| x == "--rc-addr=127.0.0.1:12345"));
        assert!(a.iter().any(|x| x == "--rc-no-auth"));
    }
}

fn main() {
    let args = Args::parse();
    match args.cmd.as_deref() {
        None => cmd_mount(&args),
        Some("version") => cmd_version(),
        Some("fuse") => cmd_fuse(&args),
        Some("config") => cmd_config(&args),
        Some("check") => cmd_check(&args),
        Some("process") => cmd_process(&args),
        Some("disk") | Some("disk-free-space") => cmd_disk(&args),
        Some("port") | Some("random-free-port") => cmd_port(&args),
        Some("check-port-free") | Some("only-check-port-free") => cmd_check_port(&args),
        Some("rc") => cmd_rc(&args),
        Some("network") => cmd_network(&args),
        Some("sys") => cmd_sys(),
        Some("drivers") => cmd_drivers(),
        Some("completion") | Some("help") => {
            println!("Usage:\n  UZMountMonitor [flags]\n  UZMountMonitor [command]");
            std::process::exit(0)
        }
        Some(other) => emit_err(&format!("unknown command: {}", other)),
    }
}
