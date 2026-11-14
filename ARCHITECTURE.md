# Desky Architecture & Codebase Index

> Comprehensive technical map and navigation index for human engineers and AI coding agents.

---

## 1. Directory Tree & Module Responsibilities

```
desky/
├── Cargo.toml                     # Dependencies, Windows APIs, and release optimization profile
├── ARCHITECTURE.md                # System architectural map & navigation index
├── README.md                      # Project introduction and user guide
└── src/
    ├── main.rs                    # Application entrypoint & CLI argument dispatcher
    ├── codec/                     # Unified Cross-Platform Codec Abstractions & Software Fallbacks
    │   ├── mod.rs                 # VideoEncoder, VideoDecoder, EncodedFrame, CodecType traits
    │   └── mjpeg.rs               # Pure-Rust Motion-JPEG encoder/decoder (4:4:4 / 4:2:0 subsampling)
    ├── identity/                  # Machine hardware identity & permanent 9-digit Desky ID
    │   └── mod.rs                 # HWID extraction, persistent caching in %APPDATA%\Desky
    ├── media/                     # Session Recording & Container Muxing
    │   ├── mod.rs                 # Media module exports
    │   └── recorder.rs            # Zero-overhead cross-platform RIFF AVI muxer (pure Rust)
    ├── p2p/                       # Peer-to-Peer Networking, Streaming & State
    │   ├── mod.rs                 # Public P2P facade
    │   ├── client.rs              # Client session runner, frame reassembly & ABR
    │   ├── host.rs                # Host session runner, screen capture loop & GPU/CPU encoding
    │   ├── radar.rs               # LAN UDP discovery & radar scanner
    │   ├── voice.rs               # Real-time intercom audio queue & notify synchronization
    │   ├── file_transfer.rs       # Chunked file streaming, ARQ NACK retransmission & HUD progress
    │   ├── admin_rpc.rs           # Chunked Silent Admin RPC request/response packet store
    │   ├── clipboard.rs           # Win32/X11 clipboard sync & FNV-1a hash deduplication
    │   └── state.rs               # Global P2P session subscribers, active sockets & metrics
    ├── protocol/                  # Binary Wire Protocol & Serialization
    │   ├── mod.rs                 # Re-exports for protocol structs, enums & opcodes
    │   ├── opcodes.rs             # Single source of truth for all MAGIC and PKT_* constants
    │   └── messages.rs            # SignalMsg, InputEvent, AdminReq, AdminResp, SystemAction
    ├── ui/                        # Unified UI Framework & Lucide Vector Icons Suite
    │   ├── mod.rs                 # High-level icon exports
    │   └── icons/                 # 26 anti-aliased GDI+ subpixel Lucide vector icons
    │       ├── mod.rs             # GDI+ / GDI fallback vector engine, IconContext, helpers
    │       ├── zap.rs             # Quick Actions icon (zap.svg)
    │       ├── pen.rs             # Whiteboard Pen icon (pencil.svg)
    │       ├── record.rs          # Session Recording icon (video.svg)
    │       ├── monitor.rs         # Display housing & numbered monitors 1, 2, 3
    │       ├── volume.rs          # Audio speaker & mute (volume-2.svg, volume-x.svg)
    │       ├── mic.rs             # Two-way intercom mic (mic.svg, mic-off.svg)
    │       ├── lock.rs            # Privacy curtain icon (lock.svg)
    │       ├── terminal.rs        # Admin terminal prompt (terminal.svg)
    │       ├── settings.rs        # Cog profile (settings.svg)
    │       ├── radio.rs           # Signal beacon (radio.svg)
    │       └── [bell, check, copy, cpu, file, folder, home, info, plus, refresh, search, server, trash, wifi, x].rs
    ├── windows/                   # Windows OS Platform Layer
    │   ├── mod.rs                 # Windows subsystem re-exports
    │   ├── os_version.rs          # NT kernel version & edition detection (Win 7/8/10/11 & Server 2012-2025)
    │   ├── admin.rs               # Silent Admin backend (processes, services, shell, SAS, reboot)
    │   ├── audio.rs               # WASAPI loopback audio capture & technician intercom
    │   ├── capture.rs             # Screen capture dispatch (DXGI Desktop Duplication / GDI)
    │   ├── clipboard.rs           # Win32 clipboard text & CF_HDROP Explorer file sync
    │   ├── codec/                 # Windows Media Foundation (MFT) Hardware Codecs
    │   │   ├── mod.rs             # Windows hardware codec probes & dispatch
    │   │   ├── h264_mf.rs         # Direct3D/MF GPU H.264 Transform encoder/decoder
    │   │   └── h265_mf.rs         # HEVC / H.265 hardware capability detector
    │   ├── cursor.rs              # Hardware cursor capture & shape translation
    │   ├── dxgi_capture.rs        # High-performance Direct3D11 / DXGI Desktop Duplication
    │   ├── input.rs               # Win32 SendInput / mouse_event / keybd_event injector
    │   ├── system.rs              # Working set trimming, directories, notifications
    │   └── ui/                    # Win32 Native UI & Multi-Window Canvas
    │       ├── mod.rs             # UI module entry
    │       ├── theme.rs           # PKASOFT Emerald design tokens, fonts, rounded boxes
    │       ├── launcher.rs        # Main Dashboard, ID generator, LAN radar, status log
    │       ├── remote_desk_edit.rs # Dedicated Remote Desk Edit control (Ctrl+C, Ctrl+V, Enter to connect)
    │       ├── admin.rs           # Remote Admin Terminal UI (processes, services, console)
    │       ├── progress.rs        # Standalone file transfer progress window
    │       └── viewport/          # Modular Multi-Tab Remote Session Canvas Window
    │           ├── mod.rs         # Module exports & component declarations
    │           ├── window.rs      # Native Win32 window procedure & message lifecycle
    │           ├── tabs.rs        # Tabbed session manager & window detaching
    │           ├── renderer.rs    # Double-buffered 60-120 FPS video frame canvas renderer
    │           ├── toolbar.rs     # Top tab bar, Lucide toolbar buttons, tooltips
    │           ├── quick_actions.rs # Quick Actions hotkeys menu (Ctrl+Alt+Del, Win+L...)
    │           ├── whiteboard.rs  # In-session Whiteboard canvas & annotation overlay
    │           ├── settings_modal.rs # Fullscreen settings & stream diagnostics page
    │           ├── file_hud.rs    # In-viewport file transfer HUD overlay
    │           └── input_injector.rs # Coordinate scaling & remote mouse/key injection
    └── linux/                     # Linux Headless Platform Layer
        ├── mod.rs
        ├── admin.rs
        ├── capture.rs
        ├── clipboard.rs
        ├── codec/                 # Linux Hardware Video Codecs (VA-API / V4L2)
        │   ├── mod.rs             # Linux hardware codec probes & unified platform dispatch
        │   ├── h264_vaapi.rs      # VA-API GPU H.264 Transform encoder/decoder (Intel/AMD)
        │   ├── h265_vaapi.rs      # HEVC / H.265 hardware capability detector
        │   └── v4l2.rs            # Video4Linux2 M2M hardware device detector
        ├── input.rs
        ├── system.rs
        └── ui.rs
```

---

## 2. Wire Protocol & Packet Types

Packets use the 4-byte magic header `DSKY` followed by a 1-byte packet opcode:

| Opcode | Constant | Description |
|---|---|---|
| `0x01` | `PKT_CLIENT_HELLO` | Client session initialization & quality negotiation |
| `0x02` | `PKT_FRAME_CHUNK` | Video stream packet (Frame ID, Chunk Index, Total Chunks, JPEG Payload) |
| `0x03` | `PKT_INPUT_EVENT` | Mouse and keyboard events sent from Client to Host |
| `0x04` | `PKT_DISCONNECT` | Graceful peer disconnection |
| `0x05` | `PKT_LAN_DISCOVERY` | Local network radar beacon broadcast |
| `0x06` | `PKT_PONG` | Heartbeat & ping latency measurement |
| `0x07` | `PKT_AUTH_STATUS` | Host approval status (Pending, Approved, Declined) |
| `0x08` | `PKT_RELAY` | Encapsulated packet routed through VDS relay |
| `0x09` | `PKT_CLIPBOARD` | UTF-8 clipboard text synchronization |
| `0x0A` | `PKT_FILE_START` | File transfer header (Total Size, Name Length, Filename) |
| `0x0B` | `PKT_FILE_CHUNK` | File transfer data payload chunk |
| `0x0C` | `PKT_FILE_FINISH` | File transfer completion notification |
| `0x0D` | `PKT_ADMIN_REQ` | Silent admin request (Processes, Services, Shell, Actions) |
| `0x0E` | `PKT_ADMIN_RESP` | Silent admin response payload |
| `0x0F` | `PKT_AUDIO_CHUNK` | High-fidelity Opus/PCM remote audio and voice intercom |
| `0x10` | `PKT_CURSOR_SHAPE` | Remote host cursor shape synchronization |
| `0x11` | `PKT_FILE_NACK` | Selective ARQ chunk retransmission request |
| `0x12` | `PKT_VOICE_CHUNK` | Technician-to-host microphone talkback audio |
| `0x16` | `PKT_FRAME_CHUNK_H264` | GPU-accelerated H.264 video stream packet |
| `0x17` | `PKT_FRAME_CHUNK_H265` | GPU-accelerated H.265 / HEVC video stream packet |

---

## 3. Viewport Hit-Test & Action ID Map

Used by `get_tab_hit_state()` and window event routing in `src/windows/ui/viewport/`:

| Hit ID | Element | Action / Handler |
|---|---|---|
| `0` | **Home Tab** | Switches to Dashboard view (`launcher.rs`) |
| `1..=99` | **Session Tab** | Switches active remote session canvas |
| `999` | **New Tab `+`** | Opens Home Tab to initiate a new connection |
| `1000..=1099` | **Close Tab `×`** | Terminates remote session and removes tab |
| `9001` | **Desktop View** | Switches to live screen desktop stream |
| `9002` | **Admin Terminal** | Switches to Silent Admin manager (Process/Service/Shell) |
| `9003` | **Audio Mute/Unmute** | Toggles remote speaker audio capture |
| `9004` | **Privacy Screen** | Toggles Host Curtain Mode (screen blackout + click-through) |
| `9005` | **Voice Intercom** | Toggles technician-to-host microphone talkback |
| `9006` | **Settings** | Opens Full-Screen Settings & Diagnostics page |
| `9007` | **Quick Actions** | Opens Quick Actions hotkeys dropdown (`QUICK_ACTIONS`) |
| `9008` | **Whiteboard Pen** | Toggles interactive screen annotation drawing mode |
| `9009` | **Record Session** | Starts / stops zero-overhead AVI Motion-JPEG session recording |
| `9010` | **In-Session Chat** | Toggles technician-host floating chat flyout |
| `9011` | **Video Codec Badge** | Displays active stream codec (`H.264 GPU`, `H.265 GPU`, `MJPEG 4:4:4`); click opens Connection Info |
| `9101` | **Monitor 1** | Switches captured host display to Monitor 1 |
| `9102` | **Monitor 2** | Switches captured host display to Monitor 2 |
| `9103` | **Monitor 3** | Switches captured host display to Monitor 3 |
| `9201..=9207` | **Quick Action Items** | `Ctrl+Alt+Del`, `Win+L`, `Task Manager`, `Win+D`, `Win+R`, `Reboot` |
| `9301..=9304` | **Whiteboard Colors** | Emerald Green, Crimson Red, Golden Yellow, Cyan |
| `9399` | **Whiteboard Clear** | Wipes all active whiteboard annotations |

---

## 4. Key Subsystem Implementations

### A. Seamless Explorer Clipboard File Copy-Paste (`Ctrl+C` ➔ `Ctrl+V`)
1. **Detection**: `src/windows/clipboard.rs` queries `CF_HDROP` via `read_clipboard_files()`.
2. **Transfer**: When `Ctrl+V` is pressed in the viewport canvas (or files are staged), Desky streams files via `PKT_FILE_START` / `CHUNK` / `FINISH`.
3. **Receipt**: `src/p2p/state.rs` saves the received files to `%USERPROFILE%\Downloads\Desky_Files\` and sets `write_clipboard_files(&[path])` on the receiver's clipboard.
4. **Execution**: Windows Explorer on the target machine handles the native paste into whichever folder is open.

### B. In-Session Interactive Whiteboard / Screen Pen
1. **Engine**: `src/windows/ui/viewport/whiteboard.rs` captures normalized stroke points `(nx, ny)` with minimal overhead.
2. **Rendering**: GDI+ subpixel antialiasing (`SmoothingModeHighQuality`) renders smooth polyline paths with round caps and joins over the video canvas without redrawing the underlying frame.
3. **Controls**: Centered floating pill palette with 4 distinct high-contrast colors and a one-click Clear tool.

### C. Zero-Overhead AVI Session Recording
1. **Architecture**: `src/media/recorder.rs` creates standard RIFF AVI containers containing `MJPG` video streams (pure Rust cross-platform implementation).
2. **Zero Transcoding**: Direct muxing of incoming remote JPEG frames straight into the AVI `movi` chunks without decoding or re-encoding. 0% extra CPU load!
3. **Output**: Stored automatically in `Videos\Desky_Recordings\Desky_Recording_<timestamp>.avi`.

### D. Native GPU Hardware Video Acceleration (MFT) & Fallback
1. **Hardware Acceleration**: `src/windows/codec/h264_mf.rs` uses Windows Media Foundation Transforms (`IMFTransform`) to leverage discrete and integrated GPUs (NVIDIA NVENC, Intel QuickSync, AMD AMF) directly without bundling heavy FFmpeg binaries or extra DLLs.
2. **Software Fallback**: When hardware MFT encoders are unavailable or on non-Windows platforms, `src/codec/mjpeg.rs` seamlessly takes over using full 4:4:4 chroma subsampling for text crispness with zero dropped frames.

### D. Quick Actions Menu
1. **Actions**: `src/windows/ui/viewport/quick_actions.rs` provides 1-click execution for `Ctrl+Alt+Del` (SAS), Workstation Lock (`Win+L`), Task Manager (`Ctrl+Shift+Esc`), Show Desktop (`Win+D`), Run Dialog (`Win+R`), and Safe Mode Reboot.
2. **Safety**: Destructive operations (Reboot, Safe Mode) trigger an explicit Windows confirmation alert before dispatching.
