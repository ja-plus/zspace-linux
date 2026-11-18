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

mod bar;
mod overlay;
mod protocol;
mod style;
mod ui;
mod vlc;

use protocol::FrameReader;
use serde_json::{json, Value};
use std::env;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use bar::Snapshot;
use ui::{AppWindow, UiOptions};

/// 控制栏弹层类型（bar.rs 与 main.rs 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Speed,
    Quality,
    Audio,
    Subtitle,
    Episode,
    More,
}

#[derive(Debug, Clone)]
pub enum Command {
    TogglePause,
    SeekBy(f64),
    VolumeBy(i32),
    MuteToggle,
    AudioNext,
    SubtitleNext,
    NextEpisode,
    PrevEpisode,
    LoadLocalSubtitle,
    QualityNext,
    RateBy(f32),
    SeekTo(f64),
    SetRate(f32),
    SetVolume(i32),
    SelectQuality(usize),
    SelectAudio(usize),
    SelectSubtitle(i64),
    PlayEpisode(usize),
    ToggleLoop,
    /// 直接设定循环播放开关（设置面板用）
    SetLoop(bool),
    /// 睡眠定时循环：关闭 → 30 分钟 → 60 分钟 → 关闭。
    CycleSleepTimer,
    /// 直接设定睡眠定时（分钟），0 = 关闭
    SetSleepTimer(u32),
    /// 结束页「重播」：回到 0 秒重新播。
    Replay,
    ToggleFullscreen,
    Quit,
}

pub enum Event {
    Command(Command),
    Net(Value),
    NetClosed(String),
}

// ---------------------------------------------------------------------------
// logging（只写 stderr；自测用重定向收集）
// ---------------------------------------------------------------------------

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

pub fn log(msg: impl AsRef<str>) {
    let (_, clock) = local_now();
    let line = format!("[{clock}] [player] {}", msg.as_ref());
    eprintln!("{line}");
    // Electron pipes the player's stderr away, so also append to the log the
    // launcher documents (~/.config/zspace/logs/player.log).
    if let Some(home) = std::env::var_os("HOME") {
        let dir = std::path::PathBuf::from(home).join(".config/zspace/logs");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("player.log"))
        {
            use std::io::Write as _;
            let _ = writeln!(f, "{line}");
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
    fn parse() -> Args {
        let mut args = Args::default();
        let mut positional: Vec<String> = Vec::new();
        for arg in env::args().skip(1) {
            match arg.as_str() {
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
        args
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
    /// 等主进程用 `7/3` 回新码流地址（`7/1`）。
    Quality { index: usize },
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
    /// NAS 字幕索引（`subtitle_list[].index`），用于 4/3 上报和外部字幕选择。
    nas_sub_index: Option<i64>,
    subtitle_list: Vec<Value>,
    episode_list: Vec<Value>,
    episode_index: Option<usize>,
    collection_info: Option<Value>,
    /// 5/5 / 5/3 的 playList（版本/码流来源）。
    play_list: Vec<Value>,
    /// 7/5 或 playList item 里的码流列表。
    format_list: Vec<Value>,
    format_index: Option<usize>,
    /// 9/x 返回的在线字幕搜索结果（仅保存，无 UI 展示）。
    online_subs: Vec<Value>,
    tracks_applied: bool,
    tracks_deadline: Instant,
    started_ack: bool,
    subtitle_hash: String,
    subtitle_setting: Value,
    chapter: String,
    last_progress: Instant,
    last_heartbeat: Instant,
    finished: bool,
    looping: bool,
    /// 睡眠定时：剩余分钟数（None = 关闭）与到点时刻。
    sleep_min: Option<u32>,
    sleep_deadline: Option<Instant>,
    /// 首帧未出/等主进程重新出地址时为真（覆盖层「正在加载」）。
    loading: bool,
    /// 播放错误原因（8/1 或 libvlc 报错），覆盖层整块显示错误页。
    error: Option<String>,
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
            nas_sub_index: None,
            subtitle_list: Vec::new(),
            episode_list: Vec::new(),
            episode_index: None,
            collection_info: None,
            play_list: Vec::new(),
            format_list: Vec::new(),
            format_index: None,
            online_subs: Vec::new(),
            tracks_applied: false,
            tracks_deadline: now,
            started_ack: false,
            subtitle_hash: String::new(),
            subtitle_setting: json!({"offset": "0.0", "position": "", "colour": "", "size": ""}),
            chapter: String::new(),
            last_progress: now,
            last_heartbeat: now,
            finished: false,
            looping: false,
            sleep_min: None,
            sleep_deadline: None,
            loading: true,
            error: None,
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

    st.loading = true;
    st.error = None;
    if let Err(e) = engine.open(uri, &options) {
        log(format!("打开媒体失败: {e}"));
        st.loading = false;
        st.error = Some(e);
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
    st.nas_sub_index = field(item, "subtitle_index").and_then(|v| if v >= 0 || v == -1 { Some(v) } else { None });
    st.subtitle_list = item
        .get("subtitle_list")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let formats = item
        .get("format_list")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if !formats.is_empty() {
        st.format_list = formats;
    }
    st.format_index = field(item, "format_index")
        .map(|v| v.max(0) as usize)
        .or_else(|| {
            let current = item.get("current_fmt").and_then(|v| v.as_str())?;
            st.format_list
                .iter()
                .position(|fmt| fmt.get("mod").and_then(|v| v.as_str()) == Some(current))
        });
    if !st.episode_list.is_empty() {
        let file_path = item.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        st.episode_index = st.episode_list.iter().position(|ep| {
            ep.get("file_path").and_then(|v| v.as_str()) == Some(file_path)
        });
    }
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
    st.finished = false;
    st.stop_sent = false;
    st.tracks_deadline = Instant::now() + Duration::from_secs(20);
    if new_session {
        st.started_ack = false;
    }
    st.last_progress = Instant::now();
    st.last_heartbeat = Instant::now();
}

/// 片头片尾提示：`12/3` collectionInfo.skip 的 `start`/`end`（秒）。
/// 官方语义：`start` = 片头结束点，`end` = 片尾开始点。
/// 位置落在片头区间就给「跳过片头」，落在片尾区间给「跳过片尾」，
/// 文案里带剩余秒数，和官方浮层的倒计时一致。
fn skip_hint(st: &PlayState, pos_ms: i64, len_ms: i64) -> Option<(String, f64)> {
    let skip = st.collection_info.as_ref()?.get("skip")?;
    let start = skip.get("start").and_then(as_f64).unwrap_or(0.0);
    let end = skip.get("end").and_then(as_f64).unwrap_or(0.0);
    let pos = (pos_ms as f64 / 1000.0).max(0.0);
    let len = len_ms as f64 / 1000.0;
    if start > 1.0 && pos < start - 0.5 {
        let left = (start - pos).ceil().max(0.0) as i64;
        return Some((format!("跳过片头 {left}s"), start));
    }
    if end > 1.0 && len > end + 1.0 && pos >= end && pos < len - 1.0 {
        let left = (len - pos).ceil().max(0.0) as i64;
        return Some((format!("跳过片尾 {left}s"), (len - 0.5).max(0.0)));
    }
    None
}

/// 控制栏绘制所需的快照（每秒刷新，跨线程共享）。
fn snapshot_of(engine: &vlc::Engine, st: &PlayState) -> Snapshot {
    let title = st
        .current
        .as_ref()
        .and_then(|item| {
            ["title", "name", "file_name", "label"]
                .iter()
                .find_map(|k| item.get(*k).and_then(|v| v.as_str()))
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let quality = st
        .format_list
        .get(st.format_index.unwrap_or(0))
        .and_then(|f| {
            ["label", "name", "quality", "resolution"]
                .iter()
                .find_map(|k| f.get(*k).and_then(|v| v.as_str()))
                .map(|s| s.to_string())
        })
        .or_else(|| {
            st.current.as_ref().and_then(|i| {
                i.get("quality").and_then(|v| v.as_str()).map(|s| s.to_string())
            })
        })
        .unwrap_or_default();
    let audio = match st.audio_ordinal {
        Some(i) => format!("{}", i + 1),
        None => "1".to_string(),
    };
    let subtitle = if st.sub_ordinal.is_some() || st.nas_sub_index.is_some() {
        "开".to_string()
    } else {
        "关".to_string()
    };
    let (has_prev, has_next) = match st.episode_index {
        Some(i) => (i > 0, i + 1 < st.episode_list.len()),
        None => (false, false),
    };
    let qualities: Vec<(String, bool)> = st
        .format_list
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let label = ["label", "name", "quality", "resolution"]
                .iter()
                .find_map(|k| f.get(*k).and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("码流 {}", i + 1));
            (label, st.format_index == Some(i))
        })
        .collect();
    // 音轨列表取 libvlc 的真实轨道（id>=0，语言名可能为空），不再写死条数。
    let audios: Vec<(String, bool)> = engine
        .audio_tracks()
        .into_iter()
        .filter(|t| t.id >= 0)
        .enumerate()
        .map(|(i, t)| {
            let name = t.name.trim();
            let label = if name.is_empty() {
                format!("音轨 {}", i + 1)
            } else {
                name.to_string()
            };
            (label, st.audio_ordinal == Some(i))
        })
        .collect();
    let mut subtitles: Vec<(String, bool)> = vec![("不显示字幕".to_string(), st.sub_ordinal.is_none() && st.nas_sub_index.is_none())];
    for (i, sub) in st.subtitle_list.iter().enumerate() {
        let name = ["label", "name", "title", "language"]
            .iter()
            .find_map(|k| sub.get(*k).and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("字幕 {}", i + 1));
        subtitles.push((name, st.nas_sub_index == Some(i as i64)));
    }
    let episodes: Vec<(String, bool)> = st
        .episode_list
        .iter()
        .enumerate()
        .map(|(i, ep)| {
            let name = ["label", "name", "title", "file_name"]
                .iter()
                .find_map(|k| ep.get(*k).and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("第 {} 集", i + 1));
            (name, st.episode_index == Some(i))
        })
        .collect();

    let (pos_ms, len_ms) = (engine.time_ms(), engine.length_ms());
    Snapshot {
        title,
        looping: st.looping,
        sleep_min: st.sleep_min,
        loading: st.loading && !st.finished,
        error: st.error.clone(),
        ended: st.finished && st.error.is_none(),
        // 由 UI 层在 pump 时按真实全屏状态覆盖
        fullscreen: false,
        skip: skip_hint(st, pos_ms, len_ms),
        qualities,
        audios,
        subtitles,
        episodes,
        playing: engine.is_playing(),
        pos_ms,
        len_ms,
        rate: engine.rate(),
        volume: engine.volume(),
        muted: engine.volume() <= 0,
        quality,
        audio,
        subtitle,
        has_prev,
        has_next,
    }
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
        "subtitle_index": st
            .nas_sub_index
            .unwrap_or_else(|| st.sub_ordinal.map(|v| v as i64).unwrap_or(-1)),
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
        PendingKind::Quality { index } => {
            log(format!("等待 7/3 超时，码流 {index} 未切换（本地无法回退）"));
        }
    }
}

/// `subtitle_list[].url` 是相对路径（实测形如 `/splay/<token>/2.srt`），
/// 需要拼主进程下发的 `apiHost`；绝对 http(s) 地址原样使用。
fn absolute_media_url(st: &PlayState, url: &str) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(url.to_string());
    }
    if st.api_host.is_empty() || url.is_empty() {
        return None;
    }
    Some(format!("{}{}", st.api_host.trim_end_matches('/'), url))
}

/// 按 NAS 字幕索引（`subtitle_list[].index`）加载外挂字幕。
///
/// 证据：官方真实 playList 的 `subtitle_list[]` 带 `index/url/support_switch/
/// external/data_source`，url 是 `/splay/<token>/<n>.srt`。索引找不到、或没有
/// url 时明确记为“待取证”，不猜测切换。
fn load_nas_subtitle(engine: &vlc::Engine, st: &mut PlayState, index: i64) {
    if index < 0 {
        st.nas_sub_index = Some(index);
        if let Err(e) = engine.set_subtitle_ordinal(None) {
            log(format!("关闭字幕失败: {e}"));
        } else {
            st.sub_ordinal = None;
            log("字幕 → 关闭");
        }
        return;
    }
    let entry = st
        .subtitle_list
        .iter()
        .find(|item| field(item, "index") == Some(index))
        .cloned();
    let entry = match entry {
        Some(e) => e,
        None => {
            st.nas_sub_index = Some(index);
            log(format!("字幕索引 {index} 不在 subtitle_list 中（待取证），暂不切换"));
            return;
        }
    };
    let url = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let full = match absolute_media_url(st, url) {
        Some(u) => u,
        None => {
            st.nas_sub_index = Some(index);
            log(format!("字幕 {index} 没有可用 url（apiHost 为空或 url 缺失），待取证"));
            return;
        }
    };
    match engine.add_subtitle(&full) {
        Ok(()) => {
            st.nas_sub_index = Some(index);
            st.sub_ordinal = None;
            log(format!(
                "已加载外挂字幕 index={index} url={full} (external={} support_switch={} data_source={})",
                entry.get("external").map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
                entry.get("support_switch").map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
                field(&entry, "data_source").map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
            ));
        }
        Err(e) => {
            st.nas_sub_index = Some(index);
            log(format!("加载外挂字幕 index={index} 失败: {e}"));
        }
    }
}

fn episode_file_path(item: &Value) -> &str {
    item.get("file_path")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("path").and_then(|v| v.as_str()))
        .unwrap_or("")
}

/// 剧集列表（12/1）里的上一集/下一集：按官方 `3/1` 语义请主进程重新出地址。
fn request_episode_switch(st: &mut PlayState, sock: &mut Socket, delta: i32) {
    if st.episode_list.is_empty() {
        log("没有剧集列表（12/1 尚未收到），无法切集");
        return;
    }
    let current_path = st
        .current
        .as_ref()
        .map(episode_file_path)
        .unwrap_or("")
        .to_string();
    let current = st
        .episode_index
        .or_else(|| st.episode_list.iter().position(|item| episode_file_path(item) == current_path));
    let len = st.episode_list.len();
    let target = match current {
        Some(i) => {
            let next = i as i32 + delta;
            if next < 0 || next >= len as i32 {
                log(if delta > 0 { "已经是最后一集" } else { "已经是第一集" });
                return;
            }
            next as usize
        }
        None => {
            if delta > 0 { 0 } else { len - 1 }
        }
    };
    let item = st.episode_list[target].clone();
    let file_path = episode_file_path(&item).to_string();
    let encrypted = item.get("encrypted_path").and_then(|v| v.as_str()).unwrap_or("");
    let info = if file_path.is_empty() { encrypted } else { &file_path };
    if info.is_empty() {
        log(format!("剧集 {target} 没有 file_path/encrypted_path，无法切换"));
        return;
    }
    let mut msg = json!({"f_type": 3, "s_type": 1, "info": info});
    if !encrypted.is_empty() {
        msg["encrypted_path"] = json!(encrypted);
    }
    st.episode_index = Some(target);
    log(format!(
        "发送 3/1 切换剧集: index={target}/{len} info={info} label={}",
        item.get("label").and_then(|v| v.as_str()).unwrap_or("")
    ));
    sock.send(msg);
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
            if let Some(items) = list {
                st.play_list = items.clone();
            }
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
        (Some(5), Some(3)) => {
            let list = v
                .get("playList")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            log(format!("收到刷新后的播放列表 5/3: {} 项", list.len()));
            if !list.is_empty() {
                st.play_list = list;
            }
            false
        }
        (Some(7), Some(5)) => {
            let list = v
                .get("format_list")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            log(format!("收到码流列表 7/5: {} 项", list.len()));
            if !list.is_empty() {
                st.format_list = list;
            }
            false
        }
        (Some(9), Some(kind)) => {
            let code = field(v, "code").unwrap_or(200);
            if code != 200 {
                log(format!(
                    "9/{kind} 失败: code={code} message={}",
                    v.get("message").and_then(|x| x.as_str()).unwrap_or("")
                ));
                return false;
            }
            let info = v
                .get("info_list")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let has_info = !info.is_empty();
            if has_info {
                log(format!("收到在线字幕结果 9/{kind}: {} 项（无搜索 UI，仅保存）", info.len()));
                st.online_subs = info;
            }
            let subtitles = v
                .get("subtitle_list")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let has_subtitles = !subtitles.is_empty();
            if has_subtitles {
                log(format!("9/{kind} 返回字幕列表: {} 项", subtitles.len()));
                st.subtitle_list = subtitles;
            }
            if let Some(index) = field(v, "subtitle_index") {
                load_nas_subtitle(engine, st, index);
            } else if kind == 4 {
                log(format!(
                    "9/4 删除字幕结果: code={code} message={}",
                    v.get("message").and_then(|x| x.as_str()).unwrap_or("")
                ));
            } else if !has_info && !has_subtitles {
                log(format!("9/{kind} 回包没有 info_list/subtitle_list"));
            }
            false
        }
        (Some(12), Some(0)) => {
            log(format!(
                "收到 12/0 片头片尾设置回包: code={:?} message={}（发送入口需要设置 UI）",
                field(v, "code"),
                v.get("message").and_then(|x| x.as_str()).unwrap_or("")
            ));
            false
        }
        (Some(12), Some(1)) => {
            let list = v
                .get("videoList")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            log(format!("收到剧集列表 12/1: {} 项", list.len()));
            st.episode_list = list;
            let current_path = st.current.as_ref().map(episode_file_path).unwrap_or("");
            st.episode_index = st
                .episode_list
                .iter()
                .position(|item| episode_file_path(item) == current_path);
            false
        }
        (Some(12), Some(2)) => {
            let list = v
                .get("subtitleList")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            log(format!("收到字幕列表 12/2: {} 项", list.len()));
            if !list.is_empty() {
                st.subtitle_list = list;
            }
            match field(v, "subtitle_index") {
                Some(index) => load_nas_subtitle(engine, st, index),
                None => log("12/2 没有 subtitle_index，仅更新字幕列表"),
            }
            false
        }
        (Some(12), Some(3)) => {
            let info = v.get("collectionInfo").cloned().unwrap_or(Value::Null);
            log(format!(
                "收到合集信息 12/3: collection_id={} title={} skip={} history={}",
                info.get("collection_id").and_then(|x| x.as_str()).unwrap_or(""),
                info.get("title").and_then(|x| x.as_str()).unwrap_or(""),
                info.get("skip").map(|x| x.to_string()).unwrap_or_else(|| "-".into()),
                info.get("history").map(|x| x.to_string()).unwrap_or_else(|| "-".into()),
            ));
            st.collection_info = Some(info);
            false
        }
        (Some(10), Some(kind)) => {
            let code = field(v, "code").unwrap_or(200);
            let list = v
                .get("subtitleList")
                .or_else(|| v.get("subtitle_list"))
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            if !list.is_empty() {
                log(format!("10/{kind} 返回字幕列表: {} 项", list.len()));
                st.subtitle_list = list;
            }
            if code != 200 {
                log(format!(
                    "10/{kind} 失败: code={code} message={}",
                    v.get("message").and_then(|x| x.as_str()).unwrap_or("")
                ));
                return false;
            }
            if let Some(index) = field(v, "subtitle_index") {
                load_nas_subtitle(engine, st, index);
            } else if kind == 3 {
                log("10/3 已下载字幕到主进程 userData/Subtitles；本地文件加载入口待取证，未接");
            } else {
                log(format!("10/{kind} 回复没有 subtitle_index"));
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
                    st.nas_sub_index = Some(idx);
                    log(format!("收到 11/3 字幕结果: subtitle_index={idx}"));
                } else if matches!((f, s), (Some(6), Some(3))) {
                    log("收到 6/3（无新地址；对应 6/2 列表请求，官方回包本身不带列表）");
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
                    PendingKind::Quality { index } => {
                        if item.get("format_index").is_none() {
                            st.format_index = Some(index);
                        }
                        log(format!("收到 7/3 码流切换结果: index={index}"));
                    }
                }
            } else {
                log(format!("收到新播放地址 (f_type={} / s_type={})", f.unwrap_or(-1), s.unwrap_or(-1)));
            }
            false
        }
        (Some(8), _) => {
            let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or("").to_string();
            log(format!(
                "播放错误消息: info_type={:?} show_info={} reason={}",
                field(v, "info_type"),
                v.get("show_info").and_then(|x| x.as_str()).unwrap_or(""),
                reason
            ));
            st.loading = false;
            st.error = Some(if reason.is_empty() { "播放出错".to_string() } else { reason });
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
        Command::NextEpisode => request_episode_switch(st, sock, 1),
        Command::PrevEpisode => request_episode_switch(st, sock, -1),
        Command::LoadLocalSubtitle => {
            if st.current.is_none() {
                log("还没有开始播放，忽略本地字幕请求");
                return;
            }
            log("发送 10/1 请求主进程打开本地字幕选择");
            sock.send(json!({"f_type": 10, "s_type": 1}));
        }
        Command::QualityNext => {
            if st.current.is_none() {
                log("还没有开始播放，忽略码流切换");
                return;
            }
            let len = st.format_list.len();
            if len <= 1 {
                log("没有可切换的码流（7/4 发送入口缺失，需要清晰度菜单）");
                return;
            }
            let current = st.format_index.unwrap_or(0).min(len - 1);
            let next = (current + 1) % len;
            let seek = engine.time_ms() as f64 / 1000.0;
            let audio = st.audio_ordinal.map(|v| v as i64).unwrap_or(-1);
            let subtitle = st
                .nas_sub_index
                .unwrap_or_else(|| st.sub_ordinal.map(|v| v as i64).unwrap_or(-1));
            st.pending = Some(Pending { kind: PendingKind::Quality { index: next }, sent_at: Instant::now() });
            log(format!(
                "发送 7/1 请求切换码流: result={next}/{len} seek={seek:.1}s audiotrack={audio} subtitle={subtitle}"
            ));
            sock.send(json!({
                "f_type": 7,
                "s_type": 1,
                "result": next,
                "seek": seek,
                "audiotrack": audio,
                "subtitle": subtitle,
            }));
        }
        Command::RateBy(delta) => {
            let rate = engine.set_rate(engine.rate() + delta);
            log(format!("倍速 → {rate:.2}x"));
        }
        Command::SeekTo(sec) => {
            if st.current.is_some() {
                engine.seek_ms((sec.max(0.0) * 1000.0) as i64);
                log(format!("seek → {sec:.1}s"));
            }
        }
        Command::SetRate(r) => {
            engine.set_rate(r.clamp(0.25, 4.0));
            log(format!("倍速 → {r:.2}x"));
        }
        Command::SetVolume(v) => {
            let nv = engine.set_volume(v.clamp(0, 100));
            log(format!("音量 → {nv}"));
        }
        Command::SelectAudio(i) => {
            let seek = engine.time_ms() as f64 / 1000.0;
            request_server_audio(st, sock, i, seek);
        }
        Command::SelectSubtitle(i) => {
            let idx = if i < 0 { None } else { Some(i as usize) };
            request_subtitle_only(st, sock, idx);
        }
        Command::SelectQuality(i) => {
            let seek = engine.time_ms() as f64 / 1000.0;
            let audio = st.audio_ordinal.map(|v| v as i64).unwrap_or(-1);
            let subtitle = st
                .nas_sub_index
                .unwrap_or_else(|| st.sub_ordinal.map(|v| v as i64).unwrap_or(-1));
            st.pending = Some(Pending { kind: PendingKind::Quality { index: i }, sent_at: Instant::now() });
            log(format!("发送 7/1 选择码流: result={i} seek={seek:.1}s"));
            sock.send(json!({
                "f_type": 7,
                "s_type": 1,
                "result": i,
                "seek": seek,
                "audiotrack": audio,
                "subtitle": subtitle,
            }));
        }
        Command::PlayEpisode(i) => {
            let cur = st.episode_index.unwrap_or(0) as i32;
            request_episode_switch(st, sock, i as i32 - cur);
        }
        Command::ToggleLoop => {
            st.looping = !st.looping;
            log(format!("循环播放：{}", if st.looping { "开" } else { "关" }));
        }
        Command::SetLoop(on) => {
            st.looping = on;
            log(format!("循环播放：{}", if on { "开" } else { "关" }));
        }
        Command::Replay => {
            st.finished = false;
            st.error = None;
            st.loading = true;
            st.stop_sent = false;
            engine.seek_ms(0);
            engine.play();
            log("结束页「重播」：回到开头");
        }
        Command::CycleSleepTimer => {
            st.sleep_min = match st.sleep_min {
                None => Some(30),
                Some(30) => Some(60),
                _ => None,
            };
            st.sleep_deadline = st
                .sleep_min
                .map(|m| Instant::now() + Duration::from_secs(m as u64 * 60));
            log(format!(
                "睡眠定时：{}",
                match st.sleep_min {
                    Some(m) => format!("{m} 分钟后暂停"),
                    None => "关闭".to_string(),
                }
            ));
        }
        Command::SetSleepTimer(min) => {
            st.sleep_min = if min == 0 { None } else { Some(min) };
            st.sleep_deadline = st
                .sleep_min
                .map(|m| Instant::now() + Duration::from_secs(m as u64 * 60));
            log(format!(
                "睡眠定时：{}",
                match st.sleep_min {
                    Some(m) => format!("{m} 分钟后暂停"),
                    None => "关闭".to_string(),
                }
            ));
        }
        // handled by the UI thread (needs the window), see AppWindow::pump
        Command::ToggleFullscreen => {}
        Command::Quit => {}
    }
}

fn tick(engine: &mut vlc::Engine, st: &mut PlayState, sock: &mut Socket) {
    if st.current.is_none() {
        return;
    }
    let state = engine.state();
    let time_ms = engine.time_ms();

    if let Some(at) = st.sleep_deadline {
        if Instant::now() >= at {
            log("睡眠定时到点，暂停播放");
            engine.set_pause(true);
            st.sleep_min = None;
            st.sleep_deadline = None;
        }
    }

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

    if state == vlc::STATE_PLAYING && time_ms > 0 {
        st.loading = false;
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
        if st.looping {
            log("循环播放：回到开头");
            engine.seek_ms(0);
            engine.play();
            st.finished = false;
            st.stop_sent = false;
            return;
        }
        st.stop_sent = true;
        st.finished = true;
        return;
    }
    if state == vlc::STATE_ERROR {
        if !st.finished {
            log("libvlc 报告播放错误");
            st.finished = true;
            st.loading = false;
            if st.error.is_none() {
                st.error = Some("libvlc 报告播放错误".to_string());
            }
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
    vlc_paths: vlc::VlcPaths,
}

fn controller(
    rx: Receiver<Event>,
    stream: TcpStream,
    opts: ControllerOptions,
    ui: Arc<Mutex<Snapshot>>,
) -> ExitCode {
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

    let mut engine = match vlc::Engine::new(&paths) {
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
                // The app sends 100/0 (`send cookie params`) right after the
                // socket is accepted and then waits for the player to speak:
                // its heartbeat timer (`Ka`) only advances on incoming data,
                // and the playList is pushed only after that. The official
                // player answers with 5/2 ("give me the playList"). Without
                // this the two sides deadlock and the app shows
                // 「打开视频时发生错误」 after 40 s.
                if field(&v, "f_type") == Some(100) {
                    log("收到 100/0（cookie/apiHost），回 5/2 请求播放列表");
                    sock.send(json!({"f_type": 5, "s_type": 2}));
                }
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
        if let Ok(mut slot) = ui.lock() {
            *slot = snapshot_of(&engine, &st);
        }
        // 播完不退出：留在结束页等用户点「重播 / 下一集」或关窗。
        // 退出仍由三条路径触发：f_type=0、socket 断开、窗口关闭（UI 线程发 Quit）。
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

fn main() -> ExitCode {
    let args = Args::parse();

    log(format!(
        "启动: encoded={:?} zoom={} center={:?} app={} lang={} user={:?}",
        args.encoded, args.zoom, args.center, args.app_name, args.lang, args.username
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
    let paths = match vlc::resolve_paths() {
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
        vlc_paths: paths.clone(),
    };

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

    let ui_state = Arc::new(Mutex::new(Snapshot::default()));
    let ui_for_ctl = Arc::clone(&ui_state);
    let controller_opts = ControllerOptions { window_id: Some(window_id), ..controller_opts };
    let handle = std::thread::spawn(move || controller(rx, stream, controller_opts, ui_for_ctl));

    loop {
        let snap = ui_state.lock().map(|s| s.clone()).unwrap_or_default();
        if !window.pump(&tx, &snap) {
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
