use serde::{Deserialize, Serialize};

/// Signaling & Matchmaking messages exchanged over UDP with the VDS Matchmaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalMsg {
    /// Host registering with its unique machine ID and optional cached Desky ID
    RegisterHost {
        machine_id: String,
        cached_id: Option<String>,
    },
    /// Server confirming host registration and assigning/reporting its permanent 9-digit Desky ID
    RegisterAck {
        desky_id: String,
        your_public_addr: String,
    },

    /// Client requesting connection to a Host by Desky ID (e.g. "842 195 204")
    ConnectRequest { target_id: String },
    /// Server telling Client: "Host is online, here is its public endpoint"
    PeerFound { target_id: String, peer_public_addr: String },
    /// Server telling Host: "Client wants to connect, punch a hole to this address!"
    IncomingPeer { client_public_addr: String },
    /// Server notifying Client that the requested Desky ID is offline or not found
    PeerNotFound { target_id: String },

    /// Keep-alive heartbeat from Host to keep NAT table entry open
    Heartbeat { desky_id: String },
    HeartbeatAck,

    /// Direct P2P Hole Punching Packets (exchanged directly between peers)
    Punch { from_id: String },
    PunchAck,
}

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
