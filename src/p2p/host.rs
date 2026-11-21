use crate::p2p::state::{
    decode_lan_endpoint, encode_lan_endpoint_port, fix_loopback_peer, format_id_display,
    host_std_socket_store, host_subscribers_store, normalize_id,
    pending_approval_store, wrap_relay_packet, AuthState, ClientSubscriber,
    PendingApprovalRequest, AUTH_APPROVED, AUTH_DECLINED, AUTH_PENDING, MAGIC_DSKY,
    PKT_AUTH_STATUS, PKT_CLIENT_HELLO, PKT_CLIPBOARD, PKT_DISCONNECT, PKT_FILE_CHUNK, PKT_FILE_FINISH, PKT_FILE_START,
    PKT_FRAME_CHUNK, PKT_FRAME_CHUNK_H264, PKT_FRAME_CHUNK_H265, PKT_INPUT_EVENT,
    PKT_LAN_DISCOVERY, PKT_PONG, SERVER_ONLINE, SERVER_PING_MS, PKT_ADMIN_REQ,
    PKT_UNATTENDED_CHALLENGE, PKT_UNATTENDED_AUTH, PKT_CHAT_MESSAGE, PKT_VOICE_CHUNK,
    PKT_FILE_NACK,
};
use crate::codec::VideoEncoder;
use crate::protocol::{
    InputEvent, SignalMsg, PKT_E2EE_ACK, PKT_E2EE_HELLO, PKT_ENCRYPTED_FRAME,
    PKT_FRAME_CHUNK_TILE, PKT_KEYFRAME_REQ,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::{error, info, warn};



/// Fast sampled 64-bit FNV-1a frame hash to detect completely static/idle screens in <0.1ms
#[inline]
fn fast_frame_hash(bgra: &[u8], w: u32, h: u32) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325 ^ ((w as u64) << 32) ^ (h as u64);
    let step = (bgra.len() / 512).max(16);
    let mut i = 0;
    while i + 4 <= bgra.len() {
        let v = u32::from_le_bytes([bgra[i], bgra[i + 1], bgra[i + 2], bgra[i + 3]]) as u64;
        hash ^= v;
        hash = hash.wrapping_mul(0x100000001b3);
        i += step;
    }
    hash
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
                let std_sock = {
                    let builder = match socket2::Socket::new(
                        socket2::Domain::IPV4,
                        socket2::Type::DGRAM,
                        Some(socket2::Protocol::UDP),
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            error!("Failed to create local UDP socket for P2P: {:?}", e);
                            return;
                        }
                    };
                    let _ = builder.set_broadcast(true);
                    let _ = builder.set_send_buffer_size(4 * 1024 * 1024);
                    let _ = builder.set_recv_buffer_size(4 * 1024 * 1024);
                    let _ = builder.set_reuse_address(true);
                    let primary_addr: std::net::SocketAddr = "0.0.0.0:21117".parse().unwrap();
                    let any_addr: std::net::SocketAddr = "0.0.0.0:0".parse().unwrap();
                    if builder.bind(&primary_addr.into()).is_err() {
                        if let Err(e) = builder.bind(&any_addr.into()) {
                            error!("Failed to bind local UDP socket for P2P: {:?}", e);
                            return;
                        }
                    }
                    let s: std::net::UdpSocket = builder.into();
                    s
                };

                let std_sock_stream = match std_sock.try_clone() {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        error!("Failed to clone Host UDP socket: {:?}", e);
                        return;
                    }
                };
                if let Ok(mut g) = host_std_socket_store().lock() {
                    *g = Some(Arc::clone(&std_sock_stream));
                }

                let _ = std_sock.set_nonblocking(true);
                let socket = match UdpSocket::from_std(std_sock) {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        error!("Failed to wrap Tokio UdpSocket: {:?}", e);
                        return;
                    }
                };

                let target_server: SocketAddr = match server_addr_str.parse() {
                    Ok(a) => a,
                    Err(_) => {
                        use std::net::ToSocketAddrs;
                        if let Ok(mut addrs) = server_addr_str.to_socket_addrs() {
                            if let Some(a) = addrs.next() {
                                a
                            } else {
                                crate::config::DEFAULT_VDS_SERVER.to_socket_addrs().ok().and_then(|mut x| x.next()).unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 21116)))
                            }
                        } else {
                            crate::config::DEFAULT_VDS_SERVER.to_socket_addrs().ok().and_then(|mut x| x.next()).unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 21116)))
                        }
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

                crate::p2p::tcp_fallback::spawn_tcp_fallback_monitor(
                    target_server,
                    machine_id.clone(),
                    cached_id.clone(),
                    Arc::clone(&running),
                );

                let active_clients = Arc::clone(host_subscribers_store());

                let current_id = Arc::new(tokio::sync::RwLock::new(
                    cached_id.clone().unwrap_or_default(),
                ));

                // LAN discovery responder on UDP 21117
                {
                    let socket_lan_main = Arc::clone(&socket);
                    let id_lan = Arc::clone(&current_id);
                    tokio::spawn(async move {
                        if let Ok(lan_sock) = UdpSocket::bind("0.0.0.0:21117").await {
                            let mut lbuf = vec![0u8; 512];
                            loop {
                                let (len, src) = match lan_sock.recv_from(&mut lbuf).await {
                                    Ok(v) => v,
                                    Err(_) => continue,
                                };
                                if len >= 14
                                    && &lbuf[0..4] == MAGIC_DSKY
                                    && lbuf[4] == PKT_LAN_DISCOVERY
                                {
                                    let req_id =
                                        String::from_utf8_lossy(&lbuf[5..14]).to_string();
                                    let my_id = normalize_id(&id_lan.read().await);
                                    if !my_id.is_empty() && my_id == req_id {
                                        info!("⚡ [LAN DISCOVERY] Peer at {} requested our Desky ID! Punching directly...", src);
                                        let punch = SignalMsg::Punch { from_id: my_id };
                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                            for _ in 0..3 {
                                                let _ = socket_lan_main.send_to(&pbytes, src).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
                }

                let session_cipher_store: Arc<std::sync::Mutex<Option<Arc<crate::p2p::crypto::SessionCipher>>>> =
                    Arc::new(std::sync::Mutex::new(None));
                let session_cipher_recv = Arc::clone(&session_cipher_store);
                let force_kf_flag = Arc::new(AtomicBool::new(false));
                let force_kf_recv = Arc::clone(&force_kf_flag);

                // Dedicated Host Screen Capture & Direct P2P / Relay Streamer OS Thread
                {
                    let clients_stream = Arc::clone(&active_clients);
                    let running_stream = Arc::clone(&running);
                    let target_server_stream = target_server;
                    let std_sock_stream_capture = Arc::clone(&std_sock_stream);
                    let session_cipher_cap = Arc::clone(&session_cipher_store);
                    let force_kf_cap = Arc::clone(&force_kf_flag);

                    std::thread::spawn(move || {
                        let mut frame_id: u32 = 1;
                        let mut mjpeg_encoder = crate::codec::mjpeg::MjpegEncoder::new(76, true);
                        let mut tile_encoder = crate::codec::desky_tile::DeskyTileEncoder::new(90);
                        let mut last_encoded_len: usize = 0;
                        let mut last_frame_hash: u64 = 0;
                        let mut last_sent_frame_time = Instant::now() - Duration::from_secs(2);
                        let mut h264_encoder: Option<crate::platform::codec::PlatformH264Encoder> = None;
                        let mut h264_w: u32 = 0;
                        let mut h264_h: u32 = 0;
                        let mut h265_encoder: Option<crate::platform::codec::PlatformH265Encoder> = None;
                        let mut h265_w: u32 = 0;
                        let mut h265_h: u32 = 0;
                        #[cfg(target_os = "windows")]
                        let mut last_cursor_shape: u8 = 0xFF;

                        while running_stream.load(Ordering::Relaxed) {
                            let loop_start = Instant::now();

                            // Only stream to APPROVED subscribers!
                            let subscribers: Vec<ClientSubscriber> = {
                                if let Ok(mut map) = clients_stream.lock() {
                                    map.retain(|_, sub| {
                                        sub.last_seen.elapsed() < Duration::from_secs(20)
                                    });
                                    map.values()
                                        .filter(|s| s.auth_state == AuthState::Approved)
                                        .cloned()
                                        .collect()
                                } else {
                                    Vec::new()
                                }
                            };

                            if subscribers.is_empty() {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            }

                            let any_relay = subscribers.iter().any(|s| !s.is_direct);
                            let max_fps = subscribers
                                .iter()
                                .map(|s| s.target_fps)
                                .max()
                                .unwrap_or(60)
                                .clamp(15, if any_relay { 35 } else { 60 });
                            let cap_cursor = subscribers.iter().any(|s| s.capture_cursor);
                            let req_quality = subscribers
                                .iter()
                                .map(|s| s.jpeg_quality)
                                .max()
                                .unwrap_or(86)
                                .clamp(50, 95);
                            let scale_pct = subscribers
                                .iter()
                                .map(|s| s.scale_pct)
                                .max()
                                .unwrap_or(100)
                                .clamp(65, 100);
                            let req_chroma_444 =
                                subscribers.iter().any(|s| s.chroma_444);

                            // Preserve high-clarity 4:4:4 chroma and crisp JPEG quality while respecting client ABR
                            let (jpeg_quality, use_chroma_444) = if any_relay {
                                if last_encoded_len > 110_000 {
                                    ((req_quality.saturating_sub(8)).max(64), false)
                                } else if last_encoded_len > 78_000 {
                                    ((req_quality.saturating_sub(4)).max(70), req_chroma_444)
                                } else {
                                    (req_quality, req_chroma_444)
                                }
                            } else if last_encoded_len > 280_000 {
                                ((req_quality.saturating_sub(8)).max(70), false)
                            } else if last_encoded_len > 190_000 {
                                ((req_quality.saturating_sub(4)).max(76), req_chroma_444)
                            } else {
                                (req_quality, req_chroma_444)
                            };

                            let cap_res = crate::platform::capture_screen_frame(
                                cap_cursor, scale_pct,
                            );

                            if let Ok((orig_w, orig_h, cap_w, cap_h, bgra)) = cap_res {
                                #[cfg(target_os = "windows")]
                                {
                                    let cur_shape = crate::windows::get_current_cursor_shape();
                                    if cur_shape != last_cursor_shape {
                                        last_cursor_shape = cur_shape;
                                        let mut cur_pkt = [0u8; 6];
                                        cur_pkt[0..4].copy_from_slice(MAGIC_DSKY);
                                        cur_pkt[4] = crate::p2p::state::PKT_CURSOR_SHAPE;
                                        cur_pkt[5] = cur_shape;
                                        for sub in &subscribers {
                                            if sub.is_direct || sub.addr != target_server_stream {
                                                let _ = std_sock_stream_capture.send_to(&cur_pkt, sub.addr);
                                                if let Some(l_addr) = sub.lan_addr {
                                                    if l_addr != sub.addr {
                                                        let _ = std_sock_stream_capture.send_to(&cur_pkt, l_addr);
                                                    }
                                                }
                                            } else if !sub.relay_id.is_empty() {
                                                let relay = wrap_relay_packet(&sub.relay_id, &cur_pkt);
                                                let _ = std_sock_stream_capture.send_to(&relay, target_server_stream);
                                            }
                                        }
                                    }
                                }

                                // Fallback periodic host clipboard check (Windows in-process Win32 API only;
                                // avoids spawning blocking xclip/wl-paste subprocesses inside the 60 FPS capture loop on Linux)
                                #[cfg(target_os = "windows")]
                                if frame_id % 30 == 0 {
                                    if let Some(text) = crate::p2p::state::read_clipboard_text() {
                                        if !text.is_empty() {
                                            crate::p2p::state::send_remote_clipboard(&text);
                                        }
                                    }
                                }

                                // Host -> Client File Transfer packets flush
                                let mut file_pkts_sent = 0;
                                while file_pkts_sent < 16 {
                                    if let Some(pkt) = crate::p2p::state::pop_outgoing_file_packet() {
                                        for sub in &subscribers {
                                            if sub.is_direct || sub.addr != target_server_stream {
                                                let _ = std_sock_stream_capture.send_to(&pkt, sub.addr);
                                                if let Some(l_addr) = sub.lan_addr {
                                                    if l_addr != sub.addr {
                                                        let _ = std_sock_stream_capture.send_to(&pkt, l_addr);
                                                    }
                                                }
                                            } else if !sub.relay_id.is_empty() {
                                                let relay = wrap_relay_packet(&sub.relay_id, &pkt);
                                                let _ = std_sock_stream_capture.send_to(&relay, target_server_stream);
                                            }
                                        }
                                        file_pkts_sent += 1;
                                        if file_pkts_sent % 4 == 0 {
                                            std::thread::yield_now();
                                        }
                                    } else {
                                        break;
                                    }
                                }

                                // Host -> Client In-Session Chat packets flush
                                for msg in crate::p2p::pull_outgoing_messages() {
                                    let msg_bytes = msg.as_bytes();
                                    let mut pkt = Vec::with_capacity(5 + msg_bytes.len());
                                    pkt.extend_from_slice(MAGIC_DSKY);
                                    pkt.push(PKT_CHAT_MESSAGE);
                                    pkt.extend_from_slice(msg_bytes);
                                    for sub in &subscribers {
                                        if sub.is_direct || sub.addr != target_server_stream {
                                            let _ = std_sock_stream_capture.send_to(&pkt, sub.addr);
                                            if let Some(l_addr) = sub.lan_addr {
                                                if l_addr != sub.addr {
                                                    let _ = std_sock_stream_capture.send_to(&pkt, l_addr);
                                                }
                                            }
                                        } else if !sub.relay_id.is_empty() {
                                            let relay = wrap_relay_packet(&sub.relay_id, &pkt);
                                            let _ = std_sock_stream_capture.send_to(&relay, target_server_stream);
                                        }
                                    }
                                }

                                // Host -> Client Intercom Voice packets flush
                                while let Some(v_pkt) = crate::p2p::state::pop_outgoing_voice_packet() {
                                    for sub in &subscribers {
                                        if sub.is_direct || sub.addr != target_server_stream {
                                            let _ = std_sock_stream_capture.send_to(&v_pkt, sub.addr);
                                            if let Some(l_addr) = sub.lan_addr {
                                                if l_addr != sub.addr {
                                                    let _ = std_sock_stream_capture.send_to(&v_pkt, l_addr);
                                                }
                                            }
                                        } else if !sub.relay_id.is_empty() {
                                            let relay = wrap_relay_packet(&sub.relay_id, &v_pkt);
                                            let _ = std_sock_stream_capture.send_to(&relay, target_server_stream);
                                        }
                                    }
                                }

                                let frame_hash = fast_frame_hash(&bgra, cap_w, cap_h);
                                let kf_requested = force_kf_cap.swap(false, Ordering::Relaxed);
                                let is_idle_keepalive = last_sent_frame_time.elapsed() >= Duration::from_millis(900);
                                if !kf_requested && !is_idle_keepalive && frame_hash == last_frame_hash {
                                    let target_frame_time = Duration::from_micros(1_000_000 / max_fps.max(1) as u64);
                                    let elapsed = loop_start.elapsed();
                                    if elapsed < target_frame_time {
                                        std::thread::sleep(target_frame_time - elapsed);
                                    }
                                    continue;
                                }
                                last_frame_hash = frame_hash;
                                last_sent_frame_time = Instant::now();

                                let mut encoded_data = Vec::new();
                                let mut encoded_pkt_type = PKT_FRAME_CHUNK;

                                let force_keyframe = kf_requested || frame_id % (max_fps * 2).max(30) == 1;

                                // Responsive dynamic target bitrate based on subscriber quality preferences & resolution:
                                let pixel_count = (cap_w * cap_h) as u64;
                                let base_bitrate: u32 = if any_relay {
                                    if req_quality >= 85 {
                                        14_000_000 // 14 Mbps over relay
                                    } else if req_quality >= 75 {
                                        10_000_000 // 10 Mbps over relay
                                    } else if req_quality >= 68 {
                                        7_000_000 // 7 Mbps over relay
                                    } else {
                                        4_000_000 // 4 Mbps over relay
                                    }
                                } else {
                                    // Direct P2P: full high-bandwidth desktop transmission
                                    if req_quality >= 85 {
                                        // Lossless / Ultra Clarity preset (20 - 40 Mbps)
                                        let scaled = (24_000_000u64 * pixel_count / (1920 * 1080)).clamp(16_000_000, 42_000_000);
                                        scaled as u32
                                    } else if req_quality >= 75 {
                                        // High Quality preset (15 - 28 Mbps)
                                        let scaled = (16_000_000u64 * pixel_count / (1920 * 1080)).clamp(10_000_000, 28_000_000);
                                        scaled as u32
                                    } else if req_quality >= 68 {
                                        // Standard Balanced preset (10 - 18 Mbps)
                                        let scaled = (12_000_000u64 * pixel_count / (1920 * 1080)).clamp(8_000_000, 18_000_000);
                                        scaled as u32
                                    } else {
                                        // Low / Data Saver preset (4 - 8 Mbps)
                                        let scaled = (5_000_000u64 * pixel_count / (1920 * 1080)).clamp(3_000_000, 8_000_000);
                                        scaled as u32
                                    }
                                };
                                let target_bitrate = base_bitrate;

                                let preferred_codec = subscribers.first().map(|s| s.codec_pref).unwrap_or(0);

                                // TIER 0 (CUSTOM DESKTOP CODEC): DeskyTile (Lossless RLE + 4:4:4 Tile-Delta)
                                if preferred_codec == 0 || preferred_codec == 4 {
                                    tile_encoder.set_quality(req_quality.clamp(78, 96));
                                    if let Ok(frame) = tile_encoder.encode(&bgra, cap_w, cap_h, force_keyframe) {
                                        if !frame.data.is_empty() {
                                            encoded_data = frame.data;
                                            encoded_pkt_type = PKT_FRAME_CHUNK_TILE;
                                        }
                                    }
                                }

                                // TIER 1 (H.265 / HEVC GPU)
                                if encoded_data.is_empty() && preferred_codec == 1 && crate::platform::codec::is_h265_hardware_available() {
                                    if h265_encoder.is_none() || h265_w != cap_w || h265_h != cap_h {
                                        h265_encoder = crate::platform::codec::PlatformH265Encoder::new(cap_w, cap_h, max_fps, target_bitrate).ok();
                                        h265_w = cap_w;
                                        h265_h = cap_h;
                                    } else if let Some(ref mut enc) = h265_encoder {
                                        enc.set_bitrate(target_bitrate);
                                    }
                                    if let Some(ref mut enc) = h265_encoder {
                                        if let Ok(frame) = enc.encode(&bgra, cap_w, cap_h, force_keyframe) {
                                            if !frame.data.is_empty() {
                                                encoded_data = frame.data;
                                                encoded_pkt_type = PKT_FRAME_CHUNK_H265;
                                            }
                                        }
                                    }
                                }

                                // TIER 2 (H.264 / AVC GPU)
                                if encoded_data.is_empty()
                                    && (preferred_codec == 0 || preferred_codec == 2 || (preferred_codec == 1 && h265_encoder.is_none()))
                                    && crate::platform::codec::is_h264_hardware_available()
                                {
                                    if h264_encoder.is_none() || h264_w != cap_w || h264_h != cap_h {
                                        h264_encoder = crate::platform::codec::PlatformH264Encoder::new(cap_w, cap_h, max_fps, target_bitrate).ok();
                                        h264_w = cap_w;
                                        h264_h = cap_h;
                                    } else if let Some(ref mut enc) = h264_encoder {
                                        enc.set_bitrate(target_bitrate);
                                    }
                                    if let Some(ref mut enc) = h264_encoder {
                                        if let Ok(frame) = enc.encode(&bgra, cap_w, cap_h, force_keyframe) {
                                            if !frame.data.is_empty() {
                                                encoded_data = frame.data;
                                                encoded_pkt_type = PKT_FRAME_CHUNK_H264;
                                            }
                                        }
                                    }
                                }

                                // TIER 3 (FALLBACK / UNIVERSAL BASELINE): Pure-Rust SIMD Motion-JPEG 4:4:4
                                if encoded_data.is_empty() {
                                    mjpeg_encoder.set_quality(jpeg_quality);
                                    mjpeg_encoder.set_chroma_444(use_chroma_444);
                                    if let Ok(frame) = mjpeg_encoder.encode(&bgra, cap_w, cap_h, true) {
                                        encoded_data = frame.data;
                                        encoded_pkt_type = PKT_FRAME_CHUNK;
                                    }
                                }

                                if !encoded_data.is_empty() {
                                    last_encoded_len = encoded_data.len();
                                    let chunk_size = 1280usize;
                                    let total_chunks =
                                        ((encoded_data.len() + chunk_size - 1) / chunk_size).max(1);

                                    if total_chunks <= 2048 {
                                        for (idx, chunk) in
                                            encoded_data.chunks(chunk_size).enumerate()
                                        {
                                            let mut pkt = Vec::with_capacity(17 + chunk.len());
                                            pkt.extend_from_slice(MAGIC_DSKY);
                                            pkt.push(encoded_pkt_type);
                                            pkt.extend_from_slice(&frame_id.to_le_bytes());
                                            pkt.extend_from_slice(&(idx as u16).to_le_bytes());
                                            pkt.extend_from_slice(
                                                &(total_chunks as u16).to_le_bytes(),
                                            );
                                            pkt.extend_from_slice(&(orig_w as u16).to_le_bytes());
                                            pkt.extend_from_slice(&(orig_h as u16).to_le_bytes());
                                            pkt.extend_from_slice(chunk);

                                            let send_chunk_retry = |sock: &std::net::UdpSocket, data: &[u8], target: std::net::SocketAddr| {
                                                for _ in 0..6 {
                                                    match sock.send_to(data, target) {
                                                        Ok(_) => return true,
                                                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                                            std::thread::sleep(Duration::from_micros(60));
                                                        }
                                                        Err(_) => return false,
                                                    }
                                                }
                                                false
                                            };

                                            let active_cipher = session_cipher_cap.lock().ok().and_then(|g| g.clone());
                                            let wire_pkt = if let Some(ref cipher) = active_cipher {
                                                cipher.encrypt_dsky_packet(&pkt).unwrap_or(pkt)
                                            } else {
                                                pkt
                                            };

                                            for sub in &subscribers {
                                                if sub.addr != target_server_stream {
                                                    send_chunk_retry(&std_sock_stream_capture, &wire_pkt, sub.addr);
                                                }
                                                if let Some(l_addr) = sub.lan_addr {
                                                    if l_addr != sub.addr {
                                                        send_chunk_retry(&std_sock_stream_capture, &wire_pkt, l_addr);
                                                    }
                                                }
                                                if !sub.is_direct && !sub.relay_id.is_empty() {
                                                    let relay_pkt =
                                                        wrap_relay_packet(&sub.relay_id, &wire_pkt);
                                                    let _ = std_sock_stream_capture
                                                        .send_to(&relay_pkt, target_server_stream);
                                                }
                                            }

                                            // Smooth micro-pacing so socket/LAN buffers never drop UDP packets
                                            if any_relay {
                                                if (idx + 1) % 3 == 0 {
                                                    std::thread::yield_now();
                                                }
                                            } else if (idx + 1) % 8 == 0 {
                                                std::thread::sleep(Duration::from_micros(30));
                                            }
                                        }
                                        frame_id = frame_id.wrapping_add(1).max(1);
                                    }
                                }
                            }

                            let target_frame_time =
                                Duration::from_micros(1_000_000 / max_fps as u64);
                            let elapsed = loop_start.elapsed();
                            if elapsed < target_frame_time {
                                std::thread::sleep(target_frame_time - elapsed);
                            }
                        }
                    });
                }

                // Dedicated Host Audio WASAPI Loopback Streamer OS Thread
                #[cfg(target_os = "windows")]
                {
                    let clients_audio = Arc::clone(&active_clients);
                    let running_audio = Arc::clone(&running);
                    let sock_audio = Arc::clone(&std_sock_stream);
                    let target_server_audio = target_server;

                    std::thread::spawn(move || {
                        let mut capturer_opt: Option<crate::windows::audio::WasapiLoopbackCapturer> = None;

                        while running_audio.load(Ordering::Relaxed) {
                            let has_subscribers = {
                                if let Ok(map) = clients_audio.lock() {
                                    map.values().any(|s| s.auth_state == AuthState::Approved)
                                } else {
                                    false
                                }
                            };
                            if !has_subscribers {
                                std::thread::sleep(Duration::from_millis(100));
                                continue;
                            }

                            if capturer_opt.is_none() {
                                match crate::windows::audio::WasapiLoopbackCapturer::new() {
                                    Ok(c) => {
                                        capturer_opt = Some(c);
                                    }
                                    Err(_) => {
                                        std::thread::sleep(Duration::from_millis(1500));
                                        continue;
                                    }
                                }
                            }

                            let capturer = capturer_opt.as_ref().unwrap();
                            let channels = capturer.channels;
                            let sample_rate = capturer.sample_rate;

                            match capturer.read_packet() {
                                Ok(Some(pcm_data)) => {
                                    if !pcm_data.is_empty() {
                                        let subscribers: Vec<ClientSubscriber> = {
                                            if let Ok(map) = clients_audio.lock() {
                                                map.values()
                                                    .filter(|s| s.auth_state == AuthState::Approved)
                                                    .cloned()
                                                    .collect()
                                            } else {
                                                Vec::new()
                                            }
                                        };
                                        if !subscribers.is_empty() {
                                            let mut pkt = Vec::with_capacity(10 + pcm_data.len());
                                            pkt.extend_from_slice(MAGIC_DSKY);
                                            pkt.push(crate::p2p::state::PKT_AUDIO_CHUNK);
                                            pkt.push(channels as u8);
                                            pkt.extend_from_slice(&sample_rate.to_le_bytes());
                                            pkt.extend_from_slice(&pcm_data);

                                            for sub in &subscribers {
                                                if sub.is_direct || sub.addr != target_server_audio {
                                                    let _ = sock_audio.send_to(&pkt, sub.addr);
                                                    if let Some(l_addr) = sub.lan_addr {
                                                        if l_addr != sub.addr {
                                                            let _ = sock_audio.send_to(&pkt, l_addr);
                                                        }
                                                    }
                                                } else if !sub.relay_id.is_empty() {
                                                    let relay = wrap_relay_packet(&sub.relay_id, &pkt);
                                                    let _ = sock_audio.send_to(&relay, target_server_audio);
                                                }
                                            }
                                        }
                                    }
                                    std::thread::sleep(Duration::from_millis(15));
                                }
                                Ok(None) => {
                                    std::thread::sleep(Duration::from_millis(15));
                                }
                                Err(_) => {
                                    capturer_opt = None;
                                    std::thread::sleep(Duration::from_millis(500));
                                }
                            }
                        }
                    });
                }

                // Dedicated Host Audio PulseAudio/PipeWire Streamer OS Thread (Linux)
                #[cfg(target_os = "linux")]
                {
                    let clients_audio = Arc::clone(&active_clients);
                    let running_audio = Arc::clone(&running);
                    let sock_audio = Arc::clone(&std_sock_stream);
                    let target_server_audio = target_server;

                    std::thread::spawn(move || {
                        let mut capturer_opt: Option<crate::linux::audio::LinuxAudioCapturer> = None;
                        let mut pcm_buf = [0u8; 3840]; // ~20ms @ 48kHz stereo S16LE

                        while running_audio.load(Ordering::Relaxed) {
                            let has_subscribers = {
                                if let Ok(map) = clients_audio.lock() {
                                    map.values().any(|s| s.auth_state == AuthState::Approved)
                                } else {
                                    false
                                }
                            };
                            if !has_subscribers {
                                capturer_opt = None;
                                std::thread::sleep(Duration::from_millis(100));
                                continue;
                            }

                            if capturer_opt.is_none() {
                                match crate::linux::audio::LinuxAudioCapturer::start() {
                                    Ok(c) => capturer_opt = Some(c),
                                    Err(_) => {
                                        std::thread::sleep(Duration::from_millis(1500));
                                        continue;
                                    }
                                }
                            }

                            if let Some(ref mut capturer) = capturer_opt {
                                match capturer.read_pcm_chunk(&mut pcm_buf) {
                                    Ok(n) if n > 0 => {
                                        let subscribers: Vec<ClientSubscriber> = {
                                            if let Ok(map) = clients_audio.lock() {
                                                map.values()
                                                    .filter(|s| s.auth_state == AuthState::Approved)
                                                    .cloned()
                                                    .collect()
                                            } else {
                                                Vec::new()
                                            }
                                        };
                                        if !subscribers.is_empty() {
                                            let mut pkt = Vec::with_capacity(10 + n);
                                            pkt.extend_from_slice(MAGIC_DSKY);
                                            pkt.push(crate::p2p::state::PKT_AUDIO_CHUNK);
                                            pkt.push(2u8); // stereo
                                            pkt.extend_from_slice(&48000u32.to_le_bytes());
                                            pkt.extend_from_slice(&pcm_buf[..n]);
                                            for sub in &subscribers {
                                                if sub.addr != target_server_audio {
                                                    let _ = sock_audio.send_to(&pkt, sub.addr);
                                                }
                                                if let Some(l_addr) = sub.lan_addr {
                                                    if l_addr != sub.addr {
                                                        let _ = sock_audio.send_to(&pkt, l_addr);
                                                    }
                                                }
                                                if !sub.is_direct && !sub.relay_id.is_empty() {
                                                    let relay = wrap_relay_packet(&sub.relay_id, &pkt);
                                                    let _ = sock_audio.send_to(&relay, target_server_audio);
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        capturer_opt = None;
                                        std::thread::sleep(Duration::from_millis(500));
                                    }
                                }
                            }
                        }
                    });
                }

                let socket_recv = Arc::clone(&socket);
                let current_id_clone = Arc::clone(&current_id);
                let clients_recv = Arc::clone(&active_clients);

                let last_hb_send = Arc::new(std::sync::Mutex::new(Instant::now()));
                let last_hb_ack = Arc::new(std::sync::Mutex::new(Instant::now()));
                let last_hb_send_clone = Arc::clone(&last_hb_send);
                let last_hb_ack_clone = Arc::clone(&last_hb_ack);

                // Receiver task: listens for VDS matchmaking acks, peer hole-punching, and direct client packets
                tokio::spawn(async move {
                    let injector = crate::platform::InputInjector::new();

                    let local_port = socket_recv.local_addr().ok().map(|a| a.port()).unwrap_or(0);
                    let host_lan_bytes = encode_lan_endpoint_port(local_port, target_server);
                    let mut last_incoming_punch: Option<(SocketAddr, Instant)> = None;
                    let auth_limiter = crate::p2p::crypto::AuthRateLimiter::new();
                    let mut buf = vec![0u8; 8192];

                    loop {
                        let (len, from_addr) = match socket_recv.recv_from(&mut buf).await {
                            Ok(v) => v,
                            Err(_) => {
                                // Ignore Windows WSAECONNRESET (10054) from ICMP Port Unreachable during hole-punching
                                continue;
                            }
                        };
                        let raw_slice = &buf[..len];
                        let decrypted_holder: Vec<u8>;
                        let slice: &[u8] = if raw_slice.len() >= 13
                            && &raw_slice[0..4] == MAGIC_DSKY
                            && raw_slice[4] == PKT_ENCRYPTED_FRAME
                        {
                            let cipher_opt = session_cipher_recv.lock().ok().and_then(|g| g.clone());
                            if let Some(cipher) = cipher_opt {
                                if let Some(plain) = cipher.decrypt_dsky_packet(raw_slice) {
                                    decrypted_holder = plain;
                                    &decrypted_holder
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        } else {
                            raw_slice
                        };

                        if slice.len() >= 5 && &slice[0..4] == MAGIC_DSKY {
                            let pkt_type = slice[4];
                            match pkt_type {
                                PKT_CLIENT_HELLO => {
                                    let fps = if slice.len() > 5 {
                                        slice[5].clamp(10, 60) as u32
                                    } else {
                                        60
                                    };
                                    let cur = if slice.len() > 6 { slice[6] != 0 } else { true };
                                    let q_val = if slice.len() > 11 {
                                        slice[11].clamp(40, 92)
                                    } else {
                                        76
                                    };
                                    let sc_val = if slice.len() > 12 {
                                        slice[12].clamp(50, 100)
                                    } else {
                                        100
                                    };
                                    let c444 = if slice.len() > 13 { slice[13] != 0 } else { true };
                                    let raw_cid = if slice.len() >= 23 {
                                        String::from_utf8_lossy(&slice[14..23]).to_string()
                                    } else {
                                        String::new()
                                    };
                                    let relay_id = if slice.len() >= 32 {
                                        normalize_id(&String::from_utf8_lossy(&slice[23..32]))
                                    } else {
                                        String::new()
                                    };
                                    let client_lan_addr = if slice.len() >= 38 {
                                        decode_lan_endpoint(&slice[32..38])
                                    } else {
                                        None
                                    };
                                    let client_confirmed_direct =
                                        slice.len() >= 39 && slice[38] != 0;
                                    let client_codec_pref = if slice.len() > 39 { slice[39].min(4) } else { 0 };

                                    let norm_cid = normalize_id(&raw_cid);
                                    let my_own_id = normalize_id(&current_id_clone.read().await);

                                    // Ignore self-bounced legacy packets from VDS if client_id equals our own Host ID and has no session relay_id
                                    if from_addr == target_server
                                        && relay_id.is_empty()
                                        && !norm_cid.is_empty()
                                        && norm_cid == my_own_id
                                    {
                                        continue;
                                    }

                                    let client_key =
                                        if !norm_cid.is_empty() && norm_cid != "000000000" {
                                            norm_cid
                                        } else if !relay_id.is_empty() {
                                            relay_id.clone()
                                        } else {
                                            from_addr.to_string()
                                        };
                                    let client_id_disp = format_id_display(&raw_cid);
                                    let is_from_server = from_addr == target_server;

                                    let mut need_ui_prompt = false;
                                    let mut should_punch_lan = false;
                                    let auth_status_byte = {
                                        if let Ok(mut map) = clients_recv.lock() {
                                            // Reset expired declines after 4 seconds so user can retry connecting
                                            if let Some(existing) = map.get_mut(&client_key) {
                                                if let AuthState::Declined(when) =
                                                    existing.auth_state
                                                {
                                                    if when.elapsed() > Duration::from_secs(4) {
                                                        map.remove(&client_key);
                                                    }
                                                }
                                            }

                                            let addr_key = from_addr.to_string();
                                            if client_key != addr_key {
                                                if let Some(mut pre_sub) = map.remove(&addr_key) {
                                                    pre_sub.client_id = client_id_disp.clone();
                                                    if !is_from_server {
                                                        pre_sub.addr = from_addr;
                                                        pre_sub.is_direct = true;
                                                    }
                                                    pre_sub.last_seen = Instant::now();
                                                    map.insert(client_key.clone(), pre_sub);
                                                }
                                            }

                                            let verified_direct = !is_from_server;

                                            let entry = map
                                                .entry(client_key.clone())
                                                .or_insert_with(|| {
                                                    let initial_addr = if !is_from_server {
                                                        from_addr
                                                    } else {
                                                        client_lan_addr.unwrap_or(from_addr)
                                                    };
                                                    info!(
                                                        "🔔 [HOST APPROVAL] Incoming connection request from {} ({})",
                                                        client_id_disp,
                                                        initial_addr
                                                    );
                                                    need_ui_prompt = true;
                                                    should_punch_lan = true;
                                                    ClientSubscriber {
                                                        client_id: client_id_disp.clone(),
                                                        relay_id: relay_id.clone(),
                                                        addr: initial_addr,
                                                        lan_addr: client_lan_addr,
                                                        auth_state: AuthState::Pending,
                                                        last_seen: Instant::now(),
                                                        target_fps: fps,
                                                        capture_cursor: cur,
                                                        jpeg_quality: q_val,
                                                        scale_pct: sc_val,
                                                        chroma_444: c444,
                                                        is_direct: verified_direct,
                                                        codec_pref: client_codec_pref,
                                                    }
                                                });
                                            entry.client_id = client_id_disp.clone();
                                            if !relay_id.is_empty() {
                                                if entry.relay_id != relay_id
                                                    && entry.auth_state == AuthState::Pending
                                                {
                                                    need_ui_prompt = true;
                                                }
                                                entry.relay_id = relay_id.clone();
                                            }
                                            if let Some(l_addr) = client_lan_addr {
                                                if entry.lan_addr != Some(l_addr) {
                                                    entry.lan_addr = Some(l_addr);
                                                    should_punch_lan = true;
                                                }
                                            }
                                            if !is_from_server {
                                                entry.addr = from_addr;
                                                entry.is_direct = true;
                                            } else if !client_confirmed_direct {
                                                entry.is_direct = false;
                                            }
                                            entry.last_seen = Instant::now();
                                            entry.target_fps = fps;
                                            entry.capture_cursor = cur;
                                            entry.jpeg_quality = q_val;
                                            entry.scale_pct = sc_val;
                                            entry.chroma_444 = c444;
                                            entry.codec_pref = client_codec_pref;

                                            match entry.auth_state {
                                                AuthState::Pending => AUTH_PENDING,
                                                AuthState::Approved => AUTH_APPROVED,
                                                AuthState::Declined(_) => AUTH_DECLINED,
                                            }
                                        } else {
                                            AUTH_PENDING
                                        }
                                    };

                                    // Guarantee that if a subscriber is Pending and pending_approval_store is empty,
                                    // the Host UI approval banner is immediately populated and brought to front!
                                    if auth_status_byte == AUTH_PENDING {
                                        if let Ok(mut pg) = pending_approval_store().lock() {
                                            if pg.is_none() || need_ui_prompt {
                                                *pg = Some(PendingApprovalRequest {
                                                    client_key: client_key.clone(),
                                                    client_id: client_id_disp.clone(),
                                                });
                                                need_ui_prompt = true;
                                            }
                                        }
                                    }

                                    if need_ui_prompt {
                                        crate::platform::ui::prompt_incoming_request(&client_id_disp);
                                    }

                                    // If client advertised a same-subnet LAN address, punch directly to it on the LAN!
                                    if let Some(l_addr) = client_lan_addr {
                                        if should_punch_lan || is_from_server {
                                            let punch = SignalMsg::Punch {
                                                from_id: my_own_id.clone(),
                                            };
                                            if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                                let _ = socket_recv.send_to(&pbytes, l_addr).await;
                                            }
                                        }
                                    }

                                    // Send 12-byte Auth Status packet (including Host's local LAN endpoint!)
                                    let mut auth_pkt = Vec::with_capacity(12);
                                    auth_pkt.extend_from_slice(MAGIC_DSKY);
                                    auth_pkt.push(PKT_AUTH_STATUS);
                                    auth_pkt.push(auth_status_byte);
                                    auth_pkt.extend_from_slice(&host_lan_bytes);

                                    if !is_from_server {
                                        let _ = socket_recv.send_to(&auth_pkt, from_addr).await;
                                    }
                                    if let Some(l_addr) = client_lan_addr {
                                        let _ = socket_recv.send_to(&auth_pkt, l_addr).await;
                                    }
                                    if !relay_id.is_empty() {
                                        let relay_auth = wrap_relay_packet(&relay_id, &auth_pkt);
                                        let _ = socket_recv.send_to(&relay_auth, target_server).await;
                                    }

                                    // If Unattended Access is enabled on Host, challenge client with salt!
                                    if crate::identity::is_unattended_enabled() {
                                        if let Some(salt) = crate::identity::get_unattended_salt() {
                                            let mut ch_pkt = Vec::with_capacity(5 + salt.len());
                                            ch_pkt.extend_from_slice(MAGIC_DSKY);
                                            ch_pkt.push(PKT_UNATTENDED_CHALLENGE);
                                            ch_pkt.extend_from_slice(salt.as_bytes());
                                            if !is_from_server {
                                                let _ = socket_recv.send_to(&ch_pkt, from_addr).await;
                                            }
                                            if let Some(l_addr) = client_lan_addr {
                                                let _ = socket_recv.send_to(&ch_pkt, l_addr).await;
                                            }
                                            if !relay_id.is_empty() {
                                                let relay_ch = wrap_relay_packet(&relay_id, &ch_pkt);
                                                let _ = socket_recv.send_to(&relay_ch, target_server).await;
                                            }
                                        }
                                    }

                                    // Reply with Pong if timestamp included
                                    if slice.len() >= 11 {
                                        let mut pong = Vec::with_capacity(9);
                                        pong.extend_from_slice(MAGIC_DSKY);
                                        pong.push(PKT_PONG);
                                        pong.extend_from_slice(&slice[7..11]);
                                        if !is_from_server {
                                            let _ = socket_recv.send_to(&pong, from_addr).await;
                                        }
                                        if !relay_id.is_empty() {
                                            let relay_pong = wrap_relay_packet(&relay_id, &pong);
                                            let _ = socket_recv.send_to(&relay_pong, target_server).await;
                                        }
                                    }
                                }
                                PKT_INPUT_EVENT => {
                                    if !crate::p2p::is_host_remote_input_allowed() {
                                        continue;
                                    }
                                    let is_approved = {
                                        if let Ok(mut map) = clients_recv.lock() {
                                            let mut approved = false;
                                            for sub in map.values_mut() {
                                                if (sub.addr == from_addr
                                                    || sub.lan_addr == Some(from_addr)
                                                    || from_addr == target_server)
                                                    && sub.auth_state == AuthState::Approved
                                                {
                                                    sub.last_seen = Instant::now();
                                                    if from_addr != target_server {
                                                        sub.addr = from_addr;
                                                        sub.is_direct = true;
                                                    }
                                                    approved = true;
                                                }
                                            }
                                            approved
                                        } else {
                                            false
                                        }
                                    };
                                    if is_approved {
                                        if let Ok(ev) =
                                            serde_json::from_slice::<InputEvent>(&slice[5..])
                                        {
                                            injector.handle_event(&ev);
                                        }
                                    }
                                }
                                PKT_DISCONNECT => {
                                    if let Ok(mut map) = clients_recv.lock() {
                                        if from_addr == target_server {
                                            map.clear();
                                        } else {
                                            map.retain(|_, sub| {
                                                sub.addr != from_addr
                                                    && sub.lan_addr != Some(from_addr)
                                            });
                                        }
                                    }
                                    if let Ok(mut pg) = pending_approval_store().lock() {
                                        *pg = None;
                                    }
                                    crate::platform::ui::request_main_redraw();
                                    info!("Client {} disconnected from host stream.", from_addr);
                                }
                                PKT_CLIPBOARD => {
                                    if !crate::p2p::is_host_clipboard_allowed() {
                                        continue;
                                    }
                                    if slice.len() > 5 {
                                        if let Ok(text) = std::str::from_utf8(&slice[5..]) {
                                            let hash = crate::p2p::state::compute_clipboard_hash(text);
                                            crate::p2p::state::LAST_SYNCED_CLIPBOARD_HASH.store(hash, Ordering::Relaxed);
                                            crate::p2p::state::write_clipboard_text(text);
                                            info!("📋 [HOST] Clipboard synced from peer ({} chars)", text.chars().count());
                                        }
                                    }
                                }
                                PKT_FILE_START => {
                                    if !crate::p2p::is_host_file_transfer_allowed() {
                                        continue;
                                    }
                                    if let Ok(mut map) = clients_recv.lock() {
                                        for sub in map.values_mut() {
                                            if (sub.addr == from_addr || sub.lan_addr == Some(from_addr) || from_addr == target_server)
                                                && sub.auth_state == AuthState::Approved
                                            {
                                                sub.last_seen = Instant::now();
                                            }
                                        }
                                    }
                                    if slice.len() >= 15 {
                                        let size = u64::from_le_bytes(slice[5..13].try_into().unwrap());
                                        let name_len = u16::from_le_bytes(slice[13..15].try_into().unwrap()) as usize;
                                        if slice.len() >= 15 + name_len {
                                            if let Ok(name) = std::str::from_utf8(&slice[15..15 + name_len]) {
                                                crate::p2p::state::handle_incoming_file_start(size, name);
                                            }
                                        }
                                    }
                                }
                                PKT_FILE_CHUNK => {
                                    if let Ok(mut map) = clients_recv.lock() {
                                        for sub in map.values_mut() {
                                            if (sub.addr == from_addr || sub.lan_addr == Some(from_addr) || from_addr == target_server)
                                                && sub.auth_state == AuthState::Approved
                                            {
                                                sub.last_seen = Instant::now();
                                            }
                                        }
                                    }
                                    if slice.len() >= 9 {
                                        let chunk_idx = u32::from_le_bytes(slice[5..9].try_into().unwrap());
                                        crate::p2p::state::handle_incoming_file_chunk(chunk_idx, &slice[9..]);
                                    }
                                }
                                PKT_FILE_FINISH => {
                                    if let Ok(mut map) = clients_recv.lock() {
                                        for sub in map.values_mut() {
                                            if (sub.addr == from_addr || sub.lan_addr == Some(from_addr) || from_addr == target_server)
                                                && sub.auth_state == AuthState::Approved
                                            {
                                                sub.last_seen = Instant::now();
                                            }
                                        }
                                    }
                                    if let Some(missing) = crate::p2p::state::handle_incoming_file_finish() {
                                        let mut nack_pkt = Vec::with_capacity(5 + missing.len() * 4);
                                        nack_pkt.extend_from_slice(MAGIC_DSKY);
                                        nack_pkt.push(PKT_FILE_NACK);
                                        for idx in missing {
                                            nack_pkt.extend_from_slice(&idx.to_le_bytes());
                                        }
                                        let targets: Vec<ClientSubscriber> = if let Ok(map) = clients_recv.lock() {
                                            map.values().filter(|s| s.auth_state == AuthState::Approved).cloned().collect()
                                        } else {
                                            Vec::new()
                                        };
                                        for sub in targets {
                                            if sub.is_direct || sub.addr != target_server {
                                                let _ = socket_recv.send_to(&nack_pkt, sub.addr).await;
                                                if let Some(l_addr) = sub.lan_addr {
                                                    if l_addr != sub.addr {
                                                        let _ = socket_recv.send_to(&nack_pkt, l_addr).await;
                                                    }
                                                }
                                            } else if !sub.relay_id.is_empty() {
                                                let relay = wrap_relay_packet(&sub.relay_id, &nack_pkt);
                                                let _ = socket_recv.send_to(&relay, target_server).await;
                                            }
                                        }
                                    }
                                }
                                PKT_FILE_NACK => {
                                    if slice.len() > 5 {
                                        let missing: Vec<u32> = slice[5..]
                                            .chunks_exact(4)
                                            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                                            .collect();
                                        crate::p2p::state::handle_outgoing_file_nack(&missing);
                                    }
                                }
                                PKT_ADMIN_REQ => {
                                    let maybe_dest = {
                                        if let Ok(mut map) = clients_recv.lock() {
                                            let mut found = None;
                                            for sub in map.values_mut() {
                                                if (sub.addr == from_addr || sub.lan_addr == Some(from_addr) || from_addr == target_server)
                                                    && sub.auth_state == AuthState::Approved
                                                {
                                                    sub.last_seen = Instant::now();
                                                    found = Some((sub.addr, sub.relay_id.clone()));
                                                    break;
                                                }
                                            }
                                            found
                                        } else {
                                            None
                                        }
                                    };
                                    if let Some((client_addr, norm_relay_id)) = maybe_dest {
                                        if slice.len() >= 13 {
                                            let msg_id = u32::from_le_bytes(slice[5..9].try_into().unwrap());
                                            let chunk_idx = u16::from_le_bytes(slice[9..11].try_into().unwrap());
                                            let total_chunks = u16::from_le_bytes(slice[11..13].try_into().unwrap());
                                            if let Some(req) = crate::p2p::state::feed_admin_req_chunk(msg_id, chunk_idx, total_chunks, &slice[13..]) {
                                                let socket_reply = socket_recv.clone();
                                                let srv_addr = target_server;
                                                tokio::spawn(async move {
                                                    let resp = crate::platform::admin::handle_admin_request(req);
                                                    if let Ok(resp_bytes) = serde_json::to_vec(&resp) {
                                                        let resp_pkts = crate::p2p::state::create_chunked_admin_packets(
                                                            crate::p2p::state::PKT_ADMIN_RESP,
                                                            msg_id,
                                                            &resp_bytes,
                                                        );
                                                        for pkt in resp_pkts {
                                                            if client_addr != srv_addr {
                                                                let _ = socket_reply.send_to(&pkt, client_addr).await;
                                                            }
                                                            if !norm_relay_id.is_empty() {
                                                                let relay_pkt = wrap_relay_packet(&norm_relay_id, &pkt);
                                                                let _ = socket_reply.send_to(&relay_pkt, srv_addr).await;
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                                PKT_UNATTENDED_AUTH => {
                                    if auth_limiter.is_locked_out(from_addr.ip()) {
                                        warn!("🔒 [HOST] Rate-limited unattended auth attempt from {}", from_addr);
                                        continue;
                                    }
                                    if slice.len() > 5 {
                                        if let Ok(hash_str) = std::str::from_utf8(&slice[5..]) {
                                            if crate::identity::verify_auth_hash(hash_str) {
                                                auth_limiter.clear(from_addr.ip());
                                                info!("🔓 [HOST] Client authenticated successfully via Unattended Password!");
                                                let mut maybe_sub = None;
                                                if let Ok(mut map) = clients_recv.lock() {
                                                    for sub in map.values_mut() {
                                                        if sub.addr == from_addr || sub.lan_addr == Some(from_addr) || from_addr == target_server {
                                                            sub.auth_state = AuthState::Approved;
                                                            sub.last_seen = Instant::now();
                                                            maybe_sub = Some((sub.addr, sub.lan_addr, sub.relay_id.clone()));
                                                        }
                                                    }
                                                }
                                                if let Ok(mut pg) = pending_approval_store().lock() {
                                                    *pg = None;
                                                }
                                                // Send immediate AUTH_APPROVED packet!
                                                let mut auth_pkt = Vec::with_capacity(12);
                                                auth_pkt.extend_from_slice(MAGIC_DSKY);
                                                auth_pkt.push(PKT_AUTH_STATUS);
                                                auth_pkt.push(AUTH_APPROVED);
                                                auth_pkt.extend_from_slice(&host_lan_bytes);
                                                if from_addr != target_server {
                                                    let _ = socket_recv.send_to(&auth_pkt, from_addr).await;
                                                }
                                                if let Some((_sub_addr, sub_lan, sub_relay)) = maybe_sub {
                                                    if let Some(l_addr) = sub_lan {
                                                        if l_addr != from_addr {
                                                            let _ = socket_recv.send_to(&auth_pkt, l_addr).await;
                                                        }
                                                    }
                                                    if !sub_relay.is_empty() {
                                                        let relay_auth = wrap_relay_packet(&sub_relay, &auth_pkt);
                                                        let _ = socket_recv.send_to(&relay_auth, target_server).await;
                                                    }
                                                }
                                                crate::platform::ui::request_main_redraw();
                                            } else {
                                                auth_limiter.record_failure(from_addr.ip());
                                                warn!("🔒 [HOST] Invalid unattended password hash attempt from {}", from_addr);
                                            }
                                        }
                                    }
                                }
                                PKT_E2EE_HELLO => {
                                    if slice.len() >= 37 {
                                        let mut peer_pub = [0u8; 32];
                                        peer_pub.copy_from_slice(&slice[5..37]);
                                        let kp = crate::p2p::crypto::E2eeKeypair::generate();
                                        let cipher = std::sync::Arc::new(kp.derive_session_cipher(&peer_pub, false));
                                        if let Ok(mut g) = session_cipher_recv.lock() {
                                            *g = Some(cipher);
                                        }
                                        force_kf_recv.store(true, Ordering::Relaxed);

                                        let mut ack = Vec::with_capacity(37);
                                        ack.extend_from_slice(MAGIC_DSKY);
                                        ack.push(PKT_E2EE_ACK);
                                        ack.extend_from_slice(&kp.public_bytes());
                                        if from_addr != target_server {
                                            let _ = socket_recv.send_to(&ack, from_addr).await;
                                        }
                                        let dests: Vec<(SocketAddr, Option<SocketAddr>, String)> = if let Ok(map) = clients_recv.lock() {
                                            map.values()
                                                .map(|s| (s.addr, s.lan_addr, s.relay_id.clone()))
                                                .collect()
                                        } else {
                                            Vec::new()
                                        };
                                        for (s_addr, s_lan, s_relay) in dests {
                                            if s_addr != target_server && s_addr != from_addr {
                                                let _ = socket_recv.send_to(&ack, s_addr).await;
                                            }
                                            if let Some(l_addr) = s_lan {
                                                let _ = socket_recv.send_to(&ack, l_addr).await;
                                            }
                                            if !s_relay.is_empty() {
                                                let relay_ack = wrap_relay_packet(&s_relay, &ack);
                                                let _ = socket_recv.send_to(&relay_ack, target_server).await;
                                            }
                                        }
                                        info!("🔐 [HOST E2EE] X25519 + ChaCha20-Poly1305 session established with {}", from_addr);
                                    }
                                }
                                PKT_KEYFRAME_REQ => {
                                    force_kf_recv.store(true, Ordering::Relaxed);
                                }
                                PKT_CHAT_MESSAGE => {
                                    if slice.len() > 5 {
                                        if let Ok(text) = std::str::from_utf8(&slice[5..]) {
                                            crate::p2p::receive_chat_message("Client", text);
                                            crate::platform::ui::request_main_redraw();
                                        }
                                    }
                                }
                                PKT_VOICE_CHUNK => {
                                    if slice.len() >= 10 {
                                        #[cfg(target_os = "windows")]
                                        {
                                            let channels = slice[5] as u16;
                                            let sample_rate = u32::from_le_bytes(slice[6..10].try_into().unwrap());
                                            let pcm = &slice[10..];
                                            crate::windows::audio::play_remote_voice_chunk(sample_rate, channels, pcm);
                                        }
                                    }
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
                                    SERVER_ONLINE.store(true, Ordering::Relaxed);
                                    if let Ok(mut g) = last_hb_ack_clone.lock() {
                                        *g = Instant::now();
                                    }
                                    info!(
                                        "✅ [VDS APPROVED] Permanent Desky ID '{}' assigned! Public NAT endpoint: {}",
                                        desky_id, your_public_addr
                                    );
                                    crate::identity::save_assigned_desky_id(&desky_id);
                                    crate::platform::set_assigned_desky_id(&desky_id);

                                    let mut id_w = current_id_clone.write().await;
                                    *id_w = desky_id;
                                }
                                SignalMsg::HeartbeatAck => {
                                    SERVER_ONLINE.store(true, Ordering::Relaxed);
                                    if let Ok(g) = last_hb_send_clone.lock() {
                                        let rtt = g.elapsed().as_millis() as u32;
                                        SERVER_PING_MS.store(rtt.clamp(1, 999), Ordering::Relaxed);
                                    }
                                    if let Ok(mut g) = last_hb_ack_clone.lock() {
                                        *g = Instant::now();
                                    }
                                }
                                SignalMsg::IncomingPeer { client_public_addr } => {
                                    if let Ok(parsed_addr) =
                                        client_public_addr.parse::<SocketAddr>()
                                    {
                                        let client_addr =
                                            fix_loopback_peer(parsed_addr, target_server);

                                        let should_spawn = match last_incoming_punch {
                                            Some((prev_addr, when))
                                                if prev_addr == client_addr
                                                    && when.elapsed() < Duration::from_secs(4) =>
                                            {
                                                false
                                            }
                                            _ => true,
                                        };

                                        if should_spawn {
                                            last_incoming_punch =
                                                Some((client_addr, Instant::now()));
                                            info!(
                                                "⚡ [P2P INCOMING] Peer connecting from {}! Firing Direct UDP hole-punch packets...",
                                                client_addr
                                            );

                                            let my_id = current_id_clone.read().await.clone();
                                            let punch = SignalMsg::Punch { from_id: my_id };
                                            if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                                let sock_punch = Arc::clone(&socket_recv);
                                                tokio::spawn(async move {
                                                    let base_ip = client_addr.ip();
                                                    let base_port = client_addr.port();
                                                    for _ in 0..35 {
                                                        let _ = sock_punch
                                                            .send_to(&pbytes, client_addr)
                                                            .await;
                                                        for offset in -6i32..=6i32 {
                                                            let p =
                                                                (base_port as i32 + offset) as u16;
                                                            if p > 1024 {
                                                                let _ = sock_punch
                                                                    .send_to(
                                                                        &pbytes,
                                                                        SocketAddr::new(base_ip, p),
                                                                    )
                                                                    .await;
                                                            }
                                                        }
                                                        tokio::time::sleep(Duration::from_millis(
                                                            50,
                                                        ))
                                                        .await;
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                                SignalMsg::Punch { from_id } => {
                                    if from_addr == target_server {
                                        continue;
                                    }
                                    let norm_cid = normalize_id(&from_id);
                                    let client_key =
                                        if !norm_cid.is_empty() && norm_cid != "000000000" {
                                            norm_cid
                                        } else {
                                            from_addr.to_string()
                                        };
                                    let client_id_disp = format_id_display(&from_id);

                                    let mut need_ui_prompt = false;
                                    let mut first_direct = false;
                                    let auth_status_byte =
                                        if let Ok(mut map) = clients_recv.lock() {
                                            let addr_key = from_addr.to_string();
                                            if client_key != addr_key {
                                                if let Some(mut pre_sub) = map.remove(&addr_key) {
                                                    pre_sub.client_id = client_id_disp.clone();
                                                    pre_sub.addr = from_addr;
                                                    pre_sub.is_direct = true;
                                                    pre_sub.last_seen = Instant::now();
                                                    map.insert(client_key.clone(), pre_sub);
                                                }
                                            }
                                            let entry = map
                                                .entry(client_key.clone())
                                                .or_insert_with(|| {
                                                    need_ui_prompt = true;
                                                    first_direct = true;
                                                    ClientSubscriber {
                                                        client_id: client_id_disp.clone(),
                                                        relay_id: String::new(),
                                                        addr: from_addr,
                                                        lan_addr: None,
                                                        auth_state: AuthState::Pending,
                                                        last_seen: Instant::now(),
                                                        target_fps: 60,
                                                        capture_cursor: true,
                                                        jpeg_quality: 76,
                                                        scale_pct: 100,
                                                        chroma_444: true,
                                                        is_direct: true,
                                                        codec_pref: 0,
                                                    }
                                                });
                                            if entry.addr != from_addr {
                                                first_direct = true;
                                            }
                                            entry.client_id = client_id_disp.clone();
                                            entry.addr = from_addr;
                                            entry.is_direct = true;
                                            entry.last_seen = Instant::now();

                                            match entry.auth_state {
                                                AuthState::Pending => AUTH_PENDING,
                                                AuthState::Approved => AUTH_APPROVED,
                                                AuthState::Declined(_) => AUTH_DECLINED,
                                            }
                                        } else {
                                            AUTH_PENDING
                                        };

                                    if first_direct {
                                        info!(
                                            "🎉 [DIRECT P2P PROBE] Received direct punch from peer '{}' ({})! Verifying return path...",
                                            client_id_disp, from_addr
                                        );
                                    }

                                    if auth_status_byte == AUTH_PENDING {
                                        if let Ok(mut pg) = pending_approval_store().lock() {
                                            if pg.is_none() || need_ui_prompt {
                                                *pg = Some(PendingApprovalRequest {
                                                    client_key: client_key.clone(),
                                                    client_id: client_id_disp.clone(),
                                                });
                                                need_ui_prompt = true;
                                            }
                                        }
                                    }

                                    if need_ui_prompt {
                                        crate::platform::ui::prompt_incoming_request(&client_id_disp);
                                    }

                                    let ack = SignalMsg::PunchAck;
                                    if let Ok(abytes) = serde_json::to_vec(&ack) {
                                        let _ = socket_recv.send_to(&abytes, from_addr).await;
                                    }
                                    let mut auth_pkt = Vec::with_capacity(12);
                                    auth_pkt.extend_from_slice(MAGIC_DSKY);
                                    auth_pkt.push(PKT_AUTH_STATUS);
                                    auth_pkt.push(auth_status_byte);
                                    auth_pkt.extend_from_slice(&host_lan_bytes);
                                    let _ = socket_recv.send_to(&auth_pkt, from_addr).await;
                                }
                                SignalMsg::PunchAck => {
                                    if from_addr != target_server {
                                        if let Ok(mut map) = clients_recv.lock() {
                                            for sub in map.values_mut() {
                                                if sub.addr == from_addr
                                                    || sub.lan_addr == Some(from_addr)
                                                {
                                                    sub.is_direct = true;
                                                    sub.addr = from_addr;
                                                    sub.last_seen = Instant::now();
                                                }
                                            }
                                        }
                                        info!(
                                            "🎉 [DIRECT P2P VERIFIED] Bidirectional direct P2P verified with {}!",
                                            from_addr
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                });

                // Keepalive heartbeat loop (every 5 seconds with live ping calculation)
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                while running.load(Ordering::Relaxed) {
                    interval.tick().await;

                    if let Ok(g) = last_hb_ack.lock() {
                        if g.elapsed() > Duration::from_secs(12) {
                            SERVER_ONLINE.store(false, Ordering::Relaxed);
                        }
                    }
                    if let Ok(mut g) = last_hb_send.lock() {
                        *g = Instant::now();
                    }

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