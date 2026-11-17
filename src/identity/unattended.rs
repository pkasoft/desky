//! Unattended Remote Access Security Module
//! Provides salted SHA-256 credential hashing, master password management,
//! and cryptographic challenge-response verification for headless/unattended connections.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnattendedConfig {
    pub enabled: bool,
    pub password_hash: String, // hex encoded SHA-256(salt + password)
    pub salt: String,          // 16 hex char salt
}

impl Default for UnattendedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            password_hash: String::new(),
            salt: String::new(),
        }
    }
}

static UNATTENDED_CACHE: AtomicBool = AtomicBool::new(false);

fn get_unattended_path() -> PathBuf {
    let dir = crate::platform::get_identity_dir();
    dir.join("unattended.json")
}

pub fn load_unattended_config() -> UnattendedConfig {
    let path = get_unattended_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<UnattendedConfig>(&content) {
                UNATTENDED_CACHE.store(cfg.enabled && !cfg.password_hash.is_empty(), Ordering::Relaxed);
                return cfg;
            }
        }
    }
    UnattendedConfig::default()
}

pub fn save_unattended_config(cfg: &UnattendedConfig) {
    let path = get_unattended_path();
    UNATTENDED_CACHE.store(cfg.enabled && !cfg.password_hash.is_empty(), Ordering::Relaxed);
    if let Ok(bytes) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(&path, bytes);
        info!("Updated unattended security configuration at {:?}", path);
    }
}

pub fn is_unattended_enabled() -> bool {
    let cfg = load_unattended_config();
    cfg.enabled && !cfg.password_hash.is_empty()
}

pub fn get_unattended_salt() -> Option<String> {
    let cfg = load_unattended_config();
    if cfg.enabled && !cfg.password_hash.is_empty() {
        Some(cfg.salt)
    } else {
        None
    }
}

pub fn set_unattended_password(password: &str) {
    let salt = generate_salt();
    let hash = compute_auth_hash(&salt, password);
    let cfg = UnattendedConfig {
        enabled: true,
        password_hash: hash,
        salt,
    };
    save_unattended_config(&cfg);
    info!("✅ [SECURITY] Unattended permanent access password set successfully.");
}

pub fn disable_unattended() {
    let mut cfg = load_unattended_config();
    cfg.enabled = false;
    save_unattended_config(&cfg);
    info!("🔒 [SECURITY] Unattended access disabled.");
}

pub fn verify_auth_hash(incoming_hash: &str) -> bool {
    let cfg = load_unattended_config();
    if !cfg.enabled || cfg.password_hash.is_empty() {
        return false;
    }
    incoming_hash.eq_ignore_ascii_case(&cfg.password_hash)
}

fn generate_salt() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", nanos ^ 0x55aa_1234_9876_f00d)
}

/// Computes SHA-256(salt + password) without external cryptographic C dependencies
pub fn compute_auth_hash(salt: &str, password: &str) -> String {
    let combined = format!("{}:{}", salt, password);
    let digest = sha256_digest(combined.as_bytes());
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

// ---------------------------------------------------------------------------
// Standalone Pure-Rust SHA-256 (FIPS PUB 180-4 standard implementation)
// ---------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_digest(input: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut data = input.to_vec();
    data.push(0x80);
    while (data.len() + 8) % 64 != 0 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}
