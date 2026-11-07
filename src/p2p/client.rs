use crate::p2p::state::{
    apply_preset_for_mode, decode_jpeg_to_bgra, fix_loopback_peer, frame_store,
    get_active_quality_params, input_notify, input_store, normalize_id, resolve_server_addr,
    set_session_status, update_client_preferences, SessionStatus, ACTIVE_CHROMA_444,
    ACTIVE_JPEG_QUALITY, ACTIVE_SCALE_PCT, AUTH_APPROVED, AUTH_DECLINED, AUTH_PENDING,
    CLIENT_CAPTURE_CURSOR, CLIENT_SESSION_ACTIVE, CLIENT_SESSION_GEN, CLIENT_TARGET_FPS,
    MAGIC_DSKY, PKT_AUTH_STATUS, PKT_CLIENT_HELLO, PKT_DISCONNECT, PKT_FRAME_CHUNK,
    PKT_INPUT_EVENT, PKT_LAN_DISCOVERY, PKT_PONG, QUALITY_MODE, SESSION_BITRATE_KBPS,
    SESSION_RTT_MS,
};
use crate::protocol::{InputEvent, SignalMsg};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::{error, info, warn};

struct PartialFrame {
    frame_id: u32,
    total_chunks: u16,
    orig_width: u16,
    orig_height: u16,
    chunks: Vec<Option<Vec<u8>>>,
    received_count: u16,
    created_at: Instant,
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
                                let (g, _) =
                                    match cvar.wait_timeout(guard, Duration::from_millis(50)) {
                                        Ok(res) => res,
                                        Err(_) => break,
                                    };
                                guard = g;
                            }
                            guard.take()
                        };

                        if let Some((f_id, orig_w, orig_h, jpeg_bytes)) = job {
                            if let Some(decoded) =
                                decode_jpeg_to_bgra(f_id, orig_w, orig_h, &jpeg_bytes)
                            {
                                if !logged_first {
                                    logged_first = true;
                                    set_session_status(SessionStatus::Connected(
                                        disp_worker.clone(),
                                    ));
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
                if !host_contacted
                    && last_matchmaker_retry.elapsed() >= Duration::from_millis(1200)
                {
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
                            .send_to(
                                &lan_pkt,
                                "255.255.255.255:21117".parse::<SocketAddr>().unwrap(),
                            )
                            .await;
                        let _ = socket
                            .send_to(
                                &lan_pkt,
                                "127.0.0.1:21117".parse::<SocketAddr>().unwrap(),
                            )
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
                        let cur = if CLIENT_CAPTURE_CURSOR.load(Ordering::Relaxed) {
                            1u8
                        } else {
                            0u8
                        };
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
                                    let _ = socket
                                        .send_to(&hello, SocketAddr::new(base_ip, p))
                                        .await;
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
