//! `zspace-player` — 协议兼容的极空间播放器进程（Linux 原生，libvlc 后端）。
//!
//! 启动参数与官方 `UZPlayer.exe` 一致：
//!
//! ```text
//! zspace-player "%7B%22server%22%3A%7B%22ip%22%3A%22127.0.0.1%22%2C%22port%22%3A13612%7D%7D" \
//!                "1" "715,451" "zspace" zh "username=foo"
//! ```
//!
//! 第一个参数是 `encodeURIComponent(JSON.stringify({server:{ip,port}}))`。

mod protocol;
mod ui;
mod vlc;

use protocol::FrameReader;
use serde_json::{json, Value};
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ui::{AppWindow, UiOptions};

#[derive(Debug)]
pub enum Command {
    TogglePause,
    SeekBy(f64),
    VolumeBy(i32),
    MuteToggle,
    AudioNext,
    SubtitleNext,
    RateBy(f32),
    Quit,
}

pub enum Event {
    Command(Command),
    Net(Value),
    NetClosed(String),
}

// ---------------------------------------------------------------------------
// logging
// ---------------------------------------------------------------------------

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

fn init_log() {
    let file = env::var("ZSPACE_PLAYER_LOG").ok().and_then(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
    });
    let _ = LOG_FILE.set(Mutex::new(file));
}

fn local_now() -> (String, String) {
    unsafe {
        let mut now: libc::time_t = 0;
        libc::time(&mut now);
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        let date = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        );
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_millis();
        let clock = format!("{:02}:{:02}:{:02}.{:03}", tm.tm_hour, tm.tm_min, tm.tm_sec, ms);
        (date, clock)
    }
}

fn log(msg: impl AsRef<str>) {
    let (_, clock) = local_now();
    let line = format!("[{clock}] [player] {}", msg.as_ref());
    eprintln!("{line}");
    if let Some(lock) = LOG_FILE.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = writeln!(file, "{line}");
                let _ = file.flush();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// args
// ---------------------------------------------------------------------------

struct Args {
    encoded: Option<String>,
    zoom: f64,
    center: Option<(i16, i16)>,
    app_name: String,
    lang: String,
    username: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            encoded: None,
            zoom: 1.0,
            center: None,
            app_name: "zspace".into(),
            lang: "zh".into(),
            username: None,
        }
    }
}

impl Args {
    /// 只接受官方 `UZPlayer.exe` 的 6 个位置参数，不引入额外命令行选项。
    fn parse() -> Result<Args, String> {
        let mut args = Args::default();
        let mut positional: Vec<String> = Vec::new();
        for arg in env::args().skip(1) {
            match arg.as_str() {
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                other if other.starts_with("--") => {
                    log(format!("忽略未知参数 {other}（原版播放器同样不使用额外参数）"));
                }
                _ => positional.push(arg),
            }
        }

        if let Some(first) = positional.first() {
            args.encoded = Some(first.clone());
        }
        if let Some(zoom) = positional.get(1) {
            if let Ok(z) = zoom.parse::<f64>() {
                if z > 0.1 && z < 5.0 {
                    args.zoom = z;
                }
            }
        }
        if let Some(center) = positional.get(2) {
            if let Some((x, y)) = center.split_once(',') {
                if let (Ok(x), Ok(y)) = (x.parse::<i16>(), y.parse::<i16>()) {
                    args.center = Some((x, y));
                }
            }
        }
        if let Some(app) = positional.get(3) {
            if !app.is_empty() && !app.starts_with("username=") {
                args.app_name = app.clone();
            }
        }
        if let Some(lang) = positional.get(4) {
            if !lang.starts_with("username=") {
                args.lang = lang.clone();
            }
        }
        for p in positional.iter().skip(4) {
            if let Some(name) = p.strip_prefix("username=") {
                args.username = Some(name.to_string());
            }
        }
        Ok(args)
    }

    fn server_from_cmd(&self) -> Option<(String, u16)> {
        let encoded = self.encoded.as_deref()?;
        let decoded = percent_decode(encoded);
        let v: Value = serde_json::from_str(&decoded).ok()?;
        let server = v.get("server")?;
        let ip = server.get("ip").and_then(|x| x.as_str()).unwrap_or("127.0.0.1");
        let port = server.get("port").and_then(|x| x.as_u64())? as u16;
        Some((ip.to_string(), port))
    }
}

fn print_usage() {
    println!(
        "zspace-player 0.1.0 — 极空间播放器的协议兼容实现（libvlc 后端）\n\
         \n\
         用法（与官方 UZPlayer.exe 完全一致）:\n\
         zspace-player \"<urlencoded {{\\\"server\\\":{{\\\"ip\\\":\\\"127.0.0.1\\\",\\\"port\\\":13612}}}}\"\" <zoom> <x,y> <appName> <lang> username=<user>\n\
         \n\
         取不到启动参数时，按原版逻辑回落到 <userData>/<appName>/zspace.conf 的 playerPort。\n\
         自测用的开关只放在环境变量里（不属于用户选项）：\n\
         ZSPACE_PLAYER_HEADLESS=1   不开窗口（dummy 输出，CI 自测用）\n\
         ZSPACE_PLAYER_LOG=<path>   追加日志到文件\n\
         ZSPACE_PLAYER_DEBUG=1      打开 libvlc 详细日志\n\
         ZSPACE_VLC_LIB=<path>      指定 libvlc.so.5（默认自动探测）\n\
         ZSPACE_VLC_PLUGINS=<path>  指定 VLC 插件目录（默认自动探测）\n"
    );
}

fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn port_from_conf(app_name: &str) -> Option<u16> {
    let mut dirs = Vec::new();
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        dirs.push(std::path::PathBuf::from(xdg));
    }
    if let Ok(home) = env::var("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".config"));
    }
    if let Ok(explicit) = env::var("ZSPACE_PLAYER_USERDATA") {
        dirs.insert(0, std::path::PathBuf::from(explicit));
    }
    for dir in dirs {
        for candidate in [dir.join(app_name).join("zspace.conf"), dir.join("zspace").join("zspace.conf")] {
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(port) = v.get("playerPort").and_then(|x| x.as_u64()) {
                        log(format!("从 {} 读取 playerPort={port}", candidate.display()));
                        return Some(port as u16);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// TCP
// ---------------------------------------------------------------------------

struct Socket {
    stream: TcpStream,
    client: Value,
    last_write: Instant,
}

impl Socket {
    fn new(stream: TcpStream) -> Result<Self, String> {
        let local = stream.local_addr().map_err(|e| e.to_string())?;
        let pid = std::process::id();
        let client = json!({
            "ip": "127.0.0.1",
            "pid": pid,
            "port": local.port().to_string(),
        });
        Ok(Socket { stream, client, last_write: Instant::now() })
    }

    /// One message per `write`, with a small gap so the Electron side never
    /// receives two JSON documents inside one `data` event (it does a bare
    /// `JSON.parse(chunk)`).
    fn send(&mut self, mut value: Value) {
        value["client"] = self.client.clone();
        let text = serde_json::to_string(&value).unwrap_or_default();
        let gap = self.last_write.elapsed();
        if gap < Duration::from_millis(25) {
            std::thread::sleep(Duration::from_millis(25) - gap);
        }
        if let Err(e) = self.stream.write_all(text.as_bytes()) {
            log(format!("发送消息失败: {e} ({text})"));
        } else {
            log(format!("发送 {text}"));
        }
        self.last_write = Instant::now();
    }
}

fn spawn_reader(mut stream: TcpStream, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let mut reader = FrameReader::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(Event::NetClosed("主进程关闭了连接".into()));
                    return;
                }
                Ok(n) => {
                    reader.feed(&buf[..n]);
                    loop {
                        match reader.next() {
                            Ok(Some(v)) => {
                                if tx.send(Event::Net(v)).is_err() {
                                    return;
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx.send(Event::NetClosed(format!("协议解析失败: {e}")));
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::NetClosed(format!("读取失败: {e}")));
                    return;
                }
            }
        }
    });
}

fn connect_with_retry(addr: SocketAddr, timeout: Duration) -> Result<TcpStream, String> {
    let deadline = Instant::now() + timeout;
    let mut last = String::new();
    while Instant::now() < deadline {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(300)) {
            Ok(stream) => {
                stream.set_nodelay(true).ok();
                return Ok(stream);
            }
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Err(format!("连接 {addr} 失败: {last}"))
}

/// The Deepin VLC bundle (`/opt/apps/org.videolan.www.vlc/files`) ships its own
/// FFmpeg under `ffmpeg445/lib`; plugins link `libavcodec.so.58` etc. Those
/// directories must be on `LD_LIBRARY_PATH` *before* the process starts, so the
/// player re-executes itself once with the bundle layout added.
fn maybe_reexec_for_bundled_vlc(paths: &vlc::VlcPaths) -> Result<(), String> {
    if env::var_os("ZSPACE_PLAYER_ENV_READY").is_some() {
        return Ok(());
    }
    let plugins = match &paths.plugins {
        Some(p) => p,
        None => return Ok(()),
    };
    // <root>/lib/vlc/plugins -> <root>
    let root = match plugins.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
        Some(root) => root,
        None => return Ok(()),
    };
    let ffmpeg_lib = root.join("ffmpeg445/lib");
    if !ffmpeg_lib.is_dir() {
        return Ok(());
    }
    let lib_dir = root.join("lib");
    let arch_dir = root.join("lib").join(match std::env::consts::ARCH {
        "x86_64" => "x86_64-linux-gnu",
        "aarch64" => "aarch64-linux-gnu",
        _ => "x86_64-linux-gnu",
    });

    let mut additions: Vec<String> = Vec::new();
    for dir in [&lib_dir, &arch_dir, &ffmpeg_lib] {
        if dir.is_dir() {
            additions.push(dir.display().to_string());
        }
    }
    let current = env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let missing = additions
        .iter()
        .any(|dir| !current.split(':').any(|existing| existing == dir));
    if !missing {
        return Ok(());
    }

    let mut combined = additions.clone();
    if !current.is_empty() {
        combined.push(current);
    }
    env::set_var("LD_LIBRARY_PATH", combined.join(":"));
    env::set_var("ZSPACE_PLAYER_ENV_READY", "1");
    log(format!(
        "为 VLC 捆绑 FFmpeg 重启自身: LD_LIBRARY_PATH={}",
        env::var("LD_LIBRARY_PATH").unwrap_or_default()
    ));

    use std::os::unix::process::CommandExt;
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let err = std::process::Command::new(exe)
        .args(env::args_os().skip(1))
        .exec();
    Err(err.to_string())
}

// ---------------------------------------------------------------------------
// player state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum PendingKind {
    /// 等主进程用 `11/3` 回新的播放地址（转码流 seek）。
    Seek { target: f64 },
    /// 等主进程用 `6/3` 回新的播放地址（转码流切音轨）。
    Audio { index: usize },
    /// 等主进程用 `11/3` 回字幕索引（`11/2`，不重新出地址）。
    Subtitle { index: Option<usize> },
}

struct Pending {
    kind: PendingKind,
    sent_at: Instant,
}

struct PlayState {
    api_host: String,
    current: Option<Value>,
    current_uri: Option<String>,
    audio_ordinal: Option<usize>,
    sub_ordinal: Option<usize>,
    tracks_applied: bool,
    tracks_deadline: Instant,
    started_ack: bool,
    subtitle_hash: String,
    subtitle_setting: Value,
    chapter: String,
    last_progress: Instant,
    last_heartbeat: Instant,
    finished: bool,
    stop_sent: bool,
    pending: Option<Pending>,
}

impl PlayState {
    fn new() -> Self {
        let now = Instant::now();
        PlayState {
            api_host: String::new(),
            current: None,
            current_uri: None,
            audio_ordinal: None,
            sub_ordinal: None,
            tracks_applied: false,
            tracks_deadline: now,
            started_ack: false,
            subtitle_hash: String::new(),
            subtitle_setting: json!({"offset": "0.0", "position": "", "colour": "", "size": ""}),
            chapter: String::new(),
            last_progress: now,
            last_heartbeat: now,
            finished: false,
            stop_sent: false,
            pending: None,
        }
    }
}

fn as_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().map(|n| n as i64))
        .or_else(|| v.as_f64().map(|n| n as i64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn field(v: &Value, name: &str) -> Option<i64> {
    v.get(name).and_then(as_i64)
}

/// `new_session` 只在“换片/换集”时为 true；seek / 切轨导致的重新出地址
/// 不重复上报 `1/2`（与官方日志一致：一次播放会话只有一条 1/2）。
fn start_item(
    engine: &mut vlc::Engine,
    st: &mut PlayState,
    item: &Value,
    uri: &str,
    new_session: bool,
) {
    let seek = item
        .get("seek")
        .and_then(as_f64)
        .or_else(|| item.get("start_time").and_then(as_f64))
        .unwrap_or(0.0)
        .max(0.0);

    let mut options: Vec<String> = Vec::new();
    if seek > 0.5 {
        options.push(format!(":start-time={seek:.3}"));
    }
    if uri.starts_with("http://") || uri.starts_with("https://") {
        options.push(":network-caching=1500".into());
    }
    if let Some(ua) = item.get("user-agent").and_then(|v| v.as_str()) {
        if !ua.is_empty() {
            options.push(format!(":http-user-agent={ua}"));
        }
    }

    log(format!(
        "开始播放 uri={uri} seek={seek:.1}s options={options:?} title={}",
        item.get("label").and_then(|v| v.as_str()).unwrap_or("")
    ));

    if let Err(e) = engine.open(uri, &options) {
        log(format!("打开媒体失败: {e}"));
        return;
    }
    let rc = engine.play();
    if rc != 0 {
        log(format!("libvlc_media_player_play 返回 {rc}"));
    }

    st.current = Some(item.clone());
    st.current_uri = Some(uri.to_string());
    st.audio_ordinal = field(item, "audio_index").and_then(|v| if v >= 0 { Some(v as usize) } else { None });
    st.sub_ordinal = field(item, "subtitle_index").and_then(|v| if v >= 0 { Some(v as usize) } else { None });
    st.subtitle_hash = item
        .get("subtitle_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    st.subtitle_setting = item
        .get("subtitle_setting")
        .cloned()
        .unwrap_or_else(|| json!({"offset": "0.0", "position": "", "colour": "", "size": ""}));
    st.chapter = item
        .get("play_chapter")
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    st.tracks_applied = false;
    st.tracks_deadline = Instant::now() + Duration::from_secs(20);
    if new_session {
        st.started_ack = false;
    }
    st.last_progress = Instant::now();
    st.last_heartbeat = Instant::now();
}

fn progress_payload(st: &PlayState, engine: &vlc::Engine) -> Value {
    let sec = (engine.time_ms() / 1000).max(0);
    let dur = (engine.length_ms() / 1000).max(0);
    json!({
        "f_type": 4,
        "s_type": 3,
        "sec": sec,
        "chapter": st.chapter,
        "audio_index": st.audio_ordinal.map(|v| v as i64).unwrap_or(-1),
        "subtitle_index": st.sub_ordinal.map(|v| v as i64).unwrap_or(-1),
        "subtitle_hash": st.subtitle_hash,
        "subtitle_setting": st.subtitle_setting,
        "video_duration": dur,
    })
}

fn apply_tracks(engine: &vlc::Engine, st: &mut PlayState) {
    let mut ok = true;
    if let Some(ord) = st.audio_ordinal {
        match engine.set_audio_ordinal(Some(ord)) {
            Ok(()) => log(format!("已切换到音轨 #{ord}")),
            Err(e) => {
                log(format!("音轨切换暂不可用: {e}"));
                ok = false;
            }
        }
    }
    if let Some(ord) = st.sub_ordinal {
        match engine.set_subtitle_ordinal(Some(ord)) {
            Ok(()) => log(format!("已切换到字幕 #{ord}")),
            Err(e) => {
                log(format!("字幕切换暂不可用: {e}"));
                ok = false;
            }
        }
    }
    if ok {
        st.tracks_applied = true;
    } else if Instant::now() > st.tracks_deadline {
        log("轨道选择超时，保持 VLC 默认轨道");
        st.tracks_applied = true;
    }
}

fn play_from_item(engine: &mut vlc::Engine, st: &mut PlayState, item: &Value, new_session: bool) {
    let uri = item.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    if uri.is_empty() {
        log(format!("消息里没有 uri，忽略: {item}"));
        return;
    }
    start_item(engine, st, item, uri, new_session);
}

fn current_sub_index(st: &PlayState) -> i64 {
    st.sub_ordinal.map(|v| v as i64).unwrap_or(-1)
}

fn apply_subtitle(engine: &vlc::Engine, st: &mut PlayState, index: Option<usize>) {
    match engine.set_subtitle_ordinal(index) {
        Ok(()) => {
            st.sub_ordinal = index;
            match index {
                Some(i) => log(format!("字幕 → #{i}")),
                None => log("字幕 → 关闭"),
            }
        }
        Err(e) => log(format!("切换字幕失败: {e}")),
    }
}

/// 原版在 seek 时发 `11/1`，由主进程重新请求 NAS 播放地址后回 `11/3`。
fn request_server_seek(st: &mut PlayState, sock: &mut Socket, target: f64) {
    if st.current.is_none() {
        return;
    }
    let sub = current_sub_index(st);
    st.pending = Some(Pending { kind: PendingKind::Seek { target }, sent_at: Instant::now() });
    log(format!("发送 11/1 请求重新出地址: seek={target:.1}s subtitle_index={sub}"));
    sock.send(json!({
        "f_type": 11,
        "s_type": 1,
        "seek": target,
        "subtitle_index": sub,
    }));
}

/// 原版切音轨发 `6/1`，主进程重新出地址后回 `6/3`。
fn request_server_audio(st: &mut PlayState, sock: &mut Socket, index: usize, seek_sec: f64) {
    if st.current.is_none() {
        return;
    }
    let sub = current_sub_index(st);
    st.pending = Some(Pending { kind: PendingKind::Audio { index }, sent_at: Instant::now() });
    log(format!(
        "发送 6/1 请求切换音轨: index={index} seek={seek_sec:.1}s subtitle={sub}"
    ));
    sock.send(json!({
        "f_type": 6,
        "s_type": 1,
        "result": index,
        "seek": seek_sec,
        "subtitle": sub,
    }));
}

/// 只切字幕时原版发 `11/2`：主进程只回索引，不重新出地址。
fn request_subtitle_only(st: &mut PlayState, sock: &mut Socket, index: Option<usize>) {
    if st.current.is_none() {
        return;
    }
    let value = index.map(|v| v as i64).unwrap_or(-1);
    st.pending = Some(Pending { kind: PendingKind::Subtitle { index }, sent_at: Instant::now() });
    log(format!("发送 11/2 请求切换字幕: subtitle_index={value}"));
    sock.send(json!({
        "f_type": 11,
        "s_type": 2,
        "subtitle_index": value,
    }));
}

/// 主进程没回、或回错误码时回退到本地操作（仅作为兜底，会写日志）。
fn apply_pending_fallback(engine: &vlc::Engine, st: &mut PlayState) {
    let pending = match st.pending.take() {
        Some(p) => p,
        None => return,
    };
    match pending.kind {
        PendingKind::Seek { target } => {
            engine.seek_ms((target * 1000.0) as i64);
            log(format!("等待 11/3 超时，回退本地 seek → {target:.1}s"));
        }
        PendingKind::Audio { index } => {
            if let Err(e) = engine.set_audio_ordinal(Some(index)) {
                log(format!("等待 6/3 超时，本地切音轨失败: {e}"));
            } else {
                st.audio_ordinal = Some(index);
                log(format!("等待 6/3 超时，回退本地切音轨 → #{index}"));
            }
        }
        PendingKind::Subtitle { index } => {
            apply_subtitle(engine, st, index);
            log("等待 11/3 超时，回退本地切字幕");
        }
    }
}

fn handle_message(v: &Value, engine: &mut vlc::Engine, st: &mut PlayState) -> bool {
    let f = field(v, "f_type");
    let s = field(v, "s_type");
    match (f, s) {
        (Some(100), _) => {
            st.api_host = v.get("apiHost").and_then(|x| x.as_str()).unwrap_or("").to_string();
            log(format!("收到 cookie/apiHost: {} (cookie keys={})", st.api_host,
                v.get("cookie").map(|c| c.as_object().map(|o| o.len()).unwrap_or(0)).unwrap_or(0)));
            false
        }
        (Some(5), Some(5)) | (Some(5), Some(1)) => {
            let list = v.get("playList").and_then(|x| x.as_array());
            log(format!("收到播放列表: {} 项", list.map(|a| a.len()).unwrap_or(0)));
            let item = list.and_then(|items| {
                items
                    .iter()
                    .find(|i| field(i, "active") == Some(1))
                    .or_else(|| items.first())
                    .cloned()
            });
            match item {
                Some(mut item) => {
                    let has_seek = item.get("seek").and_then(as_f64).unwrap_or(0.0) > 0.5;
                    if !has_seek {
                        let skip_start = v
                            .get("skip")
                            .and_then(|x| x.get("start"))
                            .and_then(as_f64)
                            .unwrap_or(0.0);
                        if skip_start > 0.5 {
                            log(format!("按片头跳过设置从 {skip_start:.0}s 开始"));
                            item["seek"] = json!(skip_start);
                        }
                    }
                    play_from_item(engine, st, &item, true);
                }
                None => log("播放列表为空"),
            }
            false
        }
        (Some(0), _) => {
            log("收到关闭指令 f_type=0");
            true
        }
        (Some(3), Some(2)) | (Some(11), Some(3)) | (Some(7), Some(3)) | (Some(6), Some(3)) => {
            if field(v, "code").unwrap_or(200) != 200 {
                log(format!(
                    "主进程返回错误: code={:?} message={}",
                    field(v, "code"),
                    v.get("message").and_then(|x| x.as_str()).unwrap_or("")
                ));
                apply_pending_fallback(engine, st);
                return false;
            }

            let has_uri = v
                .get("uri")
                .and_then(|x| x.as_str())
                .map(|uri| !uri.is_empty())
                .unwrap_or(false);

            if !has_uri {
                // `11/2` 的回复：只有字幕索引，没有新地址。
                if let Some(idx) = field(v, "subtitle_index") {
                    let ord = if idx >= 0 { Some(idx as usize) } else { None };
                    apply_subtitle(engine, st, ord);
                    log(format!("收到 11/3 字幕结果: subtitle_index={idx}"));
                } else {
                    log(format!("收到没有 uri 的 {} / {} 回复，忽略", f.unwrap_or(-1), s.unwrap_or(-1)));
                }
                if matches!(st.pending.as_ref().map(|p| &p.kind), Some(PendingKind::Subtitle { .. })) {
                    st.pending = None;
                }
                return false;
            }

            let prev_audio = st.audio_ordinal;
            let prev_sub = st.sub_ordinal;
            let mut item = v.clone();
            if let Some(obj) = item.as_object_mut() {
                obj.remove("instanceId");
                obj.remove("server");
                obj.remove("code");
            }
            // 3/2 是换集（新的播放会话）；11/3、6/3、7/3 只是重新出地址。
            let new_session = matches!((f, s), (Some(3), Some(2)));
            play_from_item(engine, st, &item, new_session);
            if item.get("audio_index").is_none() {
                st.audio_ordinal = prev_audio;
            }
            if item.get("subtitle_index").is_none() {
                st.sub_ordinal = prev_sub;
            }

            if let Some(pending) = st.pending.take() {
                match pending.kind {
                    PendingKind::Seek { target } => {
                        log(format!("收到 11/3 重新出地址，续播到 {target:.1}s"));
                    }
                    PendingKind::Audio { index } => {
                        if item.get("audio_index").is_none() {
                            st.audio_ordinal = Some(index);
                        }
                        log(format!("收到 6/3 音轨切换结果: #{index}"));
                    }
                    PendingKind::Subtitle { index } => {
                        st.sub_ordinal = index;
                        log(format!("收到 11/3 字幕结果: subtitle_index={}", index.map(|v| v as i64).unwrap_or(-1)));
                    }
                }
            } else {
                log(format!("收到新播放地址 (f_type={} / s_type={})", f.unwrap_or(-1), s.unwrap_or(-1)));
            }
            false
        }
        (Some(8), _) => {
            log(format!(
                "播放错误消息: info_type={:?} show_info={} reason={}",
                field(v, "info_type"),
                v.get("show_info").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("reason").and_then(|x| x.as_str()).unwrap_or("")
            ));
            false
        }
        _ => {
            log(format!("暂未实现的指令，忽略: f_type={f:?} s_type={s:?}"));
            false
        }
    }
}

fn handle_command(cmd: Command, engine: &mut vlc::Engine, st: &mut PlayState, sock: &mut Socket) {
    match cmd {
        Command::TogglePause => {
            let playing = engine.is_playing();
            engine.set_pause(playing);
            log(format!("暂停状态切换: {}", if playing { "暂停" } else { "继续" }));
            if st.current.is_some() {
                sock.send(progress_payload(st, engine));
            }
        }
        Command::SeekBy(delta) => {
            let target = (engine.time_ms() as f64 / 1000.0 + delta).max(0.0);
            request_server_seek(st, sock, target);
        }
        Command::VolumeBy(delta) => {
            let volume = engine.volume();
            let new = engine.set_volume(volume + delta);
            log(format!("音量 {volume} → {new}"));
        }
        Command::MuteToggle => {
            let volume = engine.volume();
            if volume > 0 {
                engine.set_volume(0);
                log(format!("静音（原音量 {volume}）"));
            } else {
                engine.set_volume(100);
                log("取消静音，音量 100");
            }
        }
        Command::AudioNext => {
            let tracks: Vec<_> = engine.audio_tracks().into_iter().filter(|t| t.id >= 0).collect();
            log(format!(
                "可用音轨: {:?}",
                tracks.iter().map(|t| format!("{}:{}", t.id, t.name)).collect::<Vec<_>>()
            ));
            if tracks.is_empty() {
                log("当前没有可切换的音轨");
                return;
            }
            let next = match st.audio_ordinal {
                Some(i) => (i + 1) % tracks.len(),
                None => 0,
            };
            let seek_sec = engine.time_ms() as f64 / 1000.0;
            request_server_audio(st, sock, next, seek_sec);
        }
        Command::SubtitleNext => {
            let tracks: Vec<_> = engine.subtitle_tracks().into_iter().filter(|t| t.id >= 0).collect();
            log(format!(
                "可用字幕轨: {:?}",
                tracks.iter().map(|t| format!("{}:{}", t.id, t.name)).collect::<Vec<_>>()
            ));
            let next: Option<usize> = match st.sub_ordinal {
                Some(i) if i + 1 < tracks.len() => Some(i + 1),
                Some(_) => None, // 关闭字幕
                None => {
                    if tracks.is_empty() { None } else { Some(0) }
                }
            };
            request_subtitle_only(st, sock, next);
        }
        Command::RateBy(delta) => {
            let rate = engine.set_rate(engine.rate() + delta);
            log(format!("倍速 → {rate:.2}x"));
        }
        Command::Quit => {}
    }
}

fn tick(engine: &mut vlc::Engine, st: &mut PlayState, sock: &mut Socket) {
    if st.current.is_none() {
        return;
    }
    let state = engine.state();
    let time_ms = engine.time_ms();

    if !st.started_ack && (state == vlc::STATE_PLAYING || time_ms > 0) {
        let uri = st.current_uri.clone().unwrap_or_default();
        let (date, _) = local_now();
        log(format!("播放已开始，上报 1/2 ({uri})"));
        sock.send(json!({
            "f_type": 1,
            "s_type": 2,
            "info": uri,
            "result": 0,
            "time": date,
        }));
        st.started_ack = true;
    }

    if state == vlc::STATE_PLAYING && !st.tracks_applied {
        apply_tracks(engine, st);
    }

    let pending_timed_out = st
        .pending
        .as_ref()
        .map(|p| p.sent_at.elapsed() > Duration::from_secs(8))
        .unwrap_or(false);
    if pending_timed_out {
        apply_pending_fallback(engine, st);
        sock.send(progress_payload(st, engine));
    }

    if state == vlc::STATE_ENDED && !st.finished {
        log("播放结束");
        sock.send(progress_payload(st, engine));
        if let Some(uri) = st.current_uri.clone() {
            sock.send(json!({"f_type": 1, "s_type": 3, "url": uri}));
        }
        st.stop_sent = true;
        st.finished = true;
        return;
    }
    if state == vlc::STATE_ERROR {
        if !st.finished {
            log("libvlc 报告播放错误");
            st.finished = true;
        }
        return;
    }

    if st.last_progress.elapsed() >= Duration::from_secs(10) {
        log(format!("上报进度 4/3: {}s", time_ms / 1000));
        sock.send(progress_payload(st, engine));
        st.last_progress = Instant::now();
        return;
    }
    if st.last_heartbeat.elapsed() >= Duration::from_secs(5) {
        let uri = st.current_uri.clone().unwrap_or_default();
        sock.send(json!({
            "f_type": 4,
            "s_type": 1,
            "info": uri,
            "videotime": (time_ms / 1000).max(0),
            "result": 0,
        }));
        st.last_heartbeat = Instant::now();
    }
}

// ---------------------------------------------------------------------------
// controller
// ---------------------------------------------------------------------------

struct ControllerOptions {
    window_id: Option<u32>,
    headless: bool,
    debug: bool,
    vlc_paths: vlc::VlcPaths,
}

fn controller(rx: Receiver<Event>, stream: TcpStream, opts: ControllerOptions) -> ExitCode {
    let mut sock = match Socket::new(stream) {
        Ok(s) => s,
        Err(e) => {
            log(format!("初始化 socket 失败: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let paths = opts.vlc_paths;
    log(format!(
        "使用 libvlc: {} (core={:?} plugins={:?})",
        paths.libvlc.display(),
        paths.libvlccore,
        paths.plugins
    ));

    let mut extra_args: Vec<String> = Vec::new();
    if opts.headless {
        extra_args.push("--vout=dummy".into());
        extra_args.push("--aout=dummy".into());
    }
    if opts.debug {
        extra_args.push("--verbose=2".into());
    }
    if let Ok(env_args) = env::var("ZSPACE_VLC_ARGS") {
        extra_args.extend(env_args.split_whitespace().map(|s| s.to_string()));
    }

    let mut engine = match vlc::Engine::new(&paths, &extra_args) {
        Ok(e) => e,
        Err(e) => {
            log(format!("libvlc 初始化失败: {e}"));
            return ExitCode::FAILURE;
        }
    };
    log(format!("libvlc 版本: {}", engine.version()));
    if let Some(id) = opts.window_id {
        engine.set_xwindow(id);
        log(format!("视频输出绑定到 X11 窗口 0x{id:x}"));
    }

    let mut st = PlayState::new();
    let mut quit = false;
    while !quit {
        match rx.recv_timeout(Duration::from_millis(150)) {
            Ok(Event::Command(Command::Quit)) => {
                log("收到退出命令");
                quit = true;
            }
            Ok(Event::Command(cmd)) => handle_command(cmd, &mut engine, &mut st, &mut sock),
            Ok(Event::Net(v)) => {
                quit = handle_message(&v, &mut engine, &mut st);
            }
            Ok(Event::NetClosed(reason)) => {
                log(format!("连接结束: {reason}"));
                quit = true;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => quit = true,
        }
        tick(&mut engine, &mut st, &mut sock);
        if st.finished && !quit {
            log("媒体已结束，退出播放器");
            quit = true;
        }
    }

    if !st.stop_sent {
        if let Some(uri) = st.current_uri.clone() {
            sock.send(json!({"f_type": 1, "s_type": 3, "url": uri}));
        }
    }
    engine.stop();
    log("播放器退出");
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn env_flag(name: &str) -> bool {
    matches!(env::var(name).ok().as_deref(), Some("1") | Some("true") | Some("yes"))
}

fn main() -> ExitCode {
    init_log();
    let args = match Args::parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("参数错误: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    let headless = env_flag("ZSPACE_PLAYER_HEADLESS");
    let debug = env_flag("ZSPACE_PLAYER_DEBUG");
    let vlc_lib = env::var("ZSPACE_VLC_LIB").ok();
    let vlc_plugins = env::var("ZSPACE_VLC_PLUGINS").ok();

    log(format!(
        "启动: encoded={:?} zoom={} center={:?} app={} lang={} user={:?} headless={}",
        args.encoded, args.zoom, args.center, args.app_name, args.lang, args.username, headless
    ));

    // 与原版一致：优先用启动参数里的 server，其次 userData/zspace.conf 的 playerPort。
    let (ip, port) = if let Some(server) = args.server_from_cmd() {
        server
    } else if let Some(port) = port_from_conf(&args.app_name) {
        ("127.0.0.1".to_string(), port)
    } else {
        log("启动参数与 zspace.conf 都没有 playerPort，无法连接主进程");
        return ExitCode::FAILURE;
    };

    let addr: SocketAddr = match format!("{ip}:{port}").parse() {
        Ok(a) => a,
        Err(e) => {
            log(format!("服务地址无效 {ip}:{port}: {e}"));
            return ExitCode::FAILURE;
        }
    };
    log(format!("连接主进程 {addr}"));

    // Resolve libvlc before touching sockets/windows: the bundled Deepin VLC
    // needs its private FFmpeg libraries on LD_LIBRARY_PATH, which only takes
    // effect after a re-exec.
    let paths = match vlc::resolve_paths(vlc_lib.as_deref(), vlc_plugins.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            log(format!("VLC 运行时不可用: {e}"));
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = maybe_reexec_for_bundled_vlc(&paths) {
        log(format!("重启自身以加载 VLC 依赖失败: {e}"));
        return ExitCode::FAILURE;
    }

    let stream = match connect_with_retry(addr, Duration::from_secs(20)) {
        Ok(s) => s,
        Err(e) => {
            log(e);
            return ExitCode::FAILURE;
        }
    };
    log("已连接主进程");

    let (tx, rx) = mpsc::channel::<Event>();
    let read_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            log(format!("复制 socket 失败: {e}"));
            return ExitCode::FAILURE;
        }
    };
    spawn_reader(read_stream, tx.clone());

    let controller_opts = ControllerOptions {
        window_id: None,
        headless,
        debug,
        vlc_paths: paths.clone(),
    };

    if headless {
        log("headless 模式（自测）：不创建窗口");
        return controller(rx, stream, controller_opts);
    }

    let width = (1280.0 * args.zoom) as u16;
    let height = (720.0 * args.zoom) as u16;
    let (cx, cy) = args.center.unwrap_or((960, 540));
    let ui_opts = UiOptions {
        title: format!("极空间播放器 - {}", args.app_name),
        x: cx - (width as i16) / 2,
        y: cy - (height as i16) / 2,
        width,
        height,
        fullscreen: false,
    };
    let mut window = match AppWindow::new(&ui_opts) {
        Ok(w) => w,
        Err(e) => {
            log(format!("创建窗口失败: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let window_id = window.id();
    log(format!("窗口已创建: 0x{window_id:x} {width}x{height} at {},{}", ui_opts.x, ui_opts.y));

    let controller_opts = ControllerOptions { window_id: Some(window_id), ..controller_opts };
    let handle = std::thread::spawn(move || controller(rx, stream, controller_opts));

    loop {
        if !window.pump(&tx) {
            log("窗口关闭");
            let _ = tx.send(Event::Command(Command::Quit));
            break;
        }
        if handle.is_finished() {
            log("控制线程已结束（主进程关闭或播放完成）");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // 给控制线程一点时间把 1/3 发出去并停止 libvlc。
    std::thread::sleep(Duration::from_millis(400));
    let _ = handle.join();
    ExitCode::SUCCESS
}
