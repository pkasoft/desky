//! Desky Wire Protocol Opcodes & Magic Signatures
//!
//! Serves as the single, authoritative source of truth for all network
//! packet types, header identifiers, and opcode constants exchanged between
//! Desky peers and relay nodes.

#![allow(dead_code)]

/// Standard 4-byte header identifying all authentic Desky UDP packets
pub const MAGIC_DSKY: &[u8; 4] = b"DSKY";

// --- Connection Handshake & Authentication (0x01 - 0x08) ---
pub const PKT_CLIENT_HELLO: u8 = 0x01;
pub const PKT_FRAME_CHUNK: u8 = 0x02; // Baseline Motion-JPEG frame chunk
pub const PKT_INPUT_EVENT: u8 = 0x03;
pub const PKT_DISCONNECT: u8 = 0x04;
pub const PKT_LAN_DISCOVERY: u8 = 0x05;
pub const PKT_PONG: u8 = 0x06;
pub const PKT_AUTH_STATUS: u8 = 0x07;
pub const PKT_RELAY: u8 = 0x08;

// --- Session Features & Subsystems (0x09 - 0x15) ---
pub const PKT_CLIPBOARD: u8 = 0x09;
pub const PKT_FILE_START: u8 = 0x0A;
pub const PKT_FILE_CHUNK: u8 = 0x0B;
pub const PKT_FILE_FINISH: u8 = 0x0C;
pub const PKT_ADMIN_REQ: u8 = 0x0D;
pub const PKT_ADMIN_RESP: u8 = 0x0E;
pub const PKT_AUDIO_CHUNK: u8 = 0x0F;
pub const PKT_CURSOR_SHAPE: u8 = 0x10;
pub const PKT_UNATTENDED_CHALLENGE: u8 = 0x11;
pub const PKT_UNATTENDED_AUTH: u8 = 0x12;
pub const PKT_CHAT_MESSAGE: u8 = 0x13;
pub const PKT_VOICE_CHUNK: u8 = 0x14;
pub const PKT_FILE_NACK: u8 = 0x15;

// --- Hardware Codec Streaming Chunks (0x16 - 0x17) ---
pub const PKT_FRAME_CHUNK_H264: u8 = 0x16; // Hardware-accelerated H.264 / AVC video chunk
pub const PKT_FRAME_CHUNK_H265: u8 = 0x17; // Hardware-accelerated H.265 / HEVC video chunk
