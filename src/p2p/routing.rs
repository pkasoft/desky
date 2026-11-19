use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};

/// Network Routing & Tunneling Modes:
/// - 0: Smart P2P (Auto - Matchmaker with automatic P2P hole-punching and relay fallback)
/// - 1: Direct Server Tunnel (Force Encrypted Relay - 100% routed through desky-server, hides IPs)
/// - 2: Serverless Direct P2P (Bypasses server completely, connects directly to IP / LAN)
pub static ROUTING_MODE: AtomicU8 = AtomicU8::new(0);

/// Get the active routing mode (0 = Smart P2P, 1 = Server Tunnel, 2 = Serverless)
#[allow(dead_code)]
pub fn get_routing_mode() -> u8 {
    ROUTING_MODE.load(Ordering::Relaxed)
}

/// Set the active routing mode (0..=2)
#[allow(dead_code)]
pub fn set_routing_mode(mode: u8) {
    ROUTING_MODE.store(mode.min(2), Ordering::Relaxed);
}

/// Returns true if all traffic must strictly route through desky-server relay
pub fn is_force_server_relay() -> bool {
    ROUTING_MODE.load(Ordering::Relaxed) == 1
}

/// Returns true if the client should operate in purely serverless direct IP/LAN mode
pub fn is_serverless_mode() -> bool {
    ROUTING_MODE.load(Ordering::Relaxed) == 2
}

/// Parses an input string to see if it represents a direct IP:Port or LAN host target.
/// Supports formats:
/// - `192.168.1.50:21117`
/// - `192.168.1.50` (defaults to port 21117)
/// - `[::1]:21117`
/// - `myserver.lan:21117`
pub fn parse_direct_target(raw: &str) -> Option<SocketAddr> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }

    // Direct SocketAddr parse: e.g. "192.168.1.50:21117"
    if let Ok(addr) = t.parse::<SocketAddr>() {
        return Some(addr);
    }

    // Direct IpAddr parse: e.g. "192.168.1.50" -> default to port 21117
    if let Ok(ip) = t.parse::<std::net::IpAddr>() {
        return Some(SocketAddr::new(ip, 21117));
    }

    // Hostname / mDNS / local domain parse with optional port
    if t.contains(':') {
        use std::net::ToSocketAddrs;
        if let Ok(mut iter) = t.to_socket_addrs() {
            if let Some(addr) = iter.next() {
                return Some(addr);
            }
        }
    } else if t.contains('.') {
        use std::net::ToSocketAddrs;
        let with_port = format!("{}:21117", t);
        if let Ok(mut iter) = with_port.to_socket_addrs() {
            if let Some(addr) = iter.next() {
                return Some(addr);
            }
        }
    }

    None
}
