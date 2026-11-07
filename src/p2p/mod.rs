use crate::protocol::{InputEvent, SignalMsg};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::{error, info, warn};

const MAGIC_DSKY: &[u8; 4] = b"DSKY";
const MAGIC_DSKR: &[u8; 4] = b"DSKR";

const PKT_CLIENT_HELLO: u8 = 0x01;
const PKT_FRAME_CHUNK: u8 = 0x02;
const PKT_INPUT_EVENT: u8 = 0x03;
const PKT_DISCONNECT: u8 = 0x04;
const PKT_LAN_DISCOVERY: u8 = 0x05;
const PKT_PONG: u8 = 0x06;

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
    Connected(String),
    Error(String),
}

static LATEST_REMOTE_FRAME: OnceLock<Mutex<Option<Arc<RemoteFrame>>>> = OnceLock::new();
static SESSION_STATUS: OnceLock<Mutex<SessionStatus>> = OnceLock::new();
static INPUT_QUEUE: OnceLock<Mutex<Vec<InputEvent>>> = OnceLock::new();

static CLIENT_SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLIENT_SESSION_GEN: AtomicU32 = AtomicU32::new(0);
static CLIENT_TARGET_FPS: AtomicU32 = AtomicU32::new(30);
static CLIENT_CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);
static SESSION_RTT_MS: AtomicU32 = AtomicU32::new(0);

fn frame_store() -> &'static Mutex<Option<Arc<RemoteFrame>>> {
    LATEST_REMOTE_FRAME.get_or_init(|| Mutex::new(None))
}

fn status_store() -> &'static Mutex<SessionStatus> {
    SESSION_STATUS.get_or_init(|| Mutex::new(SessionStatus::Idle))
}

fn input_store() -> &'static Mutex<Vec<InputEvent>> {
    INPUT_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
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

pub fn update_client_preferences(target_fps: u32, capture_cursor: bool) {
    CLIENT_TARGET_FPS.store(target_fps.clamp(10, 60), Ordering::Relaxed);
    CLIENT_CAPTURE_CURSOR.store(capture_cursor, Ordering::Relaxed);
}

pub fn send_remote_input(event: InputEvent) {
    if !CLIENT_SESSION_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut q) = input_store().lock() {
        // Coalesce excessive MouseMove events so queue stays ultra-responsive
        if matches!(event, InputEvent::MouseMove { .. }) {
            if let Some(last) = q.last_mut() {
                if matches!(last, InputEvent::MouseMove { .. }) {
                    *last = event;
                    return;
                }
            }
        }
        if q.len() < 128 {
            q.push(event);
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
    set_session_status(SessionStatus::Idle);
}

fn normalize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn resolve_server_addr() -> SocketAddr {
    let server_str = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| "131.222.194.92:21116".into());
    server_str
        .parse()
        .unwrap_or_else(|_| "131.222.194.92:21116".parse().unwrap())
}

fn fix_loopback_peer(mut peer: SocketAddr, server: SocketAddr) -> SocketAddr {
    if peer.ip().is_loopback() && !server.ip().is_loopback() {
        peer.set_ip(server.ip());
    }
    peer
}

fn wrap_relay_packet(target_id_norm: &str, from_id_norm: &str, inner: &[u8]) -> Option<Vec<u8>> {
    if target_id_norm.len() != 9 || from_id_norm.len() != 9 {
        return None;
    }
    let mut out = Vec::with_capacity(22 + inner.len());
    out.extend_from_slice(MAGIC_DSKR);
    out.extend_from_slice(target_id_norm.as_bytes());
    out.extend_from_slice(from_id_norm.as_bytes());
    out.extend_from_slice(inner);
    Some(out)
}

#[derive(Clone)]
struct ClientSubscriber {
    addr: SocketAddr,
    last_seen: Instant,
    target_fps: u32,
    capture_cursor: bool,
    via_relay_id: Option<String>,
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

/// Starts a remote desktop client session connecting to `target_id`
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
    SESSION_RTT_MS.store(0, Ordering::Relaxed);
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

            // 1. Fire LAN discovery broadcast on port 21117 (for same-LAN / local testing)
            if norm_target.len() == 9 {
                let mut lan_pkt = Vec::with_capacity(14);
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

            // 2. Send ConnectRequest to VDS Matchmaker
            let req = SignalMsg::ConnectRequest {
                target_id: display_target.clone(),
            };
            if let Ok(req_bytes) = serde_json::to_vec(&req) {
                let _ = socket.send_to(&req_bytes, server_addr).await;
            }

            let mut peer_addr: Option<SocketAddr> = None;
            let mut buf = vec![0u8; 65536];
            let mut partial_frame: Option<PartialFrame> = None;
            let mut highest_decoded_frame: u32 = 0;

            let session_start = Instant::now();
            let mut last_hello = Instant::now() - Duration::from_secs(10);
            let mut last_matchmaker_retry = Instant::now();
            let mut received_any_frame = false;

            while CLIENT_SESSION_ACTIVE.load(Ordering::Relaxed)
                && CLIENT_SESSION_GEN.load(Ordering::Relaxed) == my_gen
            {
                // Retry ConnectRequest every 1.2s while peer_addr is not yet resolved
                if peer_addr.is_none() && last_matchmaker_retry.elapsed() >= Duration::from_millis(1200) {
                    if session_start.elapsed() > Duration::from_secs(10) {
                        let msg = format!(
                            "Timed out connecting to Desky ID '{}'",
                            display_target
                        );
                        set_session_status(SessionStatus::Error(msg.clone()));
                        #[cfg(target_os = "windows")]
                        crate::windows::ui::append_status_log(&format!("❌ {}", msg));
                        break;
                    }
                    if let Ok(req_bytes) = serde_json::to_vec(&req) {
                        let _ = socket.send_to(&req_bytes, server_addr).await;
                    }
                    last_matchmaker_retry = Instant::now();
                }

                // Periodically send ClientHello / KeepAlive to peer
                let hello_interval = if received_any_frame {
                    Duration::from_millis(750)
                } else {
                    Duration::from_millis(200)
                };

                if last_hello.elapsed() >= hello_interval {
                    let fps = CLIENT_TARGET_FPS.load(Ordering::Relaxed).clamp(10, 60) as u8;
                    let cur = if CLIENT_CAPTURE_CURSOR.load(Ordering::Relaxed) { 1u8 } else { 0u8 };
                    let ts_ms = (session_start.elapsed().as_millis() & 0xFFFF_FFFF) as u32;

                    let mut hello = Vec::with_capacity(11);
                    hello.extend_from_slice(MAGIC_DSKY);
                    hello.push(PKT_CLIENT_HELLO);
                    hello.push(fps);
                    hello.push(cur);
                    hello.extend_from_slice(&ts_ms.to_le_bytes());

                    if let Some(p_addr) = peer_addr {
                        let _ = socket.send_to(&hello, p_addr).await;
                    }

                    // If direct frames haven't arrived yet after 1s, also send relay-wrapped hello via VDS
                    if !received_any_frame && session_start.elapsed() >= Duration::from_millis(900) {
                        if let Some(relayed) = wrap_relay_packet(&norm_target, &my_id_padded, &hello) {
                            let _ = socket.send_to(&relayed, server_addr).await;
                        }
                    }

                    last_hello = Instant::now();
                }

                // Flush queued remote input events immediately
                let queued_inputs: Vec<InputEvent> = {
                    if let Ok(mut q) = input_store().lock() {
                        std::mem::take(&mut *q)
                    } else {
                        Vec::new()
                    }
                };
                if !queued_inputs.is_empty() {
                    for ev in queued_inputs {
                        if let Ok(ev_bytes) = serde_json::to_vec(&ev) {
                            let mut pkt = Vec::with_capacity(5 + ev_bytes.len());
                            pkt.extend_from_slice(MAGIC_DSKY);
                            pkt.push(PKT_INPUT_EVENT);
                            pkt.extend_from_slice(&ev_bytes);

                            if let Some(p_addr) = peer_addr {
                                let _ = socket.send_to(&pkt, p_addr).await;
                            } else if let Some(relayed) = wrap_relay_packet(&norm_target, &my_id_padded, &pkt) {
                                let _ = socket.send_to(&relayed, server_addr).await;
                            }
                        }
                    }
                }

                // Wait up to 15ms for incoming UDP packets
                match tokio::time::timeout(Duration::from_millis(15), socket.recv_from(&mut buf)).await {
                    Ok(Ok((len, from_addr))) => {
                        let mut slice = &buf[..len];

                        // Unwrap relay envelope if forwarded by VDS server
                        let mut is_relayed = false;
                        if slice.len() > 22 && &slice[0..4] == MAGIC_DSKR {
                            slice = &slice[22..];
                            is_relayed = true;
                        }

                        if slice.len() >= 5 && &slice[0..4] == MAGIC_DSKY {
                            let pkt_type = slice[4];
                            if !is_relayed {
                                peer_addr = Some(from_addr);
                            }

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
                                            || total_chunks > 512
                                            || chunk_idx >= total_chunks
                                        {
                                            continue;
                                        }

                                        let pf = match &mut partial_frame {
                                            Some(existing) if existing.frame_id == frame_id => existing,
                                            _ => {
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
                                                (pf.total_chunks as usize) * 1180,
                                            );
                                            for c in pf.chunks.iter().flatten() {
                                                jpeg_data.extend_from_slice(c);
                                            }
                                            let orig_w = pf.orig_width as u32;
                                            let orig_h = pf.orig_height as u32;
                                            highest_decoded_frame = frame_id;
                                            partial_frame = None;

                                            if let Some(decoded) = decode_jpeg_to_bgra(
                                                frame_id,
                                                orig_w,
                                                orig_h,
                                                &jpeg_data,
                                            ) {
                                                if !received_any_frame {
                                                    received_any_frame = true;
                                                    set_session_status(SessionStatus::Connected(
                                                        display_target.clone(),
                                                    ));
                                                    #[cfg(target_os = "windows")]
                                                    crate::windows::ui::append_status_log(&format!(
                                                        "🎉 Connected to Remote Desk {} ({}x{})!",
                                                        display_target,
                                                        decoded.orig_width,
                                                        decoded.orig_height
                                                    ));
                                                }
                                                if let Ok(mut g) = frame_store().lock() {
                                                    *g = Some(Arc::new(decoded));
                                                }
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
                                            "⚡ Peer '{}' resolved at {}. Punching UDP tunnel...",
                                            display_target, resolved
                                        );
                                        if peer_addr.is_none() {
                                            peer_addr = Some(resolved);
                                        }
                                        let punch = SignalMsg::Punch {
                                            from_id: my_id_padded.clone(),
                                        };
                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                            for _ in 0..5 {
                                                let _ = socket.send_to(&pbytes, resolved).await;
                                            }
                                        }
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
                                    if !is_relayed {
                                        peer_addr = Some(from_addr);
                                        let ack = SignalMsg::PunchAck;
                                        if let Ok(abytes) = serde_json::to_vec(&ack) {
                                            let _ = socket.send_to(&abytes, from_addr).await;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }

                if let Some(pf) = &partial_frame {
                    if pf.created_at.elapsed() > Duration::from_millis(600) {
                        partial_frame = None;
                    }
                }
            }

            // Send Disconnect notice on session close
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

    /// Spawns the background UDP rendezvous registration, keep-alive, and Host screen streamer
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
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        error!("Failed to bind local UDP socket for P2P: {:?}", e);
                        return;
                    }
                };

                let target_server: SocketAddr = match server_addr_str.parse() {
                    Ok(a) => a,
                    Err(_) => {
                        warn!(
                            "Could not parse VDS server address '{}', using default 131.222.194.92:21116",
                            server_addr_str
                        );
                        "131.222.194.92:21116".parse().unwrap()
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

                let active_clients: Arc<Mutex<HashMap<SocketAddr, ClientSubscriber>>> =
                    Arc::new(Mutex::new(HashMap::new()));

                let current_id = Arc::new(tokio::sync::RwLock::new(
                    cached_id.clone().unwrap_or_default(),
                ));

                // Optional LAN discovery responder on UDP 21117
                {
                    let socket_lan_main = Arc::clone(&socket);
                    let id_lan = Arc::clone(&current_id);
                    let clients_lan = Arc::clone(&active_clients);
                    tokio::spawn(async move {
                        if let Ok(lan_sock) = UdpSocket::bind("0.0.0.0:21117").await {
                            let mut lbuf = vec![0u8; 512];
                            while let Ok((len, src)) = lan_sock.recv_from(&mut lbuf).await {
                                if len >= 14 && &lbuf[0..4] == MAGIC_DSKY && lbuf[4] == PKT_LAN_DISCOVERY {
                                    let req_id = String::from_utf8_lossy(&lbuf[5..14]).to_string();
                                    let my_id = normalize_id(&id_lan.read().await);
                                    if !my_id.is_empty() && my_id == req_id {
                                        info!("⚡ [LAN DISCOVERY] Peer at {} requested our Desky ID! Punching directly...", src);
                                        if let Ok(mut map) = clients_lan.lock() {
                                            map.insert(
                                                src,
                                                ClientSubscriber {
                                                    addr: src,
                                                    last_seen: Instant::now(),
                                                    target_fps: 30,
                                                    capture_cursor: true,
                                                    via_relay_id: None,
                                                },
                                            );
                                        }
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

                // Dedicated Host Screen Capture & JPEG Streamer Thread
                {
                    let clients_stream = Arc::clone(&active_clients);
                    let running_stream = Arc::clone(&running);
                    let id_stream = Arc::clone(&current_id);

                    let (pkt_tx, mut pkt_rx) = tokio::sync::mpsc::channel::<(SocketAddr, Vec<u8>)>(512);
                    let socket_sender = Arc::clone(&socket);
                    tokio::spawn(async move {
                        while let Some((dest, data)) = pkt_rx.recv().await {
                            let _ = socket_sender.send_to(&data, dest).await;
                        }
                    });

                    std::thread::spawn(move || {
                        let mut frame_id: u32 = 1;
                        let mut jpeg_buf: Vec<u8> = Vec::with_capacity(65536);

                        while running_stream.load(Ordering::Relaxed) {
                            let loop_start = Instant::now();

                            // Snapshot active subscribers (seen within last 8 seconds)
                            let subscribers: Vec<ClientSubscriber> = {
                                if let Ok(mut map) = clients_stream.lock() {
                                    map.retain(|_, sub| sub.last_seen.elapsed() < Duration::from_secs(8));
                                    map.values().cloned().collect()
                                } else {
                                    Vec::new()
                                }
                            };

                            if subscribers.is_empty() {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            }

                            let max_fps = subscribers
                                .iter()
                                .map(|s| s.target_fps)
                                .max()
                                .unwrap_or(30)
                                .clamp(10, 60);
                            let cap_cursor = subscribers.iter().any(|s| s.capture_cursor);

                            #[cfg(target_os = "windows")]
                            let cap_res = crate::windows::capture::capture_screen_frame(cap_cursor);
                            #[cfg(target_os = "linux")]
                            let cap_res = crate::linux::capture::capture_screen_frame(cap_cursor);

                            if let Ok((orig_w, orig_h, cap_w, cap_h, bgra)) = cap_res {
                                jpeg_buf.clear();
                                let encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, 65);
                                if encoder
                                    .encode(
                                        &bgra,
                                        cap_w as u16,
                                        cap_h as u16,
                                        jpeg_encoder::ColorType::Bgra,
                                    )
                                    .is_ok()
                                {
                                    let chunk_size = 1160usize;
                                    let total_chunks = ((jpeg_buf.len() + chunk_size - 1) / chunk_size).max(1);

                                    if total_chunks <= 512 {
                                        let my_norm_id = normalize_id(&id_stream.blocking_read());

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
                                                if let Some(relay_client_id) = &sub.via_relay_id {
                                                    if let Some(relayed) = wrap_relay_packet(
                                                        relay_client_id,
                                                        &my_norm_id,
                                                        &pkt,
                                                    ) {
                                                        let _ = pkt_tx.try_send((target_server, relayed));
                                                    }
                                                } else {
                                                    let _ = pkt_tx.try_send((sub.addr, pkt.clone()));
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
                }

                let socket_recv = Arc::clone(&socket);
                let current_id_clone = Arc::clone(&current_id);
                let clients_recv = Arc::clone(&active_clients);

                // Receiver task: listens for VDS acks, peer introductions, punch packets, and client inputs
                tokio::spawn(async move {
                    #[cfg(target_os = "windows")]
                    let injector = crate::windows::input::InputInjector::new();
                    #[cfg(target_os = "linux")]
                    let injector = crate::linux::input::InputInjector::new();

                    let mut buf = vec![0u8; 8192];
                    while let Ok((len, from_addr)) = socket_recv.recv_from(&mut buf).await {
                        let mut slice = &buf[..len];
                        let mut relay_from_id: Option<String> = None;

                        if slice.len() > 22 && &slice[0..4] == MAGIC_DSKR {
                            relay_from_id = Some(String::from_utf8_lossy(&slice[13..22]).to_string());
                            slice = &slice[22..];
                        }

                        if slice.len() >= 5 && &slice[0..4] == MAGIC_DSKY {
                            let pkt_type = slice[4];
                            match pkt_type {
                                PKT_CLIENT_HELLO => {
                                    let fps = if slice.len() > 5 { slice[5].clamp(10, 60) as u32 } else { 30 };
                                    let cur = if slice.len() > 6 { slice[6] != 0 } else { true };

                                    if let Ok(mut map) = clients_recv.lock() {
                                        let entry = map.entry(from_addr).or_insert_with(|| {
                                            info!("🎉 [HOST STREAM] Client session started for {}", from_addr);
                                            #[cfg(target_os = "windows")]
                                            crate::windows::ui::append_status_log(&format!(
                                                "🖥️ Remote client connected from {}",
                                                from_addr
                                            ));
                                            ClientSubscriber {
                                                addr: from_addr,
                                                last_seen: Instant::now(),
                                                target_fps: fps,
                                                capture_cursor: cur,
                                                via_relay_id: relay_from_id.clone(),
                                            }
                                        });
                                        entry.last_seen = Instant::now();
                                        entry.target_fps = fps;
                                        entry.capture_cursor = cur;
                                        if relay_from_id.is_some() {
                                            entry.via_relay_id = relay_from_id.clone();
                                        }
                                    }

                                    // Reply with Pong if timestamp included
                                    if slice.len() >= 11 {
                                        let mut pong = Vec::with_capacity(9);
                                        pong.extend_from_slice(MAGIC_DSKY);
                                        pong.push(PKT_PONG);
                                        pong.extend_from_slice(&slice[7..11]);
                                        if let Some(ref cid) = relay_from_id {
                                            let my_norm = normalize_id(&current_id_clone.read().await);
                                            if let Some(relayed) = wrap_relay_packet(cid, &my_norm, &pong) {
                                                let _ = socket_recv.send_to(&relayed, target_server).await;
                                            }
                                        } else {
                                            let _ = socket_recv.send_to(&pong, from_addr).await;
                                        }
                                    }
                                }
                                PKT_INPUT_EVENT => {
                                    if let Ok(mut map) = clients_recv.lock() {
                                        if let Some(sub) = map.get_mut(&from_addr) {
                                            sub.last_seen = Instant::now();
                                        }
                                    }
                                    if let Ok(ev) = serde_json::from_slice::<InputEvent>(&slice[5..]) {
                                        injector.handle_event(&ev);
                                    }
                                }
                                PKT_DISCONNECT => {
                                    if let Ok(mut map) = clients_recv.lock() {
                                        map.remove(&from_addr);
                                    }
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
                                        "⚡ [P2P INCOMING] Peer connecting from {}! Firing UDP hole-punch packets...",
                                        client_public_addr
                                    );
                                    if let Ok(parsed_addr) = client_public_addr.parse::<SocketAddr>() {
                                        let client_addr = fix_loopback_peer(parsed_addr, target_server);
                                        if let Ok(mut map) = clients_recv.lock() {
                                            map.insert(
                                                client_addr,
                                                ClientSubscriber {
                                                    addr: client_addr,
                                                    last_seen: Instant::now(),
                                                    target_fps: 30,
                                                    capture_cursor: true,
                                                    via_relay_id: None,
                                                },
                                            );
                                        }
                                        let my_id = current_id_clone.read().await.clone();
                                        let punch = SignalMsg::Punch { from_id: my_id };
                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                            for _ in 0..6 {
                                                let _ = socket_recv.send_to(&pbytes, client_addr).await;
                                                tokio::time::sleep(Duration::from_millis(15)).await;
                                            }
                                        }
                                    }
                                }
                                SignalMsg::Punch { from_id } => {
                                    info!(
                                        "🎉 [DIRECT P2P ESTABLISHED] Received direct hole-punch from peer ID '{}' ({})!",
                                        from_id, from_addr
                                    );
                                    if let Ok(mut map) = clients_recv.lock() {
                                        map.entry(from_addr)
                                            .and_modify(|e| e.last_seen = Instant::now())
                                            .or_insert(ClientSubscriber {
                                                addr: from_addr,
                                                last_seen: Instant::now(),
                                                target_fps: 30,
                                                capture_cursor: true,
                                                via_relay_id: None,
                                            });
                                    }
                                    let ack = SignalMsg::PunchAck;
                                    if let Ok(abytes) = serde_json::to_vec(&ack) {
                                        let _ = socket_recv.send_to(&abytes, from_addr).await;
                                    }
                                }
                                SignalMsg::PunchAck => {
                                    info!(
                                        "🎉 [DIRECT P2P VERIFIED] Received punch acknowledgement from peer at {}!",
                                        from_addr
                                    );
                                    if let Ok(mut map) = clients_recv.lock() {
                                        if let Some(sub) = map.get_mut(&from_addr) {
                                            sub.last_seen = Instant::now();
                                        }
                                    }
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
