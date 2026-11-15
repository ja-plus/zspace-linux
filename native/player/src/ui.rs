//! Minimal X11 top-level window: renders libvlc output and turns keyboard
//! shortcuts into player commands. Deliberately dependency-free apart from
//! x11rb (pure Rust X11 protocol, no libX11 headers needed).

use crate::{Command, Event};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::Instant;
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
        };
        window.set_title(&opts.title);
        window.conn.map_window(win).map_err(|e| e.to_string())?;
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
        // Fallback for WMs without EWMH fullscreen support.
        if on {
            let _ = self.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(self.screen_w as u32)
                    .height(self.screen_h as u32),
            );
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

    /// Drains pending X events. Returns `false` when the window is gone.
    pub fn pump(&mut self, tx: &Sender<Event>) -> bool {
        loop {
            let event = match self.conn.poll_for_event() {
                Ok(Some(e)) => e,
                Ok(None) => return true,
                Err(_) => return false,
            };
            match event {
                XEvent::KeyPress(e) => self.handle_key(&e, tx),
                XEvent::KeyRelease(e) => {
                    self.pressed.remove(&e.detail);
                }
                XEvent::ButtonPress(e) => {
                    if e.detail == 1 {
                        let now = Instant::now();
                        let double = self
                            .last_click
                            .map(|t| now.duration_since(t).as_millis() < 400)
                            .unwrap_or(false);
                        self.last_click = Some(now);
                        if double {
                            let on = !self.fullscreen;
                            self.set_fullscreen(on);
                        }
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
    }
}

fn intern(conn: &RustConnection, name: &[u8]) -> Result<Atom, String> {
    conn.intern_atom(false, name)
        .map_err(|e| e.to_string())?
        .reply()
        .map(|r| r.atom)
        .map_err(|e| e.to_string())
}
