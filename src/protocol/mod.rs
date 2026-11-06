use serde::{Deserialize, Serialize};

/// Core protocol messages exchanged between Host and Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Packet {
    /// Handshake initiating session with pin/auth token
    Handshake {
        client_version: String,
        session_id: String,
    },
    /// Host acknowledging connection and sending display capabilities
    HandshakeAck {
        success: bool,
        display_width: u32,
        display_height: u32,
        refresh_rate: u32,
    },
    /// Remote input events from client
    Input(InputEvent),
    /// Clipboard text synchronization
    ClipboardSync(String),
    /// Heartbeat ping/pong for RTT latency measurement
    Ping(u64),
    Pong(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    MouseMove { x: i32, y: i32 },
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    MouseWheel { delta: i32 },
    KeyDown { key_code: u32 },
    KeyUp { key_code: u32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}
