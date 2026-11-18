//! Control bar + popup menus for the native player.
//!
//! Two override-redirect X11 windows pinned to the player window: the bar and a
//! reusable popup menu. Both are drawn with cairo into an image surface and
//! blitted with XPutImage, so no toolkit dependency is needed.
//!
//! Layout mirrors the official Qt player (docs/PLAYER-FEATURE-PARITY.md):
//! play/pause · -10s · +10s · prev · next · time | seek | rate · subtitle ·
//! audio · quality · episodes · volume+slider · fullscreen · close.

use crate::style::{self, IconSet, ACCENT, FG, FG_DIM, FONT, FONT_SMALL, HOVER_BG, ICON_SIZE, MENU_SEL};
use crate::{Command, MenuKind};
use cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, CreateWindowAux, EventMask, Gcontext, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// 官方控制栏高度（QSS 里没有，按原实现的 76px 保留）
pub const BAR_H: u16 = 76;
const MENU_W: u16 = 220;
const MENU_ROW: f64 = 36.0;

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
    Close,
    Seek,
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
            .background_pixel(0x1b1b1f)
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

    pub fn close_menu(&mut self, conn: &RustConnection) {
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
            // 控制栏底色：官方是叠在视频上的深色条，QSS 未给值，取近黑
            ctx.set_source_rgb(0.055, 0.055, 0.067);
            ctx.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = ctx.fill();
            // 顶边分隔线 rgba(255,255,255,0.15)（同 QWidget#widButtons 的描边口径）
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
            ctx.rectangle(0.0, 0.0, w as f64, 1.0);
            let _ = ctx.fill();

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

        // 右簇（从右往左）：关闭 · 更多 · 全屏 · 音量 · | · 选集 · 音轨 · 字幕 · 倍速 · 清晰度
        let mut rx = w as f64 - 8.0;
        rx = self.icon_right(ctx, rx, cy, Btn::Close, "close", "close_hover", None, true, 30.0);
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
        let h = (items.len() as f64 * MENU_ROW + 12.0) as u16;
        let x = (self.abs_x + self.anchor_x - MENU_W as i16 / 2)
            .clamp(self.abs_x, self.abs_x + self.width as i16 - MENU_W as i16);
        let y = self.abs_y - h as i16 - 6;
        let _ = conn.configure_window(
            self.menu_win,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(MENU_W as u32)
                .height(h as u32),
        );
        self.menu = Some(MenuState { kind, items, w: MENU_W, h });
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
        let _ = kind;
        let mut surface = match ImageSurface::create(Format::ARgb32, mw, mh) {
            Ok(s) => s,
            Err(_) => return,
        };
        self.menu_hits.clear();
        {
            let ctx = Context::new(&surface).unwrap_or_else(|_| panic!("cairo context"));
            ctx.set_source_rgb(0.106, 0.106, 0.122);
            let _ = ctx.paint();
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.12);
            ctx.set_line_width(1.0);
            style::rounded_rect(&ctx, 0.5, 0.5, mw as f64 - 1.0, mh as f64 - 1.0, 8.0);
            let _ = ctx.stroke();
            let items = self.menu.as_ref().map(|m| m.items.clone()).unwrap_or_default();
            for (i, (label, selected)) in items.iter().enumerate() {
                let ty = 5.0 + i as f64 * MENU_ROW;
                if *selected {
                    // 官方弹层项选中态：rgba(130,149,255,0.12) + border-radius 4
                    ctx.new_path();
                    style::rounded_rect(&ctx, 5.0, ty, mw as f64 - 10.0, MENU_ROW - 4.0, 4.0);
                    ctx.set_source_rgba(MENU_SEL.0, MENU_SEL.1, MENU_SEL.2, MENU_SEL.3);
                    let _ = ctx.fill();
                }
                let color = if *selected { (1.0, 1.0, 1.0, 1.0) } else { FG };
                let mut t = label.clone();
                while self.text_width(&ctx, &t, FONT) > mw as f64 - 36.0 && t.chars().count() > 3 {
                    let cut = t.char_indices().nth(t.chars().count().saturating_sub(2)).map(|(c, _)| c).unwrap_or(0);
                    t.truncate(cut);
                    t.push('…');
                }
                // 官方弹层项 font-size 16px、padding-left 12px
                self.text_a(&ctx, &t, 17.0, ty + MENU_ROW / 2.0 + FONT / 2.8, FONT, color);
                self.menu_hits.push(Hit {
                    rect: (5, ty as i16, (mw - 10) as i16, (MENU_ROW - 4.0) as i16),
                    btn: Btn::MenuItem,
                    index: i,
                });
            }
        }
        let (w, h) = match self.menu.as_ref() {
            Some(m) => (m.w, m.h),
            None => return,
        };
        self.blit(conn, &mut surface, self.menu_win, w, h);
    }

    /// Click inside the popup menu: returns the command and closes the menu.
    pub fn on_menu_click(&mut self, x: i16, y: i16) -> Option<Command> {
        let mut chosen = None;
        if let Some(menu) = self.menu.as_ref() {
            for h in &self.menu_hits {
                let (hx, hy, hw, hh) = h.rect;
                if x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                    chosen = Some((menu.kind, h.index));
                    break;
                }
            }
        }
        self.menu = None;
        match chosen {
            Some((MenuKind::Speed, i)) => {
                let rates = [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0];
                rates.get(i).map(|r| Command::SetRate(*r))
            }
            Some((MenuKind::Quality, i)) => Some(Command::SelectQuality(i)),
            Some((MenuKind::Audio, i)) => Some(Command::SelectAudio(i)),
            Some((MenuKind::Subtitle, i)) => Some(Command::SelectSubtitle(i as i64)),
            Some((MenuKind::Episode, i)) => Some(Command::PlayEpisode(i)),
            Some((MenuKind::More, 0)) => Some(Command::ToggleLoop),
            Some((MenuKind::More, 1)) => Some(Command::CycleSleepTimer),
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
        Btn::Close => Command::Quit,
        _ => Command::TogglePause,
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
