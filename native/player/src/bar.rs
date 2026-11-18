//! Control bar + popup menus for the native player.
//!
//! Two override-redirect X11 windows pinned to the player window: the bar and a
//! reusable popup menu. Both are drawn with cairo into an image surface and
//! blitted with XPutImage, so no toolkit dependency is needed.
//!
//! Layout mirrors the official Qt player (docs/PLAYER-FEATURE-PARITY.md):
//! play/pause · -10s · +10s · prev · next · time | seek | rate · subtitle ·
//! audio · quality · episodes · volume+slider · fullscreen · close.

use crate::style::{self, IconSet, ACCENT, FG, FONT_SMALL, ICON_SIZE};
use crate::{Command, MenuKind};
use cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, CreateWindowAux, EventMask, Gcontext, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// 控制栏高度：Windows 版实测 66px（标题栏 44 + 视频 718 + 控制栏 66 = 828）
pub const BAR_H: u16 = 66;
/// 按钮位宽度：官方图标中心间距实测 56px
const BTN_SLOT: f64 = 56.0;
/// 倍速档位（官方 6 档，一位小数起步）
const RATES: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
/// 小弹层窗口：Windows 版实测固定 320x360（与内容多少无关）
const POPUP_W: u16 = 320;
const POPUP_H: u16 = 360;
/// 单行项 / 双行项的实测行高
const ROW_1: f64 = 46.0;
const ROW_2: f64 = 74.0;
const POPUP_PAD: f64 = 8.0;
/// 单选圆圈中心 x、文字左边 x
const RADIO_X: f64 = 20.0;
const TEXT_X: f64 = 53.0;
/// 弹层/面板标题栏高度（实测：标题中心 y≈28，分隔线 y=56）
const HEAD_H: f64 = 56.0;
/// 设置面板实测 720x490（含 1px 边框），左栏 178，项高 45
const PANEL_W: u16 = 720;
const PANEL_H: u16 = 490;
const SIDE_W: f64 = 178.0;
const ITEM_H: f64 = 45.0;
const CONTENT_PAD: f64 = 26.0;
/// 中文正文字号（实测原画两字宽 30 → 15px）
const FONT_CN: f64 = 15.0;
/// 副文案字号
const FONT_SUB: f64 = 13.0;

const GROUPS: [&str; 9] = [
    "声道",
    "HDR/杜比",
    "图像",
    "解码",
    "解封装",
    "个性化",
    "快捷键",
    "循环播放",
    "定时播放",
];
const AUDIO_CH: [&str; 6] = ["自动", "立体声", "双声道", "4.0", "5.1", "7.1"];
const HDR_OPTS: &[(&str, &str)] = &[
    ("兼容模式", "未开启的视频文件模式最多，兼容性较好，适合大多数场景下使用"),
    ("HDR模式", "优化硬件HDR和杜比视界视频的亮度和色彩效果，但低性能电脑会出现黑屏、卡顿"),
    ("杜比模式", "如果想体验杜比视界视频以及HDR高亮度视频，请选用这种模式播放"),
];
const DECODE_OPTS: &[(&str, &str)] = &[
    ("硬解码", "利用电脑自身的硬件来进行解码。优点是效率高、功耗低，适合大部分视频文件"),
    ("软解码", "通过软件进行解码。优点是支持的视频格式多，对画质和音效的支持更好，但CPU占用较高"),
];
const DEMUX_OPTS: &[(&str, &str)] = &[
    ("默认解封装", "默认解封装方式支持格式相对较多、也相对稳定，适合大多数视频文件"),
    ("原生解封装", "系统自带解封装器，不支持大部分视频编码格式，无法切换音轨与字幕"),
];
const LOOP_OPTS: &[(&str, &str)] = &[
    ("顺序播放", "按剧集列表依次播放，播完停止"),
    ("单视频循环", "当前视频循环播放"),
    ("全部循环", "按剧集列表循环播放"),
    ("随机循环", "随机顺序播放列表中的视频"),
];
const SHORTCUTS: &[(&str, &str)] = &[
    ("关闭播放窗口", "Ctrl + X"),
    ("静音键", "Ctrl + M"),
    ("全屏 / 退出全屏", "Ctrl + Enter"),
    ("暂停 / 续播", "Space"),
    ("字幕加快 / 延迟0.5秒", "Shift + / Shift -"),
    ("老板键（隐藏窗口）", "Alt + D"),
    ("上一集 / 下一集", "PageUp / PageDown"),
];

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
#[allow(dead_code)]
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
    /// 设置面板左栏分组
    MenuGroup,
    /// 设置面板右栏行
    MenuRow,
    /// 字幕弹层头部的三个按钮
    SubSearch,
    SubAdd,
    SubAdjust,
}

struct Hit {
    rect: (i16, i16, i16, i16),
    btn: Btn,
    index: usize,
}

/// 弹层里的一项。`sub` 有值时是双行（行高 74），否则单行（46）。
#[derive(Clone)]
struct MenuItem {
    label: String,
    sub: Option<String>,
    selected: bool,
}

fn row_h(it: &MenuItem) -> f64 {
    if it.sub.is_some() {
        ROW_2
    } else {
        ROW_1
    }
}

/// 把 "主标题 - 副标题" / "主标题 · 副标题" 拆成两行（官方的双行弹层项）。
fn split_label(raw: &str) -> (String, Option<String>) {
    for sep in [" - ", " · "] {
        if let Some((a, b)) = raw.split_once(sep) {
            let sub = b.trim().trim_matches(|c| c == '[' || c == ']').to_string();
            if !a.trim().is_empty() && !sub.is_empty() {
                return (a.trim().to_string(), Some(sub));
            }
        }
    }
    (raw.to_string(), None)
}

struct MenuState {
    kind: MenuKind,
    items: Vec<MenuItem>,
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
    /// 设置面板当前分组（0..8）
    page: usize,
    /// 各分组里选中的行（声道页用 sel[0]，HDR 页用 sel[1]）
    sel: [usize; 9],
    /// 播放窗口高度（设置面板要在这个窗口里居中）
    pub window_height: u16,
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
            POPUP_W,
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
            page: 0,
            sel: [0; 9],
            window_height: 0,
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
            if x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                hovered = Some(if h.btn == Btn::MenuClose {
                    usize::MAX
                } else {
                    h.index
                });
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
        // Windows 版实测：左右边距 24，条在栏顶往下 5px，高 4px，
        // 轨迹圆角 2。静止时**不画滑块**，只有鼠标悬停进度条才浮出。
        let left = 24.0;
        let right = w as f64 - 24.0;
        let y = 5.0;
        let frac = if snap.len_ms > 0 {
            (snap.pos_ms as f64 / snap.len_ms as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        ctx.new_path();
        style::rounded_rect(ctx, left, y, right - left, 4.0, 2.0);
        ctx.set_source_rgba(
            style::TRACK_ADD.0,
            style::TRACK_ADD.1,
            style::TRACK_ADD.2,
            style::TRACK_ADD.3,
        );
        let _ = ctx.fill();
        if frac > 0.0 {
            ctx.new_path();
            style::rounded_rect(ctx, left, y, (right - left) * frac, 4.0, 2.0);
            ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            let _ = ctx.fill();
        }
        // hover 时才画一个 12px 的白点，跟官方一致（静态帧里没有滑块）
        if self.hover == Some(Btn::Seek) {
            let cx = left + (right - left) * frac;
            ctx.new_path();
            let _ = ctx.arc(cx, y + 2.0, 6.0, 0.0, std::f64::consts::TAU);
            ctx.set_source_rgb(1.0, 1.0, 1.0);
            let _ = ctx.fill();
        }
        self.hits.push(Hit {
            rect: (left as i16, (y - 6.0) as i16, (right - left) as i16, 16),
            btn: Btn::Seek,
            index: 0,
        });
    }

    fn draw_buttons(&mut self, ctx: &Context, snap: &Snapshot, w: i32, h: i32) {
        // 布局按 Windows 版实测：按钮位 56px、图标 24px、行中心距底 32px、
        // 左起第一个按钮中心距左边 36px（即 slot 从 8 开始）
        // 按钮行中心距栏顶 35px（实测 787..808 的中点 ≈ 797，栏顶 762）
        let cy = h as f64 - 31.0;
        let mut x = 8.0;

        let (play_k, play_h) = if snap.playing {
            ("pause", "pause_hover")
        } else {
            ("play", "play_hover")
        };
        self.icon_btn(ctx, &mut x, cy, Btn::PlayPause, play_k, play_h, None, true);
        self.icon_btn(ctx, &mut x, cy, Btn::Rewind, "rewind", "rewind_hover", None, true);
        self.icon_btn(ctx, &mut x, cy, Btn::Forward, "forward", "forward_hover", None, true);

        // 时间：官方 `HH:MM:SS / HH:MM:SS`，14px，紧跟快进按钮
        let time = format!("{} / {}", fmt_time(snap.pos_ms), fmt_time(snap.len_ms));
        x += 9.0;
        self.text_a(ctx, &time, x, cy + FONT_SMALL / 2.8, FONT_SMALL, FG);
        x += self.text_width(ctx, &time, FONT_SMALL) + 16.0;
        // 右簇（从右往左）：全屏 · 音量 · 设置 · 音轨 · 选集 · 原画 · 倍速
        // 官方控制栏没有关闭、没有字幕按钮、没有「⋯」，齿轮就是设置入口
        let mut rx = w as f64;
        let (fs_k, fs_h) = if snap.fullscreen {
            ("exit_full", "exit_full_hover")
        } else {
            ("full", "full_hover")
        };
        rx = self.icon_right(ctx, rx, cy, Btn::Fullscreen, fs_k, fs_h, None, true);
        let (vol_k, vol_h) = if snap.muted {
            ("mute", "mute_hover")
        } else {
            ("volume", "volume_hover")
        };
        let vol_slot = rx - BTN_SLOT;
        rx = self.icon_right(ctx, rx, cy, Btn::Volume, vol_k, vol_h, None, true);
        rx = self.icon_right(ctx, rx, cy, Btn::More, "more", "more_hover", None, true);
        rx = self.icon_right(ctx, rx, cy, Btn::Audio, "audio", "audio_hover", None, true);
        // 实测顺序（右→左）：全屏 · 音量 · 齿轮(设置) · 音轨 · 字幕 · 清晰度 · 倍速
        rx = self.icon_right(ctx, rx, cy, Btn::Subtitle, "subtitle", "subtitle_hover", None, true);
        rx = self.text_btn_right(ctx, rx, cy, Btn::Quality, &label_or(&snap.quality, "原画"));
        rx = self.text_btn_right(ctx, rx, cy, Btn::Rate, &fmt_rate(snap.rate));
        let _ = rx;

        // 音量滑杆默认不占位（官方平时看不到），鼠标悬停喇叭时才浮出来
        if self.hover == Some(Btn::Volume) {
            let vw = 76.0;
            let vx = vol_slot + BTN_SLOT / 2.0 + ICON_SIZE / 2.0 + 8.0;
            let vfrac = (snap.volume.clamp(0, 100) as f64 / 100.0).clamp(0.0, 1.0);
            ctx.set_source_rgba(0.0, 0.0, 0.0, 0.55);
            ctx.rectangle(vx - 6.0, cy - 12.0, vw + 12.0, 24.0);
            let _ = ctx.fill();
            ctx.new_path();
            style::rounded_rect(ctx, vx, cy - 2.0, vw, 4.0, 2.0);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.15);
            let _ = ctx.fill();
            if vfrac > 0.0 {
                ctx.new_path();
                style::rounded_rect(ctx, vx, cy - 2.0, vw * vfrac, 4.0, 2.0);
                ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
                let _ = ctx.fill();
            }
            self.hits.push(Hit {
                rect: ((vx - 6.0) as i16, (cy - 12.0) as i16, (vw + 12.0) as i16, 24),
                btn: Btn::VolumeSlider,
                index: 0,
            });
        }
    }

    fn text_a(&self, ctx: &Context, s: &str, x: f64, y: f64, size: f64, color: (f64, f64, f64, f64)) {
        ctx.select_font_face("Noto Sans CJK SC", FontSlant::Normal, FontWeight::Normal);
        ctx.set_font_size(size);
        ctx.set_source_rgba(color.0, color.1, color.2, color.3);
        ctx.move_to(x, y);
        let _ = ctx.show_text(s);
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
        let cx = *x + BTN_SLOT / 2.0;
        if enabled {
            let key = if self.hover == Some(btn) { hover } else { normal };
            self.icons.draw_center(ctx, key, cx, cy, ICON_SIZE);
        } else if let Some(off) = disabled {
            self.icons.draw_center(ctx, off, cx, cy, ICON_SIZE);
        }
        self.hits.push(Hit {
            rect: (*x as i16, (cy - BTN_SLOT / 2.0) as i16, BTN_SLOT as i16, BTN_SLOT as i16),
            btn,
            index: 0,
        });
        *x += BTN_SLOT;
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
    ) -> f64 {
        rx -= BTN_SLOT;
        let cx = rx + BTN_SLOT / 2.0;
        if enabled {
            let key = if self.hover == Some(btn) { hover } else { normal };
            self.icons.draw_center(ctx, key, cx, cy, ICON_SIZE);
        } else if let Some(off) = disabled {
            self.icons.draw_center(ctx, off, cx, cy, ICON_SIZE);
        }
        self.hits.push(Hit {
            rect: (rx as i16, (cy - BTN_SLOT / 2.0) as i16, BTN_SLOT as i16, BTN_SLOT as i16),
            btn,
            index: 0,
        });
        rx
    }

    /// 右对齐的纯文字按钮（官方的倍速、清晰度都是这种：平时无底色，hover 才有淡块）
    fn text_btn_right(&mut self, ctx: &Context, rx: f64, cy: f64, btn: Btn, label: &str) -> f64 {
        let tw = self.text_width(ctx, label, FONT_CN);
        let bw = (tw + 32.0).max(BTN_SLOT);
        let x = rx - bw;
        let hovered = self.hover == Some(btn);
        if hovered {
            ctx.new_path();
            style::rounded_rect(ctx, x + 2.0, cy - 18.0, bw - 4.0, 36.0, 8.0);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
            let _ = ctx.fill();
        }
        let color = if hovered { (1.0, 1.0, 1.0, 1.0) } else { FG };
        self.text_a(ctx, label, x + (bw - tw) / 2.0, cy + FONT_CN / 2.8, FONT_CN, color);
        self.hits.push(Hit {
            rect: (x as i16, (cy - 18.0) as i16, bw as i16, 36),
            btn,
            index: 0,
        });
        x
    }

    // ------------------------------------------------------------------ menu

    pub fn open_menu(&mut self, conn: &RustConnection, kind: MenuKind, snap: &Snapshot) {
        let items = build_items(kind, snap);
        let (mw, mh) = if kind == MenuKind::More {
            (PANEL_W, PANEL_H)
        } else {
            (POPUP_W, POPUP_H)
        };
        // 官方：小弹层底边贴控制栏顶、水平中心对准触发按钮中心；
        // 设置面板在窗口内居中。
        let (x, y) = if kind == MenuKind::More {
            let wh = self.window_height.max(self.height);
            (
                self.abs_x + (self.width as i16 - mw as i16) / 2,
                self.abs_y + self.height as i16 - wh as i16 + (wh as i16 - mh as i16) / 2,
            )
        } else {
            (
                (self.abs_x + self.anchor_x - mw as i16 / 2)
                    .clamp(self.abs_x, self.abs_x + self.width as i16 - mw as i16),
                self.abs_y - mh as i16,
            )
        };
        let _ = conn.configure_window(
            self.menu_win,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(mw as u32)
                .height(mh as u32),
        );
        self.menu_hover = None;
        self.page = 0;
        self.menu = Some(MenuState { kind, items, w: mw, h: mh });
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

            if kind == MenuKind::More {
                let snap = self.last.clone().unwrap_or_default();
                self.draw_config(&ctx, &snap, mw, mh);
            } else {
                let items = self.menu.as_ref().map(|m| m.items.clone()).unwrap_or_default();
                let mut y = POPUP_PAD;
                if kind == MenuKind::Subtitle {
                    y = self.draw_sub_header(&ctx, mw);
                }
                for (i, it) in items.iter().enumerate() {
                    let h = row_h(it);
                    let hovered = self.menu_hover == Some(i);
                    if hovered {
                        ctx.new_path();
                        style::rounded_rect(&ctx, 6.0, y + 2.0, mw as f64 - 12.0, h - 4.0, 6.0);
                        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.06);
                        let _ = ctx.fill();
                    }
                    let main_cy = if it.sub.is_some() { y + 23.5 } else { y + h / 2.0 };
                    radio(&ctx, RADIO_X, main_cy, it.selected);
                    let color = if it.selected || hovered {
                        (1.0, 1.0, 1.0, 1.0)
                    } else {
                        (0.91, 0.91, 0.91, 1.0)
                    };
                    let mut t = it.label.clone();
                    while self.text_width(&ctx, &t, FONT_CN) > mw as f64 - TEXT_X - 16.0
                        && t.chars().count() > 2
                    {
                        let cut = t
                            .char_indices()
                            .nth(t.chars().count().saturating_sub(2))
                            .map(|(c, _)| c)
                            .unwrap_or(0);
                        t.truncate(cut);
                        t.push('…');
                    }
                    self.text_a(&ctx, &t, TEXT_X, main_cy + FONT_CN / 2.8, FONT_CN, color);
                    if let Some(sub) = it.sub.as_ref() {
                        self.text_a(
                            &ctx,
                            sub,
                            TEXT_X,
                            y + 50.5 + FONT_SUB / 2.8,
                            FONT_SUB,
                            style::SUB_FG,
                        );
                    }
                    self.menu_hits.push(Hit {
                        rect: (6, y as i16, (mw - 12) as i16, h as i16),
                        btn: Btn::MenuItem,
                        index: i,
                    });
                    y += h;
                }
            }
        }
        let (w, h) = match self.menu.as_ref() {
            Some(m) => (m.w, m.h),
            None => return,
        };
        self.blit(conn, &mut surface, self.menu_win, w, h);
    }

    /// 字幕弹层头部：`搜索` 药丸 + `+ 添加` + `调整`，下方 1px 分隔线。
    /// 返回列表起始 y。
    fn draw_sub_header(&mut self, ctx: &Context, mw: i32) -> f64 {
        let (pw, ph, py) = (96.0, 30.0, 13.0);
        ctx.new_path();
        style::rounded_rect(ctx, 20.0, py, pw, ph, 15.0);
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.14);
        let _ = ctx.fill();
        self.text_a(ctx, "搜索", 50.0, py + ph / 2.0 + FONT_CN / 2.8, FONT_CN, (1.0, 1.0, 1.0, 1.0));
        self.text_a(ctx, "+ 添加", 136.0, py + ph / 2.0 + FONT_CN / 2.8, FONT_CN, style::LABEL_HI);
        self.text_a(ctx, "调整", 232.0, py + ph / 2.0 + FONT_CN / 2.8, FONT_CN, style::LABEL_HI);
        self.menu_hits.push(Hit { rect: (20, py as i16, pw as i16, ph as i16), btn: Btn::SubSearch, index: 0 });
        self.menu_hits.push(Hit { rect: (130, py as i16, 66, ph as i16), btn: Btn::SubAdd, index: 0 });
        self.menu_hits.push(Hit { rect: (226, py as i16, 60, ph as i16), btn: Btn::SubAdjust, index: 0 });
        ctx.set_source_rgba(
            style::DIVIDER.0,
            style::DIVIDER.1,
            style::DIVIDER.2,
            style::DIVIDER.3,
        );
        ctx.rectangle(0.0, HEAD_H, mw as f64, 1.0);
        let _ = ctx.fill();
        HEAD_H + POPUP_PAD
    }

    /// 官方设置面板：720×490，左栏 178 九分组 + 右栏内容。
    fn draw_config(&mut self, ctx: &Context, snap: &Snapshot, mw: i32, mh: i32) {
        let pw = mw as f64;
        // 标题栏
        self.text_a(ctx, "播放设置", 25.0, HEAD_H / 2.0 + FONT_CN / 2.8, FONT_CN, style::LABEL_HI);
        ctx.set_source_rgba(
            style::DIVIDER.0,
            style::DIVIDER.1,
            style::DIVIDER.2,
            style::DIVIDER.3,
        );
        ctx.rectangle(0.0, HEAD_H, pw, 1.0);
        let _ = ctx.fill();
        let close_hover = self.menu_hover == Some(usize::MAX);
        let (cx, cy) = (pw - 30.0, HEAD_H / 2.0);
        if close_hover {
            ctx.new_path();
            let _ = ctx.arc(cx, cy, 12.0, 0.0, std::f64::consts::TAU);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
            let _ = ctx.fill();
        }
        ctx.new_path();
        ctx.set_line_width(1.6);
        ctx.set_source_rgba(1.0, 1.0, 1.0, if close_hover { 1.0 } else { 0.78 });
        ctx.move_to(cx - 6.0, cy - 6.0);
        ctx.line_to(cx + 6.0, cy + 6.0);
        ctx.move_to(cx + 6.0, cy - 6.0);
        ctx.line_to(cx - 6.0, cy + 6.0);
        let _ = ctx.stroke();
        self.menu_hits.push(Hit {
            rect: ((pw - 46.0) as i16, 10, 32, (HEAD_H - 20.0) as i16),
            btn: Btn::MenuClose,
            index: 0,
        });

        // 左栏
        let mut y = 95.5;
        for (i, name) in GROUPS.iter().enumerate() {
            let selected = i == self.page;
            if selected {
                ctx.new_path();
                style::rounded_rect(ctx, 12.0, y - 24.0, SIDE_W - 25.0, 48.0, 6.0);
                ctx.set_source_rgba(ACCENT.0, ACCENT.1, ACCENT.2, 0.16);
                let _ = ctx.fill();
            }
            let color = if selected {
                (0.44, 0.55, 1.0, 1.0)
            } else {
                (0.91, 0.91, 0.91, 1.0)
            };
            self.text_a(ctx, name, 25.0, y + FONT_CN / 2.8, FONT_CN, color);
            self.menu_hits.push(Hit {
                rect: (8, (y - 22.0) as i16, (SIDE_W - 16.0) as i16, 44),
                btn: Btn::MenuGroup,
                index: i,
            });
            y += ITEM_H;
        }
        ctx.set_source_rgba(
            style::DIVIDER.0,
            style::DIVIDER.1,
            style::DIVIDER.2,
            style::DIVIDER.3,
        );
        ctx.rectangle(SIDE_W, HEAD_H, 1.0, mh as f64 - HEAD_H);
        let _ = ctx.fill();

        // 右栏标题 + 分隔线
        self.text_a(
            ctx,
            GROUPS[self.page.min(GROUPS.len() - 1)],
            SIDE_W + CONTENT_PAD,
            95.0 + FONT_CN / 2.8,
            FONT_CN,
            (1.0, 1.0, 1.0, 1.0),
        );
        ctx.rectangle(SIDE_W + 1.0, 117.0, pw - SIDE_W - 2.0, 1.0);
        let _ = ctx.fill();

        match self.page {
            0 => self.page_radios(ctx, &AUDIO_CH, 155.0, pw),
            1 => self.page_desc_radios(ctx, HDR_OPTS, self.sel[1], pw),
            2 => self.page_sliders(ctx, pw, mh),
            3 => self.page_desc_radios(ctx, DECODE_OPTS, 0, pw),
            4 => self.page_desc_radios(ctx, DEMUX_OPTS, 0, pw),
            5 => self.page_steppers(ctx),
            6 => self.page_shortcuts(ctx, pw),
            7 => {
                let idx = if snap.looping { 1 } else { 0 };
                self.page_desc_radios(ctx, LOOP_OPTS, idx, pw)
            }
            _ => self.page_timer(ctx, snap, pw),
        }
    }

    /// 单选列表（无副文案）：首行中心 y0，行距 52。
    fn page_radios(&mut self, ctx: &Context, items: &[&str], y0: f64, pw: f64) {
        for (i, name) in items.iter().enumerate() {
            let cy = y0 + i as f64 * 52.0;
            let selected = i == self.sel[0];
            radio(ctx, SIDE_W + 39.0, cy, selected);
            self.text_a(
                ctx,
                name,
                SIDE_W + 63.0,
                cy + FONT_CN / 2.8,
                FONT_CN,
                if selected { (1.0, 1.0, 1.0, 1.0) } else { (0.88, 0.88, 0.88, 1.0) },
            );
            self.menu_hits.push(Hit {
                rect: ((SIDE_W + 20.0) as i16, (cy - 22.0) as i16, (pw - SIDE_W - 50.0) as i16, 44),
                btn: Btn::MenuRow,
                index: 1000 + i,
            });
        }
    }

    /// 单选 + 说明文字（HDR / 解码 / 解封装 / 循环播放）。
    fn page_desc_radios(&mut self, ctx: &Context, items: &[(&str, &str)], sel: usize, pw: f64) {
        let mut y = 132.0;
        for (i, (label, desc)) in items.iter().enumerate() {
            let selected = i == sel;
            radio(ctx, SIDE_W + 39.0, y, selected);
            self.text_a(
                ctx,
                label,
                SIDE_W + 63.0,
                y + FONT_CN / 2.8,
                FONT_CN,
                if selected { (1.0, 1.0, 1.0, 1.0) } else { (0.88, 0.88, 0.88, 1.0) },
            );
            let lines = wrap(ctx, desc, pw - SIDE_W - CONTENT_PAD - 40.0, FONT_SUB);
            for (li, line) in lines.iter().take(2).enumerate() {
                self.text_a(
                    ctx,
                    line,
                    SIDE_W + 63.0,
                    y + 24.0 + li as f64 * 19.0 + FONT_SUB / 2.8,
                    FONT_SUB,
                    style::SUB_FG,
                );
            }
            let h = 44.0 + lines.len().min(2) as f64 * 19.0;
            self.menu_hits.push(Hit {
                rect: ((SIDE_W + 20.0) as i16, (y - 22.0) as i16, (pw - SIDE_W - 50.0) as i16, h as i16),
                btn: Btn::MenuRow,
                index: 1000 + i,
            });
            y += h + 12.0;
        }
    }

    fn page_sliders(&mut self, ctx: &Context, pw: f64, mh: i32) {
        let names = ["亮度", "对比度", "色调", "饱和度"];
        let mut y = 160.0;
        for name in names.iter() {
            self.text_a(ctx, name, SIDE_W + 26.0, y + FONT_CN / 2.8, FONT_CN, (0.9, 0.9, 0.9, 1.0));
            let (x0, x1) = (SIDE_W + 120.0, pw - 60.0);
            ctx.new_path();
            style::rounded_rect(ctx, x0, y - 2.0, x1 - x0, 4.0, 2.0);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.15);
            let _ = ctx.fill();
            // 值为 0 时滑块在正中（官方默认 0）
            let cx = x0 + (x1 - x0) / 2.0;
            ctx.new_path();
            let _ = ctx.arc(cx, y, 7.0, 0.0, std::f64::consts::TAU);
            ctx.set_source_rgb(1.0, 1.0, 1.0);
            let _ = ctx.fill();
            self.text_a(ctx, "0", x1 + 14.0, y + FONT_CN / 2.8, FONT_CN, (0.85, 0.85, 0.85, 1.0));
            y += 46.0;
        }
        let (bw, bh) = (76.0, 30.0);
        let bx = pw - 24.0 - bw;
        let by = mh as f64 - 30.0 - bh;
        ctx.new_path();
        style::rounded_rect(ctx, bx, by, bw, bh, 6.0);
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
        let _ = ctx.fill();
        self.text_a(ctx, "重置", bx + 26.0, by + bh / 2.0 + FONT_CN / 2.8, FONT_CN, (0.95, 0.95, 0.95, 1.0));
    }

    fn page_steppers(&mut self, ctx: &Context) {
        let rows = [
            ("左右箭头快进快退", "10s"),
            ("上下箭头音量调整", "5%"),
            ("ctrl+加减号缩放窗体", "10%"),
        ];
        let mut y = 150.0;
        for (name, val) in rows.iter() {
            self.text_a(ctx, name, SIDE_W + 26.0, y + FONT_CN / 2.8, FONT_CN, (0.9, 0.9, 0.9, 1.0));
            let bx = 470.0;
            let (bw, bh) = (92.0, 30.0);
            ctx.new_path();
            style::rounded_rect(ctx, bx, y - bh / 2.0, bw, bh, 6.0);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
            let _ = ctx.fill();
            self.text_a(ctx, val, bx + 40.0, y + FONT_CN / 2.8, FONT_CN, (0.95, 0.95, 0.95, 1.0));
            for (k, sign) in ["−", "+"].iter().enumerate() {
                let mx = bx + bw + 12.0 + k as f64 * 34.0;
                ctx.new_path();
                style::rounded_rect(ctx, mx, y - bh / 2.0, 30.0, bh, 6.0);
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
                let _ = ctx.fill();
                self.text_a(ctx, sign, mx + 12.0, y + FONT_CN / 2.8, FONT_CN, (0.95, 0.95, 0.95, 1.0));
            }
            y += 54.0;
        }
    }

    fn page_shortcuts(&mut self, ctx: &Context, pw: f64) {
        let rows = SHORTCUTS;
        let mut y = 138.0;
        self.text_a(ctx, "更多快捷键指南", SIDE_W + 26.0, y + FONT_CN / 2.8, FONT_CN, style::LABEL_HI);
        y += 44.0;
        for (name, key) in rows.iter() {
            self.text_a(ctx, name, SIDE_W + 26.0, y + FONT_CN / 2.8, FONT_CN, (0.88, 0.88, 0.88, 1.0));
            self.text_a(ctx, key, 470.0, y + FONT_CN / 2.8, FONT_CN, style::SUB_FG);
            let (tx, ty, tw, th) = (pw - 54.0, y - 10.0, 34.0, 20.0);
            ctx.new_path();
            style::rounded_rect(ctx, tx, ty, tw, th, 10.0);
            ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            let _ = ctx.fill();
            ctx.new_path();
            let _ = ctx.arc(tx + tw - 10.0, ty + th / 2.0, 7.0, 0.0, std::f64::consts::TAU);
            ctx.set_source_rgb(1.0, 1.0, 1.0);
            let _ = ctx.fill();
            y += 38.0;
        }
    }

    fn page_timer(&mut self, ctx: &Context, snap: &Snapshot, pw: f64) {
        let by_ep: [&str; 4] = ["播放完本集", "播放2集", "播放3集", "播放5集"];
        let by_min: [&str; 4] = ["10分钟", "15分钟", "30分钟", "60分钟"];
        self.text_a(ctx, "按集数", SIDE_W + 26.0, 150.0, FONT_CN, style::LABEL_HI);
        for (i, name) in by_ep.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let cx = SIDE_W + 39.0 + col as f64 * 250.0;
            let cy = 186.0 + row as f64 * 50.0;
            radio(ctx, cx, cy, false);
            self.text_a(ctx, name, cx + 24.0, cy + FONT_CN / 2.8, FONT_CN, (0.88, 0.88, 0.88, 1.0));
            self.menu_hits.push(Hit {
                rect: ((cx - 16.0) as i16, (cy - 20.0) as i16, 220, 40),
                btn: Btn::MenuRow,
                index: 3000 + i,
            });
        }
        self.text_a(ctx, "按时长", SIDE_W + 26.0, 300.0, FONT_CN, style::LABEL_HI);
        for (i, name) in by_min.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let cx = SIDE_W + 39.0 + col as f64 * 250.0;
            let cy = 336.0 + row as f64 * 50.0;
            let mins = [10u32, 15, 30, 60][i];
            let selected = snap.sleep_min == Some(mins);
            radio(ctx, cx, cy, selected);
            self.text_a(ctx, name, cx + 24.0, cy + FONT_CN / 2.8, FONT_CN, (0.88, 0.88, 0.88, 1.0));
            self.menu_hits.push(Hit {
                rect: ((cx - 16.0) as i16, (cy - 20.0) as i16, 220, 40),
                btn: Btn::MenuRow,
                index: 3100 + i,
            });
        }
        let _ = pw;
    }

    /// 弹层/面板里的点击。小弹层选完即关；设置面板保持打开（除关闭叉）。
    pub fn on_menu_click(&mut self, conn: &RustConnection, x: i16, y: i16) -> Option<Command> {
        let mut hit = None;
        if let Some(menu) = self.menu.as_ref() {
            for h in &self.menu_hits {
                let (hx, hy, hw, hh) = h.rect;
                if x >= hx && x <= hx + hw && y >= hy && y <= hy + hh {
                    hit = Some((menu.kind, h.btn, h.index));
                    break;
                }
            }
        }
        let Some((kind, btn, idx)) = hit else {
            self.close_menu(conn);
            return None;
        };
        if kind == MenuKind::More {
            match btn {
                Btn::MenuClose => self.close_menu(conn),
                Btn::MenuGroup => {
                    self.page = idx;
                    self.draw_menu(conn);
                }
                Btn::MenuRow => {
                    // 3000+ 按集数（暂无实现）、3100+ 按时长（接睡眠定时）
                    if idx >= 3100 {
                        let mins = [10u32, 15, 30, 60][idx - 3100];
                        self.draw_menu(conn);
                        return Some(Command::SetSleepTimer(mins));
                    }
                    if idx >= 3000 {
                        self.draw_menu(conn);
                        return None;
                    }
                    let row = idx - 1000;
                    self.sel[self.page] = row;
                    let cmd = if self.page == 7 {
                        // 顺序播放=关；单视频/全部/随机都先按「循环开」处理
                        Some(Command::SetLoop(row != 0))
                    } else {
                        None
                    };
                    self.draw_menu(conn);
                    return cmd;
                }
                _ => self.close_menu(conn),
            }
            return None;
        }
        if matches!(btn, Btn::SubSearch | Btn::SubAdd | Btn::SubAdjust) {
            // 在线搜索 / 添加本地字幕 / 字幕样式面板还没做，先只关闭并留日志
            eprintln!("[bar] 字幕弹层头部按钮未实现：{btn:?}");
            self.close_menu(conn);
            return None;
        }
        let cmd = match (kind, btn) {
            (MenuKind::Speed, Btn::MenuItem) => {
                RATES.iter().rev().nth(idx).map(|r| Command::SetRate(*r))
            }
            (MenuKind::Quality, _) => Some(Command::SelectQuality(idx)),
            (MenuKind::Audio, _) => Some(Command::SelectAudio(idx)),
            (MenuKind::Subtitle, _) => Some(Command::SelectSubtitle(idx as i64)),
            (MenuKind::Episode, _) => Some(Command::PlayEpisode(idx)),
            _ => None,
        };
        self.close_menu(conn);
        cmd
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
        Btn::Forward => Command::SeekBy(30.0),
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
    // 官方时间轴是 `00:25:33 / 01:40:50`，小时位即使是 0 也带前导零
    format!("{h:02}:{m:02}:{s:02}")
}

// ------------------------------------------------------------------ helpers

fn build_items(kind: MenuKind, snap: &Snapshot) -> Vec<MenuItem> {
    let conv = |v: &Vec<(String, bool)>| -> Vec<MenuItem> {
        v.iter()
            .map(|(l, sel)| {
                let (label, sub) = split_label(l);
                MenuItem { label, sub, selected: *sel }
            })
            .collect()
    };
    match kind {
        // 官方倍速列表是降序：2.0x 在最上面
        MenuKind::Speed => RATES
            .iter()
            .rev()
            .map(|r| MenuItem {
                label: fmt_rate(*r),
                sub: None,
                selected: (snap.rate - r).abs() < 0.01,
            })
            .collect(),
        MenuKind::Quality => conv(&snap.qualities),
        MenuKind::Audio => conv(&snap.audios),
        MenuKind::Subtitle => conv(&snap.subtitles),
        MenuKind::Episode => conv(&snap.episodes),
        MenuKind::More => Vec::new(),
    }
}

/// 官方单选圆圈：直径 16，选中 = 主色实心 + 白点，未选 = 1.4px 空心圈。
fn radio(ctx: &Context, cx: f64, cy: f64, selected: bool) {
    ctx.new_path();
    let _ = ctx.arc(cx, cy, 8.0, 0.0, std::f64::consts::TAU);
    if selected {
        ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
        let _ = ctx.fill();
        ctx.new_path();
        let _ = ctx.arc(cx, cy, 3.2, 0.0, std::f64::consts::TAU);
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        let _ = ctx.fill();
    } else {
        ctx.set_line_width(1.4);
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.35);
        let _ = ctx.stroke();
    }
}

/// 按像素宽度折行。
fn wrap(ctx: &Context, text: &str, max_w: f64, size: f64) -> Vec<String> {
    let _ = size;
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        let w = ctx.text_extents(&cur).map(|e| e.x_advance()).unwrap_or(0.0);
        if w > max_w {
            cur.pop();
            if let Some(pos) = cur.rfind(' ') {
                if pos > 6 {
                    let rest = cur.split_off(pos + 1);
                    out.push(cur.clone());
                    cur = rest;
                    cur.push(ch);
                    continue;
                }
            }
            out.push(cur.clone());
            cur.clear();
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}
