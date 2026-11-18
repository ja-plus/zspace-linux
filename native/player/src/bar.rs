//! Control bar + popup menus for the native player.
//!
//! Two override-redirect X11 windows pinned to the player window: the bar and a
//! reusable popup menu. Both are drawn with cairo into an image surface and
//! blitted with XPutImage, so no toolkit dependency is needed.
//!
//! Layout mirrors the official Qt player (docs/PLAYER-FEATURE-PARITY.md):
//! play/pause · -10s · +10s · prev · next · time | seek | rate · subtitle ·
//! audio · quality · episodes · volume+slider · fullscreen · close.

use crate::style::{
    self, IconSet, ACCENT, FG, FG_DIM, FONT, FONT_LABEL, FONT_SMALL, HOVER_BG, ICON_SIZE, MENU_SEL,
};
use crate::{Command, MenuKind};
use cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, CreateWindowAux, EventMask, Gcontext, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// 官方控制栏高度（QSS 里没有，按原实现的 76px 保留）
pub const BAR_H: u16 = 76;
/// 倍速档位（官方 6 档，一位小数起步）
const RATES: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
const MENU_W: u16 = 220;
const MENU_ROW: f64 = 36.0;
/// 弹层标题栏高度（官方 QWidget#widTitle + QLabel 14px 的组合）
const MENU_HEAD: f64 = 38.0;
/// 官方 ConfigPopup 是「很大的一个面板」，不是小列表
const PANEL_W: u16 = 620;
const PANEL_H: u16 = 430;

/// Everything the bar needs to draw itself; rebuilt by the player loop.
#[derive(Default, Clone, PartialEq)]
pub struct Snapshot {
    pub title: String,
    pub playing: bool,
    pub pos_ms: i64,
    pub len_ms: i64,
    pub rate: f32,
    pub volume: i32,
    pub muted: bool,
    pub quality: String,
    pub audio: String,
    pub subtitle: String,
    pub has_prev: bool,
    pub has_next: bool,
    pub looping: bool,
    /// 睡眠定时剩余分钟数（None = 关闭）
    pub sleep_min: Option<u32>,
    /// 首帧未出 / 等主进程重新出地址时为真，覆盖层显示「正在加载」。
    pub loading: bool,
    /// 播放错误原因（8/1 或 libvlc 报错），有值时覆盖层整块显示错误页。
    pub error: Option<String>,
    /// 播放结束（libvlc STATE_ENDED），覆盖层显示结束页。
    pub ended: bool,
    /// 是否已进入全屏（决定显示 fullScreen 还是 exitFullScreen 图标）
    pub fullscreen: bool,
    /// 片头/片尾跳转提示：(按钮文案, 目标秒)。
    pub skip: Option<(String, f64)>,
    /// (label, selected)
    pub qualities: Vec<(String, bool)>,
    pub audios: Vec<(String, bool)>,
    pub subtitles: Vec<(String, bool)>,
    pub episodes: Vec<(String, bool)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    PlayPause,
    Rewind,
    Forward,
    Prev,
    Next,
    Volume,
    VolumeSlider,
    Rate,
    Quality,
    Audio,
    Subtitle,
    Episode,
    Fullscreen,
    More,
    Seek,
    /// 弹层标题栏右侧的关闭叉
    MenuClose,
    MenuItem,
}

struct Hit {
    rect: (i16, i16, i16, i16),
    btn: Btn,
    index: usize,
}

struct MenuState {
    kind: MenuKind,
    items: Vec<(String, bool)>,
    w: u16,
    h: u16,
}

pub struct ControlBar {
    win: Window,
    menu_win: Window,
    gc: Gcontext,
    depth: u8,
    pub width: u16,
    pub height: u16,
    pub visible: bool,
    dragging: bool,
    hits: Vec<Hit>,
    menu_hits: Vec<Hit>,
    menu: Option<MenuState>,
    last: Option<Snapshot>,
    abs_x: i16,
    abs_y: i16,
    anchor_x: i16,
    pending_menu: Option<MenuKind>,
    /// 官方 controlbar 图标（从 UZPlayer.exe 提取的 2x png）
    icons: IconSet,
    /// 当前鼠标悬停的按钮，用来切 hover 态图标
    hover: Option<Btn>,
    /// 弹层里鼠标悬停的行号（官方 item:hover）
    menu_hover: Option<usize>,
}

/// 官方控制栏高度（QSS 里没有，按原实现的 76px 保留）
impl ControlBar {
    pub fn new(conn: &RustConnection, depth: u8, x: i16, y: i16, width: u16) -> Result<Self, String> {
        let win = conn.generate_id().map_err(|e| e.to_string())?;
        let mask = EventMask::EXPOSURE
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::LEAVE_WINDOW
            | EventMask::ENTER_WINDOW;
        let aux = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0x101010)
            .event_mask(mask);
        conn.create_window(
            depth,
            win,
            conn.setup().roots[0].root,
            x,
            y,
            width,
            BAR_H,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &aux,
        )
        .map_err(|e| format!("创建控制栏失败: {e}"))?;

        let menu_win = conn.generate_id().map_err(|e| e.to_string())?;
        let maux = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0x030405)
            .event_mask(mask);
        conn.create_window(
            depth,
            menu_win,
            conn.setup().roots[0].root,
            x,
            y,
            MENU_W,
            40,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &maux,
        )
        .map_err(|e| format!("创建菜单失败: {e}"))?;

        let gc = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_gc(gc, win, &Default::default())
            .map_err(|e| e.to_string())?;
        Ok(ControlBar {
            win,
            menu_win,
            gc,
            depth,
            width,
            height: BAR_H,
            visible: false,
            dragging: false,
            hits: Vec::new(),
            menu_hits: Vec::new(),
            menu: None,
            last: None,
            abs_x: x,
            abs_y: y,
            anchor_x: 0,
            pending_menu: None,
            icons: IconSet::load(),
            hover: None,
            menu_hover: None,
        })
    }

    pub fn id(&self) -> Window {
        self.win
    }

    pub fn menu_id(&self) -> Window {
        self.menu_win
    }

    /// 把控制栏抬到最上层（覆盖层刚映射时会压在它上面）。
    pub fn raise(&self, conn: &RustConnection) {
        let _ = conn.configure_window(
            self.win,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
        );
        let _ = conn.flush();
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// 当前打开的弹层类型；没开时是 None。
    pub fn menu_kind(&self) -> Option<MenuKind> {
        self.menu.as_ref().map(|m| m.kind)
    }

    /// 弹层窗口里的鼠标移动：更新 hover 行并重绘。
    pub fn on_menu_motion(&mut self, conn: &RustConnection, x: i16, y: i16) {
        if self.menu.is_none() {
            return;
        }
        let mut hovered = None;
        for h in &self.menu_hits {
            let (hx, hy, hw, hh) = h.rect;
            if h.btn == Btn::MenuItem && x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                hovered = Some(h.index);
                break;
            }
        }
        if hovered != self.menu_hover {
            self.menu_hover = hovered;
            self.draw_menu(conn);
        }
    }

    pub fn close_menu(&mut self, conn: &RustConnection) {
        self.menu_hover = None;
        if self.menu.take().is_some() {
            let _ = conn.unmap_window(self.menu_win);
            let _ = conn.flush();
        }
    }

    pub fn show(&mut self, conn: &RustConnection) {
        if !self.visible {
            let _ = conn.map_window(self.win);
            let _ = conn.configure_window(
                self.win,
                &x11rb::protocol::xproto::ConfigureWindowAux::new()
                    .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
            );
            let _ = conn.flush();
            self.visible = true;
        }
    }

    pub fn hide(&mut self, conn: &RustConnection) {
        if self.visible {
            self.close_menu(conn);
            let _ = conn.unmap_window(self.win);
            let _ = conn.flush();
            self.visible = false;
        }
    }

    pub fn place(&mut self, conn: &RustConnection, x: i16, y: i16, width: u16) {
        self.close_menu(conn);
        self.abs_x = x;
        self.abs_y = y;
        let aux = if width == self.width {
            x11rb::protocol::xproto::ConfigureWindowAux::new().x(x as i32).y(y as i32)
        } else {
            x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(width as u32)
                .height(self.height as u32)
        };
        let _ = conn.configure_window(self.win, &aux);
        self.width = width;
        let _ = conn.flush();
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    // ---------------------------------------------------------------- drawing

    pub fn draw(&mut self, conn: &RustConnection, snap: &Snapshot, force: bool) {
        if !force && self.last.as_ref() == Some(snap) {
            return;
        }
        self.last = Some(snap.clone());
        let w = self.width as i32;
        let h = self.height as i32;
        let mut surface = match ImageSurface::create(Format::ARgb32, w, h) {
            Ok(s) => s,
            Err(_) => return,
        };
        {
            let ctx = Context::new(&surface).unwrap_or_else(|_| panic!("cairo context"));
            // 官方 QWidget#widControl：黑色渐变叠 #141620，不是一块纯色
            style::draw_control_bg(&ctx, w as f64, h as f64);

            self.hits.clear();
            self.draw_seek(&ctx, snap, w);
            self.draw_buttons(&ctx, snap, w, h);
        }
        self.blit(conn, &mut surface, self.win, self.width, self.height);
    }

    fn blit(&self, conn: &RustConnection, surface: &mut ImageSurface, win: Window, w: u16, h: u16) {
        let _ = surface.flush();
        let bytes = match surface.data() {
            Ok(d) => d.to_vec(),
            Err(_) => return,
        };
        let _ = conn.put_image(
            x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
            win,
            self.gc,
            w,
            h,
            0,
            0,
            0,
            self.depth,
            &bytes,
        );
        let _ = conn.flush();
    }

    fn text_width(&self, ctx: &Context, s: &str, size: f64) -> f64 {
        ctx.select_font_face("Noto Sans CJK SC", FontSlant::Normal, FontWeight::Normal);
        ctx.set_font_size(size);
        ctx.text_extents(s).map(|e| e.x_advance()).unwrap_or(0.0)
    }

    fn draw_seek(&mut self, ctx: &Context, snap: &Snapshot, w: i32) {
        let left = 16.0;
        let right = w as f64 - 16.0;
        let y = 14.0;
        let frac = if snap.len_ms > 0 {
            (snap.pos_ms as f64 / snap.len_ms as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // 轨道 / 已播放 / 滑块全部按官方 QSS：4px 高、圆角 2、主色 #425FFF
        style::draw_seek(ctx, &self.icons, left, right, y, frac);
        self.hits.push(Hit {
            rect: (left as i16, (y - 9.0) as i16, (right - left) as i16, 24),
            btn: Btn::Seek,
            index: 0,
        });
    }

    fn draw_buttons(&mut self, ctx: &Context, snap: &Snapshot, w: i32, h: i32) {
        // 图标位 40x40（官方资源是 80x80 的 2x 图），行中心落在进度条下方
        let cy = h as f64 - 30.0;
        let mut x = 12.0;

        let (play_k, play_h) = if snap.playing {
            ("pause", "pause_hover")
        } else {
            ("play", "play_hover")
        };
        self.icon_btn(ctx, &mut x, cy, Btn::PlayPause, play_k, play_h, None, true);
        self.icon_btn(ctx, &mut x, cy, Btn::Rewind, "rewind", "rewind_hover", None, true);
        self.icon_btn(ctx, &mut x, cy, Btn::Forward, "forward", "forward_hover", None, true);
        self.vsep(ctx, &mut x, cy);
        self.icon_btn(ctx, &mut x, cy, Btn::Prev, "prev", "prev_hover", Some("prev_off"), snap.has_prev);
        self.icon_btn(ctx, &mut x, cy, Btn::Next, "next", "next_hover", Some("next_off"), snap.has_next);

        // 时间 / 标题：官方字号 16 / 14px
        let time = format!("{} / {}", fmt_time(snap.pos_ms), fmt_time(snap.len_ms));
        x += 6.0;
        self.text_a(ctx, &time, x, cy + FONT / 2.8, FONT_SMALL, FG);
        x += self.text_width(ctx, &time, FONT_SMALL) + 14.0;
        if !snap.title.is_empty() {
            let max_w = (w as f64 * 0.28).max(80.0);
            let mut t = snap.title.clone();
            while self.text_width(ctx, &t, FONT_SMALL) > max_w && t.chars().count() > 4 {
                let cut = t
                    .char_indices()
                    .nth(t.chars().count().saturating_sub(2))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                t.truncate(cut);
                t.push('…');
            }
            self.text_a(ctx, &t, x, cy + FONT / 2.8, FONT_SMALL, FG_DIM);
        }

        // 右簇（从右往左）：更多 · 全屏 · 音量 · | · 选集 · 音轨 · 字幕 · 倍速 · 清晰度
        // 官方控制栏没有关闭按钮（关闭在标题栏），这里也不放
        let mut rx = w as f64 - 8.0;
        rx = self.icon_right(ctx, rx, cy, Btn::More, "more", "more_hover", None, true, ICON_SIZE);
        let (fs_k, fs_h) = if snap.fullscreen {
            ("exit_full", "exit_full_hover")
        } else {
            ("full", "full_hover")
        };
        rx = self.icon_right(ctx, rx, cy, Btn::Fullscreen, fs_k, fs_h, None, true, ICON_SIZE);

        let (vol_k, vol_h) = if snap.muted {
            ("mute", "mute_hover")
        } else {
            ("volume", "volume_hover")
        };
        rx = self.icon_right(ctx, rx, cy, Btn::Volume, vol_k, vol_h, None, true, ICON_SIZE);

        // 音量滑杆：与进度条同一套 QSS 口径（4px、圆角 2、已播放段 #425FFF）
        rx -= 4.0;
        let vw = 72.0;
        rx -= vw;
        let vfrac = (snap.volume.clamp(0, 100) as f64 / 100.0).clamp(0.0, 1.0);
        ctx.new_path();
        style::rounded_rect(ctx, rx, cy - 2.0, vw, 4.0, 2.0);
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.15);
        let _ = ctx.fill();
        if vfrac > 0.0 {
            ctx.new_path();
            style::rounded_rect(ctx, rx, cy - 2.0, vw * vfrac, 4.0, 2.0);
            ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            let _ = ctx.fill();
        }
        self.hits.push(Hit {
            rect: ((rx - 4.0) as i16, (cy - 10.0) as i16, (vw + 8.0) as i16, 20),
            btn: Btn::VolumeSlider,
            index: 0,
        });
        rx = self.vsep_right(ctx, rx, cy);

        rx = self.icon_right(ctx, rx, cy, Btn::Episode, "episode", "episode_hover", None, true, ICON_SIZE);
        rx = self.icon_right(ctx, rx, cy, Btn::Audio, "audio", "audio_hover", None, true, ICON_SIZE);
        rx = self.icon_right(ctx, rx, cy, Btn::Subtitle, "subtitle", "subtitle_hover", None, true, ICON_SIZE);
        // 倍速：官方是文字胶囊（btnSpeed: bg rgba(255,255,255,.15)、r8、16px）
        rx = self.pill_right(ctx, rx, cy, Btn::Rate, &fmt_rate(snap.rate), true);
        // 清晰度：官方 btnFormat 是纯文字按钮（16px，无底色，padding 8）
        let qual = label_or(&snap.quality, "清晰度");
        rx = self.pill_right(ctx, rx, cy, Btn::Quality, &qual, false);
        let _ = rx;
    }

    fn text_a(&self, ctx: &Context, s: &str, x: f64, y: f64, size: f64, color: (f64, f64, f64, f64)) {
        ctx.select_font_face("Noto Sans CJK SC", FontSlant::Normal, FontWeight::Normal);
        ctx.set_font_size(size);
        ctx.set_source_rgba(color.0, color.1, color.2, color.3);
        ctx.move_to(x, y);
        let _ = ctx.show_text(s);
    }

    /// 竖分隔线（官方 QWidget#widButtons 的 1px 右边框）
    fn vsep(&mut self, ctx: &Context, x: &mut f64, cy: f64) {
        *x += 6.0;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.15);
        ctx.rectangle(*x, cy - 10.0, 1.0, 20.0);
        let _ = ctx.fill();
        *x += 10.0;
    }

    fn vsep_right(&mut self, ctx: &Context, rx: f64, cy: f64) -> f64 {
        let x = rx - 8.0;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.15);
        ctx.rectangle(x, cy - 10.0, 1.0, 20.0);
        let _ = ctx.fill();
        x - 9.0
    }

    fn icon_btn(
        &mut self,
        ctx: &Context,
        x: &mut f64,
        cy: f64,
        btn: Btn,
        normal: &str,
        hover: &str,
        disabled: Option<&str>,
        enabled: bool,
    ) {
        let cx = *x + ICON_SIZE / 2.0;
        if enabled {
            let key = if self.hover == Some(btn) { hover } else { normal };
            self.icons.draw_center(ctx, key, cx, cy, ICON_SIZE);
        } else if let Some(off) = disabled {
            self.icons.draw_center(ctx, off, cx, cy, ICON_SIZE);
        }
        self.hits.push(Hit {
            rect: (*x as i16, (cy - ICON_SIZE / 2.0) as i16, ICON_SIZE as i16, ICON_SIZE as i16),
            btn,
            index: 0,
        });
        *x += ICON_SIZE + 2.0;
    }

    fn icon_right(
        &mut self,
        ctx: &Context,
        mut rx: f64,
        cy: f64,
        btn: Btn,
        normal: &str,
        hover: &str,
        disabled: Option<&str>,
        enabled: bool,
        size: f64,
    ) -> f64 {
        rx -= size;
        let cx = rx + size / 2.0;
        if enabled {
            let key = if self.hover == Some(btn) { hover } else { normal };
            self.icons.draw_center(ctx, key, cx, cy, size);
        } else if let Some(off) = disabled {
            self.icons.draw_center(ctx, off, cx, cy, size);
        }
        self.hits.push(Hit {
            rect: (rx as i16, (cy - size / 2.0) as i16, size as i16, size as i16),
            btn,
            index: 0,
        });
        rx - 2.0
    }

    /// 右对齐文字按钮：`filled` 为真时带官方 btnSpeed 的胶囊底色。
    fn pill_right(&mut self, ctx: &Context, rx: f64, cy: f64, btn: Btn, label: &str, filled: bool) -> f64 {
        let tw = self.text_width(ctx, label, FONT);
        let pad = 8.0;
        let bw = tw + pad * 2.0;
        let bh = 28.0;
        let x = rx - bw;
        if filled {
            ctx.new_path();
            style::rounded_rect(ctx, x, cy - bh / 2.0, bw, bh, 8.0);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.15);
            let _ = ctx.fill();
        } else if self.hover == Some(btn) {
            ctx.new_path();
            style::rounded_rect(ctx, x, cy - bh / 2.0, bw, bh, 8.0);
            ctx.set_source_rgba(HOVER_BG.0, HOVER_BG.1, HOVER_BG.2, HOVER_BG.3);
            let _ = ctx.fill();
        }
        self.text_a(ctx, label, x + pad, cy + FONT / 2.8, FONT, FG);
        self.hits.push(Hit {
            rect: (x as i16, (cy - bh / 2.0) as i16, bw as i16, bh as i16),
            btn,
            index: 0,
        });
        x - 4.0
    }

    // ------------------------------------------------------------------ menu

    pub fn open_menu(&mut self, conn: &RustConnection, kind: MenuKind, snap: &Snapshot) {
        let items: Vec<(String, bool)> = match kind {
            MenuKind::Speed => [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0]
                .iter()
                .map(|r| (format!("{r:.2}x"), (snap.rate - r).abs() < 0.01))
                .collect(),
            MenuKind::Quality => snap.qualities.clone(),
            MenuKind::Audio => snap.audios.clone(),
            MenuKind::Subtitle => snap.subtitles.clone(),
            MenuKind::Episode => snap.episodes.clone(),
            MenuKind::More => vec![
                (format!("循环播放：{}", if snap.looping { "开" } else { "关" }), snap.looping),
                (
                    match snap.sleep_min {
                        Some(m) => format!("睡眠定时：{m} 分钟后暂停"),
                        None => "睡眠定时：关闭".to_string(),
                    },
                    snap.sleep_min.is_some(),
                ),
            ],
        };
        if items.is_empty() {
            return;
        }
        let (mw, h) = if kind == MenuKind::More {
            (PANEL_W, PANEL_H)
        } else {
            (MENU_W, (MENU_HEAD + items.len() as f64 * MENU_ROW + 8.0) as u16)
        };
        let _ = &items;
        let x = (self.abs_x + self.anchor_x - mw as i16 / 2)
            .clamp(self.abs_x, self.abs_x + self.width as i16 - mw as i16);
        let y = self.abs_y - h as i16 - 6;
        let _ = conn.configure_window(
            self.menu_win,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(mw as u32)
                .height(h as u32),
        );
        self.menu_hover = None;
        self.menu = Some(MenuState { kind, items, w: mw, h });
        let _ = conn.map_window(self.menu_win);
        let _ = conn.configure_window(
            self.menu_win,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
        );
        let _ = conn.flush();
        self.draw_menu(conn);
    }

    fn draw_menu(&mut self, conn: &RustConnection) {
        let (mw, mh, kind) = match self.menu.as_ref() {
            Some(m) => (m.w as i32, m.h as i32, m.kind),
            None => return,
        };
        let mut surface = match ImageSurface::create(Format::ARgb32, mw, mh) {
            Ok(s) => s,
            Err(_) => return,
        };
        self.menu_hits.clear();
        {
            let ctx = Context::new(&surface).unwrap_or_else(|_| panic!("cairo context"));
            // 容器：官方 QWidget#widLiveBase —— rgba(0,0,0,0.85) 叠底、
            // 8px 圆角、1px rgba(179,193,205,0.15) 描边
            let bg = style::popup_bg();
            ctx.set_source_rgb(bg.0, bg.1, bg.2);
            let _ = ctx.paint();
            ctx.set_line_width(1.0);
            ctx.set_source_rgba(
                style::POPUP_BORDER.0,
                style::POPUP_BORDER.1,
                style::POPUP_BORDER.2,
                style::POPUP_BORDER.3,
            );
            style::rounded_rect(&ctx, 0.5, 0.5, mw as f64 - 1.0, mh as f64 - 1.0, style::POPUP_RADIUS);
            let _ = ctx.stroke();

            // 标题栏：官方 widTitle 是 14px 55% 白 + 底部 1px 分隔线
            self.text_a(
                &ctx,
                menu_title(kind),
                14.0,
                MENU_HEAD / 2.0 + FONT_LABEL / 2.8,
                FONT_LABEL,
                style::LABEL,
            );
            ctx.set_source_rgba(
                style::DIVIDER.0,
                style::DIVIDER.1,
                style::DIVIDER.2,
                style::DIVIDER.3,
            );
            ctx.rectangle(1.0, MENU_HEAD, mw as f64 - 2.0, 1.0);
            let _ = ctx.fill();
            // 官方 popup 标题栏右侧有关闭叉（QPushButton#btnClose → popupClose2x.png）
            self.icons
                .draw_center(&ctx, "popup_close", mw as f64 - 20.0, MENU_HEAD / 2.0, 20.0);
            self.menu_hits.push(Hit {
                rect: ((mw - 32) as i16, 4, 28, (MENU_HEAD - 8.0) as i16),
                btn: Btn::MenuClose,
                index: 0,
            });

            if kind == MenuKind::More {
                let snap = self.last.clone().unwrap_or_default();
                self.draw_config(&ctx, &snap, mw);
            } else {
            let items = self.menu.as_ref().map(|m| m.items.clone()).unwrap_or_default();
            for (i, (label, selected)) in items.iter().enumerate() {
                let ty = MENU_HEAD + 4.0 + i as f64 * MENU_ROW;
                let hovered = self.menu_hover == Some(i);
                if *selected || hovered {
                    // 官方：item:hover 与 selected 都上底色，选中更实一点
                    let alpha = if *selected { 0.22 } else { MENU_SEL.3 };
                    ctx.new_path();
                    style::rounded_rect(&ctx, 5.0, ty, mw as f64 - 10.0, MENU_ROW - 4.0, 4.0);
                    ctx.set_source_rgba(MENU_SEL.0, MENU_SEL.1, MENU_SEL.2, alpha);
                    let _ = ctx.fill();
                }
                let color = if *selected || hovered {
                    (1.0, 1.0, 1.0, 1.0)
                } else {
                    FG
                };
                let mut t = label.clone();
                while self.text_width(&ctx, &t, FONT) > mw as f64 - 36.0 && t.chars().count() > 3 {
                    let cut = t.char_indices().nth(t.chars().count().saturating_sub(2)).map(|(c, _)| c).unwrap_or(0);
                    t.truncate(cut);
                    t.push('…');
                }
                // 官方弹层项 font-size 16px、padding-left 12px
                self.text_a(&ctx, &t, 17.0, ty + (MENU_ROW - 4.0) / 2.0 + FONT / 2.8, FONT, color);
                self.menu_hits.push(Hit {
                    rect: (5, ty as i16, (mw - 10) as i16, (MENU_ROW - 4.0) as i16),
                    btn: Btn::MenuItem,
                    index: i,
                });
            }
            }
        }
        let (w, h) = match self.menu.as_ref() {
            Some(m) => (m.w, m.h),
            None => return,
        };
        self.blit(conn, &mut surface, self.menu_win, w, h);
    }

    /// 官方 ConfigPopup：一个很大的设置面板，按「分组标签 + 胶囊按钮」排。
    /// 没实现的项画灰、不挂命中区（点了不会有反应，也不会误导）。
    fn draw_config(&mut self, ctx: &Context, snap: &Snapshot, mw: i32) {
        let x0 = 20.0;
        let mut y = MENU_HEAD + 30.0;

        let chips: Vec<(String, bool)> = RATES
            .iter()
            .map(|r| (fmt_rate(*r), (snap.rate - r).abs() < 0.01))
            .collect();
        self.chip_row(ctx, x0, y, "播放速度", &chips, 100, true);
        y += 46.0;

        let chips = vec![
            ("开".to_string(), snap.looping),
            ("关".to_string(), !snap.looping),
        ];
        self.chip_row(ctx, x0, y, "循环播放", &chips, 200, true);
        y += 46.0;

        let chips: Vec<(String, bool)> = [0u32, 10, 15, 30, 60]
            .iter()
            .map(|m| {
                let label = if *m == 0 {
                    "关闭".to_string()
                } else {
                    format!("{m} 分钟")
                };
                let on = if *m == 0 {
                    snap.sleep_min.is_none()
                } else {
                    snap.sleep_min == Some(*m)
                };
                (label, on)
            })
            .collect();
        self.chip_row(ctx, x0, y, "睡眠定时", &chips, 300, true);
        y += 52.0;
        self.config_divider(ctx, mw, y - 20.0);

        // 以下两组属于功能缺口：连播策略依赖 12/1 的列表策略，解码/解复用
        // 在 libvlc 3 里没有运行时切换接口
        let chips = vec![
            ("顺序播放".to_string(), !snap.looping),
            ("单集循环".to_string(), snap.looping),
            ("随机播放".to_string(), false),
        ];
        self.chip_row(ctx, x0, y + 12.0, "连播策略", &chips, 400, false);
        y += 58.0;

        let chips = vec![
            ("硬解".to_string(), false),
            ("软解".to_string(), true),
        ];
        self.chip_row(ctx, x0, y, "解码方式", &chips, 500, false);
        y += 46.0;

        let chips = vec![
            ("AVCodec".to_string(), true),
            ("VLC".to_string(), false),
        ];
        self.chip_row(ctx, x0, y, "解复用", &chips, 600, false);
        y += 52.0;
        self.config_divider(ctx, mw, y - 20.0);

        self.text_a(
            ctx,
            "画面调节（亮度/对比度/饱和度）、字幕样式、快捷键、分享、直播：暂未实现",
            x0,
            y + 14.0,
            FONT_LABEL,
            (1.0, 1.0, 1.0, 0.35),
        );
    }

    /// 面板里的一行：左侧标签 + 右侧一排胶囊。
    fn chip_row(
        &mut self,
        ctx: &Context,
        x0: f64,
        y: f64,
        label: &str,
        chips: &[(String, bool)],
        base: usize,
        enabled: bool,
    ) {
        let lab = if enabled {
            style::LABEL
        } else {
            (1.0, 1.0, 1.0, 0.28)
        };
        self.text_a(ctx, label, x0, y + 5.0, FONT_LABEL, lab);
        let mut x = x0 + 96.0;
        for (i, (text, selected)) in chips.iter().enumerate() {
            let tw = self.text_width(ctx, text, FONT_SMALL);
            let bw = tw + 24.0;
            let bh = 30.0;
            let hovered = enabled && self.menu_hover == Some(base + i);
            if *selected || hovered {
                ctx.new_path();
                style::rounded_rect(ctx, x, y - bh / 2.0 + 5.0, bw, bh, 8.0);
                // 官方 btnSpeed 口径：灰底胶囊；选中/悬停用主色淡底
                let alpha = if *selected { 0.18 } else { MENU_SEL.3 };
                ctx.set_source_rgba(MENU_SEL.0, MENU_SEL.1, MENU_SEL.2, alpha);
                let _ = ctx.fill();
            }
            let color = if !enabled {
                (1.0, 1.0, 1.0, 0.3)
            } else if *selected || hovered {
                (1.0, 1.0, 1.0, 1.0)
            } else {
                FG
            };
            self.text_a(ctx, text, x + 12.0, y + 5.0, FONT_SMALL, color);
            if enabled {
                self.menu_hits.push(Hit {
                    rect: (x as i16, (y - 10.0) as i16, bw as i16, 30),
                    btn: Btn::MenuItem,
                    index: base + i,
                });
            }
            x += bw + 8.0;
        }
    }

    fn config_divider(&self, ctx: &Context, mw: i32, y: f64) {
        ctx.set_source_rgba(
            style::DIVIDER.0,
            style::DIVIDER.1,
            style::DIVIDER.2,
            style::DIVIDER.3,
        );
        ctx.rectangle(20.0, y, mw as f64 - 40.0, 1.0);
        let _ = ctx.fill();
    }

    /// Click inside the popup menu: returns the command and closes the menu.
    pub fn on_menu_click(&mut self, x: i16, y: i16) -> Option<Command> {
        let mut chosen = None;
        if let Some(menu) = self.menu.as_ref() {
            for h in &self.menu_hits {
                let (hx, hy, hw, hh) = h.rect;
                if x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                    // 关闭叉只关菜单，不选任何一项
                    if h.btn != Btn::MenuClose {
                        chosen = Some((menu.kind, h.index));
                    }
                    break;
                }
            }
        }
        self.menu = None;
        // 设置面板：索引按分组编码（100+ 倍速、200 循环、300+ 睡眠）
        if let Some((MenuKind::More, idx)) = chosen {
            return match idx {
                100..=105 => RATES.get(idx - 100).map(|r| Command::SetRate(*r)),
                200 => Some(Command::SetLoop(true)),
                201 => Some(Command::SetLoop(false)),
                300 => Some(Command::SetSleepTimer(0)),
                301..=304 => Some(Command::SetSleepTimer([0u32, 10, 15, 30, 60][idx - 300])),
                _ => None,
            };
        }
        match chosen {
            Some((MenuKind::Speed, i)) => {
                let rates = [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0];
                rates.get(i).map(|r| Command::SetRate(*r))
            }
            Some((MenuKind::Quality, i)) => Some(Command::SelectQuality(i)),
            Some((MenuKind::Audio, i)) => Some(Command::SelectAudio(i)),
            Some((MenuKind::Subtitle, i)) => Some(Command::SelectSubtitle(i as i64)),
            Some((MenuKind::Episode, i)) => Some(Command::PlayEpisode(i)),
            _ => None,
        }
    }

    // ----------------------------------------------------------------- input

    pub fn on_button(&mut self, x: i16, y: i16, snap: &Snapshot) -> Option<Command> {
        self.anchor_x = x;
        for h in &self.hits {
            let (hx, hy, hw, hh) = h.rect;
            if x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                match h.btn {
                    Btn::Seek => {
                        self.dragging = true;
                        return self.seek_at(x, snap);
                    }
                    Btn::VolumeSlider => return self.volume_at(x, hx, hw),
                    Btn::Rate => {
                        self.pending_menu = Some(MenuKind::Speed);
                        return None;
                    }
                    Btn::Quality => {
                        self.pending_menu = Some(MenuKind::Quality);
                        return None;
                    }
                    Btn::Audio => {
                        self.pending_menu = Some(MenuKind::Audio);
                        return None;
                    }
                    Btn::Subtitle => {
                        self.pending_menu = Some(MenuKind::Subtitle);
                        return None;
                    }
                    Btn::Episode => {
                        self.pending_menu = Some(MenuKind::Episode);
                        return None;
                    }
                    Btn::More => {
                        self.pending_menu = Some(MenuKind::More);
                        return None;
                    }
                    Btn::Prev if !snap.has_prev => return None,
                    Btn::Next if !snap.has_next => return None,
                    other => return Some(map_btn(other)),
                }
            }
        }
        None
    }

    pub fn take_pending_menu(&mut self) -> Option<MenuKind> {
        self.pending_menu.take()
    }

    pub fn on_release(&mut self, x: i16, _y: i16, snap: &Snapshot) -> Option<Command> {
        if self.dragging {
            self.dragging = false;
            return self.seek_at(x, snap);
        }
        None
    }

    pub fn on_motion(
        &mut self,
        conn: &RustConnection,
        x: i16,
        y: i16,
        snap: &Snapshot,
    ) -> Option<Command> {
        if self.dragging {
            return self.seek_at(x, snap);
        }
        // 官方按钮有 hover 态图标，这里按命中区切图并重绘
        let mut hovered = None;
        for h in &self.hits {
            let (hx, hy, hw, hh) = h.rect;
            if x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                hovered = Some(h.btn);
                break;
            }
        }
        if hovered != self.hover {
            self.hover = hovered;
            self.draw(conn, snap, true);
        }
        None
    }

    fn seek_at(&self, x: i16, snap: &Snapshot) -> Option<Command> {
        if snap.len_ms <= 0 || self.width <= 36 {
            return None;
        }
        let left = 18.0;
        let right = self.width as f64 - 18.0;
        let frac = ((x as f64 - left) / (right - left)).clamp(0.0, 1.0);
        Some(Command::SeekTo(snap.len_ms as f64 / 1000.0 * frac))
    }

    fn volume_at(&self, x: i16, hx: i16, hw: i16) -> Option<Command> {
        let frac = ((x - hx) as f64 / hw as f64).clamp(0.0, 1.0);
        Some(Command::SetVolume((frac * 100.0).round() as i32))
    }
}

/// 官方倍速文案是 `1.0x` / `1.25x` 这种，不强行两位小数。
fn fmt_rate(rate: f32) -> String {
    let pct = (rate * 100.0).round();
    if (pct as i64) % 10 == 0 {
        format!("{:.1}x", rate)
    } else {
        format!("{:.2}x", rate)
    }
}

fn map_btn(btn: Btn) -> Command {
    match btn {
        Btn::PlayPause => Command::TogglePause,
        Btn::Rewind => Command::SeekBy(-10.0),
        Btn::Forward => Command::SeekBy(10.0),
        Btn::Prev => Command::PrevEpisode,
        Btn::Next => Command::NextEpisode,
        Btn::Volume => Command::MuteToggle,
        Btn::Fullscreen => Command::ToggleFullscreen,
        _ => Command::TogglePause,
    }
}

/// 弹层标题（官方 popup 有独立标题栏，文字走 QLabel 14px 55% 白）
fn menu_title(kind: MenuKind) -> &'static str {
    match kind {
        MenuKind::Speed => "播放速度",
        MenuKind::Quality => "清晰度",
        MenuKind::Audio => "音轨",
        MenuKind::Subtitle => "字幕",
        MenuKind::Episode => "选集",
        MenuKind::More => "播放设置",
    }
}

fn label_or(s: &str, fallback: &str) -> String {
    if s.is_empty() {
        fallback.to_string()
    } else {
        s.to_string()
    }
}

fn fmt_time(ms: i64) -> String {
    let total = (ms.max(0) / 1000) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
