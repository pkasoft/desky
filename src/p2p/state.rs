use crate::protocol::InputEvent;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::net::UdpSocket;
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
    pub addr: SocketAddr,
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
static HOST_SOCKET: OnceLock<Mutex<Option<Arc<UdpSocket>>>> = OnceLock::new();

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

pub fn host_socket_store() -> &'static Mutex<Option<Arc<UdpSocket>>> {
    HOST_SOCKET.get_or_init(|| Mutex::new(None))
}

pub fn get_pending_approval() -> Option<PendingApprovalRequest> {
    pending_approval_store().lock().ok().and_then(|g| g.clone())
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

    let mut acks_to_send: Vec<(SocketAddr, u8)> = Vec::new();
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
                    acks_to_send.push((sub.addr, AUTH_APPROVED));
                } else {
                    sub.auth_state = AuthState::Declined(now);
                    acks_to_send.push((sub.addr, AUTH_DECLINED));
                }
            }
        }
    }

    if !acks_to_send.is_empty() {
        if let Ok(Some(sock)) = host_socket_store().lock().map(|g| g.clone()) {
            let server_addr = resolve_server_addr();
            let my_id = {
                let ident = crate::identity::load_or_create_identity();
                ident.desky_id.unwrap_or_default()
            };
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(async move {
                        for (addr, status_byte) in acks_to_send {
                            let mut auth_pkt = Vec::with_capacity(6);
                            auth_pkt.extend_from_slice(MAGIC_DSKY);
                            auth_pkt.push(PKT_AUTH_STATUS);
                            auth_pkt.push(status_byte);

                            // Send directly to peer
                            for _ in 0..4 {
                                let _ = sock.send_to(&auth_pkt, addr).await;
                            }
                            // ALSO send via VDS Relay so NAT never blocks approval!
                            let relay_pkt = wrap_relay_packet(&my_id, &auth_pkt);
                            for _ in 0..4 {
                                let _ = sock.send_to(&relay_pkt, server_addr).await;
                            }
                        }
                    });
                }
            });
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
            // Auto Adaptive initial baseline: 100% 1:1 native capture (zero downscale CPU overhead)
            ACTIVE_JPEG_QUALITY.store(74, Ordering::Relaxed);
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
