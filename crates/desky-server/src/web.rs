use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{format_id, Database, HostEntry};

#[derive(Default)]
pub struct ServerStats {
    pub start_time: Option<Instant>,
    pub relayed_packets: AtomicU64,
    pub relayed_bytes: AtomicU64,
    pub matches_brokered: AtomicU64,
    pub total_heartbeats: AtomicU64,
    pub total_registrations: AtomicU64,
}

impl ServerStats {
    pub fn new() -> Self {
        Self {
            start_time: Some(Instant::now()),
            relayed_packets: AtomicU64::new(0),
            relayed_bytes: AtomicU64::new(0),
            matches_brokered: AtomicU64::new(0),
            total_heartbeats: AtomicU64::new(0),
            total_registrations: AtomicU64::new(0),
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }
}

pub struct WebConfig {
    pub bind_ip: String,
    pub port: u16,
    pub admin_token: Option<String>,
    pub udp_port: u16,
    pub db_path: String,
}

pub async fn run_web_server(
    config: WebConfig,
    stats: Arc<ServerStats>,
    hosts: Arc<RwLock<HashMap<String, HostEntry>>>,
    db: Arc<Database>,
) -> anyhow::Result<()> {
    let addr = format!("{}:{}", config.bind_ip, config.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("🌐 Desky Web Dashboard & Stats API listening on: http://{}", addr);

    let config = Arc::new(config);

    loop {
        let (mut stream, _remote_addr) = match listener.accept().await {
            Ok(res) => res,
            Err(e) => {
                warn!("Web accept error: {:?}", e);
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };

        let stats_clone = Arc::clone(&stats);
        let hosts_clone = Arc::clone(&hosts);
        let db_clone = Arc::clone(&db);
        let cfg_clone = Arc::clone(&config);

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
            let mut lines = req_str.lines();
            let req_line = lines.next().unwrap_or_default();
            let mut parts = req_line.split_whitespace();
            let method = parts.next().unwrap_or("GET");
            let full_path = parts.next().unwrap_or("/");

            // Parse URL path and query string
            let (path, query) = match full_path.split_once('?') {
                Some((p, q)) => (p, q),
                None => (full_path, ""),
            };

            // Check authorization if admin_token is set
            let mut is_authorized = cfg_clone.admin_token.is_none();
            if let Some(ref token) = cfg_clone.admin_token {
                // Check query ?token=...
                for param in query.split('&') {
                    if let Some((k, v)) = param.split_once('=') {
                        if k == "token" && v == token {
                            is_authorized = true;
                            break;
                        }
                    }
                }
                // Check Authorization: Bearer ... or X-Admin-Token: ...
                if !is_authorized {
                    for line in req_str.lines() {
                        let lower = line.to_ascii_lowercase();
                        if lower.starts_with("authorization: bearer ") {
                            let header_token = line[21..].trim();
                            if header_token == token {
                                is_authorized = true;
                                break;
                            }
                        } else if lower.starts_with("x-admin-token: ") {
                            let header_token = line[15..].trim();
                            if header_token == token {
                                is_authorized = true;
                                break;
                            }
                        }
                    }
                }
            }

            if method == "OPTIONS" {
                let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, X-Admin-Token\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            match path {
                "/health" | "/status" => {
                    let uptime = stats_clone.uptime_secs();
                    let body = format!(r#"{{"status":"ok","uptime_seconds":{},"version":"0.1.0"}}"#, uptime);
                    send_json(&mut stream, 200, &body).await;
                }
                "/api/stats" => {
                    if !is_authorized {
                        send_json(&mut stream, 401, r#"{"error":"unauthorized","message":"Missing or invalid admin token"}"#).await;
                        return;
                    }
                    let online_hosts = {
                        let table = hosts_clone.read().await;
                        table.len()
                    };
                    let total_peers = db_clone.count_total_peers().unwrap_or(0);
                    let uptime = stats_clone.uptime_secs();
                    let relayed_pkts = stats_clone.relayed_packets.load(Ordering::Relaxed);
                    let relayed_bytes = stats_clone.relayed_bytes.load(Ordering::Relaxed);
                    let matches = stats_clone.matches_brokered.load(Ordering::Relaxed);
                    let heartbeats = stats_clone.total_heartbeats.load(Ordering::Relaxed);
                    let registrations = stats_clone.total_registrations.load(Ordering::Relaxed);

                    let json = format!(
                        r#"{{"online_hosts":{},"total_registered_peers":{},"uptime_seconds":{},"relayed_packets":{},"relayed_bytes":{},"matches_brokered":{},"total_heartbeats":{},"total_registrations":{},"udp_port":{},"web_port":{}}}"#,
                        online_hosts, total_peers, uptime, relayed_pkts, relayed_bytes, matches, heartbeats, registrations, cfg_clone.udp_port, cfg_clone.port
                    );
                    send_json(&mut stream, 200, &json).await;
                }
                "/api/peers" => {
                    if !is_authorized {
                        send_json(&mut stream, 401, r#"{"error":"unauthorized","message":"Missing or invalid admin token"}"#).await;
                        return;
                    }
                    let online_table = {
                        let table = hosts_clone.read().await;
                        table
                            .iter()
                            .map(|(k, v)| (k.clone(), v.addr.to_string()))
                            .collect::<HashMap<_, _>>()
                    };

                    let db_peers = db_clone.list_peers(200).unwrap_or_default();
                    let now_ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    let mut peer_items = Vec::new();
                    for p in db_peers {
                        let is_online = online_table.contains_key(&p.desky_id);
                        let endpoint = if is_online {
                            online_table.get(&p.desky_id).cloned().unwrap_or(format!("{}:{}", p.last_ip, p.last_port))
                        } else {
                            format!("{}:{}", p.last_ip, p.last_port)
                        };
                        let last_seen_ago = (now_ts - p.last_seen).max(0);
                        let masked_machine = if p.machine_id.len() > 10 {
                            format!("{}...{}", &p.machine_id[0..4], &p.machine_id[p.machine_id.len()-4..])
                        } else {
                            p.machine_id
                        };

                        peer_items.push(format!(
                            r#"{{"desky_id":"{}","formatted_id":"{}","online":{},"endpoint":"{}","machine_id":"{}","created_at":{},"last_seen_secs_ago":{}}}"#,
                            p.desky_id,
                            format_id(&p.desky_id),
                            is_online,
                            endpoint,
                            masked_machine,
                            p.created_at,
                            last_seen_ago
                        ));
                    }

                    let body = format!("[{}]", peer_items.join(","));
                    send_json(&mut stream, 200, &body).await;
                }
                "/" | "/index.html" => {
                    if !is_authorized {
                        let html = render_login_page();
                        send_html(&mut stream, 401, &html).await;
                        return;
                    }
                    let html = render_dashboard_html(&cfg_clone);
                    send_html(&mut stream, 200, &html).await;
                }
                _ => {
                    let not_found = r#"{"error":"not_found","message":"Endpoint does not exist"}"#;
                    send_json(&mut stream, 404, not_found).await;
                }
            }
        });
    }
}

async fn send_json(stream: &mut tokio::net::TcpStream, status_code: u16, json: &str) {
    let status_line = match status_code {
        200 => "200 OK",
        401 => "401 Unauthorized",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        status_line,
        json.len(),
        json
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}

async fn send_html(stream: &mut tokio::net::TcpStream, status_code: u16, html: &str) {
    let status_line = match status_code {
        200 => "200 OK",
        401 => "401 Unauthorized",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        html.len(),
        html
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}

fn render_login_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Desky Server &bull; Commercial Authentication</title>
  <style>
    :root {
      --bg: #0B0E14;
      --card-bg: #141A23;
      --border: #232D3F;
      --accent: #10B981;
      --text: #F3F4F6;
      --muted: #9CA3AF;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
    body { background-color: var(--bg); color: var(--text); display: flex; align-items: center; justify-content: center; min-height: 100vh; padding: 20px; }
    .card { background: var(--card-bg); border: 1px solid var(--border); border-radius: 12px; padding: 36px; width: 100%; max-width: 420px; box-shadow: 0 10px 25px rgba(0,0,0,0.5); text-align: center; }
    .logo { display: inline-flex; align-items: center; justify-content: center; width: 56px; height: 56px; background: rgba(16,185,129,0.12); border: 1px solid var(--accent); border-radius: 12px; margin-bottom: 20px; color: var(--accent); }
    h1 { font-size: 20px; font-weight: 700; margin-bottom: 8px; }
    p { font-size: 13px; color: var(--muted); margin-bottom: 24px; line-height: 1.5; }
    input { width: 100%; padding: 12px 14px; background: #0E121A; border: 1px solid var(--border); border-radius: 8px; color: #fff; font-size: 14px; margin-bottom: 16px; outline: none; }
    input:focus { border-color: var(--accent); }
    button { width: 100%; padding: 12px; background: var(--accent); color: #000; font-weight: 700; border: none; border-radius: 8px; cursor: pointer; transition: opacity 0.2s; font-size: 14px; }
    button:hover { opacity: 0.9; }
  </style>
</head>
<body>
  <div class="card">
    <div class="logo">
      <svg width="28" height="28" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
    </div>
    <h1>Desky Commercial Server</h1>
    <p>This matchmaking & relay node is protected by an admin access token.</p>
    <form onsubmit="event.preventDefault(); window.location.href = '/?token=' + encodeURIComponent(document.getElementById('token').value);">
      <input type="password" id="token" placeholder="Enter Admin Token" required autofocus>
      <button type="submit">Unlock Dashboard</button>
    </form>
  </div>
</body>
</html>"#
    .to_string()
}

fn render_dashboard_html(cfg: &WebConfig) -> String {
    let token_param = cfg
        .admin_token
        .as_ref()
        .map(|t| format!("?token={}", t))
        .unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Desky Commercial Server &bull; Control & Stats</title>
  <style>
    :root {{
      --bg: #0B0E14;
      --card-bg: #131822;
      --card-hover: #181F2C;
      --border: #20293A;
      --accent-green: #10B981;
      --accent-cyan: #38BDF8;
      --accent-amber: #F59E0B;
      --text-main: #F9FAFB;
      --text-muted: #9CA3AF;
      --offline-color: #6B7280;
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", sans-serif; }}
    body {{ background: var(--bg); color: var(--text-main); min-height: 100vh; padding: 24px; }}
    .container {{ max-width: 1200px; margin: 0 auto; }}

    /* Header */
    header {{ display: flex; align-items: center; justify-content: space-between; padding-bottom: 24px; border-bottom: 1px solid var(--border); margin-bottom: 24px; flex-wrap: wrap; gap: 16px; }}
    .brand {{ display: flex; align-items: center; gap: 14px; }}
    .brand-icon {{ width: 44px; height: 44px; background: rgba(16,185,129,0.15); border: 1.5px solid var(--accent-green); border-radius: 10px; display: flex; align-items: center; justify-content: center; color: var(--accent-green); }}
    .brand-title h1 {{ font-size: 20px; font-weight: 700; letter-spacing: -0.3px; }}
    .brand-title p {{ font-size: 13px; color: var(--text-muted); }}

    .header-actions {{ display: flex; align-items: center; gap: 12px; }}
    .status-badge {{ display: inline-flex; align-items: center; gap: 8px; padding: 6px 14px; border-radius: 20px; font-size: 13px; font-weight: 600; background: rgba(16,185,129,0.12); color: var(--accent-green); border: 1px solid rgba(16,185,129,0.3); }}
    .status-dot {{ width: 8px; height: 8px; border-radius: 50%; background: var(--accent-green); box-shadow: 0 0 10px var(--accent-green); animation: pulse 2s infinite; }}
    @keyframes pulse {{ 0% {{ opacity: 1; }} 50% {{ opacity: 0.4; }} 100% {{ opacity: 1; }} }}

    .btn {{ padding: 8px 14px; font-size: 13px; font-weight: 600; border-radius: 6px; border: 1px solid var(--border); background: var(--card-bg); color: var(--text-main); cursor: pointer; transition: all 0.2s; display: inline-flex; align-items: center; gap: 6px; text-decoration: none; }}
    .btn:hover {{ background: var(--card-hover); border-color: var(--accent-green); }}
    .btn-green {{ background: var(--accent-green); color: #000; border: none; }}
    .btn-green:hover {{ background: #059669; color: #fff; }}

    /* Stat Cards Grid */
    .grid-stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 16px; margin-bottom: 28px; }}
    .stat-card {{ background: var(--card-bg); border: 1px solid var(--border); border-radius: 12px; padding: 20px; transition: transform 0.2s, border-color 0.2s; position: relative; overflow: hidden; }}
    .stat-card:hover {{ border-color: rgba(16,185,129,0.4); transform: translateY(-2px); }}
    .stat-title {{ font-size: 12px; font-weight: 600; text-transform: uppercase; color: var(--text-muted); letter-spacing: 0.5px; margin-bottom: 8px; }}
    .stat-value {{ font-size: 26px; font-weight: 800; color: var(--text-main); }}
    .stat-sub {{ font-size: 12px; color: var(--text-muted); margin-top: 4px; }}
    .stat-accent-green {{ border-bottom: 3px solid var(--accent-green); }}
    .stat-accent-cyan {{ border-bottom: 3px solid var(--accent-cyan); }}
    .stat-accent-amber {{ border-bottom: 3px solid var(--accent-amber); }}

    /* Section Card */
    .content-card {{ background: var(--card-bg); border: 1px solid var(--border); border-radius: 12px; padding: 24px; margin-bottom: 24px; }}
    .card-header {{ display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px; flex-wrap: wrap; gap: 12px; }}
    .card-header h2 {{ font-size: 16px; font-weight: 700; }}
    .search-box {{ padding: 8px 14px; background: #0B0E14; border: 1px solid var(--border); border-radius: 8px; color: #fff; font-size: 13px; min-width: 260px; outline: none; }}
    .search-box:focus {{ border-color: var(--accent-green); }}

    /* Table */
    .table-container {{ overflow-x: auto; }}
    table {{ width: 100%; border-collapse: collapse; text-align: left; font-size: 13px; }}
    th {{ padding: 12px 14px; border-bottom: 1px solid var(--border); color: var(--text-muted); font-weight: 600; font-size: 12px; text-transform: uppercase; }}
    td {{ padding: 14px; border-bottom: 1px solid rgba(32,41,58,0.5); vertical-align: middle; }}
    tr:hover td {{ background: var(--card-hover); }}

    .badge-online {{ display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 12px; font-size: 11px; font-weight: 700; background: rgba(16,185,129,0.15); color: var(--accent-green); }}
    .badge-offline {{ display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 12px; font-size: 11px; font-weight: 700; background: rgba(107,114,128,0.15); color: var(--offline-color); }}
    .id-code {{ font-family: monospace; font-size: 14px; font-weight: 700; color: var(--accent-cyan); letter-spacing: 0.5px; }}
    .copy-btn {{ padding: 2px 6px; font-size: 11px; border-radius: 4px; border: 1px solid var(--border); background: #0B0E14; color: var(--text-muted); cursor: pointer; margin-left: 6px; }}
    .copy-btn:hover {{ color: #fff; border-color: var(--accent-green); }}

    /* Server Info footer */
    .server-info {{ display: flex; justify-content: space-between; align-items: center; font-size: 12px; color: var(--text-muted); padding-top: 16px; border-top: 1px solid var(--border); flex-wrap: wrap; gap: 12px; }}
    .server-info a {{ color: var(--accent-green); text-decoration: none; }}
  </style>
</head>
<body>
  <div class="container">
    <header>
      <div class="brand">
        <div class="brand-icon">
          <svg width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M12 5l7 7-7 7"/></svg>
        </div>
        <div class="brand-title">
          <h1>Desky Matchmaker & Relay Server</h1>
          <p>Enterprise Self-Hosted Infrastructure &bull; Real-time Operations Dashboard</p>
        </div>
      </div>
      <div class="header-actions">
        <div class="status-badge">
          <div class="status-dot"></div>
          <span>RUNNING</span>
        </div>
        <button class="btn" onclick="fetchData();">
          <svg width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
          Refresh
        </button>
      </div>
    </header>

    <!-- Top Metrics -->
    <div class="grid-stats">
      <div class="stat-card stat-accent-green">
        <div class="stat-title">Active Online Hosts</div>
        <div class="stat-value" id="val-online">0</div>
        <div class="stat-sub">Heartbeat active in memory</div>
      </div>
      <div class="stat-card stat-accent-cyan">
        <div class="stat-title">Registered Peers</div>
        <div class="stat-value" id="val-peers">0</div>
        <div class="stat-sub">Permanent SQLite records</div>
      </div>
      <div class="stat-card stat-accent-green">
        <div class="stat-title">Direct P2P Brokered</div>
        <div class="stat-value" id="val-matches">0</div>
        <div class="stat-sub">Hole-punch matches</div>
      </div>
      <div class="stat-card stat-accent-cyan">
        <div class="stat-title">Relayed Bandwidth</div>
        <div class="stat-value" id="val-bandwidth">0 MB</div>
        <div class="stat-sub" id="val-packets">0 packets relayed</div>
      </div>
      <div class="stat-card stat-accent-amber">
        <div class="stat-title">Server Uptime</div>
        <div class="stat-value" id="val-uptime">0s</div>
        <div class="stat-sub">Zero-downtime cluster</div>
      </div>
    </div>

    <!-- Peers Explorer -->
    <div class="content-card">
      <div class="card-header">
        <div>
          <h2>Peer Directory & Active Routing Table</h2>
          <p style="font-size: 13px; color: var(--text-muted); margin-top: 2px;">Real-time presence and network routing status for all connected Desky clients</p>
        </div>
        <input type="text" id="search-input" class="search-box" placeholder="Filter by Desky ID or Endpoint..." oninput="filterTable();">
      </div>

      <div class="table-container">
        <table>
          <thead>
            <tr>
              <th>Desky ID</th>
              <th>Status</th>
              <th>Network Endpoint</th>
              <th>Hardware ID</th>
              <th>Last Seen</th>
            </tr>
          </thead>
          <tbody id="peers-tbody">
            <tr>
              <td colspan="5" style="text-align: center; color: var(--text-muted); padding: 30px;">Loading live server metrics...</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- Server System Information -->
    <div class="server-info">
      <div>
        <strong>Desky Server v0.1.0</strong> &bull; UDP Matchmaker: <code>0.0.0.0:{}</code> &bull; Web Dashboard: <code>0.0.0.0:{}</code> &bull; DB: <code>{}</code>
      </div>
      <div>
        <a href="/api/stats{}" target="_blank">JSON Stats API</a> &bull;
        <a href="/api/peers{}" target="_blank">Peers API</a> &bull;
        <a href="/health" target="_blank">Health Check</a>
      </div>
    </div>
  </div>

  <script>
    const tokenQuery = "{}";
    let allPeers = [];

    function formatBytes(bytes) {{
      if (bytes === 0) return '0 B';
      const k = 1024;
      const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(bytes) / Math.log(k));
      return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }}

    function formatUptime(secs) {{
      const d = Math.floor(secs / (3600*24));
      const h = Math.floor(secs % (3600*24) / 3600);
      const m = Math.floor(secs % 3600 / 60);
      const s = Math.floor(secs % 60);
      if (d > 0) return d + "d " + h + "h " + m + "m";
      if (h > 0) return h + "h " + m + "m " + s + "s";
      return m + "m " + s + "s";
    }}

    function formatRelativeTime(seconds) {{
      if (seconds < 5) return 'Just now';
      if (seconds < 60) return seconds + 's ago';
      if (seconds < 3600) return Math.floor(seconds / 60) + 'm ago';
      if (seconds < 86400) return Math.floor(seconds / 3600) + 'h ago';
      return Math.floor(seconds / 86400) + 'd ago';
    }}

    async function fetchData() {{
      try {{
        const statsRes = await fetch('/api/stats' + tokenQuery);
        if (statsRes.ok) {{
          const s = await statsRes.json();
          document.getElementById('val-online').innerText = s.online_hosts.toLocaleString();
          document.getElementById('val-peers').innerText = s.total_registered_peers.toLocaleString();
          document.getElementById('val-matches').innerText = s.matches_brokered.toLocaleString();
          document.getElementById('val-bandwidth').innerText = formatBytes(s.relayed_bytes);
          document.getElementById('val-packets').innerText = s.relayed_packets.toLocaleString() + ' packets relayed';
          document.getElementById('val-uptime').innerText = formatUptime(s.uptime_seconds);
        }}

        const peersRes = await fetch('/api/peers' + tokenQuery);
        if (peersRes.ok) {{
          allPeers = await peersRes.json();
          renderTable(allPeers);
        }}
      }} catch (err) {{
        console.error("Error fetching stats:", err);
      }}
    }}

    function renderTable(peers) {{
      const tbody = document.getElementById('peers-tbody');
      if (peers.length === 0) {{
        tbody.innerHTML = '<tr><td colspan="5" style="text-align: center; color: var(--text-muted); padding: 30px;">No registered peers in database yet.</td></tr>';
        return;
      }}
      tbody.innerHTML = peers.map(p => `
        <tr>
          <td>
            <span class="id-code">${{p.formatted_id}}</span>
            <button class="copy-btn" onclick="navigator.clipboard.writeText('${{p.desky_id}}'); this.innerText='Copied!'; setTimeout(()=>this.innerText='Copy',1500);">Copy</button>
          </td>
          <td>
            ${{p.online ? '<span class="badge-online">&bull; ONLINE</span>' : '<span class="badge-offline">&bull; OFFLINE</span>'}}
          </td>
          <td><code>${{p.endpoint}}</code></td>
          <td><span style="color: var(--text-muted); font-size: 12px;">${{p.machine_id}}</span></td>
          <td>${{formatRelativeTime(p.last_seen_secs_ago)}}</td>
        </tr>
      `).join('');
    }}

    function filterTable() {{
      const query = document.getElementById('search-input').value.toLowerCase().trim();
      if (!query) {{
        renderTable(allPeers);
        return;
      }}
      const filtered = allPeers.filter(p => 
        p.desky_id.includes(query) ||
        p.formatted_id.toLowerCase().includes(query) ||
        p.endpoint.toLowerCase().includes(query) ||
        p.machine_id.toLowerCase().includes(query)
      );
      renderTable(filtered);
    }}

    fetchData();
    setInterval(fetchData, 3000);
  </script>
</body>
</html>"#,
        cfg.udp_port, cfg.port, cfg.db_path, token_param, token_param, token_param
    )
}
