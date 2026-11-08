use crate::p2p::state::{
    decode_lan_endpoint, encode_lan_endpoint, fix_loopback_peer, format_id_display,
    host_socket_store, host_subscribers_store, normalize_id, pending_approval_store,
    wrap_relay_packet, AuthState, ClientSubscriber, PendingApprovalRequest, AUTH_APPROVED,
    AUTH_DECLINED, AUTH_PENDING, MAGIC_DSKY, PKT_AUTH_STATUS, PKT_CLIENT_HELLO, PKT_DISCONNECT,
    PKT_FRAME_CHUNK, PKT_INPUT_EVENT, PKT_LAN_DISCOVERY, PKT_PONG,
};
use crate::protocol::{InputEvent, SignalMsg};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::{error, info, warn};

#[cfg(target_os = "linux")]
fn trigger_linux_approval_dialog(client_id: String) {
    crate::linux::ui::spawn_linux_approval_popup(client_id);
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
                        let _ = s.set_broadcast(true);
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

                // LAN discovery responder on UDP 21117
                {
                    let socket_lan_main = Arc::clone(&socket);
                    let id_lan = Arc::clone(&current_id);
                    tokio::spawn(async move {
                        if let Ok(lan_sock) = UdpSocket::bind("0.0.0.0:21117").await {
                            let mut lbuf = vec![0u8; 512];
                            while let Ok((len, src)) = lan_sock.recv_from(&mut lbuf).await {
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

                // Dedicated Host Screen Capture & Direct P2P / Relay Streamer Thread
                {
                    let clients_stream = Arc::clone(&active_clients);
                    let running_stream = Arc::clone(&running);
                    let socket_stream = Arc::clone(&socket);
                    let target_server_stream = target_server;

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
                                let cap_res = crate::windows::capture::capture_screen_frame(
                                    cap_cursor, scale_pct,
                                );
                                #[cfg(target_os = "linux")]
                                let cap_res = crate::linux::capture::capture_screen_frame(
                                    cap_cursor, scale_pct,
                                );

                                if let Ok((orig_w, orig_h, cap_w, cap_h, bgra)) = cap_res {
                                    jpeg_buf.clear();
                                    let mut encoder =
                                        jpeg_encoder::Encoder::new(&mut jpeg_buf, jpeg_quality);
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
                                        let total_chunks =
                                            ((jpeg_buf.len() + chunk_size - 1) / chunk_size)
                                                .max(1);

                                        if total_chunks <= 2048 {
                                            for (idx, chunk) in
                                                jpeg_buf.chunks(chunk_size).enumerate()
                                            {
                                                let mut pkt =
                                                    Vec::with_capacity(17 + chunk.len());
                                                pkt.extend_from_slice(MAGIC_DSKY);
                                                pkt.push(PKT_FRAME_CHUNK);
                                                pkt.extend_from_slice(&frame_id.to_le_bytes());
                                                pkt.extend_from_slice(&(idx as u16).to_le_bytes());
                                                pkt.extend_from_slice(
                                                    &(total_chunks as u16).to_le_bytes(),
                                                );
                                                pkt.extend_from_slice(
                                                    &(orig_w as u16).to_le_bytes(),
                                                );
                                                pkt.extend_from_slice(
                                                    &(orig_h as u16).to_le_bytes(),
                                                );
                                                pkt.extend_from_slice(chunk);

                                                for sub in &subscribers {
                                                    if sub.is_direct {
                                                        let _ = socket_stream
                                                            .send_to(&pkt, sub.addr)
                                                            .await;
                                                    } else {
                                                        if let Some(l_addr) = sub.lan_addr {
                                                            let _ = socket_stream
                                                                .send_to(&pkt, l_addr)
                                                                .await;
                                                        }
                                                        if !sub.relay_id.is_empty() {
                                                            let relay_pkt = wrap_relay_packet(
                                                                &sub.relay_id,
                                                                &pkt,
                                                            );
                                                            let _ = socket_stream
                                                                .send_to(
                                                                    &relay_pkt,
                                                                    target_server_stream,
                                                                )
                                                                .await;
                                                        }
                                                    }
                                                }

                                                // Fast micro-pacing (140µs every 6 chunks = ~1ms total per frame)
                                                if (idx + 1) % 6 == 0 {
                                                    let t_pace = Instant::now();
                                                    while t_pace.elapsed()
                                                        < Duration::from_micros(140)
                                                    {
                                                        std::hint::spin_loop();
                                                    }
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

                    let host_lan_bytes = encode_lan_endpoint(&socket_recv, target_server);
                    let mut last_incoming_punch: Option<(SocketAddr, Instant)> = None;
                    let mut buf = vec![0u8; 8192];

                    while let Ok((len, from_addr)) = socket_recv.recv_from(&mut buf).await {
                        let slice = &buf[..len];

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

                                            let entry = map
                                                .entry(client_key.clone())
                                                .or_insert_with(|| {
                                                    info!(
                                                        "🔔 [HOST APPROVAL] Incoming connection request from {} ({})",
                                                        client_id_disp,
                                                        client_lan_addr.unwrap_or(from_addr)
                                                    );
                                                    need_ui_prompt = true;
                                                    should_punch_lan = true;
                                                    ClientSubscriber {
                                                        client_id: client_id_disp.clone(),
                                                        relay_id: relay_id.clone(),
                                                        addr: client_lan_addr.unwrap_or(from_addr),
                                                        lan_addr: client_lan_addr,
                                                        auth_state: AuthState::Pending,
                                                        last_seen: Instant::now(),
                                                        target_fps: fps,
                                                        capture_cursor: cur,
                                                        jpeg_quality: q_val,
                                                        scale_pct: sc_val,
                                                        chroma_444: c444,
                                                        is_direct: !is_from_server,
                                                    }
                                                });
                                            entry.client_id = client_id_disp.clone();
                                            if !relay_id.is_empty() {
                                                entry.relay_id = relay_id.clone();
                                            }
                                            if let Some(l_addr) = client_lan_addr {
                                                if entry.lan_addr != Some(l_addr) {
                                                    entry.lan_addr = Some(l_addr);
                                                    should_punch_lan = true;
                                                }
                                            }
                                            if !is_from_server {
                                                entry.is_direct = true;
                                                entry.addr = from_addr;
                                            }
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

                                    // If client advertised a local LAN address, punch directly to it on the LAN!
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

                                    // Reply with Pong if timestamp included
                                    if slice.len() >= 11 {
                                        let mut pong = Vec::with_capacity(9);
                                        pong.extend_from_slice(MAGIC_DSKY);
                                        pong.push(PKT_PONG);
                                        pong.extend_from_slice(&slice[7..11]);
                                        if !is_from_server {
                                            let _ = socket_recv.send_to(&pong, from_addr).await;
                                        } else if !relay_id.is_empty() {
                                            let relay_pong = wrap_relay_packet(&relay_id, &pong);
                                            let _ = socket_recv.send_to(&relay_pong, target_server).await;
                                        }
                                    }
                                }
                                PKT_INPUT_EVENT => {
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
                                                    }
                                                });
                                            if !entry.is_direct || entry.addr != from_addr {
                                                first_direct = true;
                                            }
                                            entry.client_id = client_id_disp.clone();
                                            entry.is_direct = true;
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

                                    if first_direct {
                                        info!(
                                            "🎉 [DIRECT P2P ESTABLISHED] Direct UDP link active with peer '{}' ({})!",
                                            client_id_disp, from_addr
                                        );
                                    }

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
                                            trigger_linux_approval_dialog(client_id_disp);
                                        }
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
                                            "🎉 [DIRECT P2P VERIFIED] Received punch acknowledgement from peer at {}!",
                                            from_addr
                                        );
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
