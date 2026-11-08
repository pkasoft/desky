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

/// Requests sent to host in Silent Admin Mode (running silently without screen takeover)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdminReq {
    GetProcesses,
    KillProcess { pid: u32 },
    ExecuteCommand { command: String },
    GetSystemInfo,
    SwitchMonitor { monitor_index: u32 },
    RebootHost { safe_mode: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub mem_kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub os_name: String,
    pub hostname: String,
    pub uptime_secs: u64,
    pub cpu_count: usize,
    pub total_mem_mb: u64,
    pub free_mem_mb: u64,
}

/// Responses sent by host back to client in Silent Admin Mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdminResp {
    Processes(Vec<ProcessItem>),
    KillResult { pid: u32, success: bool, msg: String },
    CommandOutput { output: String, exit_code: i32 },
    SystemInfo(SystemStats),
    MonitorSwitched { monitor_index: u32, count: u32 },
    RebootScheduled { safe_mode: bool },
}
