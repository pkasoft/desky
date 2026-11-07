use crate::protocol::SignalMsg;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{error, info, warn};

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

    /// Spawns the background UDP rendezvous registration & keep-alive task
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

                // Initial registration with machine UUID and any previously cached Desky ID
                let reg_msg = SignalMsg::RegisterHost {
                    machine_id: machine_id.clone(),
                    cached_id: cached_id.clone(),
                };
                if let Ok(bytes) = serde_json::to_vec(&reg_msg) {
                    let _ = socket.send_to(&bytes, target_server).await;
                }

                let socket_recv = Arc::clone(&socket);
                let current_id = Arc::new(tokio::sync::RwLock::new(cached_id.unwrap_or_default()));
                let current_id_clone = Arc::clone(&current_id);

                // Receiver task: listens for VDS acks, peer introductions, and punch packets
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    while let Ok((len, from_addr)) = socket_recv.recv_from(&mut buf).await {
                        if let Ok(msg) = serde_json::from_slice::<SignalMsg>(&buf[..len]) {
                            match msg {
                                SignalMsg::RegisterAck {
                                    desky_id,
                                    your_public_addr,
                                } => {
                                    info!(
                                        "✅ [VDS APPROVED] Permanent Desky ID '{}' assigned! Public NAT endpoint: {}",
                                        desky_id, your_public_addr
                                    );
                                    // Save permanently in local identity.json
                                    crate::identity::save_assigned_desky_id(&desky_id);
                                    // Update UI display
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
                                    if let Ok(client_addr) = client_public_addr.parse::<SocketAddr>() {
                                        let my_id = current_id_clone.read().await.clone();
                                        let punch = SignalMsg::Punch { from_id: my_id };
                                        if let Ok(pbytes) = serde_json::to_vec(&punch) {
                                            for _ in 0..5 {
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
                    }
                }
            });
        });
    }

    /// Connects to a remote peer via the VDS Matchmaker
    pub async fn connect_peer(server_addr: SocketAddr, target_id: &str) -> anyhow::Result<SocketAddr> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let req = SignalMsg::ConnectRequest {
            target_id: target_id.to_string(),
        };
        let bytes = serde_json::to_vec(&req)?;
        socket.send_to(&bytes, server_addr).await?;

        let mut buf = vec![0u8; 4096];
        let (len, _) = tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut buf)).await??;

        let resp: SignalMsg = serde_json::from_slice(&buf[..len])?;
        match resp {
            SignalMsg::PeerFound { target_id, peer_public_addr } => {
                let peer_addr: SocketAddr = peer_public_addr.parse()?;
                info!("⚡ Peer '{}' found at {}. Initiating direct UDP hole punch...", target_id, peer_addr);
                let punch = SignalMsg::Punch {
                    from_id: "CLIENT".to_string(),
                };
                let pbytes = serde_json::to_vec(&punch)?;
                for _ in 0..5 {
                    let _ = socket.send_to(&pbytes, peer_addr).await;
                    tokio::time::sleep(Duration::from_millis(15)).await;
                }
                Ok(peer_addr)
            }
            SignalMsg::PeerNotFound { target_id } => {
                anyhow::bail!("Desky ID '{}' not found or host is offline", target_id)
            }
            _ => anyhow::bail!("Unexpected response from matchmaking server"),
        }
    }
}
