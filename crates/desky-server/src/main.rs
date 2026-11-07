use anyhow::Result;
use rand::Rng;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Signaling & Matchmaking messages exchanged over UDP with the VDS Matchmaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalMsg {
    /// Host registering with its unique machine ID and optional cached Desky ID
    RegisterHost {
        machine_id: String,
        cached_id: Option<String>,
    },
    /// Server confirming host registration and assigning/reporting its permanent 9-digit Desky ID
    RegisterAck {
        desky_id: String,
        your_public_addr: String,
    },

    /// Client requesting connection to a Host by Desky ID (e.g. "842 195 204")
    ConnectRequest { target_id: String },
    /// Server telling Client: "Host is online, here is its public endpoint"
    PeerFound {
        target_id: String,
        peer_public_addr: String,
    },
    /// Server telling Host: "Client wants to connect, punch a hole to this address!"
    IncomingPeer { client_public_addr: String },
    /// Server notifying Client that the requested Desky ID is offline or not found
    PeerNotFound { target_id: String },

    /// Keep-alive heartbeat from Host to keep NAT table entry open
    Heartbeat { desky_id: String },
    HeartbeatAck,

    /// Direct P2P Hole Punching Packets (exchanged directly between peers)
    Punch { from_id: String },
    PunchAck,
}

struct HostEntry {
    addr: SocketAddr,
    last_seen: Instant,
}

/// Normalizes any input ID (stripping spaces, hyphens, and non-digits)
fn normalize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Formats a raw 9-digit string into AnyDesk-style "XXX XXX XXX"
fn format_id(raw_digits: &str) -> String {
    if raw_digits.len() == 9 {
        format!(
            "{} {} {}",
            &raw_digits[0..3],
            &raw_digits[3..6],
            &raw_digits[6..9]
        )
    } else {
        raw_digits.to_string()
    }
}

/// SQLite Database Manager for peer identity persistence
struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS peers (
                machine_id TEXT PRIMARY KEY,
                desky_id TEXT UNIQUE NOT NULL,
                created_at INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                last_ip TEXT,
                last_port INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_peers_desky_id ON peers (desky_id);
            ",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Resolves or assigns a permanent 9-digit Desky ID for a machine_id
    fn get_or_create_peer_id(
        &self,
        machine_id: &str,
        cached_id: Option<&str>,
        ip: &str,
        port: u16,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // 1. Check if this machine already has an assigned ID
        let mut stmt = conn.prepare("SELECT desky_id FROM peers WHERE machine_id = ?1")?;
        let mut rows = stmt.query(params![machine_id])?;
        if let Some(row) = rows.next()? {
            let desky_id: String = row.get(0)?;
            // Update last_seen and network endpoint
            let _ = conn.execute(
                "UPDATE peers SET last_seen = ?1, last_ip = ?2, last_port = ?3 WHERE machine_id = ?4",
                params![now, ip, port as i64, machine_id],
            );
            return Ok(desky_id);
        }

        // 2. Check if a cached_id was provided and is still unclaimed
        if let Some(cand) = cached_id {
            let norm_cand = normalize_id(cand);
            if norm_cand.len() == 9 {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM peers WHERE desky_id = ?1",
                    params![norm_cand],
                    |r| r.get(0),
                )?;
                if count == 0 {
                    conn.execute(
                        "INSERT INTO peers (machine_id, desky_id, created_at, last_seen, last_ip, last_port)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![machine_id, norm_cand, now, now, ip, port as i64],
                    )?;
                    return Ok(norm_cand);
                }
            }
        }

        // 3. Generate a new unique 9-digit ID (e.g. 100_000_000 .. 999_999_999)
        let mut rng = rand::thread_rng();
        loop {
            let num: u32 = rng.gen_range(100_000_000..=999_999_999);
            let candidate = num.to_string();

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM peers WHERE desky_id = ?1",
                params![candidate],
                |r| r.get(0),
            )?;
            if count == 0 {
                conn.execute(
                    "INSERT INTO peers (machine_id, desky_id, created_at, last_seen, last_ip, last_port)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![machine_id, candidate, now, now, ip, port as i64],
                )?;
                return Ok(candidate);
            }
        }
    }

    /// Updates heartbeat timestamp and public endpoint
    fn touch_peer(&self, desky_id: &str, ip: &str, port: u16) {
        let conn = self.conn.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let _ = conn.execute(
            "UPDATE peers SET last_seen = ?1, last_ip = ?2, last_port = ?3 WHERE desky_id = ?4",
            params![now, ip, port as i64, desky_id],
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "desky_server=info".into()),
        )
        .init();

    // Parse port from CLI argument or environment variable, default to 21116
    let mut port = 21116u16;
    let args: Vec<String> = std::env::args().collect();
    for i in 1..args.len() {
        if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse::<u16>() {
                port = p;
            }
        }
    }
    if let Ok(env_port) = std::env::var("DESKY_PORT") {
        if let Ok(p) = env_port.parse::<u16>() {
            port = p;
        }
    }

    let db_path = std::env::var("DESKY_DB_PATH").unwrap_or_else(|_| "desky.db".into());
    let db = Arc::new(Database::new(&db_path)?);

    let bind_addr = format!("0.0.0.0:{}", port);
    let socket = Arc::new(UdpSocket::bind(&bind_addr).await?);

    info!("========================================================");
    info!(" ⚡ DESKY P2P MATCHMAKER SERVER (AnyDesk ID & SQLite)");
    info!(" Listening on: UDP {}", bind_addr);
    info!(" Database: SQLite '{}'", db_path);
    info!(" Mode: Direct UDP Hole-Punching (STUN/ICE Rendezvous)");
    info!(" Bandwidth Cost: ~0 (All video & audio bypass this VDS)");
    info!(" Ready for peer matchmaking!");
    info!("========================================================");

    let hosts: Arc<RwLock<HashMap<String, HostEntry>>> = Arc::new(RwLock::new(HashMap::new()));

    // Background task: Purge expired host routing entries that missed heartbeats (> 60 seconds)
    let cleaner_hosts = Arc::clone(&hosts);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut table = cleaner_hosts.write().await;
            let now = Instant::now();
            let before_count = table.len();
            table.retain(|id, entry| {
                let keep = now.duration_since(entry.last_seen) < Duration::from_secs(60);
                if !keep {
                    info!("[PURGE] Host session expired: Desky ID {}", format_id(id));
                }
                keep
            });
            if table.len() != before_count {
                info!("[STATE] Active online hosts: {}", table.len());
            }
        }
    });

    let mut buf = vec![0u8; 4096];

    loop {
        let (len, src_addr) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(e) => {
                error!("Error receiving UDP packet: {:?}", e);
                continue;
            }
        };


        let msg: SignalMsg = match serde_json::from_slice(&buf[..len]) {
            Ok(m) => m,
            Err(_) => {
                // Ignore malformed or raw STUN probe packets
                continue;
            }
        };

        let socket_clone = Arc::clone(&socket);
        let hosts_clone = Arc::clone(&hosts);
        let db_clone = Arc::clone(&db);

        tokio::spawn(async move {
            match msg {
                SignalMsg::RegisterHost {
                    machine_id,
                    cached_id,
                } => {
                    let ip_str = src_addr.ip().to_string();
                    let port_u16 = src_addr.port();

                    // Query or assign permanent 9-digit Desky ID from SQLite
                    let desky_id = match db_clone.get_or_create_peer_id(
                        &machine_id,
                        cached_id.as_deref(),
                        &ip_str,
                        port_u16,
                    ) {
                        Ok(id) => id,
                        Err(e) => {
                            error!("Database error assigning Desky ID: {:?}", e);
                            return;
                        }
                    };

                    let formatted_id = format_id(&desky_id);
                    info!(
                        "[REGISTER] Peer approved! ID: '{}' (Machine: {}) from endpoint {}",
                        formatted_id, machine_id, src_addr
                    );

                    // Insert or update in-memory active routing table
                    {
                        let mut table = hosts_clone.write().await;
                        table.insert(
                            desky_id.clone(),
                            HostEntry {
                                addr: src_addr,
                                last_seen: Instant::now(),
                            },
                        );
                    }

                    // Acknowledge back with approved ID and public endpoint
                    let ack = SignalMsg::RegisterAck {
                        desky_id: formatted_id,
                        your_public_addr: src_addr.to_string(),
                    };
                    if let Ok(payload) = serde_json::to_vec(&ack) {
                        let _ = socket_clone.send_to(&payload, src_addr).await;
                    }
                }

                SignalMsg::Heartbeat { desky_id } => {
                    let norm_id = normalize_id(&desky_id);
                    let ip_str = src_addr.ip().to_string();
                    let port_u16 = src_addr.port();

                    db_clone.touch_peer(&norm_id, &ip_str, port_u16);

                    let mut table = hosts_clone.write().await;
                    if let Some(entry) = table.get_mut(&norm_id) {
                        entry.addr = src_addr;
                        entry.last_seen = Instant::now();
                    } else {
                        table.insert(
                            norm_id,
                            HostEntry {
                                addr: src_addr,
                                last_seen: Instant::now(),
                            },
                        );
                    }

                    let ack = SignalMsg::HeartbeatAck;
                    if let Ok(payload) = serde_json::to_vec(&ack) {
                        let _ = socket_clone.send_to(&payload, src_addr).await;
                    }
                }

                SignalMsg::ConnectRequest { target_id } => {
                    let norm_id = normalize_id(&target_id);
                    let display_id = format_id(&norm_id);

                    info!(
                        "[CONNECT-REQ] Client {} requesting connection to Desky ID '{}'",
                        src_addr, display_id
                    );

                    let host_target = {
                        let table = hosts_clone.read().await;
                        table.get(&norm_id).map(|e| e.addr)
                    };

                    match host_target {
                        Some(host_addr) => {
                            info!(
                                "⚡ [P2P MATCHMAKING] Matching Client {} <===> Host ID {} ({})",
                                src_addr, display_id, host_addr
                            );

                            // 1. Send Host's public endpoint to the Client
                            let to_client = SignalMsg::PeerFound {
                                target_id: display_id.clone(),
                                peer_public_addr: host_addr.to_string(),
                            };
                            if let Ok(payload) = serde_json::to_vec(&to_client) {
                                let _ = socket_clone.send_to(&payload, src_addr).await;
                            }

                            // 2. Send Client's public endpoint to the Host (Initiates hole punching!)
                            let to_host = SignalMsg::IncomingPeer {
                                client_public_addr: src_addr.to_string(),
                            };
                            if let Ok(payload) = serde_json::to_vec(&to_host) {
                                let _ = socket_clone.send_to(&payload, host_addr).await;
                            }

                            info!(
                                "[PUNCH-TRIGGERED] Hole-punch instructions sent to both peers for Desky ID '{}'",
                                display_id
                            );
                        }
                        None => {
                            warn!(
                                "[REJECT] Client {} requested offline/invalid Desky ID '{}'",
                                src_addr, display_id
                            );
                            let not_found = SignalMsg::PeerNotFound {
                                target_id: display_id,
                            };
                            if let Ok(payload) = serde_json::to_vec(&not_found) {
                                let _ = socket_clone.send_to(&payload, src_addr).await;
                            }
                        }
                    }
                }

                _ => {}
            }
        });
    }
}
