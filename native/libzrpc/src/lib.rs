//! Clean-room LAN reimplementation of the ZSpace libzrpc connection SDK.
//!
//! Contract sources: REPORT-linux-native-re.md (§2.1 FFI signatures, Phase 0
//! discovery/crypto). This crate is a from-scratch implementation guided by the
//! recovered architecture, not a copy of any official binary.

pub mod crypto;
pub mod discovery;
pub mod ffi;
pub mod proxy;
pub mod relay;