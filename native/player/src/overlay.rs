//! 覆盖层：结束页 / 错误页（整块暗底）与加载卡片 / 片头片尾浮层（小卡片）。
//!
//! 和 `bar.rs` 一样走 cairo + X11 override-redirect 窗口，不引入 UI 工具包。
//! 窗口本身没有 ARGB visual（根窗口 depth 24），所以这里不用半透明蒙版：
//! 结束/错误用不透明暗底盖住画面（官方同样是暗底结束页），
//! 加载与跳过提示用小尺寸卡片，只占画面中间/一角，不遮住整幅视频。

use crate::{Command, Snapshot};
use cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, Gcontext, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

const ACCENT: (f64, f64, f64) = (0.29, 0.56, 1.0);
const FG: (f64, f64, f64) = (0.92, 0.92, 0.94);
const FG_DIM: (f64, f64, f64) = (0.62, 0.62, 0.66);
const BG: (f64, f64, f64) = (0.06, 0.06, 0.07);

const CARD_W: u16 = 152;
const CARD_H: u16 = 44;
const SKIP_W: u16 = 136;
const SKIP_H: u16 = 34;

/// 覆盖层当前该画什么。内容变化才重绘，避免每帧 put_image。
#[derive(Clone, PartialEq)]
enum Mode {
    Hidden,
    Ended { has_next: bool },
    Error(String),
    Loading,
    Skip { label: String },
}

struct Hit {
    rect: (i16, i16, u16, u16),
    cmd: Command,
}

pub struct Overlay {
    /// 整块画面区域：结束页 / 错误页。
    big: Window,
    /// 小卡片：加载提示 / 跳过片头片尾。
    small: Window,
    gc: Gcontext,
    depth: u8,
    /// 画面区域几何（不含控制栏）。
    x: i16,
    y: i16,
    w: u16,
    h: u16,
    big_mode: Option<Mode>,
    small_mode: Option<Mode>,
    hits: Vec<Hit>,
    small_rect: (i16, i16, u16, u16),
    skip_target: f64,
}

impl Overlay {
    pub fn new(
        conn: &RustConnection,
        depth: u8,
        x: i16,
        y: i16,
        w: u16,
        h: u16,
    ) -> Result<Self, String> {
        let mask = EventMask::EXPOSURE | EventMask::BUTTON_PRESS;
        let mk = |name: &str| -> Result<Window, String> {
            let win = conn.generate_id().map_err(|e| e.to_string())?;
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
                w,
                h,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &aux,
            )
            .map_err(|e| format!("创建{name}失败: {e}"))?;
            Ok(win)
        };
        let big = mk("结束页窗口")?;
        let small = mk("提示窗口")?;
        let gc = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_gc(gc, big, &CreateGCAux::new())
            .map_err(|e| e.to_string())?;
        let _ = conn.flush();
        Ok(Overlay {
            big,
            small,
            gc,
            depth,
            x,
            y,
            w,
            h,
            big_mode: None,
            small_mode: None,
            hits: Vec::new(),
            small_rect: (x, y, CARD_W, CARD_H),
            skip_target: 0.0,
        })
    }

    pub fn big_id(&self) -> Window {
        self.big
    }

    pub fn small_id(&self) -> Window {
        self.small
    }

    /// 画面区域变化时重新摆放（只盖视频区域，底部控制栏单独一条，互不重叠）。
    pub fn place(&mut self, conn: &RustConnection, x: i16, y: i16, w: u16, h: u16) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
        let _ = conn.configure_window(
            self.big,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(w as u32)
                .height(h as u32),
        );
        // 小卡片位置取决于内容，交给下一次 sync 重排。
        self.big_mode = None;
        self.small_mode = None;
        let _ = conn.flush();
    }

    /// 按状态显示/隐藏并重绘。返回本轮是否有窗口被新映射（调用方据此重排层级）。
    pub fn sync(&mut self, conn: &RustConnection, snap: &Snapshot) -> bool {
        let (big, small) = decide(snap);
        if let Some((_, target)) = &snap.skip {
            self.skip_target = *target;
        }
        self.apply(conn, big, small)
    }

    /// 覆盖层上的点击。返回要执行的命令。
    pub fn on_click(&mut self, win: Window, x: i16, y: i16) -> Option<Command> {
        if win == self.small {
            if matches!(self.small_mode, Some(Mode::Skip { .. })) {
                return Some(Command::SeekTo(self.skip_target));
            }
            return None;
        }
        if win != self.big {
            return None;
        }
        for h in &self.hits {
            let (hx, hy, hw, hh) = h.rect;
            if x >= hx && x <= hx + hw as i16 && y >= hy && y <= hy + hh as i16 {
                return Some(h.cmd.clone());
            }
        }
        None
    }

    fn apply(&mut self, conn: &RustConnection, big_mode: Mode, small_mode: Mode) -> bool {
        let mut mapped = false;
        if self.big_mode.as_ref() != Some(&big_mode) {
            crate::log(format!("覆盖层: {}", describe(&big_mode)));
            match &big_mode {
                Mode::Hidden => {
                    let _ = conn.unmap_window(self.big);
                }
                _ => {
                    let _ = conn.map_window(self.big);
                    raise(conn, self.big);
                    self.draw_big(conn, &big_mode);
                    mapped = true;
                }
            }
            self.big_mode = Some(big_mode);
        }

        if self.small_mode.as_ref() != Some(&small_mode) {
            crate::log(format!("提示卡片: {}", describe(&small_mode)));
            match &small_mode {
                Mode::Hidden => {
                    let _ = conn.unmap_window(self.small);
                }
                Mode::Loading => {
                    let cx = self.x + (self.w as i16 - CARD_W as i16) / 2;
                    let cy = self.y + (self.h as i16 - CARD_H as i16) / 2;
                    self.resize_small(conn, cx, cy, CARD_W, CARD_H);
                    self.draw_loading(conn);
                    let _ = conn.map_window(self.small);
                    raise(conn, self.small);
                    mapped = true;
                }
                Mode::Skip { label } => {
                    let label = label.clone();
                    let cx = self.x + self.w as i16 - SKIP_W as i16 - 24;
                    let cy = self.y + self.h as i16 - SKIP_H as i16 - 18;
                    self.resize_small(conn, cx, cy, SKIP_W, SKIP_H);
                    self.draw_skip(conn, &label);
                    let _ = conn.map_window(self.small);
                    raise(conn, self.small);
                    mapped = true;
                }
                _ => {}
            }
            self.small_mode = Some(small_mode);
        }
        let _ = conn.flush();
        mapped
    }

    fn resize_small(&mut self, conn: &RustConnection, x: i16, y: i16, w: u16, h: u16) {
        self.small_rect = (x, y, w, h);
        let _ = conn.configure_window(
            self.small,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(w as u32)
                .height(h as u32),
        );
    }

    // ------------------------------------------------------------------ 绘制

    fn draw_big(&mut self, conn: &RustConnection, mode: &Mode) {
        self.hits.clear();
        let w = self.w as i32;
        let h = self.h as i32;
        let mut surface = match ImageSurface::create(Format::ARgb32, w, h) {
            Ok(s) => s,
            Err(_) => return,
        };
        {
            let ctx = Context::new(&surface).expect("cairo context");
            ctx.set_source_rgb(BG.0, BG.1, BG.2);
            ctx.paint().ok();
            let (title, sub): (&str, String) = match mode {
                Mode::Ended { has_next } => (
                    "播放结束",
                    if *has_next { "接着看下一集".to_string() } else { "这是最后一集".to_string() },
                ),
                Mode::Error(reason) => ("播放失败", reason.clone()),
                _ => ("", String::new()),
            };
            let title_size = (h as f64 / 18.0).clamp(20.0, 30.0);
            centered(&ctx, title, w, (h as f64 / 2.0) - title_size * 0.9, title_size, FG);
            let sub = clip(&ctx, &sub, w as f64 - 160.0, 13.0);
            centered(&ctx, &sub, w, (h as f64 / 2.0) - 4.0, 13.0, FG_DIM);

            if let Mode::Ended { has_next } = mode {
                let by = (h as f64 / 2.0) + 22.0;
                let bx = (w as f64 / 2.0) - 108.0;
                let replay = (bx, by, 96.0, 34.0);
                draw_button(&ctx, replay, "重播", true);
                self.hits.push(Hit { rect: rect_i16(replay), cmd: Command::Replay });
                if *has_next {
                    let next = (bx + 108.0, by, 108.0, 34.0);
                    draw_button(&ctx, next, "下一集", false);
                    self.hits.push(Hit { rect: rect_i16(next), cmd: Command::NextEpisode });
                }
            }
        }
        self.blit(conn, &mut surface, self.big, self.w, self.h);
    }

    fn draw_loading(&mut self, conn: &RustConnection) {
        let mut surface = match ImageSurface::create(Format::ARgb32, CARD_W as i32, CARD_H as i32) {
            Ok(s) => s,
            Err(_) => return,
        };
        {
            let ctx = Context::new(&surface).expect("cairo context");
            ctx.set_source_rgb(0.11, 0.11, 0.13);
            rounded(&ctx, 0.0, 0.0, CARD_W as f64, CARD_H as f64, 8.0);
            ctx.fill().ok();
            ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            ctx.set_line_width(2.0);
            ctx.arc(30.0, CARD_H as f64 / 2.0, 9.0, 0.5, 5.5);
            ctx.stroke().ok();
            text(&ctx, "正在加载…", 52.0, CARD_H as f64 / 2.0 + 4.5, 13.0, FG);
        }
        self.blit(conn, &mut surface, self.small, CARD_W, CARD_H);
    }

    fn draw_skip(&mut self, conn: &RustConnection, label: &str) {
        let mut surface = match ImageSurface::create(Format::ARgb32, SKIP_W as i32, SKIP_H as i32) {
            Ok(s) => s,
            Err(_) => return,
        };
        {
            let ctx = Context::new(&surface).expect("cairo context");
            ctx.set_source_rgb(0.11, 0.11, 0.13);
            rounded(&ctx, 0.0, 0.0, SKIP_W as f64, SKIP_H as f64, 8.0);
            ctx.fill().ok();
            ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            ctx.set_line_width(1.0);
            rounded(&ctx, 0.5, 0.5, SKIP_W as f64 - 1.0, SKIP_H as f64 - 1.0, 8.0);
            ctx.stroke().ok();
            let label = clip(&ctx, label, SKIP_W as f64 - 24.0, 13.0);
            centered(&ctx, &label, SKIP_W as i32, SKIP_H as f64 / 2.0 + 4.5, 13.0, FG);
        }
        self.blit(conn, &mut surface, self.small, SKIP_W, SKIP_H);
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
}

fn describe(mode: &Mode) -> String {
    match mode {
        Mode::Hidden => "隐藏".to_string(),
        Mode::Ended { has_next } => format!("结束页(下一集={has_next})"),
        Mode::Error(reason) => format!("错误页({reason})"),
        Mode::Loading => "加载中".to_string(),
        Mode::Skip { label } => format!("跳过提示({label})"),
    }
}

/// 覆盖层优先级：错误 > 结束 > 加载 > 跳过提示。错误/结束独占整块画面，
/// 加载与跳过共用小卡片（加载优先，避免两个窗口互相压）。
fn decide(snap: &Snapshot) -> (Mode, Mode) {
    if let Some(err) = snap.error.clone() {
        return (Mode::Error(err), Mode::Hidden);
    }
    if snap.ended {
        return (Mode::Ended { has_next: snap.has_next }, Mode::Hidden);
    }
    if snap.loading {
        return (Mode::Hidden, Mode::Loading);
    }
    match snap.skip.clone() {
        Some((label, _)) => (Mode::Hidden, Mode::Skip { label }),
        None => (Mode::Hidden, Mode::Hidden),
    }
}

fn raise(conn: &RustConnection, win: Window) {
    let _ = conn.configure_window(
        win,
        &x11rb::protocol::xproto::ConfigureWindowAux::new()
            .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
    );
}

fn draw_button(ctx: &Context, rect: (f64, f64, f64, f64), label: &str, primary: bool) {
    let (x, y, w, h) = rect;
    if primary {
        ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
    } else {
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.14);
    }
    rounded(ctx, x, y, w, h, 6.0);
    ctx.fill().ok();
    let tw = text_width(ctx, label, 13.0);
    text(ctx, label, x + (w - tw) / 2.0, y + h / 2.0 + 4.5, 13.0, FG);
}

fn rect_i16(r: (f64, f64, f64, f64)) -> (i16, i16, u16, u16) {
    (r.0 as i16, r.1 as i16, r.2 as u16, r.3 as u16)
}

fn text(ctx: &Context, s: &str, x: f64, y: f64, size: f64, color: (f64, f64, f64)) {
    ctx.select_font_face("Noto Sans CJK SC", FontSlant::Normal, FontWeight::Normal);
    ctx.set_source_rgb(color.0, color.1, color.2);
    ctx.set_font_size(size);
    ctx.move_to(x, y);
    ctx.show_text(s).ok();
}

fn text_width(ctx: &Context, s: &str, size: f64) -> f64 {
    ctx.select_font_face("Noto Sans CJK SC", FontSlant::Normal, FontWeight::Normal);
    ctx.set_font_size(size);
    ctx.text_extents(s).map(|e| e.x_advance()).unwrap_or(0.0)
}

fn centered(ctx: &Context, s: &str, w: i32, baseline: f64, size: f64, color: (f64, f64, f64)) {
    let tw = text_width(ctx, s, size);
    text(ctx, s, (w as f64 - tw) / 2.0, baseline, size, color);
}

/// 超宽时截断加省略号（错误原因可能很长）。
fn clip(ctx: &Context, s: &str, max_w: f64, size: f64) -> String {
    let mut out = s.to_string();
    while text_width(ctx, &out, size) > max_w && out.chars().count() > 4 {
        let cut = out
            .char_indices()
            .nth(out.chars().count().saturating_sub(2))
            .map(|(i, _)| i)
            .unwrap_or(0);
        out.truncate(cut);
        out.push('…');
    }
    out
}

fn rounded(ctx: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    ctx.new_sub_path();
    ctx.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    ctx.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    ctx.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    ctx.close_path();
}
