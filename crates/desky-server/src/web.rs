use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use askama::Template;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::info;

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

pub struct AppState {
    pub config: Arc<WebConfig>,
    pub stats: Arc<ServerStats>,
    pub hosts: Arc<RwLock<HashMap<String, HostEntry>>>,
    pub db: Arc<Database>,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error_message: Option<String>,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct DashboardTemplate {
    pub online_hosts: usize,
    pub total_registered_peers: u64,
    pub matches_brokered: u64,
    pub relayed_bytes_formatted: String,
    pub relayed_packets: u64,
    pub uptime_formatted: String,
    pub udp_port: u16,
    pub web_port: u16,
    pub db_path: String,
    pub token_query: String,
    pub peers: Vec<PeerRow>,
}

pub struct PeerRow {
    pub desky_id: String,
    pub formatted_id: String,
    pub online: bool,
    pub endpoint: String,
    pub machine_id: String,
    pub last_seen_text: String,
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const K: f64 = 1024.0;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(K).floor() as usize;
    let i = i.min(sizes.len() - 1);
    let val = (bytes as f64) / K.powi(i as i32);
    format!("{:.2} {}", val, sizes[i])
}

fn format_uptime(secs: u64) -> String {
    let d = secs / (3600 * 24);
    let h = (secs % (3600 * 24)) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{}d {}h {}m", d, h, m)
    } else if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else {
        format!("{}m {}s", m, s)
    }
}

fn format_relative_time(seconds: i64) -> String {
    if seconds < 5 {
        "Just now".to_string()
    } else if seconds < 60 {
        format!("{}s ago", seconds)
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}

fn is_request_authorized(req: &HttpRequest, admin_token: &Option<String>) -> bool {
    let Some(expected) = admin_token else {
        return true;
    };

    // 1. Check query parameter ?token=...
    let query_str = req.query_string();
    for param in query_str.split('&') {
        if let Some((k, v)) = param.split_once('=') {
            if k == "token" && v == expected {
                return true;
            }
        }
    }

    // 2. Check Authorization header
    if let Some(auth_val) = req.headers().get("Authorization").and_then(|v| v.to_str().ok()) {
        if auth_val.strip_prefix("Bearer ").map(|t| t.trim()) == Some(expected.as_str()) {
            return true;
        }
    }

    // 3. Check X-Admin-Token header
    if let Some(token_val) = req.headers().get("X-Admin-Token").and_then(|v| v.to_str().ok()) {
        if token_val.trim() == expected.as_str() {
            return true;
        }
    }

    false
}

#[get("/health")]
async fn health_check(data: web::Data<AppState>) -> impl Responder {
    let uptime = data.stats.uptime_secs();
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "uptime_seconds": uptime,
        "version": "0.1.0"
    }))
}

#[get("/status")]
async fn status_check(data: web::Data<AppState>) -> impl Responder {
    let uptime = data.stats.uptime_secs();
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "uptime_seconds": uptime,
        "version": "0.1.0"
    }))
}

#[get("/api/stats")]
async fn api_stats(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if !is_request_authorized(&req, &data.config.admin_token) {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "unauthorized",
            "message": "Missing or invalid admin token"
        }));
    }

    let online_hosts = data.hosts.read().await.len();
    let total_peers = data.db.count_total_peers().unwrap_or(0);
    let uptime = data.stats.uptime_secs();
    let relayed_pkts = data.stats.relayed_packets.load(Ordering::Relaxed);
    let relayed_bytes = data.stats.relayed_bytes.load(Ordering::Relaxed);
    let matches = data.stats.matches_brokered.load(Ordering::Relaxed);
    let heartbeats = data.stats.total_heartbeats.load(Ordering::Relaxed);
    let registrations = data.stats.total_registrations.load(Ordering::Relaxed);

    HttpResponse::Ok().json(serde_json::json!({
        "online_hosts": online_hosts,
        "total_registered_peers": total_peers,
        "uptime_seconds": uptime,
        "relayed_packets": relayed_pkts,
        "relayed_bytes": relayed_bytes,
        "matches_brokered": matches,
        "total_heartbeats": heartbeats,
        "total_registrations": registrations,
        "udp_port": data.config.udp_port,
        "web_port": data.config.port
    }))
}

#[get("/api/peers")]
async fn api_peers(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if !is_request_authorized(&req, &data.config.admin_token) {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "unauthorized",
            "message": "Missing or invalid admin token"
        }));
    }

    let online_table = {
        let table = data.hosts.read().await;
        table
            .iter()
            .map(|(k, v)| (k.clone(), v.addr.to_string()))
            .collect::<HashMap<_, _>>()
    };

    let db_peers = data.db.list_peers(200).unwrap_or_default();
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let peer_items: Vec<_> = db_peers
        .into_iter()
        .map(|p| {
            let is_online = online_table.contains_key(&p.desky_id);
            let endpoint = if is_online {
                online_table
                    .get(&p.desky_id)
                    .cloned()
                    .unwrap_or_else(|| format!("{}:{}", p.last_ip, p.last_port))
            } else {
                format!("{}:{}", p.last_ip, p.last_port)
            };
            let last_seen_ago = (now_ts - p.last_seen).max(0);
            let masked_machine = if p.machine_id.len() > 10 {
                format!(
                    "{}...{}",
                    &p.machine_id[0..4],
                    &p.machine_id[p.machine_id.len() - 4..]
                )
            } else {
                p.machine_id
            };

            serde_json::json!({
                "desky_id": p.desky_id,
                "formatted_id": format_id(&p.desky_id),
                "online": is_online,
                "endpoint": endpoint,
                "machine_id": masked_machine,
                "created_at": p.created_at,
                "last_seen_secs_ago": last_seen_ago
            })
        })
        .collect();

    HttpResponse::Ok().json(peer_items)
}

#[get("/")]
async fn index_page(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
    if !is_request_authorized(&req, &data.config.admin_token) {
        let tpl = LoginTemplate {
            error_message: if req.query_string().contains("token=") {
                Some("Invalid admin password/token provided.".to_string())
            } else {
                None
            },
        };
        return match tpl.render() {
            Ok(html) => HttpResponse::Unauthorized()
                .content_type("text/html; charset=utf-8")
                .body(html),
            Err(e) => HttpResponse::InternalServerError().body(format!("Template error: {:?}", e)),
        };
    }

    let online_hosts = data.hosts.read().await.len();
    let total_registered_peers = data.db.count_total_peers().unwrap_or(0);
    let matches_brokered = data.stats.matches_brokered.load(Ordering::Relaxed);
    let relayed_bytes = data.stats.relayed_bytes.load(Ordering::Relaxed);
    let relayed_packets = data.stats.relayed_packets.load(Ordering::Relaxed);
    let uptime_secs = data.stats.uptime_secs();

    let online_table = {
        let table = data.hosts.read().await;
        table
            .iter()
            .map(|(k, v)| (k.clone(), v.addr.to_string()))
            .collect::<HashMap<_, _>>()
    };

    let db_peers = data.db.list_peers(200).unwrap_or_default();
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let peer_rows: Vec<_> = db_peers
        .into_iter()
        .map(|p| {
            let is_online = online_table.contains_key(&p.desky_id);
            let endpoint = if is_online {
                online_table
                    .get(&p.desky_id)
                    .cloned()
                    .unwrap_or_else(|| format!("{}:{}", p.last_ip, p.last_port))
            } else {
                format!("{}:{}", p.last_ip, p.last_port)
            };
            let last_seen_ago = (now_ts - p.last_seen).max(0);
            let masked_machine = if p.machine_id.len() > 10 {
                format!(
                    "{}...{}",
                    &p.machine_id[0..4],
                    &p.machine_id[p.machine_id.len() - 4..]
                )
            } else {
                p.machine_id
            };

            PeerRow {
                desky_id: p.desky_id.clone(),
                formatted_id: format_id(&p.desky_id),
                online: is_online,
                endpoint,
                machine_id: masked_machine,
                last_seen_text: format_relative_time(last_seen_ago),
            }
        })
        .collect();

    let token_query = data
        .config
        .admin_token
        .as_ref()
        .map(|t| format!("?token={}", t))
        .unwrap_or_default();

    let tpl = DashboardTemplate {
        online_hosts,
        total_registered_peers,
        matches_brokered,
        relayed_bytes_formatted: format_bytes(relayed_bytes),
        relayed_packets,
        uptime_formatted: format_uptime(uptime_secs),
        udp_port: data.config.udp_port,
        web_port: data.config.port,
        db_path: data.config.db_path.clone(),
        token_query,
        peers: peer_rows,
    };

    match tpl.render() {
        Ok(html) => HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html),
        Err(e) => HttpResponse::InternalServerError().body(format!("Template error: {:?}", e)),
    }
}

pub async fn run_web_server(
    config: WebConfig,
    stats: Arc<ServerStats>,
    hosts: Arc<RwLock<HashMap<String, HostEntry>>>,
    db: Arc<Database>,
) -> anyhow::Result<()> {
    let bind_addr = format!("{}:{}", config.bind_ip, config.port);
    info!(
        "🌐 Actix-Web + Askama Dashboard listening on: http://{}",
        bind_addr
    );

    let app_state = web::Data::new(AppState {
        config: Arc::new(config),
        stats,
        hosts,
        db,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(health_check)
            .service(status_check)
            .service(api_stats)
            .service(api_peers)
            .service(index_page)
    })
    .bind(&bind_addr)?
    .run()
    .await?;

    Ok(())
}
