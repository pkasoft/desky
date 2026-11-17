# Desky 🖥️⚡

> **Zero-friction, high-performance peer-to-peer remote desktop built with Rust.**

Desky is an ultra-lightweight, high-framerate remote desktop application designed as a modern, private, and bloat-free alternative to legacy remote access software.

---

## ✨ Key Features

- **⚡ Blazing Speed & Low Latency:** Sub-10ms glass-to-glass latency utilizing native DirectX Desktop Duplication (DXGI) and GPU-accelerated video encoding (NVENC / AMF / AV1).
- **🔒 True End-to-End Encryption:** Direct peer-to-peer connections protected by ChaCha20-Poly1305 and modern cryptographic handshakes.
- **🌐 Zero-Config P2P:** Instant NAT hole-punching over WebRTC/QUIC—connect with a simple 6-character ephemeral session code or clickable magic link.
- **🪶 Ultra-Lightweight Single Executable:** Under 10 MB total binary size with zero external runtimes, drivers, or heavy background services.
- **🖥️ Virtual Monitor Support:** Extend your workspace by generating high-refresh virtual secondary monitors on the fly.
- **📋 Native Clipboard & File Drop:** Seamless copy-paste and high-speed direct file transfers.

---

## 🛠️ Architecture

```
                       [ Desky Host ]
                             │
            ┌────────────────┴────────────────┐
            ▼                                 ▼
   [ DirectX / DXGI ]                [ Audio Loopback ]
   (Frame Capture)                   (WASAPI Audio)
            │                                 │
            ▼                                 ▼
   [ Hardware Encoder ]              [ Opus Audio Codec ]
   (NVENC / AV1 / H.264)                      │
            │                                 │
            └────────────────┬────────────────┘
                             │
                             ▼
             [ WebRTC Data & Media Transport ]
               (Direct Encrypted P2P Tunnel)
                             │
                             ▼
                      [ Desky Client ]
               (Hardware Video Decode & Render)
```

---

## 🚀 Building from Source

### Prerequisites

- [Rust & Cargo](https://rustup.rs/) (v1.85+)
- Windows 10/11 (for host capture features)

### Build

```bash
git clone git@github.com:PraxiumX/desky.git
cd desky
cargo build --release
```

---

## 📄 License

Desky is distributed under a **Dual-Licensing Model**:
- **Personal & Home Use:** 100% Free under the **GNU Affero General Public License v3.0 (AGPLv3)**.
- **Commercial & Enterprise Use:** Commercial users and organizations must obtain a commercial license. See [LICENSE.md](LICENSE.md) or contact `contact@pkasoft.com` for details.
