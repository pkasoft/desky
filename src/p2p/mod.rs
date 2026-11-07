use crate::protocol::{InputEvent, SignalMsg};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Notify;
use tracing::{error, info, warn};

const MAGIC_DSKY: &[u8; 4] = b"DSKY";

const PKT_CLIENT_HELLO: u8 = 0x01;
const PKT_FRAME_CHUNK: u8 = 0x02;
const PKT_INPUT_EVENT: u8 = 0x03;
const PKT_DISCONNECT: u8 = 0x04;
const PKT_LAN_DISCOVERY: u8 = 0x05;
const PKT_PONG: u8 = 0x06;
const PKT_AUTH_STATUS: u8 = 0x07;

const AUTH_PENDING: u8 = 0;
const AUTH_APPROVED: u8 = 1;
const AUTH_DECLINED: u8 = 2;

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
enum AuthState {
    Pending,
    Approved,
    Declined(Instant),
}

static LATEST_REMOTE_FRAME: OnceLock<Mutex<Option<Arc<RemoteFrame>>>> = OnceLock::new();
static SESSION_STATUS: OnceLock<Mutex<SessionStatus>> = OnceLock::new();
static INPUT_QUEUE: OnceLock<Mutex<Vec<InputEvent>>> = OnceLock::new();
static INPUT_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

static PENDING_APPROVAL: OnceLock<Mutex<Option<PendingApprovalRequest>>> = OnceLock::new();
static HOST_SUBSCRIBERS: OnceLock<Arc<Mutex<HashMap<String, ClientSubscriber>>>> = OnceLock::new();
static HOST_SOCKET: OnceLock<Mutex<Option<Arc<UdpSocket>>>> = OnceLock::new();

static CLIENT_SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLIENT_SESSION_GEN: AtomicU32 = AtomicU32::new(0);
static CLIENT_TARGET_FPS: AtomicU32 = AtomicU32::new(60);
static CLIENT_CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

// Quality / Bitrate Mode:
// 0 = Auto Adaptive (ABR based on network speed & loss)
// 1 = Ultra Text (100% Native, Q90, 4:4:4 Chroma)
// 2 = High Clarity (100% Native, Q80, 4:4:4 Chroma)
// 3 = Balanced (85% Scale, Q74, 4:4:4 Chroma)
// 4 = Low Data / Fast (70% Scale, Q60, 4:2:0 Chroma)
static QUALITY_MODE: AtomicU8 = AtomicU8::new(0);

static ACTIVE_JPEG_QUALITY: AtomicU8 = AtomicU8::new(76);
static ACTIVE_SCALE_PCT: AtomicU8 = AtomicU8::new(100);
static ACTIVE_CHROMA_444: AtomicBool = AtomicBool::new(true);
static SESSION_RTT_MS: AtomicU32 = AtomicU32::new(0);
static SESSION_BITRATE_KBPS: AtomicU32 = AtomicU32::new(0);

fn frame_store() -> &'static Mutex<Option<Arc<RemoteFrame>>> {
    LATEST_REMOTE_FRAME.get_or_init(|| Mutex::new(None))
}

fn status_store() -> &'static Mutex<SessionStatus> {
    SESSION_STATUS.get_or_init(|| Mutex::new(SessionStatus::Idle))
}

fn input_store() -> &'static Mutex<Vec<InputEvent>> {
    INPUT_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

fn input_notify() -> &'static Arc<Notify> {
    INPUT_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

fn pending_approval_store() -> &'static Mutex<Option<PendingApprovalRequest>> {
    PENDING_APPROVAL.get_or_init(|| Mutex::new(None))
}

fn host_subscribers_store() -> &'static Arc<Mutex<HashMap<String, ClientSubscriber>>> {
    HOST_SUBSCRIBERS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn host_socket_store() -> &'static Mutex<Option<Arc<UdpSocket>>> {
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
                                        if let Ok(Some(sock)) = host_socket_store()
                                            .lock()
                                            .map(|g| g.clone())
                                        {
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
                                                            for _ in 0..4 {
                                                                let _ = sock.send_to(&auth_pkt, addr).await;
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

fn set_session_status(status: SessionStatus) {
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

fn apply_preset_for_mode(mode: u8) {
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

fn normalize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn format_id_display(raw: &str) -> String {
    let digits = normalize_id(raw);
    if digits.len() == 9 && digits != "000000000" {
        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
    } else if !raw.trim().is_empty() && raw != "000000000" {
        raw.to_string()
    } else {
        "Remote Peer".to_string()
    }
}

fn resolve_server_addr() -> SocketAddr {
    let server_str = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| "0.0.0.0:21116".into());
    server_str
        .parse()
        .unwrap_or_else(|_| "0.0.0.0:21116".parse().unwrap())
}

fn fix_loopback_peer(mut peer: SocketAddr, server: SocketAddr) -> SocketAddr {
    if peer.ip().is_loopback() && !server.ip().is_loopback() {
        peer.set_ip(server.ip());
    }
    peer
}

#[derive(Clone)]
struct ClientSubscriber {
    client_id: String,
    addr: SocketAddr,
    auth_state: AuthState,
    last_seen: Instant,
    target_fps: u32,
    capture_cursor: bool,
    jpeg_quality: u8,
    scale_pct: u8,
    chroma_444: bool,
}

struct PartialFrame {
    frame_id: u32,
    total_chunks: u16,
    orig_width: u16,
    orig_height: u16,
    chunks: Vec<Option<Vec<u8>>>,
    received_count: u16,
    created_at: Instant,
}

#[cfg(target_os = "linux")]
fn trigger_linux_approval_dialog(client_id: String) {
    crate::linux::ui::spawn_linux_approval_popup(client_id);
}

/// Starts a remote desktop client session connecting directly to `target_id`
/// (VDS is used ONLY for address lookup & UDP hole-punching; all screen/input traffic is direct P2P)
pub fn start_client_session(target_id: &str, initial_fps: u32, capture_cursor: bool) {
    let norm_target = normalize_id(target_id);
    let display_target = if norm_target.len() == 9 {
        format!(
            "{} {} {}",
            &norm_target[0..3],
            &norm_target[3..6],
            &norm_target[6..9]
        )
    } else {
        target_id.to_string()
    };

    if let Ok(mut g) = frame_store().lock() {
        *g = None;
    }
    if let Ok(mut q) = input_store().lock() {
        q.clear();
    }

    update_client_preferences(initial_fps, capture_cursor);
    apply_preset_for_mode(QUALITY_MODE.load(Ordering::Relaxed));
    SESSION_RTT_MS.store(0, Ordering::Relaxed);
    SESSION_BITRATE_KBPS.store(0, Ordering::Relaxed);
    set_session_status(SessionStatus::Connecting(display_target.clone()));

    let my_gen = CLIENT_SESSION_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    CLIENT_SESSION_ACTIVE.store(true, Ordering::SeqCst);

    let my_id_norm = {
        let ident = crate::identity::load_or_create_identity();
        normalize_id(&ident.desky_id.unwrap_or_else(|| "000000000".into()))
    };
    let my_id_padded = if my_id_norm.len() == 9 {
        my_id_norm
    } else {
        "000000000".to_string()
    };

    let notify = Arc::clone(input_notify());

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to build client Tokio runtime: {:?}", e);
                return;
            }
        };

        rt.block_on(async move {
            let socket = match UdpSocket::bind("0.0.0.0:0").await {
                Ok(s) => {
                    let _ = s.set_broadcast(true);
                    Arc::new(s)
                }
                Err(e) => {
                    set_session_status(SessionStatus::Error(format!("Socket error: {}", e)));
                    return;
                }
            };

            let server_addr = resolve_server_addr();

            // 1. Fire LAN discovery broadcast on port 21117 (for same-router / LAN peers)
            let mut lan_pkt = Vec::with_capacity(14);
            if norm_target.len() == 9 {
                lan_pkt.extend_from_slice(MAGIC_DSKY);
                lan_pkt.push(PKT_LAN_DISCOVERY);
                lan_pkt.extend_from_slice(norm_target.as_bytes());
                let _ = socket
                    .send_to(&lan_pkt, "255.255.255.255:21117".parse::<SocketAddr>().unwrap())
                    .await;
                let _ = socket
                    .send_to(&lan_pkt, "127.0.0.1:21117".parse::<SocketAddr>().unwrap())
                    .await;
            }

            // 2. Send ConnectRequest to VDS Matchmaker (Strictly for IP:Port lookup & hole-punching)
            let req = SignalMsg::ConnectRequest {
                target_id: display_target.clone(),
            };
            if let Ok(req_bytes) = serde_json::to_vec(&req) {
                let _ = socket.send_to(&req_bytes, server_addr).await;
            }

            let mut peer_addr: Option<SocketAddr> = None;
            let mut host_contacted = false;
            let mut buf = vec![0u8; 65536];
            let mut partial_frame: Option<PartialFrame> = None;
            let mut highest_decoded_frame: u32 = 0;

            let session_start = Instant::now();
            let mut last_hello = Instant::now() - Duration::from_secs(10);
            let mut last_matchmaker_retry = Instant::now();
            let mut received_any_frame = false;

            // Dedicated Latest-Frame JPEG Decoder Thread so UDP recv_from NEVER blocks on CPU decoding!
            let decode_slot: Arc<(Mutex<Option<(u32, u32, u32, Vec<u8>)>>, std::sync::Condvar)> =
                Arc::new((Mutex::new(None), std::sync::Condvar::new()));
            {
                let slot_worker = Arc::clone(&decode_slot);
                let disp_worker = display_target.clone();
                std::thread::spawn(move || {
                    let mut logged_first = false;
                    while CLIENT_SESSION_ACTIVE.load(Ordering::Relaxed)
                        && CLIENT_SESSION_GEN.load(Ordering::Relaxed) == my_gen
                    {
                        let job = {
                            let (lock, cvar) = &*slot_worker;
                            let mut guard = match lock.lock() {
                                Ok(g) => g,
                                Err(_) => break,
                            };
                            if guard.is_none() {
                                let (g, _) = match cvar.wait_timeout(guard, Duration::from_millis(50)) {
                                    Ok(res) => res,
                                    Err(_) => break,
                                };
                                guard = g;
                            }
                            guard.take()
                        };

                        if let Some((f_id, orig_w, orig_h, jpeg_bytes)) = job {
                            if let Some(decoded) = decode_jpeg_to_bgra(f_id, orig_w, orig_h, &jpeg_bytes) {
                                if !logged_first {
                                    logged_first = true;
                                    set_session_status(SessionStatus::Connected(disp_worker.clone()));
                                    #[cfg(target_os = "windows")]
                                    crate::windows::ui::append_status_log(&format!(
                                        "🎉 Connected to Remote Desk {} ({}x{})!",
                                        disp_worker, decoded.orig_width, decoded.orig_height
                                    ));
                                }
                                if let Ok(mut g) = frame_store().lock() {
                                    *g = Some(Arc::new(decoded));
                                }
                            }
                        }
                    }
                });
            }

            // Dynamic Adaptive Bitrate (ABR) telemetry state
            let mut abr_window_start = Instant::now();
            let mut abr_bytes_received: u64 = 0;
            let mut abr_frames_completed: u32 = 0;
            let mut abr_frames_dropped: u32 = 0;
            let mut abr_tier: i32 = 2;

            while CLIENT_SESSION_ACTIVE.load(Ordering::Relaxed)
                && CLIENT_SESSION_GEN.load(Ordering::Relaxed) == my_gen
            {
                // Update ABR & live bitrate telemetry every 650ms
                let abr_elapsed = abr_window_start.elapsed();
                if abr_elapsed >= Duration::from_millis(650) {
                    let secs = abr_elapsed.as_secs_f64().max(0.1);
                    let kbps = ((abr_bytes_received as f64 * 8.0) / 1000.0 / secs) as u32;
                    SESSION_BITRATE_KBPS.store(kbps, Ordering::Relaxed);

                    if QUALITY_MODE.load(Ordering::Relaxed) == 0 {
                        let rtt = SESSION_RTT_MS.load(Ordering::Relaxed);
                        if abr_frames_dropped >= 2 || rtt > 220 {
                            abr_tier = (abr_tier - 1).max(0);
                        } else if received_any_frame
                            && abr_frames_dropped == 0
                            && abr_frames_completed >= 5
                            && rtt < 110
                        {
                            abr_tier = (abr_tier + 1).min(3);
                        }

                        // Keep 100% 1:1 native capture on Tiers 1..3 so Host never wastes CPU downscaling!
                        let (q, sc, c444) = match abr_tier {
                            3 => (82u8, 100u8, true),
                            2 => (75u8, 100u8, true),
                            1 => (68u8, 100u8, true),
                            _ => (58u8, 80u8, false),
                        };
                        ACTIVE_JPEG_QUALITY.store(q, Ordering::Relaxed);
                        ACTIVE_SCALE_PCT.store(sc, Ordering::Relaxed);
                        ACTIVE_CHROMA_444.store(c444, Ordering::Relaxed);
                    }

                    abr_window_start = Instant::now();
                    abr_bytes_received = 0;
                    abr_frames_completed = 0;
                    abr_frames_dropped = 0;
                }

                // Retry ConnectRequest + LAN discovery every 1.2s until host responds
                if !host_contacted && last_matchmaker_retry.elapsed() >= Duration::from_millis(1200) {
                    if session_start.elapsed() > Duration::from_secs(15) {
                        let msg = format!(
                            "Timed out connecting to Desky ID '{}'",
                            display_target
                        );
                        set_session_status(SessionStatus::Error(msg.clone()));
                        #[cfg(target_os = "windows")]
                        crate::windows::ui::append_status_log(&format!("❌ {}", msg));
                        break;
                    }
                    if !lan_pkt.is_empty() {
                        let _ = socket
                            .send_to(&lan_pkt, "255.255.255.255:21117".parse::<SocketAddr>().unwrap())
                            .await;
                        let _ = socket
                            .send_to(&lan_pkt, "127.0.0.1:21117".parse::<SocketAddr>().unwrap())
                            .await;
                    }
                    if let Ok(req_bytes) = serde_json::to_vec(&req) {
                        let _ = socket.send_to(&req_bytes, server_addr).await;
                    }
                    last_matchmaker_retry = Instant::now();
                }

                // Periodically send ClientHello / KeepAlive with ABR quality parameters & client ID directly to peer
                let hello_interval = if received_any_frame {
                    Duration::from_millis(350)
                } else {
                    Duration::from_millis(160)
                };

                if last_hello.elapsed() >= hello_interval {
                    if let Some(p_addr) = peer_addr {
                        let fps = CLIENT_TARGET_FPS.load(Ordering::Relaxed).clamp(10, 60) as u8;
                        let cur = if CLIENT_CAPTURE_CURSOR.load(Ordering::Relaxed) { 1u8 } else { 0u8 };
                        let ts_ms = (session_start.elapsed().as_millis() & 0xFFFF_FFFF) as u32;
                        let (q_val, sc_val, c444) = get_active_quality_params();

                        let mut hello = Vec::with_capacity(23);
                        hello.extend_from_slice(MAGIC_DSKY);
                        hello.push(PKT_CLIENT_HELLO);
                        hello.push(fps);
                        hello.push(cur);
                        hello.extend_from_slice(&ts_ms.to_le_bytes());
                        hello.push(q_val);
                        hello.push(sc_val);
                        hello.push(if c444 { 1u8 } else { 0u8 });
                        hello.extend_from_slice(my_id_padded.as_bytes());

                        let _ = socket.send_to(&hello, p_addr).await;
                        if !host_contacted {
                            // Also probe neighboring ports (+1..+8) in case the Host's router uses Symmetric NAT
                            let base_ip = p_addr.ip();
                            let base_port = p_addr.port();
                            for offset in 1..=8u16 {
                                if let Some(p) = base_port.checked_add(offset) {
                                    let _ = socket.send_to(&hello, SocketAddr::new(base_ip, p)).await;
                                }
                            }
                        }
                    }
                    last_hello = Instant::now();
                }

                // Flush queued remote input events immediately over direct P2P UDP
                let queued_inputs: Vec<InputEvent> = {
                    if let Ok(mut q) = input_store().lock() {
                        std::mem::take(&mut *q)
                    } else {
                        Vec::new()
                    }
                };
                if !queued_inputs.is_empty() && received_any_frame {
                    if let Some(p_addr) = peer_addr {
                        for ev in queued_inputs {
                            if let Ok(ev_bytes) = serde_json::to_vec(&ev) {
                                let mut pkt = Vec::with_capacity(5 + ev_bytes.len());
                                pkt.extend_from_slice(MAGIC_DSKY);
                                pkt.push(PKT_INPUT_EVENT);
                                pkt.extend_from_slice(&ev_bytes);
                                let _ = socket.send_to(&pkt, p_addr).await;
                            }
                        }
                    }
                }

                // Event-driven select: wakes immediately on incoming UDP packet OR local mouse/keyboard input!
                tokio::select! {
                    biased;
                    recv_res = socket.recv_from(&mut buf) => {
                        if let Ok((len, from_addr)) = recv_res {
                            abr_bytes_received += len as u64;
                            let slice = &buf[..len];

                            if slice.len() >= 5 && &slice[0..4] == MAGIC_DSKY {
                                let pkt_type = slice[4];
                                host_contacted = true;
                                peer_addr = Some(from_addr);

                                match pkt_type {
                                    PKT_PONG => {
                                        if slice.len() >= 9 {
                                            let sent_ts = u32::from_le_bytes([
                                                slice[5], slice[6], slice[7], slice[8],
                                            ]);
                                            let now_ts = (session_start.elapsed().as_millis() & 0xFFFF_FFFF) as u32;
                                            let rtt = now_ts.wrapping_sub(sent_ts);
                                            if rtt < 10_000 {
                                                SESSION_RTT_MS.store(rtt, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                    PKT_AUTH_STATUS => {
                                        if slice.len() >= 6 {
                                            match slice[5] {
                                                AUTH_PENDING => {
                                                    if !received_any_frame {
                                                        set_session_status(SessionStatus::WaitingApproval(
                                                            display_target.clone(),
                                                        ));
                                                    }
                                                }
                                                AUTH_APPROVED => {
                                                    if !received_any_frame {
                                                        set_session_status(SessionStatus::Connected(
                                                            display_target.clone(),
                                                        ));
                                                    }
                                                }
                                                AUTH_DECLINED => {
                                                    let msg = format!(
                                                        "Connection declined by Remote Desk {}",
                                                        display_target
                                                    );
                                                    set_session_status(SessionStatus::Error(msg.clone()));
                                                    #[cfg(target_os = "windows")]
                                                    crate::windows::ui::append_status_log(&format!("🚫 {}", msg));
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    PKT_FRAME_CHUNK => {
                                        if slice.len() > 17 {
                                            let frame_id = u32::from_le_bytes([
                                                slice[5], slice[6], slice[7], slice[8],
                                            ]);
                                            let chunk_idx = u16::from_le_bytes([slice[9], slice[10]]);
                                            let total_chunks = u16::from_le_bytes([slice[11], slice[12]]);
                                            let orig_width = u16::from_le_bytes([slice[13], slice[14]]);
                                            let orig_height = u16::from_le_bytes([slice[15], slice[16]]);
                                            let payload = &slice[17..];

                                            if frame_id <= highest_decoded_frame
                                                || total_chunks == 0
                                                || total_chunks > 2048
                                                || chunk_idx >= total_chunks
                                            {
                                                continue;
                                            }

                                            let pf = match &mut partial_frame {
                                                Some(existing) if existing.frame_id == frame_id => existing,
                                                Some(old_incomplete) => {
                                                    if frame_id < old_incomplete.frame_id {
                                                        continue;
                                                    }
                                                    if old_incomplete.received_count < old_incomplete.total_chunks {
                                                        abr_frames_dropped += 1;
                                                    }
                                                    partial_frame = Some(PartialFrame {
                                                        frame_id,
                                                        total_chunks,
                                                        orig_width,
                                                        orig_height,
                                                        chunks: vec![None; total_chunks as usize],
                                                        received_count: 0,
                                                        created_at: Instant::now(),
                                                    });
                                                    partial_frame.as_mut().unwrap()
                                                }
                                                None => {
                                                    partial_frame = Some(PartialFrame {
                                                        frame_id,
                                                        total_chunks,
                                                        orig_width,
                                                        orig_height,
                                                        chunks: vec![None; total_chunks as usize],
                                                        received_count: 0,
                                                        created_at: Instant::now(),
                                                    });
                                                    partial_frame.as_mut().unwrap()
                                                }
                                            };

                                            if pf.chunks[chunk_idx as usize].is_none() {
                                                pf.chunks[chunk_idx as usize] = Some(payload.to_vec());
                                                pf.received_count += 1;
                                            }

                                            if pf.received_count == pf.total_chunks {
                                                let mut jpeg_data = Vec::with_capacity(
                                                    (pf.total_chunks as usize) * 1240,
                                                );
                                                for c in pf.chunks.iter().flatten() {
                                                    jpeg_data.extend_from_slice(c);
                                                }
                                                let orig_w = pf.orig_width as u32;
                                                let orig_h = pf.orig_height as u32;
                                                highest_decoded_frame = frame_id;
                                                partial_frame = None;
                                                abr_frames_completed += 1;
                                                received_any_frame = true;

                                                // Hand off JPEG buffer to dedicated decoder thread in <1µs
                                                // so UDP recv_from never pauses and never drops incoming chunks!
                                                let (lock, cvar) = &*decode_slot;
                                                if let Ok(mut slot) = lock.lock() {
                                                    *slot = Some((frame_id, orig_w, orig_h, jpeg_data));
                                                    cvar.notify_one();
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            } else if let Ok(sig) = serde_json::from_slice::<SignalMsg>(slice) {
                                match sig {
                                    SignalMsg::PeerFound {
                                        target_id: _,
                                        peer_public_addr,
                                    } => {
                                        if let Ok(parsed) = peer_public_addr.parse::<SocketAddr>() {
                                            let resolved = fix_loopback_peer(parsed, server_addr);
                                            info!(
                                                "⚡ Peer '{}' resolved at {}. Punching Direct UDP tunnel...",
                                                display_target, resolved
                                            );
                                            if peer_addr.is_none() {
                                                peer_addr = Some(resolved);
                                            }
                                            let punch = SignalMsg::Punch {
                                                from_id: my_id_padded.clone(),
                                            };
                                            if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                                for _ in 0..4 {
                                                    let _ = socket.send_to(&pbytes, resolved).await;
                                                }
                                            }
                                            // Also immediately fire a ClientHello so the host sees the request right away
                                            last_hello = Instant::now() - Duration::from_secs(10);
                                        }
                                    }
                                    SignalMsg::PeerNotFound { target_id } => {
                                        let msg = format!(
                                            "Desky ID '{}' is offline or not found",
                                            target_id
                                        );
                                        warn!("{}", msg);
                                        set_session_status(SessionStatus::Error(msg.clone()));
                                        #[cfg(target_os = "windows")]
                                        crate::windows::ui::append_status_log(&format!("❌ {}", msg));
                                        break;
                                    }
                                    SignalMsg::Punch { .. } | SignalMsg::PunchAck => {
                                        peer_addr = Some(from_addr);
                                        let ack = SignalMsg::PunchAck;
                                        if let Ok(abytes) = serde_json::to_vec(&ack) {
                                            let _ = socket.send_to(&abytes, from_addr).await;
                                        }
                                        last_hello = Instant::now() - Duration::from_secs(10);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ = notify.notified() => {
                        // Woken immediately to flush mouse/keyboard input!
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                }

                if let Some(pf) = &partial_frame {
                    if pf.created_at.elapsed() > Duration::from_millis(450) {
                        abr_frames_dropped += 1;
                        partial_frame = None;
                    }
                }
            }

            if let Some(p_addr) = peer_addr {
                let mut disc = Vec::with_capacity(5);
                disc.extend_from_slice(MAGIC_DSKY);
                disc.push(PKT_DISCONNECT);
                let _ = socket.send_to(&disc, p_addr).await;
            }
        });
    });
}

fn decode_jpeg_to_bgra(
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

pub struct P2PManager {
    machine_id: String,
    cached_id: Option<String>,
    server_addr: String,
    is_running: Arc<AtomicBool>,
}

impl P2PManager {
    pub fn new(machine_id: String, cached_id: Option<String>, server_addr: String) -> Self {
        Self {
            machine_id,
            cached_id,
            server_addr,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Spawns the background UDP rendezvous registration, keep-alive, and Direct P2P Host screen streamer
    pub fn start_host_registration(&self) {
        let machine_id = self.machine_id.clone();
        let cached_id = self.cached_id.clone();
        let server_addr_str = self.server_addr.clone();
        let running = Arc::clone(&self.is_running);
        running.store(true, Ordering::SeqCst);

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    error!("Failed to create Tokio runtime for P2P: {:?}", e);
                    return;
                }
            };

            rt.block_on(async move {
                let socket = match UdpSocket::bind("0.0.0.0:0").await {
                    Ok(s) => {
                        let arc_s = Arc::new(s);
                        if let Ok(mut g) = host_socket_store().lock() {
                            *g = Some(Arc::clone(&arc_s));
                        }
                        arc_s
                    }
                    Err(e) => {
                        error!("Failed to bind local UDP socket for P2P: {:?}", e);
                        return;
                    }
                };

                let target_server: SocketAddr = match server_addr_str.parse() {
                    Ok(a) => a,
                    Err(_) => {
                        warn!(
                            "Could not parse VDS server address '{}', using default 0.0.0.0:21116",
                            server_addr_str
                        );
                        "0.0.0.0:21116".parse().unwrap()
                    }
                };

                info!(
                    "⚡ P2P Engine: Requesting server identity verification for machine '{}' at {}...",
                    machine_id, target_server
                );

                let reg_msg = SignalMsg::RegisterHost {
                    machine_id: machine_id.clone(),
                    cached_id: cached_id.clone(),
                };
                if let Ok(bytes) = serde_json::to_vec(&reg_msg) {
                    let _ = socket.send_to(&bytes, target_server).await;
                }

                let active_clients = Arc::clone(host_subscribers_store());

                let current_id = Arc::new(tokio::sync::RwLock::new(
                    cached_id.clone().unwrap_or_default(),
                ));

                // Optional LAN discovery responder on UDP 21117
                {
                    let socket_lan_main = Arc::clone(&socket);
                    let id_lan = Arc::clone(&current_id);
                    tokio::spawn(async move {
                        if let Ok(lan_sock) = UdpSocket::bind("0.0.0.0:21117").await {
                            let mut lbuf = vec![0u8; 512];
                            while let Ok((len, src)) = lan_sock.recv_from(&mut lbuf).await {
                                if len >= 14 && &lbuf[0..4] == MAGIC_DSKY && lbuf[4] == PKT_LAN_DISCOVERY {
                                    let req_id = String::from_utf8_lossy(&lbuf[5..14]).to_string();
                                    let my_id = normalize_id(&id_lan.read().await);
                                    if !my_id.is_empty() && my_id == req_id {
                                        info!("⚡ [LAN DISCOVERY] Peer at {} requested our Desky ID! Punching directly...", src);
                                        let punch = SignalMsg::Punch { from_id: my_id };
                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                            let _ = socket_lan_main.send_to(&pbytes, src).await;
                                        }
                                    }
                                }
                            }
                        }
                    });
                }

                // Dedicated Host Screen Capture & Direct P2P Streamer Thread
                {
                    let clients_stream = Arc::clone(&active_clients);
                    let running_stream = Arc::clone(&running);
                    let socket_stream = Arc::clone(&socket);

                    std::thread::spawn(move || {
                        let stream_rt = match tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            Ok(r) => r,
                            Err(e) => {
                                error!("Failed to build streamer Tokio runtime: {:?}", e);
                                return;
                            }
                        };

                        stream_rt.block_on(async move {
                            let mut frame_id: u32 = 1;
                            let mut jpeg_buf: Vec<u8> = Vec::with_capacity(98304);
                            let mut last_jpeg_len: usize = 0;

                            while running_stream.load(Ordering::Relaxed) {
                                let loop_start = Instant::now();

                                // Only stream to APPROVED subscribers!
                                let subscribers: Vec<ClientSubscriber> = {
                                    if let Ok(mut map) = clients_stream.lock() {
                                        map.retain(|_, sub| sub.last_seen.elapsed() < Duration::from_secs(20));
                                        map.values()
                                            .filter(|s| s.auth_state == AuthState::Approved)
                                            .cloned()
                                            .collect()
                                    } else {
                                        Vec::new()
                                    }
                                };

                                if subscribers.is_empty() {
                                    std::thread::sleep(Duration::from_millis(12));
                                    continue;
                                }

                                let max_fps = subscribers
                                    .iter()
                                    .map(|s| s.target_fps)
                                    .max()
                                    .unwrap_or(60)
                                    .clamp(15, 60);
                                let cap_cursor = subscribers.iter().any(|s| s.capture_cursor);
                                let req_quality = subscribers
                                    .iter()
                                    .map(|s| s.jpeg_quality)
                                    .max()
                                    .unwrap_or(75)
                                    .clamp(40, 92);
                                let scale_pct = subscribers
                                    .iter()
                                    .map(|s| s.scale_pct)
                                    .max()
                                    .unwrap_or(100)
                                    .clamp(50, 100);
                                let req_chroma_444 = subscribers.iter().any(|s| s.chroma_444);

                                // Single-pass adaptive sizing based on previous frame size (zero double-encoding!)
                                let (jpeg_quality, use_chroma_444) = if last_jpeg_len > 72_000 {
                                    ((req_quality.saturating_sub(10)).max(55), false)
                                } else if last_jpeg_len > 55_000 {
                                    ((req_quality.saturating_sub(5)).max(62), req_chroma_444)
                                } else {
                                    (req_quality, req_chroma_444)
                                };

                                #[cfg(target_os = "windows")]
                                let cap_res = crate::windows::capture::capture_screen_frame(cap_cursor, scale_pct);
                                #[cfg(target_os = "linux")]
                                let cap_res = crate::linux::capture::capture_screen_frame(cap_cursor, scale_pct);

                                if let Ok((orig_w, orig_h, cap_w, cap_h, bgra)) = cap_res {
                                    jpeg_buf.clear();
                                    let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, jpeg_quality);
                                    encoder.set_sampling_factor(if use_chroma_444 {
                                        jpeg_encoder::SamplingFactor::R_4_4_4
                                    } else {
                                        jpeg_encoder::SamplingFactor::R_4_2_0
                                    });

                                    if encoder
                                        .encode(
                                            &bgra,
                                            cap_w as u16,
                                            cap_h as u16,
                                            jpeg_encoder::ColorType::Bgra,
                                        )
                                        .is_ok()
                                    {
                                        last_jpeg_len = jpeg_buf.len();
                                        let chunk_size = 1280usize;
                                        let total_chunks = ((jpeg_buf.len() + chunk_size - 1) / chunk_size).max(1);

                                        if total_chunks <= 2048 {
                                            for (idx, chunk) in jpeg_buf.chunks(chunk_size).enumerate() {
                                                let mut pkt = Vec::with_capacity(17 + chunk.len());
                                                pkt.extend_from_slice(MAGIC_DSKY);
                                                pkt.push(PKT_FRAME_CHUNK);
                                                pkt.extend_from_slice(&frame_id.to_le_bytes());
                                                pkt.extend_from_slice(&(idx as u16).to_le_bytes());
                                                pkt.extend_from_slice(&(total_chunks as u16).to_le_bytes());
                                                pkt.extend_from_slice(&(orig_w as u16).to_le_bytes());
                                                pkt.extend_from_slice(&(orig_h as u16).to_le_bytes());
                                                pkt.extend_from_slice(chunk);

                                                for sub in &subscribers {
                                                    let _ = socket_stream.send_to(&pkt, sub.addr).await;
                                                }

                                                // Fast micro-pacing (140µs every 6 chunks = ~1ms total per frame)
                                                if (idx + 1) % 6 == 0 {
                                                    let t_pace = Instant::now();
                                                    while t_pace.elapsed() < Duration::from_micros(140) {
                                                        std::hint::spin_loop();
                                                    }
                                                }
                                            }
                                            frame_id = frame_id.wrapping_add(1).max(1);
                                        }
                                    }
                                }

                                let target_frame_time = Duration::from_micros(1_000_000 / max_fps as u64);
                                let elapsed = loop_start.elapsed();
                                if elapsed < target_frame_time {
                                    std::thread::sleep(target_frame_time - elapsed);
                                }
                            }
                        });
                    });
                }

                let socket_recv = Arc::clone(&socket);
                let current_id_clone = Arc::clone(&current_id);
                let clients_recv = Arc::clone(&active_clients);

                // Receiver task: listens for VDS matchmaking acks, peer hole-punching, and direct client packets
                tokio::spawn(async move {
                    #[cfg(target_os = "windows")]
                    let injector = crate::windows::input::InputInjector::new();
                    #[cfg(target_os = "linux")]
                    let injector = crate::linux::input::InputInjector::new();

                    let mut buf = vec![0u8; 8192];
                    while let Ok((len, from_addr)) = socket_recv.recv_from(&mut buf).await {
                        let slice = &buf[..len];

                        if slice.len() >= 5 && &slice[0..4] == MAGIC_DSKY {
                            let pkt_type = slice[4];
                            match pkt_type {
                                PKT_CLIENT_HELLO => {
                                    let fps = if slice.len() > 5 { slice[5].clamp(10, 60) as u32 } else { 60 };
                                    let cur = if slice.len() > 6 { slice[6] != 0 } else { true };
                                    let q_val = if slice.len() > 11 { slice[11].clamp(40, 92) } else { 76 };
                                    let sc_val = if slice.len() > 12 { slice[12].clamp(50, 100) } else { 90 };
                                    let c444 = if slice.len() > 13 { slice[13] != 0 } else { true };
                                    let raw_cid = if slice.len() >= 23 {
                                        String::from_utf8_lossy(&slice[14..23]).to_string()
                                    } else {
                                        String::new()
                                    };
                                    let norm_cid = normalize_id(&raw_cid);
                                    let client_key = if !norm_cid.is_empty() && norm_cid != "000000000" {
                                        norm_cid
                                    } else {
                                        from_addr.to_string()
                                    };
                                    let client_id_disp = format_id_display(&raw_cid);

                                    let mut need_ui_prompt = false;
                                    let auth_status_byte = {
                                        if let Ok(mut map) = clients_recv.lock() {
                                            // Reset expired declines after 4 seconds so user can retry connecting
                                            if let Some(existing) = map.get_mut(&client_key) {
                                                if let AuthState::Declined(when) = existing.auth_state {
                                                    if when.elapsed() > Duration::from_secs(4) {
                                                        map.remove(&client_key);
                                                    }
                                                }
                                            }

                                            let addr_key = from_addr.to_string();
                                            if client_key != addr_key {
                                                if let Some(mut pre_sub) = map.remove(&addr_key) {
                                                    pre_sub.client_id = client_id_disp.clone();
                                                    pre_sub.addr = from_addr;
                                                    pre_sub.last_seen = Instant::now();
                                                    map.insert(client_key.clone(), pre_sub);
                                                }
                                            }
                                            let entry = map.entry(client_key.clone()).or_insert_with(|| {
                                                info!(
                                                    "🔔 [HOST APPROVAL] Incoming connection request from {} ({})",
                                                    client_id_disp, from_addr
                                                );
                                                need_ui_prompt = true;
                                                ClientSubscriber {
                                                    client_id: client_id_disp.clone(),
                                                    addr: from_addr,
                                                    auth_state: AuthState::Pending,
                                                    last_seen: Instant::now(),
                                                    target_fps: fps,
                                                    capture_cursor: cur,
                                                    jpeg_quality: q_val,
                                                    scale_pct: sc_val,
                                                    chroma_444: c444,
                                                }
                                            });
                                            entry.client_id = client_id_disp.clone();
                                            entry.addr = from_addr;
                                            entry.last_seen = Instant::now();
                                            entry.target_fps = fps;
                                            entry.capture_cursor = cur;
                                            entry.jpeg_quality = q_val;
                                            entry.scale_pct = sc_val;
                                            entry.chroma_444 = c444;

                                            match entry.auth_state {
                                                AuthState::Pending => AUTH_PENDING,
                                                AuthState::Approved => AUTH_APPROVED,
                                                AuthState::Declined(_) => AUTH_DECLINED,
                                            }
                                        } else {
                                            AUTH_PENDING
                                        }
                                    };

                                    if need_ui_prompt {
                                        if let Ok(mut pg) = pending_approval_store().lock() {
                                            *pg = Some(PendingApprovalRequest {
                                                client_key: client_key.clone(),
                                                client_id: client_id_disp.clone(),
                                            });
                                        }
                                        #[cfg(target_os = "windows")]
                                        {
                                            crate::windows::ui::append_status_log(&format!(
                                                "🔔 Incoming request from ID: {} — Click Accept or Decline!",
                                                client_id_disp
                                            ));
                                            crate::windows::ui::bring_main_window_to_front();
                                        }
                                        #[cfg(target_os = "linux")]
                                        {
                                            trigger_linux_approval_dialog(client_id_disp.clone());
                                        }
                                    }

                                    // Send Auth Status packet directly back to Client
                                    let mut auth_pkt = Vec::with_capacity(6);
                                    auth_pkt.extend_from_slice(MAGIC_DSKY);
                                    auth_pkt.push(PKT_AUTH_STATUS);
                                    auth_pkt.push(auth_status_byte);
                                    let _ = socket_recv.send_to(&auth_pkt, from_addr).await;

                                    // Reply with Pong if timestamp included
                                    if slice.len() >= 11 {
                                        let mut pong = Vec::with_capacity(9);
                                        pong.extend_from_slice(MAGIC_DSKY);
                                        pong.push(PKT_PONG);
                                        pong.extend_from_slice(&slice[7..11]);
                                        let _ = socket_recv.send_to(&pong, from_addr).await;
                                    }
                                }
                                PKT_INPUT_EVENT => {
                                    let is_approved = {
                                        if let Ok(mut map) = clients_recv.lock() {
                                            let mut approved = false;
                                            for sub in map.values_mut() {
                                                if sub.addr == from_addr && sub.auth_state == AuthState::Approved {
                                                    sub.last_seen = Instant::now();
                                                    approved = true;
                                                }
                                            }
                                            approved
                                        } else {
                                            false
                                        }
                                    };
                                    if is_approved {
                                        if let Ok(ev) = serde_json::from_slice::<InputEvent>(&slice[5..]) {
                                            injector.handle_event(&ev);
                                        }
                                    }
                                }
                                PKT_DISCONNECT => {
                                    if let Ok(mut map) = clients_recv.lock() {
                                        map.retain(|_, sub| sub.addr != from_addr);
                                    }
                                    if let Ok(mut pg) = pending_approval_store().lock() {
                                        *pg = None;
                                    }
                                    #[cfg(target_os = "windows")]
                                    crate::windows::ui::request_main_redraw();
                                    info!("Client {} disconnected from host stream.", from_addr);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if let Ok(msg) = serde_json::from_slice::<SignalMsg>(slice) {
                            match msg {
                                SignalMsg::RegisterAck {
                                    desky_id,
                                    your_public_addr,
                                } => {
                                    info!(
                                        "✅ [VDS APPROVED] Permanent Desky ID '{}' assigned! Public NAT endpoint: {}",
                                        desky_id, your_public_addr
                                    );
                                    crate::identity::save_assigned_desky_id(&desky_id);
                                    #[cfg(target_os = "windows")]
                                    crate::windows::set_assigned_desky_id(&desky_id);
                                    #[cfg(target_os = "linux")]
                                    crate::linux::set_assigned_desky_id(&desky_id);

                                    let mut id_w = current_id_clone.write().await;
                                    *id_w = desky_id;
                                }
                                SignalMsg::IncomingPeer { client_public_addr } => {
                                    info!(
                                        "⚡ [P2P INCOMING] Peer connecting from {}! Firing Direct UDP hole-punch packets...",
                                        client_public_addr
                                    );
                                    if let Ok(parsed_addr) = client_public_addr.parse::<SocketAddr>() {
                                        let client_addr = fix_loopback_peer(parsed_addr, target_server);
                                        let client_key = client_addr.to_string();

                                        // Immediately register incoming peer & show Accept/Decline dialog on Host!
                                        // (Even before direct UDP hole-punch completes across symmetric/same-IP NATs)
                                        let mut need_ui_prompt = false;
                                        if let Ok(mut map) = clients_recv.lock() {
                                            let already_active = map.values().any(|sub| {
                                                sub.addr == client_addr
                                                    || (sub.auth_state == AuthState::Pending
                                                        && sub.last_seen.elapsed() < Duration::from_secs(12))
                                            });
                                            if !already_active {
                                                need_ui_prompt = true;
                                                map.insert(
                                                    client_key.clone(),
                                                    ClientSubscriber {
                                                        client_id: format!("Peer ({})", client_addr.ip()),
                                                        addr: client_addr,
                                                        auth_state: AuthState::Pending,
                                                        last_seen: Instant::now(),
                                                        target_fps: 60,
                                                        capture_cursor: true,
                                                        jpeg_quality: 76,
                                                        scale_pct: 100,
                                                        chroma_444: true,
                                                    },
                                                );
                                            }
                                        }

                                        if need_ui_prompt {
                                            let display_cid = format!("Peer ({})", client_addr.ip());
                                            if let Ok(mut pg) = pending_approval_store().lock() {
                                                *pg = Some(PendingApprovalRequest {
                                                    client_key,
                                                    client_id: display_cid.clone(),
                                                });
                                            }
                                            #[cfg(target_os = "windows")]
                                            {
                                                crate::windows::ui::append_status_log(&format!(
                                                    "🔔 Incoming request from {} — Click Accept or Decline!",
                                                    display_cid
                                                ));
                                                crate::windows::ui::bring_main_window_to_front();
                                            }
                                            #[cfg(target_os = "linux")]
                                            {
                                                trigger_linux_approval_dialog(display_cid);
                                            }
                                        }

                                        let my_id = current_id_clone.read().await.clone();
                                        let punch = SignalMsg::Punch { from_id: my_id };
                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                            let sock_punch = Arc::clone(&socket_recv);
                                            tokio::spawn(async move {
                                                // Probe the reported port + neighboring ports (+0..+12) to pierce Symmetric NATs!
                                                let base_ip = client_addr.ip();
                                                let base_port = client_addr.port();
                                                for _ in 0..8 {
                                                    let _ = sock_punch.send_to(&pbytes, client_addr).await;
                                                    for offset in 1..=12u16 {
                                                        if let Some(p) = base_port.checked_add(offset) {
                                                            let _ = sock_punch
                                                                .send_to(&pbytes, SocketAddr::new(base_ip, p))
                                                                .await;
                                                        }
                                                    }
                                                    tokio::time::sleep(Duration::from_millis(25)).await;
                                                }
                                            });
                                        }
                                    }
                                }
                                SignalMsg::Punch { from_id } => {
                                    info!(
                                        "🎉 [DIRECT P2P ESTABLISHED] Received direct hole-punch from peer ID '{}' ({})!",
                                        from_id, from_addr
                                    );
                                    let norm_cid = normalize_id(&from_id);
                                    let client_key = if !norm_cid.is_empty() && norm_cid != "000000000" {
                                        norm_cid
                                    } else {
                                        from_addr.to_string()
                                    };
                                    let client_id_disp = format_id_display(&from_id);

                                    let mut need_ui_prompt = false;
                                    let auth_status_byte = if let Ok(mut map) = clients_recv.lock() {
                                        // Upgrade any IP-based entry created by IncomingPeer to the peer's real 9-digit Desky ID
                                        let addr_key = from_addr.to_string();
                                        if client_key != addr_key {
                                            if let Some(mut pre_sub) = map.remove(&addr_key) {
                                                pre_sub.client_id = client_id_disp.clone();
                                                pre_sub.addr = from_addr;
                                                pre_sub.last_seen = Instant::now();
                                                map.insert(client_key.clone(), pre_sub);
                                            }
                                        }
                                        let entry = map.entry(client_key.clone()).or_insert_with(|| {
                                            need_ui_prompt = true;
                                            ClientSubscriber {
                                                client_id: client_id_disp.clone(),
                                                addr: from_addr,
                                                auth_state: AuthState::Pending,
                                                last_seen: Instant::now(),
                                                target_fps: 60,
                                                capture_cursor: true,
                                                jpeg_quality: 76,
                                                scale_pct: 100,
                                                chroma_444: true,
                                            }
                                        });
                                        entry.client_id = client_id_disp.clone();
                                        entry.addr = from_addr;
                                        entry.last_seen = Instant::now();

                                        match entry.auth_state {
                                            AuthState::Pending => AUTH_PENDING,
                                            AuthState::Approved => AUTH_APPROVED,
                                            AuthState::Declined(_) => AUTH_DECLINED,
                                        }
                                    } else {
                                        AUTH_PENDING
                                    };

                                    if let Ok(mut pg) = pending_approval_store().lock() {
                                        if pg.is_some() || need_ui_prompt {
                                            *pg = Some(PendingApprovalRequest {
                                                client_key: client_key.clone(),
                                                client_id: client_id_disp.clone(),
                                            });
                                        }
                                    }
                                    if need_ui_prompt {
                                        #[cfg(target_os = "windows")]
                                        {
                                            crate::windows::ui::append_status_log(&format!(
                                                "🔔 Incoming request from ID: {} — Click Accept or Decline!",
                                                client_id_disp
                                            ));
                                            crate::windows::ui::bring_main_window_to_front();
                                        }
                                        #[cfg(target_os = "linux")]
                                        {
                                            trigger_linux_approval_dialog(client_id_disp);
                                        }
                                    } else {
                                        #[cfg(target_os = "windows")]
                                        crate::windows::ui::request_main_redraw();
                                    }

                                    let ack = SignalMsg::PunchAck;
                                    if let Ok(abytes) = serde_json::to_vec(&ack) {
                                        let _ = socket_recv.send_to(&abytes, from_addr).await;
                                    }
                                    let mut auth_pkt = Vec::with_capacity(6);
                                    auth_pkt.extend_from_slice(MAGIC_DSKY);
                                    auth_pkt.push(PKT_AUTH_STATUS);
                                    auth_pkt.push(auth_status_byte);
                                    let _ = socket_recv.send_to(&auth_pkt, from_addr).await;
                                }
                                SignalMsg::PunchAck => {
                                    info!(
                                        "🎉 [DIRECT P2P VERIFIED] Received punch acknowledgement from peer at {}!",
                                        from_addr
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                });

                // Keepalive heartbeat loop (every 10 seconds to keep router NAT mapping open)
                let mut interval = tokio::time::interval(Duration::from_secs(10));
                while running.load(Ordering::Relaxed) {
                    interval.tick().await;
                    let my_id = current_id.read().await.clone();
                    if !my_id.is_empty() {
                        let hb = SignalMsg::Heartbeat { desky_id: my_id };
                        if let Ok(bytes) = serde_json::to_vec(&hb) {
                            let _ = socket.send_to(&bytes, target_server).await;
                        }
                    } else {
                        let reg = SignalMsg::RegisterHost {
                            machine_id: machine_id.clone(),
                            cached_id: cached_id.clone(),
                        };
                        if let Ok(bytes) = serde_json::to_vec(&reg) {
                            let _ = socket.send_to(&bytes, target_server).await;
                        }
                    }
                }
            });
        });
    }
}
