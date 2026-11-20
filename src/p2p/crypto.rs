//! End-to-End Encryption (X25519 + ChaCha20-Poly1305 AEAD) & Auth Rate Limiter
//!
//! Provides per-session ephemeral X25519 Diffie-Hellman key agreement,
//! directional ChaCha20-Poly1305 authenticated encryption, and brute-force
//! protection for unattended access authentication.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::protocol::{MAGIC_DSKY, PKT_ENCRYPTED_FRAME};

const E2EE_AAD: &[u8] = b"DSKY_E2EE_V1";

pub struct E2eeKeypair {
    secret: StaticSecret,
    public: PublicKey,
}

impl E2eeKeypair {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Derives directional ChaCha20-Poly1305 keys so client->host and host->client
    /// never share the same (key, nonce) pair.
    pub fn derive_session_cipher(&self, peer_public_bytes: &[u8; 32], is_initiator: bool) -> SessionCipher {
        let peer_pub = PublicKey::from(*peer_public_bytes);
        let shared = self.secret.diffie_hellman(&peer_pub);

        let mut h_c2h = Sha256::new();
        h_c2h.update(shared.as_bytes());
        h_c2h.update(b"desky-e2ee-v1-c2h");
        let k_c2h: [u8; 32] = h_c2h.finalize().into();

        let mut h_h2c = Sha256::new();
        h_h2c.update(shared.as_bytes());
        h_h2c.update(b"desky-e2ee-v1-h2c");
        let k_h2c: [u8; 32] = h_h2c.finalize().into();

        let (tx_key, rx_key) = if is_initiator {
            (k_c2h, k_h2c)
        } else {
            (k_h2c, k_c2h)
        };

        SessionCipher {
            tx_cipher: ChaCha20Poly1305::new(Key::from_slice(&tx_key)),
            rx_cipher: ChaCha20Poly1305::new(Key::from_slice(&rx_key)),
            tx_seq: AtomicU64::new(1),
            rx_max_seq: Mutex::new(0),
        }
    }
}

pub struct SessionCipher {
    tx_cipher: ChaCha20Poly1305,
    rx_cipher: ChaCha20Poly1305,
    tx_seq: AtomicU64,
    rx_max_seq: Mutex<u64>,
}

impl SessionCipher {
    fn make_nonce(seq: u64) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[4..12].copy_from_slice(&seq.to_le_bytes());
        n
    }

    /// Encrypts a full `DSKY...` packet into `DSKY || PKT_ENCRYPTED_FRAME || seq_u64_le || ciphertext+tag`.
    pub fn encrypt_dsky_packet(&self, plaintext: &[u8]) -> Option<Vec<u8>> {
        let seq = self.tx_seq.fetch_add(1, Ordering::Relaxed);
        let nonce_bytes = Self::make_nonce(seq);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .tx_cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: E2EE_AAD,
                },
            )
            .ok()?;

        let mut out = Vec::with_capacity(5 + 8 + ciphertext.len());
        out.extend_from_slice(MAGIC_DSKY);
        out.push(PKT_ENCRYPTED_FRAME);
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&ciphertext);
        Some(out)
    }

    /// Decrypts a `DSKY || PKT_ENCRYPTED_FRAME || seq_u64_le || ciphertext+tag` packet.
    /// Enforces monotonic/windowed sequence check to reject stale replayed packets.
    pub fn decrypt_dsky_packet(&self, packet: &[u8]) -> Option<Vec<u8>> {
        if packet.len() < 13 || &packet[0..4] != MAGIC_DSKY || packet[4] != PKT_ENCRYPTED_FRAME {
            return None;
        }
        let seq = u64::from_le_bytes(packet[5..13].try_into().ok()?);
        if seq == 0 {
            return None;
        }
        let nonce_bytes = Self::make_nonce(seq);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = self
            .rx_cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &packet[13..],
                    aad: E2EE_AAD,
                },
            )
            .ok()?;

        // Anti-replay window: allow out-of-order UDP within 1024 packets of highest seen seq
        if let Ok(mut max_seq) = self.rx_max_seq.lock() {
            if seq + 1024 < *max_seq {
                return None;
            }
            if seq > *max_seq {
                *max_seq = seq;
            }
        }
        Some(plaintext)
    }
}

/// Rate-limiter for failed unattended access password attempts (5 failures -> 30s lockout).
pub struct AuthRateLimiter {
    attempts: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

impl AuthRateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_locked_out(&self, ip: IpAddr) -> bool {
        if let Ok(mut map) = self.attempts.lock() {
            if let Some((count, last)) = map.get(&ip) {
                if *count >= 5 {
                    if last.elapsed() < Duration::from_secs(30) {
                        return true;
                    } else {
                        map.remove(&ip);
                    }
                }
            }
        }
        false
    }

    pub fn record_failure(&self, ip: IpAddr) {
        if let Ok(mut map) = self.attempts.lock() {
            let entry = map.entry(ip).or_insert((0, Instant::now()));
            if entry.1.elapsed() > Duration::from_secs(60) {
                *entry = (1, Instant::now());
            } else {
                entry.0 = entry.0.saturating_add(1);
                entry.1 = Instant::now();
            }
        }
    }

    pub fn clear(&self, ip: IpAddr) {
        if let Ok(mut map) = self.attempts.lock() {
            map.remove(&ip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x25519_chacha20_roundtrip_and_tamper_rejection() {
        let client_kp = E2eeKeypair::generate();
        let host_kp = E2eeKeypair::generate();

        let client_cipher = client_kp.derive_session_cipher(&host_kp.public_bytes(), true);
        let host_cipher = host_kp.derive_session_cipher(&client_kp.public_bytes(), false);

        let payload = b"DSKY\x03hello-encrypted-input";
        let mut encrypted = client_cipher.encrypt_dsky_packet(payload).expect("encrypt ok");
        assert_ne!(&encrypted[13..], payload);

        let decrypted = host_cipher.decrypt_dsky_packet(&encrypted).expect("decrypt ok");
        assert_eq!(decrypted.as_slice(), payload);

        // Tamper with ciphertext -> must fail authentication tag
        let last_idx = encrypted.len() - 1;
        encrypted[last_idx] ^= 0xFF;
        assert!(host_cipher.decrypt_dsky_packet(&encrypted).is_none());
    }

    #[test]
    fn test_auth_rate_limiter_lockout() {
        let limiter = AuthRateLimiter::new();
        let ip: IpAddr = "192.0.2.10".parse().unwrap();
        for _ in 0..4 {
            limiter.record_failure(ip);
            assert!(!limiter.is_locked_out(ip));
        }
        limiter.record_failure(ip);
        assert!(limiter.is_locked_out(ip));
        limiter.clear(ip);
        assert!(!limiter.is_locked_out(ip));
    }
}
