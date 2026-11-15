//! Minimal libvlc 3.x binding.
//!
//! We deliberately do **not** link against `libvlc-dev`: the system here ships
//! the runtime (`libvlc.so.5`) without headers/pkg-config, so the library is
//! loaded at run time with `dlopen` and the ABI is declared by hand.

use libc::{c_char, c_float, c_int, c_void};
use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};
use std::ffi::{CStr, CString};
use std::path::PathBuf;

pub const STATE_PLAYING: i32 = 3;
pub const STATE_ENDED: i32 = 6;
pub const STATE_ERROR: i32 = 7;

const SLAVE_TYPE_SUBTITLE: c_int = 0;

#[derive(Debug, Clone)]
pub struct VlcPaths {
    pub libvlc: PathBuf,
    pub libvlccore: Option<PathBuf>,
    pub plugins: Option<PathBuf>,
}

/// Picks a self-consistent (library, plugins) pair.
///
/// A libvlc runtime without its matching plugin directory cannot even create an
/// instance, and mixing a 3.0.21 runtime with 3.0.24 plugins is undefined
/// behaviour, so the pair is resolved together.
pub fn resolve_paths(explicit_lib: Option<&str>, explicit_plugins: Option<&str>) -> Result<VlcPaths, String> {
    if let Some(lib) = explicit_lib {
        let lib = PathBuf::from(lib);
        let dir = lib.parent().ok_or("--vlc-lib has no parent directory")?.to_path_buf();
        let plugins = match explicit_plugins {
            Some(p) => Some(PathBuf::from(p)),
            None => {
                let guess = dir.join("vlc/plugins");
                if guess.is_dir() { Some(guess) } else { None }
            }
        };
        let core = dir.join("libvlccore.so.9");
        return Ok(VlcPaths {
            libvlc: lib,
            libvlccore: if core.exists() { Some(core) } else { None },
            plugins,
        });
    }

    let mut candidates: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();

    // Debian/Deepin multi-arch layout.
    for arch in ["x86_64-linux-gnu", "aarch64-linux-gnu", "i386-linux-gnu"] {
        candidates.push((
            PathBuf::from(format!("/usr/lib/{arch}/libvlc.so.5")),
            PathBuf::from(format!("/usr/lib/{arch}/libvlccore.so.9")),
            PathBuf::from(format!("/usr/lib/{arch}/vlc/plugins")),
        ));
    }
    // Deepin ships the full VLC app under /opt (runtime + plugins together).
    let opt = PathBuf::from("/opt/apps/org.videolan.www.vlc/files/lib");
    candidates.push((
        opt.join("libvlc.so.5"),
        opt.join("libvlccore.so.9"),
        opt.join("vlc/plugins"),
    ));
    // Some distros keep plugins out of the multi-arch dir.
    candidates.push((
        PathBuf::from("/usr/lib/libvlc.so.5"),
        PathBuf::from("/usr/lib/libvlccore.so.9"),
        PathBuf::from("/usr/lib/vlc/plugins"),
    ));

    let mut partial: Option<VlcPaths> = None;
    for (lib, core, plugins) in candidates {
        if !lib.exists() {
            continue;
        }
        let core = if core.exists() { Some(core) } else { None };
        if plugins.is_dir() {
            let plugins = match explicit_plugins {
                Some(p) => Some(PathBuf::from(p)),
                None => Some(plugins),
            };
            return Ok(VlcPaths { libvlc: lib, libvlccore: core, plugins });
        }
        if partial.is_none() {
            partial = Some(VlcPaths { libvlc: lib, libvlccore: core, plugins: None });
        }
    }

    partial.ok_or_else(|| {
        "找不到 libvlc.so.5（可用 --vlc-lib 指定；本机未安装 VLC 运行库时请先安装 libvlc5/vlc-plugin-base）"
            .to_string()
    })
}

struct Functions {
    libvlc_new: unsafe extern "C" fn(c_int, *const *const c_char) -> *mut c_void,
    libvlc_release: unsafe extern "C" fn(*mut c_void),
    libvlc_get_version: unsafe extern "C" fn() -> *const c_char,
    media_new_location: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    media_add_option: unsafe extern "C" fn(*mut c_void, *const c_char),
    media_release: unsafe extern "C" fn(*mut c_void),
    media_player_new: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    media_player_set_media: unsafe extern "C" fn(*mut c_void, *mut c_void),
    media_player_release: unsafe extern "C" fn(*mut c_void),
    media_player_play: unsafe extern "C" fn(*mut c_void) -> c_int,
    media_player_set_pause: unsafe extern "C" fn(*mut c_void, c_int),
    media_player_stop: unsafe extern "C" fn(*mut c_void),
    media_player_get_state: unsafe extern "C" fn(*mut c_void) -> c_int,
    media_player_is_playing: unsafe extern "C" fn(*mut c_void) -> c_int,
    media_player_get_time: unsafe extern "C" fn(*mut c_void) -> i64,
    media_player_set_time: unsafe extern "C" fn(*mut c_void, i64),
    media_player_get_length: unsafe extern "C" fn(*mut c_void) -> i64,
    media_player_set_xwindow: unsafe extern "C" fn(*mut c_void, u32),
    audio_set_volume: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    audio_get_volume: unsafe extern "C" fn(*mut c_void) -> c_int,
    audio_get_track_description: unsafe extern "C" fn(*mut c_void) -> *mut TrackDescription,
    video_get_spu_description: unsafe extern "C" fn(*mut c_void) -> *mut TrackDescription,
    audio_set_track: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    video_set_spu: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    track_description_list_release: unsafe extern "C" fn(*mut TrackDescription),
    media_player_add_slave: unsafe extern "C" fn(*mut c_void, c_int, *const c_char, c_int) -> c_int,
    media_player_set_rate: unsafe extern "C" fn(*mut c_void, c_float) -> c_int,
    media_player_get_rate: unsafe extern "C" fn(*mut c_void) -> c_float,
}

#[repr(C)]
pub struct TrackDescription {
    pub i_id: c_int,
    pub psz_name: *mut c_char,
    pub p_next: *mut TrackDescription,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: i32,
    pub name: String,
}

pub struct Vlc {
    _core: Option<Library>,
    _lib: Library,
    inst: *mut c_void,
    f: Functions,
}

unsafe fn sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
    let symbol = lib
        .get::<T>(name)
        .map_err(|e| format!("libvlc 缺少符号 {}: {e}", String::from_utf8_lossy(name)))?;
    Ok(std::mem::transmute_copy(&*symbol))
}

impl Vlc {
    pub fn new(paths: &VlcPaths, extra_args: &[String]) -> Result<Self, String> {
        // libvlc 3.0 reads the plugin directory from this env variable at
        // instance creation time.
        if let Some(plugins) = &paths.plugins {
            std::env::set_var("VLC_PLUGIN_PATH", plugins);
        }

        // Load libvlccore first with RTLD_GLOBAL so that plugins (which link
        // against it) resolve against the same copy.
        let core = match &paths.libvlccore {
            Some(core) if core.exists() => {
                let lib = unsafe { Library::open(Some(core), RTLD_NOW | RTLD_GLOBAL) }
                    .map_err(|e| format!("加载 {} 失败: {e}", core.display()))?;
                Some(lib)
            }
            _ => None,
        };

        let lib = unsafe { Library::open(Some(&paths.libvlc), RTLD_NOW | RTLD_GLOBAL) }
            .map_err(|e| format!("加载 {} 失败: {e}", paths.libvlc.display()))?;

        let f = unsafe {
            Functions {
                libvlc_new: sym(&lib, b"libvlc_new\0")?,
                libvlc_release: sym(&lib, b"libvlc_release\0")?,
                libvlc_get_version: sym(&lib, b"libvlc_get_version\0")?,
                media_new_location: sym(&lib, b"libvlc_media_new_location\0")?,
                media_add_option: sym(&lib, b"libvlc_media_add_option\0")?,
                media_release: sym(&lib, b"libvlc_media_release\0")?,
                media_player_new: sym(&lib, b"libvlc_media_player_new\0")?,
                media_player_set_media: sym(&lib, b"libvlc_media_player_set_media\0")?,
                media_player_release: sym(&lib, b"libvlc_media_player_release\0")?,
                media_player_play: sym(&lib, b"libvlc_media_player_play\0")?,
                media_player_set_pause: sym(&lib, b"libvlc_media_player_set_pause\0")?,
                media_player_stop: sym(&lib, b"libvlc_media_player_stop\0")?,
                media_player_get_state: sym(&lib, b"libvlc_media_player_get_state\0")?,
                media_player_is_playing: sym(&lib, b"libvlc_media_player_is_playing\0")?,
                media_player_get_time: sym(&lib, b"libvlc_media_player_get_time\0")?,
                media_player_set_time: sym(&lib, b"libvlc_media_player_set_time\0")?,
                media_player_get_length: sym(&lib, b"libvlc_media_player_get_length\0")?,
                media_player_set_xwindow: sym(&lib, b"libvlc_media_player_set_xwindow\0")?,
                audio_set_volume: sym(&lib, b"libvlc_audio_set_volume\0")?,
                audio_get_volume: sym(&lib, b"libvlc_audio_get_volume\0")?,
                audio_get_track_description: sym(&lib, b"libvlc_audio_get_track_description\0")?,
                video_get_spu_description: sym(&lib, b"libvlc_video_get_spu_description\0")?,
                audio_set_track: sym(&lib, b"libvlc_audio_set_track\0")?,
                video_set_spu: sym(&lib, b"libvlc_video_set_spu\0")?,
                track_description_list_release: sym(&lib, b"libvlc_track_description_list_release\0")?,
                media_player_add_slave: sym(&lib, b"libvlc_media_player_add_slave\0")?,
                media_player_set_rate: sym(&lib, b"libvlc_media_player_set_rate\0")?,
                media_player_get_rate: sym(&lib, b"libvlc_media_player_get_rate\0")?,
            }
        };

        let mut owned: Vec<CString> = vec![
            CString::new("--intf=dummy").unwrap(),
            CString::new("--no-video-title-show").unwrap(),
            CString::new("--no-lua").unwrap(),
        ];
        for extra in extra_args {
            owned.push(CString::new(extra.as_str()).map_err(|_| "VLC 参数包含 NUL".to_string())?);
        }
        let mut argv: Vec<*const c_char> = owned.iter().map(|s| s.as_ptr()).collect();
        argv.push(std::ptr::null());

        let inst = unsafe { (f.libvlc_new)(argv.len() as c_int - 1, argv.as_ptr()) };
        if inst.is_null() {
            return Err(format!("libvlc_new 失败 ({} / plugins={:?})", paths.libvlc.display(), paths.plugins));
        }

        Ok(Vlc { _core: core, _lib: lib, inst, f })
    }

    pub fn version(&self) -> String {
        unsafe {
            let p = (self.f.libvlc_get_version)();
            if p.is_null() { "unknown".into() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
        }
    }
}

impl Drop for Vlc {
    fn drop(&mut self) {
        unsafe { (self.f.libvlc_release)(self.inst) };
    }
}

pub struct Engine {
    vlc: Vlc,
    mp: *mut c_void,
    media: *mut c_void,
}

impl Engine {
    pub fn new(paths: &VlcPaths, extra_args: &[String]) -> Result<Self, String> {
        let vlc = Vlc::new(paths, extra_args)?;
        let mp = unsafe { (vlc.f.media_player_new)(vlc.inst) };
        if mp.is_null() {
            return Err("libvlc_media_player_new 失败".into());
        }
        Ok(Engine { vlc, mp, media: std::ptr::null_mut() })
    }

    pub fn version(&self) -> String {
        self.vlc.version()
    }

    pub fn set_xwindow(&self, id: u32) {
        unsafe { (self.vlc.f.media_player_set_xwindow)(self.mp, id) };
    }

    /// Opens `uri`; `options` are libvlc media options such as `:start-time=12`.
    pub fn open(&mut self, uri: &str, options: &[String]) -> Result<(), String> {
        unsafe { (self.vlc.f.media_player_stop)(self.mp) };
        if !self.media.is_null() {
            unsafe {
                (self.vlc.f.media_player_set_media)(self.mp, std::ptr::null_mut());
                (self.vlc.f.media_release)(self.media);
            }
            self.media = std::ptr::null_mut();
        }

        let curi = CString::new(uri).map_err(|_| "播放地址包含 NUL".to_string())?;
        let media = unsafe { (self.vlc.f.media_new_location)(self.vlc.inst, curi.as_ptr()) };
        if media.is_null() {
            return Err(format!("libvlc_media_new_location 失败: {uri}"));
        }
        for option in options {
            if let Ok(copt) = CString::new(option.as_str()) {
                unsafe { (self.vlc.f.media_add_option)(media, copt.as_ptr()) };
            }
        }
        unsafe { (self.vlc.f.media_player_set_media)(self.mp, media) };
        self.media = media;
        Ok(())
    }

    pub fn play(&self) -> i32 {
        unsafe { (self.vlc.f.media_player_play)(self.mp) }
    }

    pub fn stop(&mut self) {
        unsafe { (self.vlc.f.media_player_stop)(self.mp) };
    }

    pub fn set_pause(&self, paused: bool) {
        unsafe { (self.vlc.f.media_player_set_pause)(self.mp, paused as c_int) };
    }

    pub fn state(&self) -> i32 {
        unsafe { (self.vlc.f.media_player_get_state)(self.mp) }
    }

    pub fn is_playing(&self) -> bool {
        unsafe { (self.vlc.f.media_player_is_playing)(self.mp) != 0 }
    }

    pub fn time_ms(&self) -> i64 {
        unsafe { (self.vlc.f.media_player_get_time)(self.mp) }
    }

    pub fn length_ms(&self) -> i64 {
        unsafe { (self.vlc.f.media_player_get_length)(self.mp) }
    }

    pub fn seek_ms(&self, ms: i64) {
        unsafe { (self.vlc.f.media_player_set_time)(self.mp, ms.max(0)) };
    }

    pub fn volume(&self) -> i32 {
        unsafe { (self.vlc.f.audio_get_volume)(self.mp) }
    }

    pub fn set_volume(&self, volume: i32) -> i32 {
        unsafe { (self.vlc.f.audio_set_volume)(self.mp, volume.clamp(0, 125)) }
    }

    pub fn rate(&self) -> f32 {
        unsafe { (self.vlc.f.media_player_get_rate)(self.mp) }
    }

    pub fn set_rate(&self, rate: f32) -> f32 {
        let rate = rate.clamp(0.25, 4.0);
        unsafe { (self.vlc.f.media_player_set_rate)(self.mp, rate) };
        rate
    }

    fn read_tracks(list: *mut TrackDescription) -> Vec<Track> {
        let mut out = Vec::new();
        let mut p = list;
        while !p.is_null() {
            unsafe {
                let t = &*p;
                let name = if t.psz_name.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(t.psz_name).to_string_lossy().into_owned()
                };
                out.push(Track { id: t.i_id, name });
                p = t.p_next;
            }
        }
        out
    }

    pub fn audio_tracks(&self) -> Vec<Track> {
        unsafe {
            let list = (self.vlc.f.audio_get_track_description)(self.mp);
            let tracks = Self::read_tracks(list);
            if !list.is_null() {
                (self.vlc.f.track_description_list_release)(list);
            }
            tracks
        }
    }

    pub fn subtitle_tracks(&self) -> Vec<Track> {
        unsafe {
            let list = (self.vlc.f.video_get_spu_description)(self.mp);
            let tracks = Self::read_tracks(list);
            if !list.is_null() {
                (self.vlc.f.track_description_list_release)(list);
            }
            tracks
        }
    }

    /// Applies an ordinal index from the NAS (0 = first real track).
    /// `None` disables the track (`subtitle` only); audio falls back to auto.
    pub fn set_audio_ordinal(&self, ordinal: Option<usize>) -> Result<(), String> {
        let tracks: Vec<Track> = self.audio_tracks().into_iter().filter(|t| t.id >= 0).collect();
        let id = match ordinal {
            Some(i) => tracks.get(i).map(|t| t.id).ok_or_else(|| format!("音轨索引 {i} 越界(共{})", tracks.len()))?,
            None => -1,
        };
        let rc = unsafe { (self.vlc.f.audio_set_track)(self.mp, id) };
        if rc == 0 { Ok(()) } else { Err(format!("切换音轨失败 rc={rc}")) }
    }

    pub fn set_subtitle_ordinal(&self, ordinal: Option<usize>) -> Result<(), String> {
        let tracks: Vec<Track> = self.subtitle_tracks().into_iter().filter(|t| t.id >= 0).collect();
        let id = match ordinal {
            Some(i) => tracks.get(i).map(|t| t.id).ok_or_else(|| format!("字幕索引 {i} 越界(共{})", tracks.len()))?,
            None => -1,
        };
        let rc = unsafe { (self.vlc.f.video_set_spu)(self.mp, id) };
        if rc == 0 { Ok(()) } else { Err(format!("切换字幕失败 rc={rc}")) }
    }

    /// Reserved for the `10/x` external-subtitle messages (not implemented yet).
    #[allow(dead_code)]
    pub fn add_subtitle(&self, uri: &str) -> Result<(), String> {
        let curi = CString::new(uri).map_err(|_| "字幕地址包含 NUL".to_string())?;
        let rc = unsafe { (self.vlc.f.media_player_add_slave)(self.mp, SLAVE_TYPE_SUBTITLE, curi.as_ptr(), 1) };
        if rc == 0 { Ok(()) } else { Err(format!("加载外挂字幕失败 rc={rc}")) }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            (self.vlc.f.media_player_stop)(self.mp);
            if !self.media.is_null() {
                (self.vlc.f.media_player_set_media)(self.mp, std::ptr::null_mut());
                (self.vlc.f.media_release)(self.media);
            }
            (self.vlc.f.media_player_release)(self.mp);
        }
    }
}
