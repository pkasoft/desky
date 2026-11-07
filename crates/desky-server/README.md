# Desky Matchmaker Server ⚡ (Zero-Relay P2P Rendezvous)

> **High-Performance, Zero-Bandwidth P2P Signaling & STUN Server for Desky**

`desky-server` is a standalone, ultra-lightweight Rust daemon designed to run on a cheap **$3–$5/month Linux VPS or Windows VDS**. 

### 💡 Why Zero-Relay Cuts Costs 100x–1,000x:
- **Traditional Remote Desktops:** All 60 FPS video & audio traffic routes through the server (costing hundreds of dollars in bandwidth).
- **Desky's Smart Matchmaker:** The server **NEVER** touches video or audio streams. It only exchanges public IP:Port endpoints (~1 KB JSON) and orchestrates **UDP Hole Punching** so both machines stream **directly peer-to-peer**.
- One single small VDS can handle **50,000+ simultaneous Desky sessions** with ~0% CPU and negligible bandwidth.

---

## 🚀 Quick Start on your VDS

### 1. Build for Linux (from your PC or on the VDS)

#### If compiling directly on Linux VDS:
```bash
# Clone and build
git clone https://github.com/your-repo/desky.git
cd desky
cargo build --release -p desky-server
```

#### If cross-compiling from Windows for Linux:
```powershell
rustup target add x86_64-unknown-linux-musl
cargo build --release -p desky-server --target x86_64-unknown-linux-musl
# Copy target/x86_64-unknown-linux-musl/release/desky-server to your VDS!
```

---

### 2. Run on your VDS

```bash
# Run on default port 21116 (UDP)
./desky-server --port 21116
```

#### Run as a Background Service (Systemd on Linux):
Create `/etc/systemd/system/desky-server.service`:
```ini
[Unit]
Description=Desky P2P Matchmaker Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/desky-server --port 21116
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start it:
```bash
sudo systemctl enable --now desky-server
sudo systemctl status desky-server
```

---

### 3. Open Firewall Port on VDS
Ensure UDP port `21116` is open:
```bash
# UFW (Ubuntu/Debian)
sudo ufw allow 21116/udp

# Firewalld (CentOS/AlmaLinux/RHEL)
sudo firewall-cmd --add-port=21116/udp --permanent
sudo firewall-cmd --reload
```

---

## 📡 Protocol Specification

| Message | Flow | Purpose |
| :--- | :--- | :--- |
| `RegisterHost` | Host ➔ VDS | Registers Host PIN and discovers Host's public NAT mapping |
| `RegisterAck` | VDS ➔ Host | Acknowledges registration and returns public endpoint |
| `ConnectRequest` | Client ➔ VDS | Client asks to connect to a specific PIN |
| `PeerFound` | VDS ➔ Client | Returns Host's public IP:Port to the Client |
| `IncomingPeer` | VDS ➔ Host | Informs Host to fire punch packets to Client's IP:Port |
| `Punch` | Peer ◄══► Peer | Direct UDP packets between peers establishing P2P tunnel |
