//! 官方样式常量与图标资源。
//!
//! 所有数值都来自官方 `UZPlayer.exe` 里提取出来的 QSS 原文
//! （`assets/official.qss`，由 `tools/extract_uzplayer_assets.py` 生成），
//! 不是目测调出来的。图标是官方 `:/resource/...` 原图，80x80 的 2x 资源
//! 在 40px 的按钮位上按比例缩放绘制。
//!
//! QSS 依据：
//! - `QSlider::sub-page:horizontal { background: #425FFF; border-radius: 2px; }`
//! - `QSlider::groove:horizontal { background: rgba(255,255,255,0.15); height: 4px; border-radius: 2px; }`
//! - `QSlider::add-page:horizontal { background: rgba(228,228,228,0.15); }`
//! - `QPushButton#btnSpeed { background: rgba(255,255,255,0.15); border-radius: 8px;
//!    font-size: 16px; color: rgba(255,255,255,0.9); padding-left/right: 8px; }`
//! - `QPushButton#btnFormat { font-size: 16px; color: rgba(255,255,255,0.9); padding: 0 8px; }`
//! - `QWidget#widButtons { border-right: 1px solid rgba(255,255,255,0.15); }`
//! - 弹层项 hover：`background: rgba(130,149,255,0.12); border-radius: 4px; font-size: 16px;`

use cairo::{Context, ImageSurface};
use std::collections::HashMap;
use std::io::Cursor;

/// 官方主色 #425FFF（进度条已播放段、选中态）
pub const ACCENT: (f64, f64, f64) = (66.0 / 255.0, 95.0 / 255.0, 1.0);
/// 进度条未播放轨道 rgba(255,255,255,0.15)
pub const TRACK: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 0.15);
/// 已播放段右侧的余量 rgba(228,228,228,0.15)
pub const TRACK_ADD: (f64, f64, f64, f64) = (0.894, 0.894, 0.894, 0.15);
/// 主文字色 rgba(255,255,255,0.9)
pub const FG: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 0.9);
/// 次要文字（标题等）比正文再压一档
pub const FG_DIM: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 0.65);
/// 官方字号：控制栏正文与弹层项都是 16px
pub const FONT: f64 = 16.0;
pub const FONT_SMALL: f64 = 14.0;
/// 说明性文字（官方 `QLabel { color: rgba(255,255,255,0.55); font-size: 14px; }`）
pub const LABEL: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 0.55);
/// 面板里更亮一档的标签（标题、页签名）
pub const LABEL_HI: (f64, f64, f64, f64) = (0.91, 0.91, 0.91, 1.0);
/// 弹层副标题灰（实测 (190,191,192)）
pub const SUB_FG: (f64, f64, f64, f64) = (0.72, 0.72, 0.74, 1.0);
pub const FONT_LABEL: f64 = 14.0;
/// 弹层项 hover/选中的底色 rgba(130,149,255,0.12)
pub const MENU_SEL: (f64, f64, f64, f64) = (0.510, 0.584, 1.0, 0.12);
/// 分隔线 rgba(255,255,255,0.15)（widTitle / widDetail / widButtons 共用）
pub const DIVIDER: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 0.15);

/// `QWidget#widControl { background: linear-gradient(180deg, rgba(0,0,0,0) 0%,
/// rgba(0,0,0,0.6) 55%, rgba(0,0,0,0.75) 100%), #141620; }`
/// 底色 #141620
pub const CONTROL_BASE: (f64, f64, f64) = (20.0 / 255.0, 22.0 / 255.0, 32.0 / 255.0);

/// `QWidget#widLiveBase { background: rgba(0,0,0,0.85); border-radius: 8px;
/// border: 1px solid rgba(179,193,205,0.15); }`
/// 窗口本身不透明，0.85 的黑要预先混到 #141620 底上。
pub const POPUP_RADIUS: f64 = 8.0;
pub const POPUP_BORDER: (f64, f64, f64, f64) = (179.0 / 255.0, 193.0 / 255.0, 205.0 / 255.0, 0.15);

/// 控制栏图标尺寸：资源是 80x80 的 2x 图，官方界面上量到约 24px。
/// 控制栏图标绘制尺寸：资源是 80x80 的 @2x 图，按逻辑尺寸 40 绘制，
/// 实际墨迹 ≈18-22px，与 Windows 版实测一致（28 会偏小、只剩 12-16px）。
pub const ICON_SIZE: f64 = 40.0;

macro_rules! icons {
    ($(($key:literal, $path:literal)),* $(,)?) => {
        const ICON_SOURCES: &[(&str, &[u8])] = &[
            $(($key, include_bytes!(concat!("../assets/icons/", $path)))),*
        ];
    };
}

icons![
    ("play", "controlbar/playWhite2x.png"),
    ("play_hover", "controlbar/playHoverWhite2x.png"),
    ("pause", "controlbar/pauseWhite2x.png"),
    ("pause_hover", "controlbar/pauseHoverWhite2x.png"),
    ("rewind", "controlbar/rewind2x.png"),
    ("rewind_hover", "controlbar/rewindHover2x.png"),
    ("forward", "controlbar/fastForward2x.png"),
    ("forward_hover", "controlbar/fastForwardHover2x.png"),
    ("prev", "controlbar/previousVideoWhite2x.png"),
    ("prev_hover", "controlbar/previousVideoHoverWhite2x.png"),
    ("prev_off", "controlbar/previousVideoWhiteDisable2x.png"),
    ("next", "controlbar/nextVideoWhite2x.png"),
    ("next_hover", "controlbar/nextVideoHoverWhite2x.png"),
    ("next_off", "controlbar/nextVideoWhiteDisable2x.png"),
    ("subtitle", "controlbar/subtitleWhite2x.png"),
    ("subtitle_hover", "controlbar/subtitleHoverWhite2x.png"),
    ("audio", "controlbar/audioTrackWhite2x.png"),
    ("audio_hover", "controlbar/audioTrackHoverWhite2x.png"),
    ("episode", "controlbar/episodesWhite2x.png"),
    ("episode_hover", "controlbar/episodesHoverWhite2x.png"),
    ("volume", "controlbar/volumeWhite2x.png"),
    ("volume_hover", "controlbar/volumeHoverWhite2x.png"),
    ("mute", "controlbar/muteWhite2x.png"),
    ("mute_hover", "controlbar/muteHoverWhite2x.png"),
    ("full", "controlbar/fullScreenWhite2x.png"),
    ("full_hover", "controlbar/fullScreenHoverWhite2x.png"),
    ("exit_full", "controlbar/exitFullScreenWhite2x.png"),
    ("exit_full_hover", "controlbar/exitFullScreenHoverWhite2x.png"),
    ("more", "controlbar/moreWhite2x.png"),
    ("more_hover", "controlbar/moreHoverWhite2x.png"),
    ("slider_handle", "controlbar/sliderbtn2x.png"),
    ("close", "titlebar/winClose2x.png"),
    ("close_hover", "titlebar/winCloseHover2x.png"),
    // 弹层标题栏右侧的关闭叉。
    // 官方用的是 popup/popupClose2x.png，但 popup 目录那一段的「块↔名字」
    // 对齐还没校准（提取出来的 popupClose2x 实际是下箭头），
    // 这里先改用已验证正确的标题栏关闭图。
    ("popup_close", "titlebar/winClose2x.png"),
    ("popup_close_hover", "titlebar/winCloseHover2x.png"),
];

pub struct IconSet {
    map: HashMap<&'static str, ImageSurface>,
}

impl IconSet {
    pub fn load() -> Self {
        let mut map = HashMap::new();
        for (key, bytes) in ICON_SOURCES {
            let mut cur = Cursor::new(*bytes);
            match ImageSurface::create_from_png(&mut cur) {
                Ok(s) => {
                    map.insert(*key, s);
                }
                Err(e) => eprintln!("图标 {key} 解析失败: {e}"),
            }
        }
        IconSet { map }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// 以 (cx, cy) 为中心、边长 `size` 绘制。
    pub fn draw_center(&self, ctx: &Context, key: &str, cx: f64, cy: f64, size: f64) {
        let Some(surface) = self.map.get(key) else { return };
        let (w, h) = (surface.width() as f64, surface.height() as f64);
        let scale = size / w.max(h);
        let _ = ctx.save();
        let _ = ctx.translate(cx - w * scale / 2.0, cy - h * scale / 2.0);
        let _ = ctx.scale(scale, scale);
        let _ = ctx.set_source_surface(surface, 0.0, 0.0);
        ctx.rectangle(0.0, 0.0, w, h);
        let _ = ctx.fill();
        let _ = ctx.restore();
    }

    #[allow(dead_code)]
    pub fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
}

/// 进度条：轨道 → 已播放 → 未播放余量 → 官方滑块图。
pub fn draw_seek(
    ctx: &Context,
    icons: &IconSet,
    left: f64,
    right: f64,
    y: f64,
    frac: f64,
) {
    let w = right - left;
    let frac = frac.clamp(0.0, 1.0);

    // groove
    ctx.new_path();
    rounded_rect(ctx, left, y, w, 4.0, 2.0);
    ctx.set_source_rgba(TRACK.0, TRACK.1, TRACK.2, TRACK.3);
    let _ = ctx.fill();

    // add-page（右侧未缓冲段，官方单独一个更亮的灰）
    if frac < 1.0 {
        ctx.new_path();
        rounded_rect(ctx, left + w * frac, y, w * (1.0 - frac), 4.0, 2.0);
        ctx.set_source_rgba(TRACK_ADD.0, TRACK_ADD.1, TRACK_ADD.2, TRACK_ADD.3);
        let _ = ctx.fill();
    }

    // sub-page（已播放）
    if frac > 0.0 {
        ctx.new_path();
        rounded_rect(ctx, left, y, w * frac, 4.0, 2.0);
        ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
        let _ = ctx.fill();
    }

    // 滑块：官方用 sliderbtn2x.png，不是画出来的圆点
    icons.draw_center(ctx, "slider_handle", left + w * frac, y + 2.0, 16.0);
}

/// 控制栏背景：官方是一条自上而下的黑色渐变叠在 `#141620` 上
/// （0% 透明 → 55% 黑 0.6 → 100% 黑 0.75）。窗口没有 ARGB，
/// 所以把「渐变叠底色」的合成结果直接算成三段实色。
pub fn draw_control_bg(ctx: &Context, w: f64, h: f64) {
    // Windows 版实测：控制栏是一块**纯色** #141620，没有渐变
    // （逐像素取值为 (20,22,32)，从栏顶到栏底完全一致）。
    ctx.set_source_rgb(CONTROL_BASE.0, CONTROL_BASE.1, CONTROL_BASE.2);
    ctx.rectangle(0.0, 0.0, w, h.max(1.0));
    let _ = ctx.fill();
}

/// 弹层底色：官方 `rgba(0,0,0,0.85)` 落到不透明窗口上，
/// 按叠在 `#141620` 上预先合成。
pub fn popup_bg() -> (f64, f64, f64) {
    (
        CONTROL_BASE.0 * 0.15,
        CONTROL_BASE.1 * 0.15,
        CONTROL_BASE.2 * 0.15,
    )
}

pub fn rounded_rect(ctx: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {    let r = r.min(w / 2.0).min(h / 2.0);
    ctx.new_sub_path();
    ctx.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    ctx.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    ctx.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    ctx.close_path();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_load() {
        let icons = IconSet::load();
        assert_eq!(icons.len(), ICON_SOURCES.len(), "有图标没加载上");
        for key in ["play", "pause", "volume", "slider_handle", "more"] {
            assert!(icons.has(key), "{key} 缺失");
        }
    }

    #[test]
    fn accent_matches_qss() {
        // #425FFF = (66, 95, 255) / 255
        assert!((ACCENT.0 - 66.0 / 255.0).abs() < 1e-9);
        assert!((ACCENT.1 - 95.0 / 255.0).abs() < 1e-9);
        assert!((ACCENT.2 - 1.0).abs() < 1e-9);
    }
}
