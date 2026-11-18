//! Minimal X11 top-level window: renders libvlc output and turns keyboard
//! shortcuts into player commands. Deliberately dependency-free apart from
//! x11rb (pure Rust X11 protocol, no libX11 headers needed).

use crate::bar::{ControlBar, Snapshot, BAR_H};
use crate::overlay::Overlay;
use crate::{Command, Event};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageData, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _,
    CreateWindowAux, EventMask, PropMode, Window, WindowClass,
};
use x11rb::protocol::Event as XEvent;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

pub struct UiOptions {
    pub title: String,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub fullscreen: bool,
}

struct Atoms {
    wm_protocols: Atom,
    wm_delete: Atom,
    net_wm_name: Atom,
    net_wm_state: Atom,
    net_wm_state_fullscreen: Atom,
    utf8_string: Atom,
    net_wm_pid: Atom,
}

pub struct AppWindow {
    conn: RustConnection,
    win: Window,
    bar: Option<ControlBar>,
    overlay: Option<Overlay>,
    geom: (i16, i16, u16, u16),
    /// 进入全屏前的窗口几何，退出全屏时用来还原（有的 WM 不会自己还原）。
    restore_geom: Option<(i16, i16, u16, u16)>,
    /// 退出全屏后稍等一下再强制还原几何，避免和 WM 的异步还原打架。
    restore_at: Option<Instant>,
    last_activity: Instant,
    atoms: Atoms,
    min_key: u8,
    keysyms_per_key: usize,
    keysyms: Vec<u32>,
    pressed: HashSet<u8>,
    fullscreen: bool,
    screen_w: u16,
    screen_h: u16,
    last_click: Option<Instant>,
}

// X11 keysyms used below.
const XK_SPACE: u32 = 0x0020;
const XK_ESCAPE: u32 = 0xff1b;
const XK_LEFT: u32 = 0xff51;
const XK_UP: u32 = 0xff52;
const XK_RIGHT: u32 = 0xff53;
const XK_DOWN: u32 = 0xff54;

impl AppWindow {
    pub fn new(opts: &UiOptions) -> Result<Self, String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|e| format!("连接 X11 失败: {e}"))?;
        let screen = &conn.setup().roots[screen_num];
        let screen_w = screen.width_in_pixels;
        let screen_h = screen.height_in_pixels;

        let width = opts.width.min(screen_w).max(320);
        let height = opts.height.min(screen_h).max(180);
        let x = opts.x.clamp(0, (screen_w - width) as i16);
        let y = opts.y.clamp(0, (screen_h - height) as i16);

        let win = conn.generate_id().map_err(|e| e.to_string())?;
        let event_mask = EventMask::KEY_PRESS
            | EventMask::KEY_RELEASE
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::EXPOSURE
            | EventMask::STRUCTURE_NOTIFY
            | EventMask::POINTER_MOTION;
        let aux = CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .event_mask(event_mask);
        conn.create_window(
            screen.root_depth,
            win,
            screen.root,
            x,
            y,
            width,
            height,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &aux,
        )
        .map_err(|e| format!("创建窗口失败: {e}"))?;

        let atoms = Atoms {
            wm_protocols: intern(&conn, b"WM_PROTOCOLS")?,
            wm_delete: intern(&conn, b"WM_DELETE_WINDOW")?,
            net_wm_name: intern(&conn, b"_NET_WM_NAME")?,
            net_wm_state: intern(&conn, b"_NET_WM_STATE")?,
            net_wm_state_fullscreen: intern(&conn, b"_NET_WM_STATE_FULLSCREEN")?,
            utf8_string: intern(&conn, b"UTF8_STRING")?,
            net_wm_pid: intern(&conn, b"_NET_WM_PID")?,
        };

        conn.change_property32(
            PropMode::REPLACE,
            win,
            atoms.wm_protocols,
            AtomEnum::ATOM,
            &[atoms.wm_delete],
        )
        .map_err(|e| e.to_string())?;
        let _ = conn.change_property32(
            PropMode::REPLACE,
            win,
            atoms.net_wm_pid,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        );
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"zspace-player\0ZspacePlayer\0",
        )
        .map_err(|e| e.to_string())?;

        let setup = conn.setup();
        let min_key = setup.min_keycode;
        let max_key = setup.max_keycode;
        let count = max_key - min_key + 1;
        let mapping = conn
            .get_keyboard_mapping(min_key, count)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        let keysyms_per_key = mapping.keysyms_per_keycode as usize;
        let keysyms = mapping.keysyms.iter().map(|k| *k as u32).collect();

        let mut window = AppWindow {
            conn,
            win,
            atoms,
            min_key,
            keysyms_per_key,
            keysyms,
            pressed: HashSet::new(),
            fullscreen: false,
            screen_w,
            screen_h,
            last_click: None,
            bar: None,
            overlay: None,
            geom: (x, y, width, height),
            restore_geom: None,
            restore_at: None,
            last_activity: Instant::now(),
        };
        window.set_title(&opts.title);
        window.conn.map_window(win).map_err(|e| e.to_string())?;
        let depth = window.conn.setup().roots[screen_num].root_depth;
        match ControlBar::new(&window.conn, depth, x, y + height as i16 - BAR_H as i16, width) {
            Ok(bar) => window.bar = Some(bar),
            Err(e) => eprintln!("[ui] 控制栏创建失败: {e}"),
        }
        // 覆盖层只盖视频区域，底部 76 px 留给控制栏：两个窗口不重叠，
        // 就不用跟 WM 抢层级（之前盖满整窗时控制栏会被压住）。
        match Overlay::new(&window.conn, depth, x, y, width, height.saturating_sub(BAR_H)) {
            Ok(ov) => window.overlay = Some(ov),
            Err(e) => eprintln!("[ui] 覆盖层创建失败: {e}"),
        }
        window.conn.flush().map_err(|e| e.to_string())?;
        if opts.fullscreen {
            window.set_fullscreen(true);
        }
        Ok(window)
    }

    pub fn id(&self) -> u32 {
        self.win
    }

    pub fn set_title(&mut self, title: &str) {
        // WM_NAME is legacy Latin-1; modern WMs use _NET_WM_NAME (UTF-8).
        let _ = self.conn.change_property8(
            PropMode::REPLACE,
            self.win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"ZSpace Player",
        );
        let _ = self.conn.change_property8(
            PropMode::REPLACE,
            self.win,
            self.atoms.net_wm_name,
            self.atoms.utf8_string,
            title.as_bytes(),
        );
        let _ = self.conn.flush();
    }

    fn set_fullscreen(&mut self, on: bool) {
        if on && !self.fullscreen {
            // 记住当前几何（含用户已经手动放大的情况），退出全屏要回到这里。
            self.restore_geom = Some(self.geom);
        }
        let action = if on { 1u32 } else { 0u32 };
        let data = ClientMessageData::from([
            action,
            self.atoms.net_wm_state_fullscreen,
            0,
            1, // source indication: normal application
            0,
        ]);
        let event = ClientMessageEvent::new(32, self.win, self.atoms.net_wm_state, data);
        let _ = self.conn.send_event(false, self.win, EventMask::NO_EVENT, event);
        if on {
            // Fallback for WMs without EWMH fullscreen support.
            let _ = self.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(self.screen_w as u32)
                    .height(self.screen_h as u32),
            );
        } else if self.restore_geom.is_some() {
            // 先只发 EWMH 取消全屏，给 WM 300 ms 自己还原几何；
            // 到点还在屏幕尺寸才强制压回去，避免和 WM 抢位置（会把窗口越推越偏）。
            self.restore_at = Some(Instant::now() + Duration::from_millis(1200));
        }
        let _ = self.conn.flush();
        self.fullscreen = on;
    }

    fn keysym(&self, keycode: u8) -> Option<u32> {
        if keycode < self.min_key {
            return None;
        }
        let idx = (keycode - self.min_key) as usize * self.keysyms_per_key;
        self.keysyms.get(idx).copied()
    }

    fn handle_key(&mut self, event: &x11rb::protocol::xproto::KeyPressEvent, tx: &Sender<Event>) {
        let keycode = event.detail;
        let ks = match self.keysym(keycode) {
            Some(k) => k,
            None => return,
        };
        let first_press = self.pressed.insert(keycode);

        let cmd = match ks {
            XK_SPACE => {
                if first_press { Some(Command::TogglePause) } else { None }
            }
            XK_LEFT => Some(Command::SeekBy(-5.0)),
            XK_RIGHT => Some(Command::SeekBy(5.0)),
            XK_UP => Some(Command::VolumeBy(5)),
            XK_DOWN => Some(Command::VolumeBy(-5)),
            XK_ESCAPE => {
                if self.fullscreen {
                    self.set_fullscreen(false);
                    None
                } else {
                    Some(Command::Quit)
                }
            }
            // f / F
            0x0066 | 0x0046 => {
                if first_press {
                    let on = !self.fullscreen;
                    self.set_fullscreen(on);
                }
                None
            }
            // q
            0x0071 => Some(Command::Quit),
            // a: next audio track
            0x0061 => {
                if first_press { Some(Command::AudioNext) } else { None }
            }
            // s: next subtitle track (incl. off)
            0x0073 => {
                if first_press { Some(Command::SubtitleNext) } else { None }
            }
            // m: mute toggle
            0x006d => {
                if first_press { Some(Command::MuteToggle) } else { None }
            }
            // n / p: next / previous episode (12/1 list)
            0x006e => {
                if first_press { Some(Command::NextEpisode) } else { None }
            }
            0x0070 => {
                if first_press { Some(Command::PrevEpisode) } else { None }
            }
            // l: 选择本地字幕（走 10/1，由主窗口弹选择框）
            0x006c => {
                if first_press { Some(Command::LoadLocalSubtitle) } else { None }
            }
            // y: 下一个码流（7/1；官方为清晰度菜单）
            0x0079 => {
                if first_press { Some(Command::QualityNext) } else { None }
            }
            // ] / = : faster
            0x005d | 0x003d => {
                if first_press { Some(Command::RateBy(0.25)) } else { None }
            }
            // [ / - : slower
            0x005b | 0x002d => {
                if first_press { Some(Command::RateBy(-0.25)) } else { None }
            }
            _ => None,
        };
        if let Some(cmd) = cmd {
            let _ = tx.send(Event::Command(cmd));
        }
    }

    /// Drains pending X events, keeps the control bar in sync and returns
    /// `false` when the window is gone.
    pub fn pump(&mut self, tx: &Sender<Event>, snap_in: &Snapshot) -> bool {
        // 全屏状态在 UI 层，播放状态在 main 里，合并成一份给控制栏用
        let mut snapshot = snap_in.clone();
        snapshot.fullscreen = self.fullscreen;
        let snap = &snapshot;
        let bar_id = self.bar.as_ref().map(|b| b.id());
        let menu_id = self.bar.as_ref().map(|b| b.menu_id());
        let ov_big = self.overlay.as_ref().map(|o| o.big_id());
        let ov_small = self.overlay.as_ref().map(|o| o.small_id());
        loop {
            let event = match self.conn.poll_for_event() {
                Ok(Some(e)) => e,
                Ok(None) => break,
                Err(_) => return false,
            };
            match event {
                XEvent::KeyPress(e) => self.handle_key(&e, tx),
                XEvent::KeyRelease(e) => {
                    self.pressed.remove(&e.detail);
                }
                XEvent::MotionNotify(e) => {
                    self.last_activity = Instant::now();
                    if Some(e.event) == bar_id {
                        if let Some(bar) = self.bar.as_mut() {
                            if let Some(cmd) = bar.on_motion(&self.conn, e.event_x, e.event_y, snap) {
                                let _ = tx.send(Event::Command(cmd));
                            }
                        }
                    }
                }
                XEvent::EnterNotify(_) => self.last_activity = Instant::now(),
                XEvent::ButtonPress(e) => {
                    if Some(e.event) == ov_big || Some(e.event) == ov_small {
                        if let Some(ov) = self.overlay.as_mut() {
                            if let Some(cmd) = ov.on_click(e.event, e.event_x, e.event_y) {
                                self.dispatch(cmd, tx);
                            }
                        }
                    } else if Some(e.event) == menu_id {
                        if let Some(mut bar) = self.bar.take() {
                            let cmd = bar.on_menu_click(e.event_x, e.event_y);
                            bar.close_menu(&self.conn);
                            self.bar = Some(bar);
                            if let Some(cmd) = cmd {
                                self.dispatch(cmd, tx);
                            }
                        }
                    } else if Some(e.event) == bar_id {
                        if let Some(mut bar) = self.bar.take() {
                            bar.close_menu(&self.conn);
                            let cmd = bar.on_button(e.event_x, e.event_y, snap);
                            let pending = bar.take_pending_menu();
                            if let Some(kind) = pending {
                                bar.open_menu(&self.conn, kind, snap);
                            }
                            self.bar = Some(bar);
                            if let Some(cmd) = cmd {
                                self.dispatch(cmd, tx);
                            }
                        }
                    } else if e.detail == 1 {
                        let now = Instant::now();
                        let double = self
                            .last_click
                            .map(|t| now.duration_since(t).as_millis() < 400)
                            .unwrap_or(false);
                        self.last_click = Some(now);
                        self.last_activity = now;
                        if let Some(mut bar) = self.bar.take() {
                            bar.close_menu(&self.conn);
                            self.bar = Some(bar);
                        }
                        if double {
                            let on = !self.fullscreen;
                            self.set_fullscreen(on);
                        }
                    }
                }
                XEvent::ButtonRelease(e) => {
                    if Some(e.event) == bar_id {
                        if let Some(bar) = self.bar.as_mut() {
                            if let Some(cmd) = bar.on_release(e.event_x, e.event_y, snap) {
                                self.dispatch(cmd, tx);
                            }
                        }
                    }
                }
                XEvent::Expose(e) => {
                    if Some(e.window) == bar_id {
                        if let Some(bar) = self.bar.as_mut() {
                            bar.draw(&self.conn, snap, true);
                        }
                    }
                }
                XEvent::ConfigureNotify(e) => {
                    if e.window == self.win {
                        self.geom = (e.x, e.y, e.width, e.height);
                        self.place_bar();
                    }
                }
                XEvent::ClientMessage(e) => {
                    if e.format == 32 && e.data.as_data32()[0] == self.atoms.wm_delete {
                        return false;
                    }
                }
                XEvent::DestroyNotify(_) => return false,
                XEvent::Error(_) => {}
                _ => {}
            }
        }

        // 退出全屏后 300 ms 检查一次：WM 没自己还原（窗口还占满屏宽）才强制压回去。
        if let Some(at) = self.restore_at {
            if Instant::now() >= at {
                self.restore_at = None;
                if let Some((tx, ty, tw, th)) = self.restore_geom {
                    let (gx, gy, gw, gh) = self.geom;
                    if gw as f64 >= self.screen_w as f64 * 0.95 {
                        // 尺寸还占满屏宽：先只改尺寸（位置留给 WM），改完再看位置。
                        let _ = self.conn.configure_window(
                            self.win,
                            &ConfigureWindowAux::new().width(tw as u32).height(th as u32),
                        );
                        let _ = self.conn.flush();
                        self.restore_at = Some(Instant::now() + Duration::from_millis(300));
                        eprintln!("[ui] WM 未还原全屏前尺寸，强制改回 {tw}x{th}");
                    } else {
                        // 尺寸已经回来了：位置由 WM 决定。DDE/treeland 不会把窗口
                        // 挪回全屏前的位置（停在左上角），客户端 configure x/y
                        // 对它无效，这里不再强行掰，免得和 WM 抢坐标。
                        let _ = (tx, ty, gx, gy, gh);
                    }
                }
            }
        }

        // auto-hide: 3 s after the last mouse movement while playing
        let idle = self.last_activity.elapsed();
        let want = snap.pos_ms == 0
            || !snap.playing
            || snap.ended
            || snap.error.is_some()
            || snap.skip.is_some()
            || idle < Duration::from_secs(3)
            || self.bar.as_ref().map(|b| b.is_dragging()).unwrap_or(false)
            || self.bar.as_ref().map(|b| b.menu_open()).unwrap_or(false);
        let bar_visible_before = self.bar.as_ref().map(|b| b.visible).unwrap_or(false);
        if let Some(bar) = self.bar.as_mut() {
            if want {
                bar.show(&self.conn);
                if !snap.playing {
                    self.last_activity = Instant::now();
                }
            } else {
                bar.hide(&self.conn);
            }
            if bar.visible {
                // 刚从隐藏变可见要强制画：X11 不保留未映射窗口的内容，
                // 只靠「快照变了才画」会留下一条空白黑条。
                bar.draw(&self.conn, snap, !bar_visible_before);
            }
        }
        // 覆盖层（结束页/错误页/加载卡片/跳过提示）跟着状态走。
        let ov_mapped = self
            .overlay
            .as_mut()
            .map(|ov| ov.sync(&self.conn, snap))
            .unwrap_or(false);
        // 层级：覆盖层刚映射、或控制栏刚刚从隐藏变可见，都要把它重新抬到最上层
        // （覆盖层是后映射的窗口，不抬的话会把控制栏整条盖住）。
        let bar_visible_now = self.bar.as_ref().map(|b| b.visible).unwrap_or(false);
        if bar_visible_now && (ov_mapped || !bar_visible_before) {
            if let Some(bar) = self.bar.as_ref() {
                bar.raise(&self.conn);
            }
        }
        true
    }

    fn dispatch(&mut self, cmd: Command, tx: &Sender<Event>) {
        match cmd {
            Command::ToggleFullscreen => {
                let on = !self.fullscreen;
                self.set_fullscreen(on);
            }
            other => {
                let _ = tx.send(Event::Command(other));
            }
        }
    }

    fn place_bar(&mut self) {
        let (x, y, w, h) = self.geom;
        if let Some(bar) = self.bar.as_mut() {
            bar.place(&self.conn, x, y + h as i16 - BAR_H as i16, w);
        }
        if let Some(ov) = self.overlay.as_mut() {
            ov.place(&self.conn, x, y, w, h.saturating_sub(BAR_H));
        }
    }
}

fn intern(conn: &RustConnection, name: &[u8]) -> Result<Atom, String> {
    conn.intern_atom(false, name)
        .map_err(|e| e.to_string())?
        .reply()
        .map(|r| r.atom)
        .map_err(|e| e.to_string())
}
