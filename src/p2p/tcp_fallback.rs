//! TCP Fallback Signaling & Keepalive Transport
//!
//! When outbound UDP (port 21116) is blocked by strict corporate/hotel firewalls,
//! Desky automatically falls back to length-prefixed TCP framing (port 21118 or
//! `DESKY_TCP_FALLBACK_ADDR`) to maintain online registration and heartbeats.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, warn};

use crate::p2p::state::{SERVER_ONLINE, SERVER_PING_MS};
use crate::protocol::SignalMsg;

pub async fn write_framed<W: AsyncWriteExt + Unpin>(writer: &mut W, payload: &[u8]) -> std::io::Result<()> {
    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await
}

pub async fn read_framed<R: AsyncReadExt + Unpin>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > 4 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid TCP frame length",
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

pub fn resolve_tcp_fallback_addr(udp_server: SocketAddr) -> SocketAddr {
    if let Ok(custom) = std::env::var("DESKY_TCP_FALLBACK_ADDR") {
        if let Ok(addr) = custom.parse::<SocketAddr>() {
            return addr;
        }
    }
    SocketAddr::new(udp_server.ip(), 21118)
}

pub fn spawn_tcp_fallback_monitor(
    udp_server: SocketAddr,
    machine_id: String,
    cached_id: Option<String>,
    running: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        // Give UDP 6 seconds first; only engage TCP fallback if UDP is blocked/offline
        tokio::time::sleep(Duration::from_secs(6)).await;

        let tcp_addr = resolve_tcp_fallback_addr(udp_server);
        let mut active_id = cached_id.clone().unwrap_or_default();

        while running.load(Ordering::Relaxed) {
            if SERVER_ONLINE.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(4)).await;
                continue;
            }

            warn!("⚠️ UDP signaling unreachable — attempting TCP fallback to {}...", tcp_addr);
            match tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(tcp_addr)).await {
                Ok(Ok(mut stream)) => {
                    info!("🔌 TCP fallback connected to {}", tcp_addr);
                    let reg = SignalMsg::RegisterHost {
                        machine_id: machine_id.clone(),
                        cached_id: if active_id.is_empty() { cached_id.clone() } else { Some(active_id.clone()) },
                    };
                    if let Ok(bytes) = serde_json::to_vec(&reg) {
                        if write_framed(&mut stream, &bytes).await.is_err() {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                    }

                    let mut last_hb = Instant::now();
                    while running.load(Ordering::Relaxed) {
                        tokio::select! {
                            res = tokio::time::timeout(Duration::from_secs(6), read_framed(&mut stream)) => {
                                match res {
                                    Ok(Ok(frame)) => {
                                        if let Ok(msg) = serde_json::from_slice::<SignalMsg>(&frame) {
                                            match msg {
                                                SignalMsg::RegisterAck { desky_id, .. } => {
                                                    SERVER_ONLINE.store(true, Ordering::Relaxed);
                                                    active_id = desky_id.clone();
                                                    crate::identity::save_assigned_desky_id(&desky_id);
                                                    crate::platform::set_assigned_desky_id(&desky_id);
                                                }
                                                SignalMsg::HeartbeatAck => {
                                                    SERVER_ONLINE.store(true, Ordering::Relaxed);
                                                    let rtt = (last_hb.elapsed().as_millis() as u32).clamp(1, 999);
                                                    SERVER_PING_MS.store(rtt, Ordering::Relaxed);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    _ => {
                                        SERVER_ONLINE.store(false, Ordering::Relaxed);
                                        break;
                                    }
                                }
                            }
                            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                                last_hb = Instant::now();
                                if !active_id.is_empty() {
                                    let hb = SignalMsg::Heartbeat { desky_id: active_id.clone() };
                                    if let Ok(bytes) = serde_json::to_vec(&hb) {
                                        if write_framed(&mut stream, &bytes).await.is_err() {
                                            SERVER_ONLINE.store(false, Ordering::Relaxed);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    tokio::time::sleep(Duration::from_secs(6)).await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_framed_roundtrip() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let msg = b"{\"HeartbeatAck\":null}";
        write_framed(&mut client, msg).await.unwrap();
        let got = read_framed(&mut server).await.unwrap();
        assert_eq!(got, msg);
    }
}
