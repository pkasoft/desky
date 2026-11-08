//! Zero-Config LAN Radar Engine
//!
//! Automatically discovers nearby Desky instances across the local Wi-Fi / LAN subnet
//! via UDP broadcast beacons, enabling zero-cloud 1-click direct local connections.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::{debug, info};

pub const MAGIC_DSKY: &[u8; 4] = b"DSKY";
pub const PKT_LAN_RADAR_BEACON: u8 = 0x07;
pub const PKT_LAN_RADAR_PING: u8 = 0x08;
pub const LAN_RADAR_PORT: u16 = 21118;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanPeer {
    pub id: String,
    pub hostname: String,
    pub user: String,
    pub os: String,
    pub port: u16,
    #[serde(skip)]
    pub ip: Option<IpAddr>,
    #[serde(skip)]
    pub last_seen: Option<Instant>,
    #[serde(default)]
    pub latency_ms: u32,
}

static LAN_PEERS: Mutex<Vec<LanPeer>> = Mutex::new(Vec::new());
static RADAR_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Retrieves a cloned list of currently active LAN peers (pruned within 8 seconds)
pub fn get_lan_peers() -> Vec<LanPeer> {
    if let Ok(mut lock) = LAN_PEERS.lock() {
        let now = Instant::now();
        lock.retain(|p| {
            if let Some(seen) = p.last_seen {
                now.duration_since(seen) < Duration::from_secs(8)
            } else {
                false
            }
        });
        lock.clone()
    } else {
        Vec::new()
    }
}

/// Triggers a broadcast ping to elicit immediate responses from nearby Desks
pub fn trigger_lan_ping() {
    let mut pkt = Vec::with_capacity(5);
    pkt.extend_from_slice(MAGIC_DSKY);
    pkt.push(PKT_LAN_RADAR_PING);

    std::thread::spawn(move || {
        let sock = std::net::UdpSocket::bind("0.0.0.0:0");
        if let Ok(s) = sock {
            let _ = s.set_broadcast(true);
            let bcast_target = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), LAN_RADAR_PORT);
            let _ = s.send_to(&pkt, bcast_target);
        }
    });
}

/// Starts the background LAN Radar broadcaster and receiver task
pub fn start_lan_radar(local_id_getter: impl Fn() -> String + Send + Sync + 'static) {
    if RADAR_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }

    let id_getter = Arc::new(local_id_getter);

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to initialize Tokio runtime for LAN Radar: {}", e);
                RADAR_ACTIVE.store(false, Ordering::Relaxed);
                return;
            }
        };

        rt.block_on(async move {
            let socket = match UdpSocket::bind(format!("0.0.0.0:{}", LAN_RADAR_PORT)).await {
                Ok(s) => {
                    let _ = s.set_broadcast(true);
                    Arc::new(s)
                }
                Err(e) => {
                    // If 21118 is occupied, bind any ephemeral port and operate in client-receive mode
                    debug!("LAN Radar port {} in use, binding ephemeral: {}", LAN_RADAR_PORT, e);
                    match UdpSocket::bind("0.0.0.0:0").await {
                        Ok(s) => {
                            let _ = s.set_broadcast(true);
                            Arc::new(s)
                        }
                        Err(err) => {
                            tracing::warn!("Failed to bind LAN Radar socket: {}", err);
                            RADAR_ACTIVE.store(false, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            };

            let hostname = std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "Desktop".into());
            let user = std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default();
            let os = if cfg!(target_os = "windows") {
                "Windows"
            } else {
                "Linux"
            };

            info!("Zero-Config LAN Radar active on broadcast port {}", LAN_RADAR_PORT);

            // Task 1: Periodic Beacon Broadcaster
            let sock_tx = Arc::clone(&socket);
            let id_tx = Arc::clone(&id_getter);
            let host_tx = hostname.clone();
            let user_tx = user.clone();
            tokio::spawn(async move {
                let bcast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), LAN_RADAR_PORT);
                loop {
                    let current_id = id_tx();
                    if !current_id.trim().is_empty() {
                        let beacon = LanPeer {
                            id: current_id,
                            hostname: host_tx.clone(),
                            user: user_tx.clone(),
                            os: os.to_string(),
                            port: LAN_RADAR_PORT,
                            ip: None,
                            last_seen: None,
                            latency_ms: 1,
                        };

                        if let Ok(json_bytes) = serde_json::to_vec(&beacon) {
                            let mut pkt = Vec::with_capacity(5 + json_bytes.len());
                            pkt.extend_from_slice(MAGIC_DSKY);
                            pkt.push(PKT_LAN_RADAR_BEACON);
                            pkt.extend_from_slice(&json_bytes);

                            let _ = sock_tx.send_to(&pkt, bcast_addr).await;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(2500)).await;
                }
            });

            // Task 2: Radar Receiver & Subnet Listener
            let mut buf = [0u8; 2048];
            loop {
                let (len, remote_addr) = match socket.recv_from(&mut buf).await {
                    Ok(v) => v,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                };

                if len < 5 || &buf[0..4] != MAGIC_DSKY {
                    continue;
                }

                let pkt_type = buf[4];
                let my_id = id_getter();

                if pkt_type == PKT_LAN_RADAR_PING {
                    // Immediate response to ping
                    if !my_id.trim().is_empty() {
                        let reply = LanPeer {
                            id: my_id.clone(),
                            hostname: hostname.clone(),
                            user: user.clone(),
                            os: os.to_string(),
                            port: LAN_RADAR_PORT,
                            ip: None,
                            last_seen: None,
                            latency_ms: 1,
                        };
                        if let Ok(json_bytes) = serde_json::to_vec(&reply) {
                            let mut pkt = Vec::with_capacity(5 + json_bytes.len());
                            pkt.extend_from_slice(MAGIC_DSKY);
                            pkt.push(PKT_LAN_RADAR_BEACON);
                            pkt.extend_from_slice(&json_bytes);
                            let _ = socket.send_to(&pkt, remote_addr).await;
                        }
                    }
                } else if pkt_type == PKT_LAN_RADAR_BEACON {
                    if let Ok(mut peer) = serde_json::from_slice::<LanPeer>(&buf[5..len]) {
                        // Ignore our own broadcast
                        let norm_my = my_id.replace(' ', "");
                        let norm_peer = peer.id.replace(' ', "");
                        if !norm_my.is_empty() && norm_my == norm_peer {
                            continue;
                        }

                        peer.ip = Some(remote_addr.ip());
                        peer.last_seen = Some(Instant::now());

                        let mut updated = false;
                        if let Ok(mut lock) = LAN_PEERS.lock() {
                            if let Some(existing) = lock.iter_mut().find(|p| p.id.replace(' ', "") == norm_peer) {
                                existing.hostname = peer.hostname;
                                existing.user = peer.user;
                                existing.os = peer.os;
                                existing.ip = peer.ip;
                                existing.last_seen = peer.last_seen;
                                updated = true;
                            } else {
                                lock.push(peer);
                                updated = true;
                            }
                        }

                        if updated {
                            #[cfg(target_os = "windows")]
                            crate::windows::ui::request_main_redraw();
                        }
                    }
                }
            }
        });
    });
}
