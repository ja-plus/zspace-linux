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
    CreateWindowAux, EventMask, PropMode, StackMode, Window, WindowClass,
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
    net_moveresize: Atom,
    net_frame_extents: Atom,
}

/// 退出全屏后待校验的还原任务。
#[derive(Clone, Copy)]
struct PendingRestore {
    /// 目标几何（进入全屏前的位置与尺寸，内容区根坐标）
    geom: (i16, i16, u16, u16),
    /// 进入全屏前的窗口装饰尺寸 (left, top) —— 全屏期间读不到，必须提前存
    frame: (i16, i16),
    /// 已经重试了几次，超过 4 次就交回 WM
    attempts: u8,
    /// 下次检查时间
    next_at: Instant,
}

pub struct AppWindow {
    conn: RustConnection,    win: Window,
    bar: Option<ControlBar>,
    overlay: Option<Overlay>,
    geom: (i16, i16, u16, u16),
    /// 进入全屏前的窗口几何，退出全屏时用来还原（有的 WM 不会自己还原）。
    restore_geom: Option<(i16, i16, u16, u16)>,
    /// 进入全屏前的窗口装饰 (left, top)，退出时要用它换算外框坐标
    restore_frame: (i16, i16),
    /// 进入全屏后的兜底检查时刻：WM 若没接管就自己铺满屏幕。
    pending_fullscreen: Option<Instant>,
    /// 退出全屏后的还原任务：目标几何 + 已重试次数 + 下次检查时间。
    /// 有的 WM 不还原、有的会还原成别的尺寸，所以不带条件地校验重试。
    pending_restore: Option<PendingRestore>,
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
            net_moveresize: intern(&conn, b"_NET_MOVERESIZE_WINDOW")?,
            net_frame_extents: intern(&conn, b"_NET_FRAME_EXTENTS")?,
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
            restore_frame: (0, 0),
            pending_fullscreen: None,
            pending_restore: None,
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

    /// 窗口内容区在根窗口上的真实坐标。
    /// `ConfigureNotify` 给的 x/y 是相对父窗口（reparenting WM 的 frame）的，
    /// 直接拿去定位 override-redirect 的控制栏或者还原窗口都会偏一个装饰宽度。
    fn root_origin(&self) -> (i16, i16) {
        let root = self.conn.setup().roots[0].root;
        match self.conn.translate_coordinates(self.win, root, 0, 0) {
            Ok(cookie) => match cookie.reply() {
                Ok(r) => (r.dst_x as i16, r.dst_y as i16),
                Err(_) => (self.geom.0, self.geom.1),
            },
            Err(_) => (self.geom.0, self.geom.1),
        }
    }

    /// 窗口装饰（标题栏/边框）的尺寸，顺序是 left, right, top, bottom。
    /// WM 的移动请求算的是**外框**坐标，内容区坐标要减掉这一圈。
    fn frame_extents(&self) -> (i16, i16) {
        let prop = self.conn.get_property(
            false,
            self.win,
            self.atoms.net_frame_extents,
            AtomEnum::CARDINAL,
            0,
            4,
        );
        if let Ok(cookie) = prop {
            if let Ok(reply) = cookie.reply() {
                let v: Vec<u32> = reply.value32().map(|it| it.collect()).unwrap_or_default();
                if v.len() >= 4 {
                    return (v[0] as i16, v[2] as i16);
                }
            }
        }
        (0, 0)
    }

    /// 按**根坐标**请求移动/改尺寸。
    /// 直接 `configure_window` 的 x/y 是相对 frame 的，会被 WM 加一圈装饰偏移；
    /// `_NET_MOVERESIZE_WINDOW` 是 EWMH 给的标准请求，WM 会自己算装饰。
    fn request_geometry(&self, x: i16, y: i16, w: u16, h: u16, frame: (i16, i16)) {
        // gravity = NorthWest(1)，低 8 位；bit8..11 表示 x/y/w/h 有效
        // 传进来的是内容区根坐标，WM 要的是外框坐标，先减掉装饰
        let (fl, ft) = frame;
        let flags = 1u32 | (0xFu32 << 8);
        let data = ClientMessageData::from([
            flags,
            (x - fl) as u32,
            (y - ft) as u32,
            w as u32,
            h as u32,
        ]);
        let event = ClientMessageEvent::new(32, self.win, self.atoms.net_moveresize, data);
        let root = self.conn.setup().roots[0].root;
        let _ = self.conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        );
        // 不支持该协议的 WM 兜底：至少把尺寸改对
        let _ = self.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().width(w as u32).height(h as u32),
        );
        let _ = self.conn.flush();
    }

    fn set_fullscreen(&mut self, on: bool) {
        if on && !self.fullscreen {
            // 记住当前几何（含用户已经手动放大的情况），退出全屏要回到这里。
            // 位置必须是根坐标，否则还原后会被 WM 的装饰偏移带偏。
            let (ox, oy) = self.root_origin();
            self.restore_frame = self.frame_extents();
            self.restore_geom = Some((ox, oy, self.geom.2, self.geom.3));
        }
        let action = if on { 1u32 } else { 0u32 };
        let data = ClientMessageData::from([
            action,
            self.atoms.net_wm_state_fullscreen,
            0,
            1, // source indication: normal application
            0,
        ]);
        // EWMH 规定 _NET_WM_STATE 必须发到**根窗口**，掩码是
        // SubstructureRedirect|SubstructureNotify。之前发到自己的窗口 + NO_EVENT，
        // WM 根本收不到，只剩下面那句 configure —— 表现就是「窗口变大但没真全屏」。
        let event = ClientMessageEvent::new(32, self.win, self.atoms.net_wm_state, data);
        let root = self.conn.setup().roots[0].root;
        let _ = self.conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        );
        if on {
            // 先让 WM 处理；300 ms 后还没铺满再自己兜底
            self.pending_fullscreen = Some(Instant::now() + Duration::from_millis(300));
            self.pending_restore = None;
            // 有的 WM 只改尺寸不抬层级，全屏后仍被任务栏/其它窗口压着，
            // 这里显式抬到最上层
            let _ = self.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            );
            let _ = self.conn.flush();
        } else if let Some(g) = self.restore_geom {
            // 不等 WM：立刻把几何摆回去，再在主循环里校验几次。
            // 以前是「等 1.2 s，只有窗口还占满屏宽才强制改」，这样 WM 把窗口
            // 还原成别的尺寸（比如最大化）时不满足条件，窗口就永远回不到原样。
            self.request_geometry(g.0, g.1, g.2, g.3, self.restore_frame);
            self.pending_restore = Some(PendingRestore {
                geom: g,
                frame: self.restore_frame,
                attempts: 0,
                next_at: Instant::now() + Duration::from_millis(120),
            });
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
                    } else if Some(e.event) == menu_id {
                        if let Some(bar) = self.bar.as_mut() {
                            bar.on_menu_motion(&self.conn, e.event_x, e.event_y);
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
                            // 关闭动作由 on_menu_click 自己决定：
                            // 小弹层选完即关，设置面板要保持打开。
                            let cmd = bar.on_menu_click(&self.conn, e.event_x, e.event_y);
                            self.bar = Some(bar);
                            if let Some(cmd) = cmd {
                                self.dispatch(cmd, tx);
                            }
                        }
                    } else if Some(e.event) == bar_id {
                        if let Some(mut bar) = self.bar.take() {
                            // 记住点之前开着哪个弹层：同一个按钮再点一次应当收起，
                            // 不能「关掉再立刻打开」（那就是点了没反应）
                            let was_open = bar.menu_kind();
                            bar.close_menu(&self.conn);
                            let cmd = bar.on_button(e.event_x, e.event_y, snap);
                            let pending = bar.take_pending_menu();
                            if let Some(kind) = pending {
                                if was_open != Some(kind) {
                                    bar.open_menu(&self.conn, kind, snap);
                                }
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

        // 进入全屏后 300 ms 兜底：WM 没接管就自己铺满屏幕（无 EWMH 的 WM）
        if let Some(at) = self.pending_fullscreen {
            if Instant::now() >= at {
                self.pending_fullscreen = None;
                let (_, _, gw, gh) = self.geom;
                let (sw, sh) = (self.screen_w, self.screen_h);
                if (gw as u32) < (sw as f64 * 0.98) as u32
                    || (gh as u32) < (sh as f64 * 0.98) as u32
                {
                    let _ = self.conn.configure_window(
                        self.win,
                        &ConfigureWindowAux::new()
                            .x(0)
                            .y(0)
                            .width(sw as u32)
                            .height(sh as u32),
                    );
                    let _ = self.conn.flush();
                    eprintln!("[ui] WM 未接管全屏，退化为铺满屏幕 {sw}x{sh}");
                }
            }
        }

        // 退出全屏后的几何校验：尺寸对上就收工，对不上就再压一次，最多 4 次。
        if let Some(p) = self.pending_restore {
            if Instant::now() >= p.next_at {
                let (_, _, gw, gh) = self.geom;
                let (tx, ty, tw, th) = p.geom;
                if (gw, gh) == (tw, th) || p.attempts >= 4 {
                    if (gw, gh) != (tw, th) {
                        eprintln!(
                            "[ui] 退出全屏后尺寸仍是 {gw}x{gh}，目标是 {tw}x{th}，交回 WM 处理"
                        );
                    }
                    self.pending_restore = None;
                } else {
                    self.request_geometry(tx, ty, tw, th, p.frame);
                    self.pending_restore = Some(PendingRestore {
                        attempts: p.attempts + 1,
                        next_at: Instant::now() + Duration::from_millis(150),
                        geom: p.geom,
                        frame: p.frame,
                    });
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
            bar.window_height = h;
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
