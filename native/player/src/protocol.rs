//! Wire protocol used between the ZSpace desktop main process and its player.
//!
//! Downstream (main -> player), one frame is:
//!
//! ```text
//! len=<base64 length>{<base64(utf-8 json)>
//! ```
//!
//! The reference implementation in `background.js` builds the frame as
//! `"len=" + b64.length + "{" + b64` — there is **no** trailing brace and the
//! length counts base64 characters, not decoded bytes.
//!
//! Upstream (player -> main) is a bare JSON document, one `write` per message.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine as _;
use serde_json::Value;

/// Hard ceiling for one downstream frame. The real playList messages are a few
/// KB; anything in the megabytes is either corruption or an attack.
pub const MAX_FRAME_B64: usize = 32 * 1024 * 1024;

const PREFIX: &[u8] = b"len=";

#[derive(Debug)]
pub enum FrameError {
    InvalidHeader(String),
    TooLarge(usize),
    InvalidBase64(String),
    InvalidUtf8,
    InvalidJson(String),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::InvalidHeader(s) => write!(f, "invalid TCP frame header: {s}"),
            FrameError::TooLarge(n) => write!(f, "TCP frame too large: {n}"),
            FrameError::InvalidBase64(s) => write!(f, "invalid TCP frame base64: {s}"),
            FrameError::InvalidUtf8 => write!(f, "TCP frame is not valid UTF-8"),
            FrameError::InvalidJson(s) => write!(f, "invalid JSON in TCP frame: {s}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Encodes one downstream-style frame (used by the protocol tests).
#[allow(dead_code)]
pub fn encode_frame(v: &Value) -> String {
    let json = serde_json::to_string(v).expect("json serialisation cannot fail");
    let b64 = STANDARD.encode(json.as_bytes());
    format!("len={}{{{}", b64.len(), b64)
}

/// Incremental parser: feed arbitrary byte chunks, pull complete messages.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
    /// Bytes discarded because they were not part of a valid frame.
    pub discarded: u64,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Returns the next complete JSON message, or `None` when more bytes are
    /// needed. Errors are fatal for the stream (the caller closes the socket).
    pub fn next(&mut self) -> Result<Option<Value>, FrameError> {
        loop {
            if self.buf.is_empty() {
                return Ok(None);
            }

            // Resynchronise: drop anything before the next `len=`.
            if !self.buf.starts_with(PREFIX) {
                match find_subslice(&self.buf, PREFIX) {
                    Some(0) => {}
                    Some(pos) => {
                        self.buf.drain(..pos);
                        self.discarded += pos as u64;
                        continue;
                    }
                    None => {
                        // Keep a tail that may be a partial `len=` prefix.
                        let keep = partial_prefix_suffix(&self.buf, PREFIX);
                        let drop = self.buf.len() - keep;
                        if drop == 0 {
                            return Ok(None);
                        }
                        self.buf.drain(..drop);
                        self.discarded += drop as u64;
                        continue;
                    }
                }
            }

            // `len=<digits>{`
            let mut i = PREFIX.len();
            let mut digits = 0usize;
            let mut len = 0usize;
            let mut saw_brace = false;
            while i < self.buf.len() {
                let c = self.buf[i];
                if c == b'{' {
                    saw_brace = true;
                    break;
                }
                if !c.is_ascii_digit() {
                    return Err(FrameError::InvalidHeader(String::from_utf8_lossy(
                        &self.buf[..self.buf.len().min(i + 1)],
                    )
                    .into_owned()));
                }
                digits += 1;
                if digits > 20 {
                    return Err(FrameError::InvalidHeader("length field too long".into()));
                }
                len = len.saturating_mul(10).saturating_add((c - b'0') as usize);
                i += 1;
            }
            if !saw_brace {
                return Ok(None); // header still incomplete
            }
            if len > MAX_FRAME_B64 {
                return Err(FrameError::TooLarge(len));
            }
            let payload_start = i + 1;
            if self.buf.len() < payload_start + len {
                return Ok(None); // payload still incomplete
            }

            let payload = &self.buf[payload_start..payload_start + len];
            let decoded = STANDARD
                .decode(payload)
                .or_else(|_| STANDARD_NO_PAD.decode(payload))
                .map_err(|e| FrameError::InvalidBase64(e.to_string()))?;
            let text = String::from_utf8(decoded).map_err(|_| FrameError::InvalidUtf8)?;
            let value: Value =
                serde_json::from_str(&text).map_err(|e| FrameError::InvalidJson(e.to_string()))?;

            self.buf.drain(..payload_start + len);
            return Ok(Some(value));
        }
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Length of the longest suffix of `buf` that is a proper prefix of `pat`.
fn partial_prefix_suffix(buf: &[u8], pat: &[u8]) -> usize {
    let max = pat.len().saturating_sub(1).min(buf.len());
    for len in (1..=max).rev() {
        if buf.ends_with(&pat[..len]) {
            return len;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(tag: &str) -> Value {
        json!({"f_type": 5, "s_type": 5, "tag": tag})
    }

    #[test]
    fn roundtrip_single_frame() {
        let mut r = FrameReader::new();
        let raw = encode_frame(&msg("a"));
        assert!(raw.starts_with("len="));
        assert!(!raw.ends_with('}') || raw.matches('{').count() == 1);
        r.feed(raw.as_bytes());
        assert_eq!(r.next().unwrap().unwrap()["tag"], "a");
        assert!(r.next().unwrap().is_none());
    }

    #[test]
    fn split_at_every_byte() {
        let raw = encode_frame(&msg("split")).into_bytes();
        for cut in 1..raw.len() {
            let mut r = FrameReader::new();
            r.feed(&raw[..cut]);
            assert!(r.next().unwrap().is_none(), "cut={cut} must wait");
            r.feed(&raw[cut..]);
            assert_eq!(r.next().unwrap().unwrap()["tag"], "split", "cut={cut}");
        }
    }

    #[test]
    fn sticky_two_frames_in_one_read() {
        let mut r = FrameReader::new();
        let mut raw = encode_frame(&msg("one")).into_bytes();
        raw.extend_from_slice(encode_frame(&msg("two")).as_bytes());
        r.feed(&raw);
        assert_eq!(r.next().unwrap().unwrap()["tag"], "one");
        assert_eq!(r.next().unwrap().unwrap()["tag"], "two");
        assert!(r.next().unwrap().is_none());
    }

    #[test]
    fn garbage_before_frame_is_skipped() {
        let mut r = FrameReader::new();
        let mut raw = b"\x00\x01noise".to_vec();
        raw.extend_from_slice(encode_frame(&msg("after")).as_bytes());
        r.feed(&raw);
        assert_eq!(r.next().unwrap().unwrap()["tag"], "after");
        assert_eq!(r.discarded, 7);
    }

    #[test]
    fn partial_prefix_is_kept() {
        let mut r = FrameReader::new();
        r.feed(b"xxxle");
        assert!(r.next().unwrap().is_none());
        let raw = encode_frame(&msg("late"));
        r.feed(raw.as_bytes());
        assert_eq!(r.next().unwrap().unwrap()["tag"], "late");
    }

    #[test]
    fn invalid_header_is_an_error() {
        let mut r = FrameReader::new();
        r.feed(b"len=12x{AAAA");
        assert!(matches!(r.next(), Err(FrameError::InvalidHeader(_))));
    }

    #[test]
    fn huge_length_is_rejected() {
        let mut r = FrameReader::new();
        r.feed(format!("len={}{{", MAX_FRAME_B64 + 1).as_bytes());
        assert!(matches!(r.next(), Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn invalid_json_is_an_error() {
        let mut r = FrameReader::new();
        let b64 = STANDARD.encode(b"not json");
        r.feed(format!("len={}{{{}", b64.len(), b64).as_bytes());
        assert!(matches!(r.next(), Err(FrameError::InvalidJson(_))));
    }

    #[test]
    fn utf8_chinese_payload() {
        let mut r = FrameReader::new();
        r.feed(encode_frame(&json!({"title": "复仇者联盟 4K"})).as_bytes());
        assert_eq!(r.next().unwrap().unwrap()["title"], "复仇者联盟 4K");
    }
}
