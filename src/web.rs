use askama_axum::Template;
use axum::{
    extract::{Form, State},
    response::{IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{error, info};

use crate::forward;
use crate::scanner;
use crate::wol;

const DB_PATH: &str = "machines.json";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Machine {
    mac: String,
    ip: Ipv4Addr,
    description: Option<String>,
    forward_target_port: Option<u16>,
    forward_local_port: Option<u16>,
}

#[derive(Deserialize)]
pub struct WakeForm {
    mac: String,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    mac: String,
}

#[derive(Template)]
#[template(path = "machines.html")]
struct MachinesTemplate {
    machines: Vec<Machine>,
}

#[derive(Clone)]
struct AppState {
    machines: Arc<Mutex<Vec<Machine>>>,
    proxies: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

fn load_machines() -> Result<Vec<Machine>, std::io::Error> {
    let data = fs::read_to_string(DB_PATH)?;
    serde_json::from_str(&data).map_err(|e| e.into())
}

fn save_machines(machines: &[Machine]) -> Result<(), std::io::Error> {
    let data = serde_json::to_string_pretty(machines)?;
    fs::write(DB_PATH, data)
}

fn start_proxy_if_configured(machine: &Machine, state: &AppState) {
    if let (Some(local_port), Some(target_port)) =
        (machine.forward_local_port, machine.forward_target_port)
    {
        let remote_addr = SocketAddr::new(machine.ip.into(), target_port);
        let mac_str = machine.mac.clone();
        let wol_port = 9; // Default WOL port

        let (tx, rx) = watch::channel(true);
        state
            .proxies
            .lock()
            .unwrap()
            .insert(mac_str.clone(), tx);

        tokio::spawn(async move {
            if let Err(e) = forward::proxy(local_port, remote_addr, mac_str, wol_port, rx).await {
                error!(
                    "Forwarder for {} -> {} failed: {}",
                    local_port, remote_addr, e
                );
            }
        });
    }
}

pub async fn run(port: u16) {
    let initial_machines = load_machines().unwrap_or_default();

    let state = AppState {
        machines: Arc::new(Mutex::new(initial_machines.clone())),
        proxies: Arc::new(Mutex::new(HashMap::new())),
    };

    for machine in &initial_machines {
        start_proxy_if_configured(machine, &state);
    }

    let app = Router::new()
        .route("/", get(show_machines))
        .route("/scan", get(scan_network_handler))
        .route("/machines", post(add_machine))
        .route("/machines/delete", post(delete_machine))
        .route("/wol", post(wake_machine))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

pub async fn run_client_server(port: u16) {
    let app = Router::new().route("/health", get(health_check));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> impl IntoResponse {
    let status = serde_json::json!({ "status": "ok" });
    Json(status)
}

async fn scan_network_handler() -> impl IntoResponse {
    match scanner::scan_network().await {
        Ok(devices) => Ok(Json(devices)),
        Err(e) => {
            error!("Network scan failed: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn show_machines(State(state): State<AppState>) -> impl IntoResponse {
    let machines = state.machines.lock().unwrap().clone();
    MachinesTemplate { machines }
}

async fn add_machine(State(state): State<AppState>, Form(new_machine): Form<Machine>) -> Redirect {
    start_proxy_if_configured(&new_machine, &state);
    let mut machines = state.machines.lock().unwrap();
    machines.push(new_machine);
    if let Err(e) = save_machines(&machines) {
        error!("Error saving machines: {}", e);
    }
    Redirect::to("/")
}

async fn delete_machine(
    State(state): State<AppState>,
    Form(payload): Form<DeleteForm>,
) -> Redirect {
    if let Some(tx) = state.proxies.lock().unwrap().remove(&payload.mac) {
        if tx.send(false).is_ok() {
            info!("Sent stop signal to proxy for MAC: {}", payload.mac);
        }
    }

    let mut machines = state.machines.lock().unwrap();
    machines.retain(|m| m.mac != payload.mac);
    if let Err(e) = save_machines(&machines) {
        error!("Error saving machines: {}", e);
    }
    Redirect::to("/")
}

async fn wake_machine(Form(payload): Form<WakeForm>) -> (axum::http::StatusCode, String) {
    let mac = match wol::parse_mac(&payload.mac) {
        Ok(mac) => mac,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Invalid MAC address '{}': {}", payload.mac, e),
            );
        }
    };

    let bcast = Ipv4Addr::new(255, 255, 255, 255);
    let port = 9; // Default WOL port
    let count = 3;

    match wol::send_packets(&mac, bcast, port, count) {
        Ok(_) => (
            axum::http::StatusCode::OK,
            format!("Sent WOL packet to {}", payload.mac),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send WOL packet: {}", e),
        ),
    }
}
