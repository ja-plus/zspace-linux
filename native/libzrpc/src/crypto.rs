//! Crypto primitives recovered from the official client.
//!
//! Two related schemes share the same deterministic key source:
//!   MT19937(seed = 4728423) -> 64 x genrand_int32 -> decimal concat
//!     -> SHA-256 -> hex string -> key = hex[0..32] (ASCII bytes),
//!                                iv  = hex[0..16] (ASCII bytes)
//!     -> AES-256-GCM (16-byte IV, 16-byte tag).
//!
//! The JS service-config path (background.js `encodeService`) uses exactly this
//! and is the reference we reproduce byte-for-byte. The LAN discovery path
//! (mac `discover_utils::make_cipher_key` + `zsi::crypto::aes::encrypt`) uses the
//! same MT19937 seed and SHA-256 + AES-256-GCM; its precise key/iv slicing is
//! confirmed against a real packet in M2.

use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{AesGcm, Nonce};
use sha2::{Digest, Sha256};

/// AES-256-GCM with a 16-byte nonce (matches Node crypto's non-default IV length).
type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;

/// The fixed MT19937 seed used by both the JS and native key derivation.
pub const CIPHER_SEED: u32 = 4728423;

/// Standard MT19937 (Matsumoto & Nishimura), init_genrand seeding.
/// Matches boost::random::mt19937(seed) as used in the official mac build.
pub struct Mt19937 {
    mt: [u32; 624],
    mti: usize,
}

impl Mt19937 {
    pub fn new(seed: u32) -> Self {
        let mut mt = [0u32; 624];
        mt[0] = seed;
        for i in 1..624 {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { mt, mti: 624 }
    }

    pub fn next_u32(&mut self) -> u32 {
        const N: usize = 624;
        const M: usize = 397;
        const MATRIX_A: u32 = 0x9908_b0df;
        const UPPER: u32 = 0x8000_0000;
        const LOWER: u32 = 0x7fff_ffff;

        if self.mti >= N {
            for i in 0..N {
                let y = (self.mt[i] & UPPER) | (self.mt[(i + 1) % N] & LOWER);
                self.mt[i] = self.mt[(i + M) % N] ^ (y >> 1) ^ if y & 1 != 0 { MATRIX_A } else { 0 };
            }
            self.mti = 0;
        }

        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
}

/// A derived AES-256-GCM key/iv pair (key 32 bytes, iv 16 bytes).
#[derive(Clone, Copy)]
pub struct CipherKey {
    pub key: [u8; 32],
    pub iv: [u8; 16],
}

/// Reproduce the JS `encodeService` key derivation exactly.
///
/// MT19937(4728423) x64 -> decimal concat -> SHA-256 hex ->
/// key = first 32 hex chars as ASCII bytes, iv = first 16 hex chars as ASCII bytes.
pub fn derive_key_js() -> CipherKey {
    let mut mt = Mt19937::new(CIPHER_SEED);
    let mut concat = String::new();
    for _ in 0..64 {
        concat.push_str(&mt.next_u32().to_string());
    }
    let digest = Sha256::digest(concat.as_bytes());
    let hexstr = hex::encode(digest);
    let mut key = [0u8; 32];
    let mut iv = [0u8; 16];
    key.copy_from_slice(&hexstr.as_bytes()[0..32]);
    iv.copy_from_slice(&hexstr.as_bytes()[0..16]);
    CipherKey { key, iv }
}

/// AES-256-GCM encrypt; returns ciphertext || 16-byte tag (Node convention).
pub fn encrypt_gcm(k: &CipherKey, plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm16::new((&k.key).into());
    let nonce = Nonce::from_slice(&k.iv);
    cipher
        .encrypt(nonce, Payload { msg: plaintext, aad: &[] })
        .expect("aes-gcm encrypt")
}

/// AES-256-GCM decrypt; input is ciphertext || 16-byte tag.
pub fn decrypt_gcm(k: &CipherKey, ciphertext: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
    let cipher = Aes256Gcm16::new((&k.key).into());
    let nonce = Nonce::from_slice(&k.iv);
    cipher.decrypt(nonce, Payload { msg: ciphertext, aad: &[] })
}

/// Hex-encode (discovery wraps the ciphertext in hex on the wire).
pub fn to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Decrypt AES-256-GCM ciphertext that was sent WITHOUT the auth tag.
///
/// The official `encodeService` does `concat([update, final]).toString('base64')`
/// and never appends `getAuthTag()`, so the wire carries only the CTR-encrypted
/// payload. GCM without tag verification is CTR mode with the GCM counter:
/// for a 16-byte (128-bit) IV, J0 = GHASH_H(IV || 0^64 || [128]_64), and the
/// data counter starts at inc32(J0).
pub fn decrypt_gcm_no_tag(key: &[u8; 32], iv: &[u8; 16], ct: &[u8]) -> Vec<u8> {
    use aes::cipher::{BlockEncrypt, KeyInit as _};
    use cipher::{KeyIvInit, StreamCipher};
    use ghash::universal_hash::UniversalHash;

    // H = AES_K(0^128)
    let aes = aes::Aes256::new(key.into());
    let mut h_block = aes::Block::default();
    aes.encrypt_block(&mut h_block);
    let h = ghash::Key::from_slice(&h_block);

    // J0 = GHASH_H(IV || 0^64 || [len(IV)=128]_64)   (IV is exactly 128 bits)
    let mut ghash = ghash::GHash::new(h);
    let mut input = Vec::with_capacity(32);
    input.extend_from_slice(iv);
    input.extend_from_slice(&[0u8; 8]);
    input.extend_from_slice(&128u64.to_be_bytes());
    ghash.update_padded(&input);
    let j0 = ghash.finalize();

    // counter = inc32(J0)
    let mut counter = [0u8; 16];
    counter.copy_from_slice(&j0);
    let c = u32::from_be_bytes([counter[12], counter[13], counter[14], counter[15]]).wrapping_add(1);
    counter[12..16].copy_from_slice(&c.to_be_bytes());

    let mut cipher = ctr::Ctr32BE::<aes::Aes256>::new(key.into(), (&counter).into());
    let mut out = ct.to_vec();
    cipher.apply_keystream(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical MT19937 reference: init_genrand(5489) first five outputs.
    #[test]
    fn mt19937_reference_vector() {
        let mut mt = Mt19937::new(5489);
        let expected = [3499211612u32, 581869302, 3890346734, 3586334585, 545404204];
        for e in expected {
            assert_eq!(mt.next_u32(), e);
        }
    }

    /// Key derivation is deterministic and well-formed.
    #[test]
    fn derive_key_deterministic() {
        let a = derive_key_js();
        let b = derive_key_js();
        assert_eq!(a.key, b.key);
        assert_eq!(a.iv, b.iv);
        // key/iv are ASCII hex characters (0-9a-f), per the JS substring scheme.
        assert!(a.key.iter().all(|c| c.is_ascii_hexdigit()));
        assert!(a.iv.iter().all(|c| c.is_ascii_hexdigit()));
        // iv is the first 16 bytes of the same hex string the key starts with.
        assert_eq!(&a.key[0..16], &a.iv[..]);
        eprintln!("derived key (hex-ascii): {}", String::from_utf8_lossy(&a.key));
        eprintln!("derived iv  (hex-ascii): {}", String::from_utf8_lossy(&a.iv));
    }

    /// AES-256-GCM roundtrip with the 16-byte IV.
    #[test]
    fn aes_gcm_roundtrip() {
        let k = derive_key_js();
        let msg = br#"{"key":"tdxnassearch"}"#;
        let ct = encrypt_gcm(&k, msg);
        assert_eq!(ct.len(), msg.len() + 16); // ciphertext + 16-byte tag
        let pt = decrypt_gcm(&k, &ct).expect("decrypt");
        assert_eq!(pt, msg);
    }

    /// Tampered ciphertext fails authentication.
    #[test]
    fn aes_gcm_rejects_tamper() {
        let k = derive_key_js();
        let mut ct = encrypt_gcm(&k, b"hello");
        ct[0] ^= 0xff;
        assert!(decrypt_gcm(&k, &ct).is_err());
    }

    /// GCM ciphertext WITHOUT tag (app encodeService) decrypts via CTR+J0.
    #[test]
    fn gcm_no_tag_roundtrip() {
        let k = derive_key_js();
        let msg = br#"{"local_port":13579,"remote_port":5055,"nas_id":"Z0220022QBRF4"}"#;
        let ct_tag = encrypt_gcm(&k, msg);
        let ct = &ct_tag[..ct_tag.len() - 16]; // strip the 16-byte tag
        let pt = decrypt_gcm_no_tag(&k.key, &k.iv, ct);
        assert_eq!(pt, msg);
    }
}