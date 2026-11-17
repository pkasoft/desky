# Desky Server ⚡ (Matchmaker, Zero-Copy Relay & Commercial Web Dashboard)

> **High-Performance, Zero-Bandwidth P2P Signaling, Fallback Relay & Live Web Operations Dashboard for Desky**

`desky-server` is a standalone, ultra-lightweight Rust daemon designed to run on a cheap **$3–$5/month Linux VPS or Windows VDS**. It now features an enterprise-grade **Commercial Web Dashboard** and **Live Monitoring API** for IT administrators and commercial deployments.

---

## 🌟 Key Features

1. **Direct P2P Matchmaking (Zero-Bandwidth)**:
   - Server orchestrates STUN NAT hole-punching so peers stream directly P2P without touching server bandwidth.
2. **Instant Zero-Copy Relay Fallback**:
   - For symmetric NATs, enterprise firewalls, or strict university networks, seamlessly forwards UDP traffic with micro-buffering.
3. **Commercial Web Operations Dashboard**:
   - Built-in responsive HTML5 web interface (no Node.js/external dependencies).
   - Real-time active host count, total registered peers, brokered matches, relayed bandwidth, and server uptime.
   - Live searchable routing table with peer IDs, network endpoints, and last seen timestamps.
4. **Monitoring & Automation APIs**:
   - `/api/stats` - JSON telemetry endpoint for Datadog, Prometheus, Grafana, or internal dashboards.
   - `/api/peers` - Real-time active routing table and database peer list.
   - `/health` - Liveness probe endpoint for Kubernetes / Docker / load balancers.
5. **Enterprise Security**:
   - Optional `--admin-token` / `DESKY_ADMIN_TOKEN` authentication protecting dashboard and API endpoints.

---

## 🚀 Quick Start on your Server / VPS

### 1. Build

```bash
# Build standalone server binary
cargo build --release -p desky-server
```

*(Linux binary output: `target/release/desky-server`)*

---

### 2. Run with Web Dashboard

```bash
# Default: UDP 21116 for matchmaking, HTTP 21117 for web dashboard
./desky-server

# Custom ports with admin authentication
./desky-server --port 21116 --web-port 8080 --admin-token "MySecretCommercialToken123"
```

Open your browser at:
```
http://your-server-ip:21117/
```

If `--admin-token` is configured, visit:
```
http://your-server-ip:21117/?token=MySecretCommercialToken123
```

---

## ⚙️ Configuration & CLI Options

| Argument | Environment Variable | Default | Description |
| :--- | :--- | :--- | :--- |
| `--port, -p` | `DESKY_PORT` | `21116` | UDP port for P2P matchmaking and relaying |
| `--web-port, -w` | `DESKY_WEB_PORT` | `21117` | HTTP port for web dashboard and stats API (`0` to disable) |
| `--admin-token, -t` | `DESKY_ADMIN_TOKEN` | *(None)* | Secret token for dashboard & API authentication |
| `--no-web` | *(None)* | *(False)* | Disables the embedded HTTP web server |
| *(None)* | `DESKY_DB_PATH` | `desky.db` | Path to SQLite peer persistence database |

---

## 📊 Live Monitoring REST Endpoints

### 1. System Health Check
```http
GET /health
```
```json
{
  "status": "ok",
  "uptime_seconds": 128450,
  "version": "0.1.0"
}
```

### 2. Real-Time Telemetry Stats
```http
GET /api/stats
Authorization: Bearer <ADMIN_TOKEN>
```
```json
{
  "online_hosts": 14,
  "total_registered_peers": 482,
  "uptime_seconds": 86420,
  "relayed_packets": 248900,
  "relayed_bytes": 134217728,
  "matches_brokered": 192,
  "total_heartbeats": 51200,
  "total_registrations": 482,
  "udp_port": 21116,
  "web_port": 21117
}
```

### 3. Active Peer Directory
```http
GET /api/peers
Authorization: Bearer <ADMIN_TOKEN>
```
```json
[
  {
    "desky_id": "800258756",
    "formatted_id": "800 258 756",
    "online": true,
    "endpoint": "198.51.100.42:54210",
    "machine_id": "a1b2...f8e9",
    "created_at": 1731800000,
    "last_seen_secs_ago": 2
  }
]
```

---

## 🛡️ Production Deployment (Systemd on Linux)

Create `/etc/systemd/system/desky-server.service`:
```ini
[Unit]
Description=Desky P2P Matchmaker & Commercial Web Server
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/var/lib/desky
ExecStart=/usr/local/bin/desky-server --port 21116 --web-port 21117 --admin-token "your-secure-token"
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now desky-server
sudo systemctl status desky-server
```

Firewall rules:
```bash
# Allow UDP for P2P Matchmaking
sudo ufw allow 21116/udp
# Allow HTTP for Web Dashboard
sudo ufw allow 21117/tcp
```
