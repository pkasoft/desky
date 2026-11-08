use crate::protocol::InputEvent;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::Notify;
use tracing::info;

pub const MAGIC_DSKY: &[u8; 4] = b"DSKY";

pub const PKT_CLIENT_HELLO: u8 = 0x01;
pub const PKT_FRAME_CHUNK: u8 = 0x02;
pub const PKT_INPUT_EVENT: u8 = 0x03;
pub const PKT_DISCONNECT: u8 = 0x04;
pub const PKT_LAN_DISCOVERY: u8 = 0x05;
pub const PKT_PONG: u8 = 0x06;
pub const PKT_AUTH_STATUS: u8 = 0x07;
pub const PKT_RELAY: u8 = 0x08;

pub const AUTH_PENDING: u8 = 0;
pub const AUTH_APPROVED: u8 = 1;
pub const AUTH_DECLINED: u8 = 2;

#[derive(Clone)]
pub struct RemoteFrame {
    pub frame_id: u32,
    pub orig_width: u32,
    pub orig_height: u32,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Connecting(String),
    WaitingApproval(String),
    Connected(String),
    Error(String),
}

#[derive(Clone, Debug)]
pub struct PendingApprovalRequest {
    pub client_key: String,
    pub client_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthState {
    Pending,
    Approved,
    Declined(Instant),
}

#[derive(Clone)]
pub struct ClientSubscriber {
    pub client_id: String,
    pub relay_id: String,
    pub addr: SocketAddr,
    pub lan_addr: Option<SocketAddr>,
    pub auth_state: AuthState,
    pub last_seen: Instant,
    pub target_fps: u32,
    pub capture_cursor: bool,
    pub jpeg_quality: u8,
    pub scale_pct: u8,
    pub chroma_444: bool,
    pub is_direct: bool,
}

static LATEST_REMOTE_FRAME: OnceLock<Mutex<Option<Arc<RemoteFrame>>>> = OnceLock::new();
static SESSION_STATUS: OnceLock<Mutex<SessionStatus>> = OnceLock::new();
static INPUT_QUEUE: OnceLock<Mutex<Vec<InputEvent>>> = OnceLock::new();
static INPUT_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

static PENDING_APPROVAL: OnceLock<Mutex<Option<PendingApprovalRequest>>> = OnceLock::new();
static HOST_SUBSCRIBERS: OnceLock<Arc<Mutex<HashMap<String, ClientSubscriber>>>> = OnceLock::new();
static HOST_STD_SOCKET: OnceLock<Mutex<Option<Arc<std::net::UdpSocket>>>> = OnceLock::new();

pub static CLIENT_SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static CLIENT_SESSION_GEN: AtomicU32 = AtomicU32::new(0);
pub static CLIENT_TARGET_FPS: AtomicU32 = AtomicU32::new(60);
pub static CLIENT_CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

// Quality / Bitrate Mode:
// 0 = Auto Adaptive (ABR based on network speed & loss)
// 1 = Ultra Text (100% Native, Q90, 4:4:4 Chroma)
// 2 = High Clarity (100% Native, Q80, 4:4:4 Chroma)
// 3 = Balanced (85% Scale, Q74, 4:4:4 Chroma)
// 4 = Low Data / Fast (70% Scale, Q60, 4:2:0 Chroma)
pub static QUALITY_MODE: AtomicU8 = AtomicU8::new(0);

pub static ACTIVE_JPEG_QUALITY: AtomicU8 = AtomicU8::new(76);
pub static ACTIVE_SCALE_PCT: AtomicU8 = AtomicU8::new(100);
pub static ACTIVE_CHROMA_444: AtomicBool = AtomicBool::new(true);
pub static SESSION_RTT_MS: AtomicU32 = AtomicU32::new(0);
pub static SESSION_BITRATE_KBPS: AtomicU32 = AtomicU32::new(0);

pub fn frame_store() -> &'static Mutex<Option<Arc<RemoteFrame>>> {
    LATEST_REMOTE_FRAME.get_or_init(|| Mutex::new(None))
}

pub fn status_store() -> &'static Mutex<SessionStatus> {
    SESSION_STATUS.get_or_init(|| Mutex::new(SessionStatus::Idle))
}

pub fn input_store() -> &'static Mutex<Vec<InputEvent>> {
    INPUT_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn input_notify() -> &'static Arc<Notify> {
    INPUT_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

pub fn pending_approval_store() -> &'static Mutex<Option<PendingApprovalRequest>> {
    PENDING_APPROVAL.get_or_init(|| Mutex::new(None))
}

pub fn host_subscribers_store() -> &'static Arc<Mutex<HashMap<String, ClientSubscriber>>> {
    HOST_SUBSCRIBERS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

pub fn host_std_socket_store() -> &'static Mutex<Option<Arc<std::net::UdpSocket>>> {
    HOST_STD_SOCKET.get_or_init(|| Mutex::new(None))
}

pub fn get_pending_approval() -> Option<PendingApprovalRequest> {
    pending_approval_store().lock().ok().and_then(|g| g.clone())
}

/// Detects the machine's active outbound LAN IPv4 address via OS routing table lookup (zero packets sent)
pub fn detect_local_lan_ip(target_server: SocketAddr) -> Option<Ipv4Addr> {
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    let _ = probe.connect(target_server);
    if let Ok(SocketAddr::V4(v4)) = probe.local_addr() {
        let ip = *v4.ip();
        if !ip.is_loopback() && !ip.is_unspecified() {
            return Some(ip);
        }
    }
    None
}

/// Returns true if `candidate` is on the exact same `/24` private IPv4 subnet as our local LAN IP
pub fn is_same_lan_subnet(local_lan_bytes: &[u8; 6], candidate: SocketAddr) -> bool {
    if local_lan_bytes[0] == 0 {
        return false;
    }
    if let IpAddr::V4(cand_v4) = candidate.ip() {
        let oct = cand_v4.octets();
        oct[0] == local_lan_bytes[0] && oct[1] == local_lan_bytes[1] && oct[2] == local_lan_bytes[2]
    } else {
        false
    }
}

/// Returns the 6-byte `[ipv4 (4 bytes), port_le (2 bytes)]` of `local_port` on the active local LAN
pub fn encode_lan_endpoint_port(local_port: u16, target_server: SocketAddr) -> [u8; 6] {
    let mut out = [0u8; 6];
    if local_port > 0 {
        if let Some(ip) = detect_local_lan_ip(target_server) {
            let octs = ip.octets();
            out[0..4].copy_from_slice(&octs);
            out[4..6].copy_from_slice(&local_port.to_le_bytes());
        }
    }
    out
}

/// Decodes a 6-byte `[ipv4 (4 bytes), port_le (2 bytes)]` LAN endpoint if valid
pub fn decode_lan_endpoint(bytes: &[u8]) -> Option<SocketAddr> {
    if bytes.len() < 6 {
        return None;
    }
    let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
    let port = u16::from_le_bytes([bytes[4], bytes[5]]);
    if ip.is_unspecified() || ip.is_loopback() || port == 0 {
        return None;
    }
    Some(SocketAddr::new(IpAddr::V4(ip), port))
}

/// Returns broadcast addresses for LAN discovery including the active `/24` subnet broadcast
pub fn lan_broadcast_addrs(target_server: SocketAddr, port: u16) -> Vec<SocketAddr> {
    let mut list = Vec::with_capacity(3);
    if let Some(ip) = detect_local_lan_ip(target_server) {
        let oct = ip.octets();
        let subnet_bcast = Ipv4Addr::new(oct[0], oct[1], oct[2], 255);
        list.push(SocketAddr::new(IpAddr::V4(subnet_bcast), port));
    }
    list.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port));
    list.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    list
}

/// Called by the Host UI when the user clicks Accept (`approved = true`) or Decline (`approved = false`)
pub fn respond_to_pending_approval(approved: bool) {
    let req = {
        if let Ok(mut g) = pending_approval_store().lock() {
            g.take()
        } else {
            None
        }
    };

    let mut acks_to_send: Vec<(SocketAddr, Option<SocketAddr>, String, u8)> = Vec::new();
    if let Ok(mut map) = host_subscribers_store().lock() {
        let now = Instant::now();
        for (key, sub) in map.iter_mut() {
            let matches_req = req
                .as_ref()
                .map(|r| &r.client_key == key || r.client_id == sub.client_id)
                .unwrap_or(false);
            if matches_req || sub.auth_state == AuthState::Pending {
                if approved {
                    sub.auth_state = AuthState::Approved;
                    sub.last_seen = now;
                    acks_to_send.push((
                        sub.addr,
                        sub.lan_addr,
                        sub.relay_id.clone(),
                        AUTH_APPROVED,
                    ));
                } else {
                    sub.auth_state = AuthState::Declined(now);
                    acks_to_send.push((
                        sub.addr,
                        sub.lan_addr,
                        sub.relay_id.clone(),
                        AUTH_DECLINED,
                    ));
                }
            }
        }
    }

    if !acks_to_send.is_empty() {
        if let Ok(Some(sock)) = host_std_socket_store().lock().map(|g| g.clone()) {
            let server_addr = resolve_server_addr();
            let local_port = sock.local_addr().ok().map(|a| a.port()).unwrap_or(0);
            let lan_bytes = encode_lan_endpoint_port(local_port, server_addr);
            for (addr, lan_addr, relay_id, status_byte) in acks_to_send {
                let mut auth_pkt = Vec::with_capacity(12);
                auth_pkt.extend_from_slice(MAGIC_DSKY);
                auth_pkt.push(PKT_AUTH_STATUS);
                auth_pkt.push(status_byte);
                auth_pkt.extend_from_slice(&lan_bytes);

                if addr != server_addr {
                    for _ in 0..4 {
                        let _ = sock.send_to(&auth_pkt, addr);
                    }
                }
                if let Some(l_addr) = lan_addr {
                    for _ in 0..4 {
                        let _ = sock.send_to(&auth_pkt, l_addr);
                    }
                }
                if !relay_id.is_empty() {
                    let relay_pkt = wrap_relay_packet(&relay_id, &auth_pkt);
                    for _ in 0..4 {
                        let _ = sock.send_to(&relay_pkt, server_addr);
                    }
                }
            }
        }
    }

    if let Some(pending) = req {
        if approved {
            info!(
                "✅ Host user APPROVED incoming session from {}",
                pending.client_id
            );
            #[cfg(target_os = "windows")]
            crate::windows::ui::append_status_log(&format!(
                "✅ Approved remote session for ID: {}",
                pending.client_id
            ));
        } else {
            info!(
                "❌ Host user DECLINED incoming session from {}",
                pending.client_id
            );
            #[cfg(target_os = "windows")]
            crate::windows::ui::append_status_log(&format!(
                "🚫 Declined connection request from ID: {}",
                pending.client_id
            ));
        }
    }
    #[cfg(target_os = "windows")]
    crate::windows::ui::request_main_redraw();
}

pub fn get_latest_remote_frame() -> Option<Arc<RemoteFrame>> {
    frame_store().lock().ok().and_then(|g| g.clone())
}

pub fn get_session_status() -> SessionStatus {
    status_store()
        .lock()
        .map(|g| g.clone())
        .unwrap_or(SessionStatus::Idle)
}

pub fn set_session_status(status: SessionStatus) {
    if let Ok(mut g) = status_store().lock() {
        *g = status;
    }
}

pub fn get_session_rtt_ms() -> u32 {
    SESSION_RTT_MS.load(Ordering::Relaxed)
}

pub fn get_session_bitrate_kbps() -> u32 {
    SESSION_BITRATE_KBPS.load(Ordering::Relaxed)
}

pub fn get_quality_mode() -> u8 {
    QUALITY_MODE.load(Ordering::Relaxed)
}

pub fn set_quality_mode(mode: u8) {
    let clamped = mode.min(4);
    QUALITY_MODE.store(clamped, Ordering::Relaxed);
    apply_preset_for_mode(clamped);
}

pub fn get_active_quality_params() -> (u8, u8, bool) {
    (
        ACTIVE_JPEG_QUALITY.load(Ordering::Relaxed),
        ACTIVE_SCALE_PCT.load(Ordering::Relaxed),
        ACTIVE_CHROMA_444.load(Ordering::Relaxed),
    )
}

pub fn apply_preset_for_mode(mode: u8) {
    match mode {
        1 => {
            // Ultra Text
            ACTIVE_JPEG_QUALITY.store(88, Ordering::Relaxed);
            ACTIVE_SCALE_PCT.store(100, Ordering::Relaxed);
            ACTIVE_CHROMA_444.store(true, Ordering::Relaxed);
        }
        2 => {
            // High Clarity
            ACTIVE_JPEG_QUALITY.store(78, Ordering::Relaxed);
            ACTIVE_SCALE_PCT.store(100, Ordering::Relaxed);
            ACTIVE_CHROMA_444.store(true, Ordering::Relaxed);
        }
        3 => {
            // Balanced
            ACTIVE_JPEG_QUALITY.store(72, Ordering::Relaxed);
            ACTIVE_SCALE_PCT.store(85, Ordering::Relaxed);
            ACTIVE_CHROMA_444.store(true, Ordering::Relaxed);
        }
        4 => {
            // Low Data / Speed
            ACTIVE_JPEG_QUALITY.store(60, Ordering::Relaxed);
            ACTIVE_SCALE_PCT.store(70, Ordering::Relaxed);
            ACTIVE_CHROMA_444.store(false, Ordering::Relaxed);
        }
        _ => {
            // Auto Adaptive initial baseline
            ACTIVE_JPEG_QUALITY.store(72, Ordering::Relaxed);
            ACTIVE_SCALE_PCT.store(100, Ordering::Relaxed);
            ACTIVE_CHROMA_444.store(true, Ordering::Relaxed);
        }
    }
}

pub fn update_client_preferences(target_fps: u32, capture_cursor: bool) {
    CLIENT_TARGET_FPS.store(target_fps.clamp(10, 60), Ordering::Relaxed);
    CLIENT_CAPTURE_CURSOR.store(capture_cursor, Ordering::Relaxed);
}

pub fn send_remote_input(event: InputEvent) {
    if !CLIENT_SESSION_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut q) = input_store().lock() {
        if matches!(event, InputEvent::MouseMove { .. }) {
            if let Some(last) = q.last_mut() {
                if matches!(last, InputEvent::MouseMove { .. }) {
                    *last = event;
                    input_notify().notify_one();
                    return;
                }
            }
        }
        if q.len() < 128 {
            q.push(event);
            input_notify().notify_one();
        }
    }
}

pub fn stop_client_session() {
    CLIENT_SESSION_ACTIVE.store(false, Ordering::SeqCst);
    CLIENT_SESSION_GEN.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut g) = frame_store().lock() {
        *g = None;
    }
    if let Ok(mut q) = input_store().lock() {
        q.clear();
    }
    SESSION_BITRATE_KBPS.store(0, Ordering::Relaxed);
    set_session_status(SessionStatus::Idle);
    input_notify().notify_one();
}

pub fn normalize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_digit()).collect()
}

pub fn format_id_display(raw: &str) -> String {
    let digits = normalize_id(raw);
    if digits.len() == 9 && digits != "000000000" {
        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
    } else if !raw.trim().is_empty() && raw != "000000000" {
        raw.to_string()
    } else {
        "Remote Peer".to_string()
    }
}

pub fn resolve_server_addr() -> SocketAddr {
    let server_str = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| "0.0.0.0:21116".into());
    server_str
        .parse()
        .unwrap_or_else(|_| "0.0.0.0:21116".parse().unwrap())
}

/// Wraps a standard DSKY packet into a VDS Relay packet addressed to `target_id`
pub fn wrap_relay_packet(target_id: &str, inner_packet: &[u8]) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(14 + inner_packet.len());
    pkt.extend_from_slice(MAGIC_DSKY);
    pkt.push(PKT_RELAY);
    let norm = normalize_id(target_id);
    let mut padded = [b'0'; 9];
    let b = norm.as_bytes();
    let copy_len = b.len().min(9);
    padded[9 - copy_len..].copy_from_slice(&b[..copy_len]);
    pkt.extend_from_slice(&padded);
    pkt.extend_from_slice(inner_packet);
    pkt
}

pub fn fix_loopback_peer(mut peer: SocketAddr, server: SocketAddr) -> SocketAddr {
    if peer.ip().is_loopback() && !server.ip().is_loopback() {
        peer.set_ip(server.ip());
    }
    peer
}

pub fn decode_jpeg_to_bgra(
    frame_id: u32,
    orig_width: u32,
    orig_height: u32,
    jpeg_bytes: &[u8],
) -> Option<RemoteFrame> {
    let mut decoder = zune_jpeg::JpegDecoder::new(jpeg_bytes);
    let rgb = decoder.decode().ok()?;
    let info = decoder.info()?;
    let w = info.width as u32;
    let h = info.height as u32;
    if w == 0 || h == 0 || rgb.len() < (w as usize) * (h as usize) * 3 {
        return None;
    }

    let pixel_count = (w as usize) * (h as usize);
    let mut bgra = vec![255u8; pixel_count * 4];
    for i in 0..pixel_count {
        let s = i * 3;
        let d = i * 4;
        bgra[d] = rgb[s + 2];     // B
        bgra[d + 1] = rgb[s + 1]; // G
        bgra[d + 2] = rgb[s];     // R
    }

    Some(RemoteFrame {
        frame_id,
        orig_width: orig_width.max(w),
        orig_height: orig_height.max(h),
        width: w,
        height: h,
        bgra,
    })
}
