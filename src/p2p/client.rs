use crate::p2p::state::{
    apply_preset_for_mode, decode_jpeg_to_bgra, decode_lan_endpoint, encode_lan_endpoint_port,
    fix_loopback_peer, frame_store, get_active_quality_params, get_client_codec_pref, input_notify, input_store,
    lan_broadcast_addrs, normalize_id, resolve_server_addr,
    set_active_session_target,
    set_session_status, update_client_preferences, wrap_relay_packet, SessionStatus,
    ACTIVE_CHROMA_444, ACTIVE_JPEG_QUALITY, ACTIVE_SCALE_PCT, AUTH_APPROVED, AUTH_DECLINED,
    AUTH_PENDING, AUTH_REQUIRES_PASSWORD, CLIENT_CAPTURE_CURSOR, CLIENT_SESSION_ACTIVE, CLIENT_SESSION_GEN,
    CLIENT_TARGET_FPS, MAGIC_DSKY, PKT_AUTH_STATUS, PKT_CLIENT_HELLO, PKT_DISCONNECT,
    PKT_FRAME_CHUNK, PKT_FRAME_CHUNK_H264, PKT_FRAME_CHUNK_H265, PKT_INPUT_EVENT, PKT_LAN_DISCOVERY, PKT_PONG, QUALITY_MODE,
    PKT_CLIPBOARD, PKT_FILE_START, PKT_FILE_CHUNK, PKT_FILE_FINISH, PKT_ADMIN_RESP,
    PKT_AUDIO_CHUNK, PKT_CURSOR_SHAPE, PKT_UNATTENDED_CHALLENGE, PKT_UNATTENDED_AUTH, PKT_CHAT_MESSAGE,
    PKT_VOICE_CHUNK, pop_outgoing_voice_packet, voice_notify, PKT_FILE_NACK,
    admin_notify, pop_outgoing_admin_packet, feed_admin_resp_chunk, store_admin_response,
    clipboard_notify, pop_outgoing_clipboard, file_notify, pop_outgoing_file_packet,
    handle_incoming_file_start, handle_incoming_file_chunk, handle_incoming_file_finish,
    write_clipboard_text, compute_clipboard_hash, LAST_SYNCED_CLIPBOARD_HASH,
    SESSION_BITRATE_KBPS, SESSION_RTT_MS,
};
use crate::protocol::{
    InputEvent, SignalMsg, PKT_E2EE_ACK, PKT_E2EE_HELLO, PKT_ENCRYPTED_FRAME, PKT_KEYFRAME_REQ,
};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tracing::{error, info, warn};

struct PartialFrame {
    frame_id: u32,
    codec_type: u8,
    total_chunks: u16,
    orig_width: u16,
    orig_height: u16,
    chunks: Vec<Option<Vec<u8>>>,
    received_count: u16,
    created_at: Instant,
}

/// Starts a remote desktop client session connecting to `target_id`
pub fn start_client_session(target_id: &str, initial_fps: u32, capture_cursor: bool) {
    let direct_target_addr = crate::p2p::parse_direct_target(target_id);
    let is_serverless = direct_target_addr.is_some() || crate::p2p::is_serverless_mode();
    let force_relay = crate::p2p::is_force_server_relay();

    let norm_target = if direct_target_addr.is_some() {
        "DIRECT_IP".to_string()
    } else {
        normalize_id(target_id)
    };
    let display_target = if let Some(addr) = direct_target_addr {
        format!("Direct IP: {}", addr)
    } else if norm_target.len() == 9 {
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
    set_active_session_target(Some(target_id.to_string()));

    let my_id_norm = {
        let ident = crate::identity::load_or_create_identity();
        normalize_id(&ident.desky_id.unwrap_or_else(|| "000000000".into()))
    };
    let my_id_padded = if my_id_norm.len() == 9 {
        my_id_norm
    } else {
        "000000000".to_string()
    };

    // Unique 9-digit session relay ID prefixed with '0' so VDS can route Host -> Client replies
    // directly to this client socket without colliding with 100M..999M Host Desky IDs.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let my_relay_id = format!(
        "0{:08}",
        ((nanos ^ (my_gen.wrapping_mul(1_664_525))) % 89_999_999) + 10_000_000
    );

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
            let socket = {
                let builder = match socket2::Socket::new(
                    socket2::Domain::IPV4,
                    socket2::Type::DGRAM,
                    Some(socket2::Protocol::UDP),
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        set_session_status(SessionStatus::Error(format!("Socket create error: {}", e)));
                        return;
                    }
                };
                let _ = builder.set_broadcast(true);
                let _ = builder.set_send_buffer_size(4 * 1024 * 1024);
                let _ = builder.set_recv_buffer_size(4 * 1024 * 1024);
                let any_addr: std::net::SocketAddr = "0.0.0.0:0".parse().unwrap();
                if let Err(e) = builder.bind(&any_addr.into()) {
                    set_session_status(SessionStatus::Error(format!("Socket bind error: {}", e)));
                    return;
                }
                let _ = builder.set_nonblocking(true);
                let std_sock: std::net::UdpSocket = builder.into();
                match UdpSocket::from_std(std_sock) {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        set_session_status(SessionStatus::Error(format!("Socket wrap error: {}", e)));
                        return;
                    }
                }
            };

            let server_addr = resolve_server_addr();
            let lan_bcast_targets = lan_broadcast_addrs(server_addr, 21117);
            let local_port = socket.local_addr().ok().map(|a| a.port()).unwrap_or(0);
            let my_lan_bytes = encode_lan_endpoint_port(local_port, server_addr);

            let relay_hb = SignalMsg::Heartbeat {
                desky_id: my_relay_id.clone(),
            };
            let mut lan_pkt = Vec::with_capacity(14);
            if norm_target.len() == 9 {
                lan_pkt.extend_from_slice(MAGIC_DSKY);
                lan_pkt.push(PKT_LAN_DISCOVERY);
                lan_pkt.extend_from_slice(norm_target.as_bytes());
            }
            let req = SignalMsg::ConnectRequest {
                target_id: display_target.clone(),
            };

            // 0. Register this client socket's session relay ID on VDS for instant Host -> Client return routing
            if !is_serverless {
                if let Ok(hb_bytes) = serde_json::to_vec(&relay_hb) {
                    let _ = socket.send_to(&hb_bytes, server_addr).await;
                }
            }

            // 1. Fire LAN discovery broadcast on port 21117 (subnet broadcast + global broadcast + localhost)
            if !is_serverless && !lan_pkt.is_empty() {
                for bcast in &lan_bcast_targets {
                    let _ = socket.send_to(&lan_pkt, *bcast).await;
                }
            }

            // 2. Send ConnectRequest to VDS Matchmaker (skip in serverless direct IP mode)
            if !is_serverless {
                if let Ok(req_bytes) = serde_json::to_vec(&req) {
                    let _ = socket.send_to(&req_bytes, server_addr).await;
                }
            } else if let Some(direct_addr) = direct_target_addr {
                info!("🚀 [SERVERLESS] Direct P2P session initiated to {}", direct_addr);
            }

            let mut peer_addr: Option<SocketAddr> = direct_target_addr;
            let mut peer_lan_addr: Option<SocketAddr> = None;
            let mut last_punched_peer: Option<SocketAddr> = None;
            let mut is_direct_peer = direct_target_addr.is_some() && !force_relay;
            let mut logged_direct_peer = false;
            let mut host_contacted = direct_target_addr.is_some();
            let mut buf = vec![0u8; 65536];
            let mut partial_frames: Vec<PartialFrame> = Vec::with_capacity(32);
            let mut highest_decoded_frame: u32 = 0;

            let session_start = Instant::now();
            let mut last_hello = Instant::now() - Duration::from_secs(10);
            let mut last_matchmaker_retry = Instant::now();
            let mut last_relay_hb = Instant::now();
            let mut received_any_frame = false;

            // Dedicated Latest-Frame Decoder Thread so UDP recv_from NEVER blocks on CPU decoding!
            let decode_slot: Arc<(Mutex<Option<(u32, u32, u32, Vec<u8>, u8)>>, std::sync::Condvar)> =
                Arc::new((Mutex::new(None), std::sync::Condvar::new()));
            {
                let slot_worker = Arc::clone(&decode_slot);
                let disp_worker = display_target.clone();
                std::thread::spawn(move || {
                    let mut logged_codec_info = false;
                    let mut logged_first = false;
                    let mut h264_decoder = crate::platform::codec::PlatformH264Decoder::new().ok();
                    let mut h265_decoder = crate::platform::codec::PlatformH265Decoder::new().ok();

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
                                let (g, _) =
                                    match cvar.wait_timeout(guard, Duration::from_millis(50)) {
                                        Ok(res) => res,
                                        Err(_) => break,
                                    };
                                guard = g;
                            }
                            guard.take()
                        };

                        if let Some((f_id, orig_w, orig_h, frame_bytes, codec_type)) = job {
                            crate::p2p::state::set_active_stream_codec(codec_type);
                            if !logged_codec_info {
                                logged_codec_info = true;
                                tracing::info!(
                                    "Active Remote Video Stream: Codec = '{}' ({}x{})",
                                    crate::p2p::state::get_active_stream_codec_name(),
                                    orig_w,
                                    orig_h
                                );
                            }
                            let decoded_opt = if codec_type == PKT_FRAME_CHUNK_H265 {
                                use crate::codec::VideoDecoder;
                                if let Some(ref mut dec) = h265_decoder {
                                    if let Ok(bgra) = dec.decode(&frame_bytes, orig_w, orig_h) {
                                        Some(crate::p2p::state::RemoteFrame {
                                            frame_id: f_id,
                                            orig_width: orig_w,
                                            orig_height: orig_h,
                                            width: orig_w,
                                            height: orig_h,
                                            bgra,
                                        })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else if codec_type == PKT_FRAME_CHUNK_H264 {
                                use crate::codec::VideoDecoder;
                                if let Some(ref mut dec) = h264_decoder {
                                    if let Ok(bgra) = dec.decode(&frame_bytes, orig_w, orig_h) {
                                        Some(crate::p2p::state::RemoteFrame {
                                            frame_id: f_id,
                                            orig_width: orig_w,
                                            orig_height: orig_h,
                                            width: orig_w,
                                            height: orig_h,
                                            bgra,
                                        })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                decode_jpeg_to_bgra(f_id, orig_w, orig_h, &frame_bytes)
                            };

                            if let Some(decoded) = decoded_opt
                            {
                                if !logged_first {
                                    logged_first = true;
                                    #[cfg(target_os = "windows")]
                                    crate::windows::trim_process_working_set();
                                    set_session_status(SessionStatus::Connected(
                                        disp_worker.clone(),
                                    ));
                                    if crate::recorder::is_auto_record_enabled() && !crate::recorder::is_recording() {
                                        let _ = crate::recorder::start_recording(decoded.orig_width, decoded.orig_height, 60);
                                    }
                                    crate::platform::ui::append_status_log(&format!(
                                        "🎉 Connected to Remote Desk {} ({}x{})!",
                                        disp_worker, decoded.orig_width, decoded.orig_height
                                    ));
                                }

                                if crate::recorder::is_recording() {
                                    if codec_type == PKT_FRAME_CHUNK_H264 || codec_type == PKT_FRAME_CHUNK_H265 {
                                        crate::recorder::push_bgra_frame(&decoded.bgra, decoded.width, decoded.height);
                                    } else {
                                        crate::recorder::push_frame(&frame_bytes);
                                    }
                                }

                                if let Ok(mut g) = frame_store().lock() {
                                    *g = Some(Arc::new(decoded));
                                }
                                crate::platform::ui::request_main_redraw();
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
            let e2ee_kp = crate::p2p::crypto::E2eeKeypair::generate();
            let mut e2ee_cipher: Option<crate::p2p::crypto::SessionCipher> = None;
            let mut last_kf_req = Instant::now() - Duration::from_secs(2);
            let mut current_challenge_salt: Option<String> = None;
            let mut last_rx_from_peer = Instant::now();
            let mut is_reconnecting = false;

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
                        if abr_frames_dropped >= 2
                            || (abr_frames_dropped >= 1 && abr_frames_completed == 0)
                            || rtt > 220
                        {
                            abr_tier = (abr_tier - 1).max(0);
                        } else if received_any_frame
                            && abr_frames_dropped == 0
                            && abr_frames_completed >= 5
                            && rtt < 110
                        {
                            abr_tier = (abr_tier + 1).min(3);
                        }

                        let (q, sc, c444) = match abr_tier {
                            3 => (90u8, 100u8, true),
                            2 => (85u8, 100u8, true),
                            1 => (78u8, 100u8, is_direct_peer),
                            _ => (70u8, 90u8, false),
                        };
                        ACTIVE_JPEG_QUALITY.store(q, Ordering::Relaxed);
                        ACTIVE_SCALE_PCT.store(sc, Ordering::Relaxed);
                        ACTIVE_CHROMA_444.store(c444, Ordering::Relaxed);
                    }

                    if abr_frames_dropped >= 1 && last_kf_req.elapsed() >= Duration::from_millis(450) {
                        last_kf_req = Instant::now();
                        let mut kf_pkt = Vec::with_capacity(5);
                        kf_pkt.extend_from_slice(MAGIC_DSKY);
                        kf_pkt.push(PKT_KEYFRAME_REQ);
                        if !force_relay {
                            if let Some(p_addr) = peer_addr {
                                if p_addr != server_addr {
                                    let _ = socket.send_to(&kf_pkt, p_addr).await;
                                }
                            }
                            if let Some(l_addr) = peer_lan_addr {
                                let _ = socket.send_to(&kf_pkt, l_addr).await;
                            }
                        } else {
                            let relay_kf = wrap_relay_packet(&norm_target, &kf_pkt);
                            let _ = socket.send_to(&relay_kf, server_addr).await;
                        }
                    }

                    abr_window_start = Instant::now();
                    abr_bytes_received = 0;
                    abr_frames_completed = 0;
                    abr_frames_dropped = 0;
                }


                // Keep VDS relay return route alive if direct P2P hasn't been established yet (skip in serverless)
                if !is_serverless && !is_direct_peer && last_relay_hb.elapsed() >= Duration::from_secs(6) {
                    if let Ok(hb_bytes) = serde_json::to_vec(&relay_hb) {
                        let _ = socket.send_to(&hb_bytes, server_addr).await;
                    }
                    last_relay_hb = Instant::now();
                }

                // Retry ConnectRequest + LAN discovery every 1.2s until host responds (skip in serverless)
                if !host_contacted
                    && !is_serverless
                    && last_matchmaker_retry.elapsed() >= Duration::from_millis(1200)
                {
                    if session_start.elapsed() > Duration::from_secs(90) {
                        let msg = format!(
                            "Timed out connecting to Desky ID '{}'",
                            display_target
                        );
                        set_session_status(SessionStatus::Error(msg.clone()));
                        crate::platform::ui::append_status_log(&format!("[x] {}", msg));
                        break;
                    }
                    if !lan_pkt.is_empty() {
                        for bcast in &lan_bcast_targets {
                            let _ = socket.send_to(&lan_pkt, *bcast).await;
                        }
                    }
                    if let Ok(hb_bytes) = serde_json::to_vec(&relay_hb) {
                        let _ = socket.send_to(&hb_bytes, server_addr).await;
                    }
                    if let Ok(req_bytes) = serde_json::to_vec(&req) {
                        let _ = socket.send_to(&req_bytes, server_addr).await;
                    }
                    last_matchmaker_retry = Instant::now();
                }

                // Auto-fallback to server relay if direct P2P fails to deliver video frames within 3s
                if !received_any_frame
                    && is_direct_peer
                    && !force_relay
                    && !is_serverless
                    && session_start.elapsed() > Duration::from_secs(3)
                    && last_rx_from_peer.elapsed() > Duration::from_secs(2)
                {
                    warn!("⚠️ Direct P2P path stalled; falling back to server relay tunneling for {}", display_target);
                    is_direct_peer = false;
                    last_hello = Instant::now() - Duration::from_secs(10);
                }

                // Auto-reconnect & network blip resilience for active sessions
                if received_any_frame {
                    let rx_elapsed = last_rx_from_peer.elapsed();
                    if rx_elapsed > Duration::from_secs(35) {
                        let msg = format!(
                            "Connection timed out with Desky ID '{}' (No response for 35s)",
                            display_target
                        );
                        warn!("{}", msg);
                        set_session_status(SessionStatus::Error(msg.clone()));
                        crate::platform::ui::append_status_log(&format!("[x] {}", msg));
                        break;
                    } else if rx_elapsed > Duration::from_secs(4) {
                        if !is_reconnecting {
                            is_reconnecting = true;
                            warn!(
                                "⚠️ Network blip detected (>4s without peer packet). Re-punching UDP and enabling Relay mirroring..."
                            );
                            set_session_status(SessionStatus::Connecting(format!(
                                "{} (Reconnecting...)",
                                display_target
                            )));
                        }
                        if last_matchmaker_retry.elapsed() >= Duration::from_millis(1500) {
                            if let Ok(req_bytes) = serde_json::to_vec(&req) {
                                let _ = socket.send_to(&req_bytes, server_addr).await;
                            }
                            last_matchmaker_retry = Instant::now();
                        }
                    }
                }

                // Periodically send ClientHello with ABR quality parameters, relay_id, LAN endpoint, and direct confirmation flag
                let hello_interval = if received_any_frame {
                    Duration::from_millis(350)
                } else {
                    Duration::from_millis(160)
                };

                if last_hello.elapsed() >= hello_interval {
                    let fps = CLIENT_TARGET_FPS.load(Ordering::Relaxed).clamp(10, 60) as u8;
                    let cur = if CLIENT_CAPTURE_CURSOR.load(Ordering::Relaxed) {
                        1u8
                    } else {
                        0u8
                    };
                    let ts_ms = (session_start.elapsed().as_millis() & 0xFFFF_FFFF) as u32;

                    // Request full 100% native resolution and high clarity from the very first frame!
                    let (q_val, sc_val, c444) = if !received_any_frame {
                        (82u8, 100u8, true)
                    } else {
                        get_active_quality_params()
                    };

                    let mut hello = Vec::with_capacity(39);
                    hello.extend_from_slice(MAGIC_DSKY);
                    hello.push(PKT_CLIENT_HELLO);
                    hello.push(fps);
                    hello.push(cur);
                    hello.extend_from_slice(&ts_ms.to_le_bytes());
                    hello.push(q_val);
                    hello.push(sc_val);
                    hello.push(if c444 { 1u8 } else { 0u8 });
                    hello.extend_from_slice(my_id_padded.as_bytes());
                    hello.extend_from_slice(my_relay_id.as_bytes());
                    hello.extend_from_slice(&my_lan_bytes);
                    hello.push(if is_direct_peer { 1u8 } else { 0u8 });
                    #[cfg(target_os = "linux")]
                    {
                        let _ = get_client_codec_pref();
                        hello.push(3u8);
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        hello.push(get_client_codec_pref());
                    }

                    // 1. Direct WAN / Active Peer path (only if not forced relay)
                    if !force_relay {
                        if let Some(p_addr) = peer_addr {
                            if p_addr != server_addr {
                                let _ = socket.send_to(&hello, p_addr).await;
                                if !is_direct_peer {
                                    let base_ip = p_addr.ip();
                                    let base_port = p_addr.port();
                                    for offset in 1..=6u16 {
                                        if let Some(p) = base_port.checked_add(offset) {
                                            let _ = socket
                                                .send_to(&hello, SocketAddr::new(base_ip, p))
                                                .await;
                                        }
                                    }
                                }
                            }
                        }

                        // 2. Direct LAN path (always punch/hello directly if LAN endpoint is known!)
                        if let Some(l_addr) = peer_lan_addr {
                            if peer_addr != Some(l_addr) {
                                let _ = socket.send_to(&hello, l_addr).await;
                            }
                        }
                    }

                    // 3. VDS Relay fallback (used whenever direct Host -> Client P2P is not yet verified, awaiting first frame, reconnecting, or force_relay)
                    if (!is_direct_peer || !received_any_frame || is_reconnecting || force_relay) && !is_serverless {
                        let relay_hello = wrap_relay_packet(&norm_target, &hello);
                        let _ = socket.send_to(&relay_hello, server_addr).await;
                    }

                    // 4. E2EE X25519 Key Exchange initiation if session cipher not yet derived
                    if e2ee_cipher.is_none() {
                        let mut e2ee_hello = Vec::with_capacity(37);
                        e2ee_hello.extend_from_slice(MAGIC_DSKY);
                        e2ee_hello.push(PKT_E2EE_HELLO);
                        e2ee_hello.extend_from_slice(&e2ee_kp.public_bytes());
                        if !force_relay {
                            if let Some(p_addr) = peer_addr {
                                if p_addr != server_addr {
                                    let _ = socket.send_to(&e2ee_hello, p_addr).await;
                                }
                            }
                            if let Some(l_addr) = peer_lan_addr {
                                let _ = socket.send_to(&e2ee_hello, l_addr).await;
                            }
                        }
                        if (!is_direct_peer || !received_any_frame || force_relay) && !is_serverless {
                            let relay_e2ee = wrap_relay_packet(&norm_target, &e2ee_hello);
                            let _ = socket.send_to(&relay_e2ee, server_addr).await;
                        }
                    }

                    last_hello = Instant::now();
                }

                // Flush outgoing clipboard text
                while let Some(text) = pop_outgoing_clipboard() {
                    let text_bytes = text.as_bytes();
                    let mut pkt = Vec::with_capacity(5 + text_bytes.len());
                    pkt.extend_from_slice(MAGIC_DSKY);
                    pkt.push(PKT_CLIPBOARD);
                    pkt.extend_from_slice(text_bytes);

                    if let Some(p_addr) = peer_addr {
                        if p_addr != server_addr && !force_relay {
                            let _ = socket.send_to(&pkt, p_addr).await;
                        }
                    }
                    if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                        let relay = wrap_relay_packet(&norm_target, &pkt);
                        let _ = socket.send_to(&relay, server_addr).await;
                    }
                }

                // Flush outgoing file transfer packets (in smooth batches of 16 chunks with micro-pacing)
                let mut file_sent = 0;
                while file_sent < 16 {
                    if let Some(pkt) = pop_outgoing_file_packet() {
                        if let Some(p_addr) = peer_addr {
                            if p_addr != server_addr && !force_relay {
                                let _ = socket.send_to(&pkt, p_addr).await;
                            }
                        }
                        if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                            let relay = wrap_relay_packet(&norm_target, &pkt);
                            let _ = socket.send_to(&relay, server_addr).await;
                        }
                        file_sent += 1;
                        if file_sent % 4 == 0 {
                            tokio::task::yield_now().await;
                        }
                    } else {
                        break;
                    }
                }

                // Flush outgoing admin packets (Silent Admin mode)
                while let Some(pkt) = pop_outgoing_admin_packet() {
                    if let Some(p_addr) = peer_addr {
                        if p_addr != server_addr && !force_relay {
                            let _ = socket.send_to(&pkt, p_addr).await;
                        }
                    }
                    if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                        let relay = wrap_relay_packet(&norm_target, &pkt);
                        let _ = socket.send_to(&relay, server_addr).await;
                    }
                }

                // Flush outgoing intercom voice packets (Technician -> Host)
                while let Some(pkt) = pop_outgoing_voice_packet() {
                    if let Some(p_addr) = peer_addr {
                        if p_addr != server_addr && !force_relay {
                            let _ = socket.send_to(&pkt, p_addr).await;
                        }
                    }
                    if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                        let relay = wrap_relay_packet(&norm_target, &pkt);
                        let _ = socket.send_to(&relay, server_addr).await;
                    }
                }

                // Check for submitted unattended password
                if let Some(pwd) = crate::p2p::take_unattended_password() {
                    let salt = current_challenge_salt.clone().unwrap_or_else(|| "0000000000000000".to_string());
                    let hash = crate::identity::compute_auth_hash(&salt, &pwd);
                    let mut auth_pkt = Vec::with_capacity(5 + hash.len());
                    auth_pkt.extend_from_slice(MAGIC_DSKY);
                    auth_pkt.push(PKT_UNATTENDED_AUTH);
                    auth_pkt.extend_from_slice(hash.as_bytes());
                    if let Some(p_addr) = peer_addr {
                        if p_addr != server_addr && !force_relay {
                            let _ = socket.send_to(&auth_pkt, p_addr).await;
                        }
                    }
                    if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                        let relay = wrap_relay_packet(&norm_target, &auth_pkt);
                        let _ = socket.send_to(&relay, server_addr).await;
                    }
                    set_session_status(SessionStatus::Connecting(display_target.clone()));
                }

                // Flush outgoing chat messages
                for msg in crate::p2p::pull_outgoing_messages() {
                    let msg_bytes = msg.as_bytes();
                    let mut pkt = Vec::with_capacity(5 + msg_bytes.len());
                    pkt.extend_from_slice(MAGIC_DSKY);
                    pkt.push(PKT_CHAT_MESSAGE);
                    pkt.extend_from_slice(msg_bytes);
                    if let Some(p_addr) = peer_addr {
                        if p_addr != server_addr && !force_relay {
                            let _ = socket.send_to(&pkt, p_addr).await;
                        }
                    }
                    if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                        let relay = wrap_relay_packet(&norm_target, &pkt);
                        let _ = socket.send_to(&relay, server_addr).await;
                    }
                }

                // Flush queued remote input events immediately over direct P2P or Relay
                let queued_inputs: Vec<InputEvent> = {
                    if let Ok(mut q) = input_store().lock() {
                        std::mem::take(&mut *q)
                    } else {
                        Vec::new()
                    }
                };
                if !queued_inputs.is_empty() && received_any_frame {
                    for ev in queued_inputs {
                        if let Ok(ev_bytes) = serde_json::to_vec(&ev) {
                            let mut pkt = Vec::with_capacity(5 + ev_bytes.len());
                            pkt.extend_from_slice(MAGIC_DSKY);
                            pkt.push(PKT_INPUT_EVENT);
                            pkt.extend_from_slice(&ev_bytes);

                            if let Some(p_addr) = peer_addr {
                                if p_addr != server_addr && !force_relay {
                                    let _ = socket.send_to(&pkt, p_addr).await;
                                }
                            }
                            if let Some(l_addr) = peer_lan_addr {
                                if peer_addr != Some(l_addr) && !force_relay {
                                    let _ = socket.send_to(&pkt, l_addr).await;
                                }
                            }
                            if (!is_direct_peer || is_reconnecting || force_relay) && !is_serverless {
                                let relay_input = wrap_relay_packet(&norm_target, &pkt);
                                let _ = socket.send_to(&relay_input, server_addr).await;
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
                            let raw_slice = &buf[..len];
                            let decrypted_holder: Vec<u8>;
                            let slice: &[u8] = if raw_slice.len() >= 13
                                && &raw_slice[0..4] == MAGIC_DSKY
                                && raw_slice[4] == PKT_ENCRYPTED_FRAME
                            {
                                if let Some(ref cipher) = e2ee_cipher {
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
                                host_contacted = true;
                                last_rx_from_peer = Instant::now();
                                if is_reconnecting {
                                    is_reconnecting = false;
                                    info!("🔄 Session re-established successfully with {}!", display_target);
                                    set_session_status(SessionStatus::Connected(display_target.clone()));
                                }
                                if from_addr != server_addr && !force_relay {
                                    is_direct_peer = true;
                                    peer_addr = Some(from_addr);
                                    if !logged_direct_peer {
                                        logged_direct_peer = true;
                                        info!("🎉 Direct P2P tunnel established with {}!", from_addr);
                                    }
                                }

                                match pkt_type {
                                    PKT_E2EE_ACK => {
                                        if slice.len() >= 37 && e2ee_cipher.is_none() {
                                            let mut host_pub = [0u8; 32];
                                            host_pub.copy_from_slice(&slice[5..37]);
                                            e2ee_cipher = Some(e2ee_kp.derive_session_cipher(&host_pub, true));
                                            info!("🔐 [CLIENT E2EE] X25519 + ChaCha20-Poly1305 session established with {}", from_addr);
                                        }
                                    }
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
                                        if slice.len() >= 12 {
                                            if let Some(h_lan) = decode_lan_endpoint(&slice[6..12]) {
                                                if peer_lan_addr != Some(h_lan) {
                                                    peer_lan_addr = Some(h_lan);
                                                    let punch = SignalMsg::Punch {
                                                        from_id: my_id_padded.clone(),
                                                    };
                                                    if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                                        for _ in 0..3 {
                                                            let _ = socket.send_to(&pbytes, h_lan).await;
                                                        }
                                                    }
                                                    last_hello = Instant::now() - Duration::from_secs(10);
                                                }
                                            }
                                        }
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
                                                        last_hello = Instant::now() - Duration::from_secs(10);
                                                        crate::platform::ui::request_main_redraw();
                                                    }
                                                }
                                                AUTH_DECLINED => {
                                                    let msg = format!(
                                                        "Connection declined by Remote Desk {}",
                                                        display_target
                                                    );
                                                    set_session_status(SessionStatus::Error(msg.clone()));
                                                    crate::platform::ui::append_status_log(&format!("[-] {}", msg));
                                                    break;
                                                }
                                                AUTH_REQUIRES_PASSWORD => {
                                                    if !received_any_frame {
                                                        set_session_status(SessionStatus::PasswordRequired(
                                                            display_target.clone(),
                                                        ));
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    PKT_FRAME_CHUNK | PKT_FRAME_CHUNK_H264 | PKT_FRAME_CHUNK_H265 => {
                                        if slice.len() > 17 {
                                            let frame_id = u32::from_le_bytes([
                                                slice[5], slice[6], slice[7], slice[8],
                                            ]);
                                            let chunk_idx = u16::from_le_bytes([slice[9], slice[10]]);
                                            let total_chunks = u16::from_le_bytes([slice[11], slice[12]]);
                                            let orig_width = u16::from_le_bytes([slice[13], slice[14]]);
                                            let orig_height = u16::from_le_bytes([slice[15], slice[16]]);
                                            let payload = &slice[17..];

                                            if frame_id < highest_decoded_frame.saturating_sub(120) {
                                                highest_decoded_frame = 0;
                                            }

                                            if frame_id <= highest_decoded_frame
                                                || total_chunks == 0
                                                || total_chunks > 2048
                                                || chunk_idx >= total_chunks
                                            {
                                                continue;
                                            }

                                            // Multi-frame reassembly buffer (tolerates out-of-order chunks across frame boundaries!)
                                            let pos = if let Some(idx) = partial_frames
                                                .iter()
                                                .position(|pf| pf.frame_id == frame_id)
                                            {
                                                idx
                                            } else {
                                                let (max_partial_frames, max_frame_age) = crate::p2p::get_buffer_reassembly_limits();
                                                partial_frames.retain(|f| f.created_at.elapsed() < max_frame_age);
                                                if partial_frames.len() >= max_partial_frames {
                                                    let oldest = partial_frames.remove(0);
                                                    if oldest.received_count < oldest.total_chunks {
                                                        abr_frames_dropped += 1;
                                                    }
                                                }
                                                partial_frames.push(PartialFrame {
                                                    frame_id,
                                                    codec_type: pkt_type,
                                                    total_chunks,
                                                    orig_width,
                                                    orig_height,
                                                    chunks: vec![None; total_chunks as usize],
                                                    received_count: 0,
                                                    created_at: Instant::now(),
                                                });
                                                partial_frames.len() - 1
                                            };

                                            let pf = &mut partial_frames[pos];
                                            if (chunk_idx as usize) < pf.chunks.len()
                                                && pf.chunks[chunk_idx as usize].is_none()
                                            {
                                                pf.chunks[chunk_idx as usize] = Some(payload.to_vec());
                                                pf.received_count += 1;
                                            }

                                            if pf.received_count == pf.total_chunks {
                                                let mut frame_data = Vec::with_capacity(
                                                    (pf.total_chunks as usize) * 1240,
                                                );
                                                for c in pf.chunks.iter().flatten() {
                                                    frame_data.extend_from_slice(c);
                                                }
                                                let orig_w = pf.orig_width as u32;
                                                let orig_h = pf.orig_height as u32;
                                                let codec_type = pf.codec_type;
                                                highest_decoded_frame = frame_id;
                                                partial_frames.retain(|f| f.frame_id > frame_id);
                                                abr_frames_completed += 1;
                                                received_any_frame = true;

                                                // Hand off frame buffer to dedicated decoder thread in <1µs
                                                let (lock, cvar) = &*decode_slot;
                                                if let Ok(mut slot) = lock.lock() {
                                                    *slot = Some((frame_id, orig_w, orig_h, frame_data, codec_type));
                                                    cvar.notify_one();
                                                }
                                            }
                                        }
                                    }
                                    PKT_CLIPBOARD => {
                                        if slice.len() > 5 {
                                            if let Ok(text) = std::str::from_utf8(&slice[5..]) {
                                                let hash = compute_clipboard_hash(text);
                                                LAST_SYNCED_CLIPBOARD_HASH.store(hash, Ordering::Relaxed);
                                                write_clipboard_text(text);
                                                info!("📋 Received remote clipboard text ({} chars)", text.chars().count());
                                            }
                                        }
                                    }
                                    PKT_FILE_START => {
                                        if slice.len() >= 15 {
                                            let size = u64::from_le_bytes(slice[5..13].try_into().unwrap());
                                            let name_len = u16::from_le_bytes(slice[13..15].try_into().unwrap()) as usize;
                                            if slice.len() >= 15 + name_len {
                                                if let Ok(name) = std::str::from_utf8(&slice[15..15 + name_len]) {
                                                    handle_incoming_file_start(size, name);
                                                }
                                            }
                                        }
                                    }
                                    PKT_FILE_CHUNK => {
                                        if slice.len() >= 9 {
                                            let chunk_idx = u32::from_le_bytes(slice[5..9].try_into().unwrap());
                                            handle_incoming_file_chunk(chunk_idx, &slice[9..]);
                                        }
                                    }
                                    PKT_FILE_FINISH => {
                                        if let Some(missing) = handle_incoming_file_finish() {
                                            let mut nack_pkt = Vec::with_capacity(5 + missing.len() * 4);
                                            nack_pkt.extend_from_slice(MAGIC_DSKY);
                                            nack_pkt.push(PKT_FILE_NACK);
                                            for idx in missing {
                                                nack_pkt.extend_from_slice(&idx.to_le_bytes());
                                            }
                                            if let Some(p_addr) = peer_addr {
                                                if p_addr != server_addr {
                                                    let _ = socket.send_to(&nack_pkt, p_addr).await;
                                                }
                                            }
                                            if !is_direct_peer || is_reconnecting {
                                                let relay = wrap_relay_packet(&norm_target, &nack_pkt);
                                                let _ = socket.send_to(&relay, server_addr).await;
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
                                    PKT_AUDIO_CHUNK => {
                                        if slice.len() >= 10 {
                                            #[cfg(target_os = "windows")]
                                            {
                                                let channels = slice[5] as u16;
                                                let sample_rate = u32::from_le_bytes(slice[6..10].try_into().unwrap());
                                                let pcm = &slice[10..];
                                                crate::windows::audio::play_remote_audio_chunk(sample_rate, channels, pcm);
                                            }
                                            #[cfg(target_os = "linux")]
                                            {
                                                let pcm = &slice[10..];
                                                crate::linux::audio::play_remote_audio_chunk(pcm);
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
                                    PKT_CURSOR_SHAPE => {
                                        if slice.len() >= 6 {
                                            crate::p2p::set_remote_cursor_shape(slice[5]);
                                        }
                                    }
                                    PKT_ADMIN_RESP => {
                                        if slice.len() >= 13 {
                                            let msg_id = u32::from_le_bytes(slice[5..9].try_into().unwrap());
                                            let chunk_idx = u16::from_le_bytes(slice[9..11].try_into().unwrap());
                                            let total_chunks = u16::from_le_bytes(slice[11..13].try_into().unwrap());
                                            if let Some(resp) = feed_admin_resp_chunk(msg_id, chunk_idx, total_chunks, &slice[13..]) {
                                                store_admin_response(resp);
                                                crate::platform::ui::request_main_redraw();
                                            }
                                        }
                                    }
                                    PKT_UNATTENDED_CHALLENGE => {
                                        if slice.len() > 5 {
                                            let salt = String::from_utf8_lossy(&slice[5..]).to_string();
                                            current_challenge_salt = Some(salt.clone());
                                            let saved_pwd = crate::identity::find_contact(&norm_target)
                                                .and_then(|c| c.saved_password);
                                            if let Some(pwd) = saved_pwd {
                                                let hash = crate::identity::compute_auth_hash(&salt, &pwd);
                                                let mut auth_pkt = Vec::with_capacity(5 + hash.len());
                                                auth_pkt.extend_from_slice(MAGIC_DSKY);
                                                auth_pkt.push(PKT_UNATTENDED_AUTH);
                                                auth_pkt.extend_from_slice(hash.as_bytes());
                                                if let Some(p_addr) = peer_addr {
                                                    if p_addr != server_addr {
                                                        let _ = socket.send_to(&auth_pkt, p_addr).await;
                                                    }
                                                }
                                                if !is_direct_peer || is_reconnecting {
                                                    let relay = wrap_relay_packet(&norm_target, &auth_pkt);
                                                    let _ = socket.send_to(&relay, server_addr).await;
                                                }
                                            } else {
                                                set_session_status(SessionStatus::PasswordRequired(display_target.clone()));
                                            }
                                        }
                                    }
                                    PKT_DISCONNECT => {
                                        info!("🔴 Remote host ended the session.");
                                        set_session_status(SessionStatus::Error("Remote session was disconnected by host.".to_string()));
                                        CLIENT_SESSION_ACTIVE.store(false, Ordering::Relaxed);
                                        crate::platform::ui::request_main_redraw();
                                    }
                                    PKT_CHAT_MESSAGE => {
                                        if slice.len() > 5 {
                                            if let Ok(text) = std::str::from_utf8(&slice[5..]) {
                                                crate::p2p::receive_chat_message("Host", text);
                                                crate::platform::ui::request_main_redraw();
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
                                            if !is_direct_peer && peer_addr.is_none() {
                                                peer_addr = Some(resolved);
                                            }

                                            if last_punched_peer != Some(resolved) {
                                                last_punched_peer = Some(resolved);
                                                info!(
                                                    "⚡ Peer '{}' resolved at {}. Starting Direct UDP Hole-Punching & Relay fallback...",
                                                    display_target, resolved
                                                );

                                                // Continuous hole-puncher probe task (skip if forced server relay)
                                                if !force_relay {
                                                    let sock_punch = Arc::clone(&socket);
                                                    let my_punch_id = my_id_padded.clone();
                                                    tokio::spawn(async move {
                                                        let punch = SignalMsg::Punch {
                                                            from_id: my_punch_id,
                                                        };
                                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                                            let base_ip = resolved.ip();
                                                            let base_port = resolved.port();
                                                            for _ in 0..35 {
                                                                let _ = sock_punch.send_to(&pbytes, resolved).await;
                                                                for offset in -6i32..=6i32 {
                                                                    let target_p = (base_port as i32 + offset) as u16;
                                                                    if target_p > 1024 {
                                                                        let _ = sock_punch
                                                                            .send_to(&pbytes, SocketAddr::new(base_ip, target_p))
                                                                            .await;
                                                                    }
                                                                }
                                                                tokio::time::sleep(Duration::from_millis(50)).await;
                                                            }
                                                        }
                                                    });
                                                }

                                                // Immediately fire a ClientHello so the host sees the request right away
                                                last_hello = Instant::now() - Duration::from_secs(10);
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
                                        crate::platform::ui::append_status_log(&format!("[x] {}", msg));
                                        break;
                                    }
                                    SignalMsg::Punch { .. } | SignalMsg::PunchAck => {
                                        if from_addr != server_addr && !force_relay {
                                            peer_addr = Some(from_addr);
                                            is_direct_peer = true;
                                            host_contacted = true;
                                            last_rx_from_peer = Instant::now();
                                            if is_reconnecting {
                                                is_reconnecting = false;
                                                info!("🔄 Direct P2P tunnel re-punched successfully with {}!", display_target);
                                                set_session_status(SessionStatus::Connected(display_target.clone()));
                                            }
                                            if !logged_direct_peer {
                                                logged_direct_peer = true;
                                                info!("🎉 Direct P2P tunnel established with {}!", from_addr);
                                            }
                                            let ack = SignalMsg::PunchAck;
                                            if let Ok(abytes) = serde_json::to_vec(&ack) {
                                                let _ = socket.send_to(&abytes, from_addr).await;
                                            }
                                            last_hello = Instant::now() - Duration::from_secs(10);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ = notify.notified() => {}
                    _ = clipboard_notify().notified() => {}
                    _ = file_notify().notified() => {}
                    _ = admin_notify().notified() => {}
                    _ = voice_notify().notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                }

                let (_, max_frame_age) = crate::p2p::get_buffer_reassembly_limits();
                partial_frames.retain(|pf| {
                    if pf.created_at.elapsed() > max_frame_age {
                        abr_frames_dropped += 1;
                        false
                    } else {
                        true
                    }
                });
            }

            let mut disc = Vec::with_capacity(5);
            disc.extend_from_slice(MAGIC_DSKY);
            disc.push(PKT_DISCONNECT);
            if let Some(p_addr) = peer_addr {
                let _ = socket.send_to(&disc, p_addr).await;
            }
            if !is_direct_peer {
                let relay_disc = wrap_relay_packet(&norm_target, &disc);
                let _ = socket.send_to(&relay_disc, server_addr).await;
            }
        });
    });
}
