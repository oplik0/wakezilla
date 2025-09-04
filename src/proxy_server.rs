use askama_axum::Template;
use axum::{
    extract::{Form, State},
    response::{IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::scanner;
use crate::web::{self, AppState, DeleteForm, Machine, RemoteTurnOffForm, WakeForm};
use crate::wol;

pub async fn start(port: u16) {
    let initial_machines = web::load_machines().unwrap_or_default();

    let state = AppState {
        machines: Arc::new(Mutex::new(initial_machines.clone())),
        proxies: Arc::new(Mutex::new(HashMap::new())),
    };

    for machine in &initial_machines {
        web::start_proxy_if_configured(machine, &state);
    }

    let app = Router::new()
        .route("/", get(show_machines))
        .route("/scan", get(scan_network_handler))
        .route("/machines", post(add_machine))
        .route("/machines/delete", post(delete_machine))
        .route("/machines/remote-turn-off", post(turn_off_remote_machine))
        .route("/machines/add-port-forward", post(add_port_forward))
        .route("/wol", post(wake_machine))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[derive(Template)]
#[template(path = "machines.html")]
struct MachinesTemplate {
    machines: Vec<Machine>,
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

async fn add_machine(State(state): State<AppState>, Form(new_machine_form): Form<web::AddMachineForm>) -> Redirect {
    let new_machine = Machine {
        mac: new_machine_form.mac,
        ip: new_machine_form.ip,
        name: new_machine_form.name,
        description: new_machine_form.description,
        turn_off_port: new_machine_form.turn_off_port,
        can_be_turned_off: new_machine_form.can_be_turned_off,
        request_rate: web::RequestRateConfig {
            max_requests: new_machine_form.requests_per_hour.unwrap_or(1000),
            period_minutes: 60,
        },

        port_forwards: Vec::new(),
    };
    let mut machines = state.machines.lock().unwrap();
    machines.push(new_machine);
    if let Err(e) = web::save_machines(&machines) {
        error!("Error saving machines: {}", e);
    }
    Redirect::to("/")
}

async fn delete_machine(
    State(state): State<AppState>,
    Form(payload): Form<DeleteForm>,
) -> Redirect {
    // Stop all proxies associated with this machine
    state.proxies.lock().unwrap().retain(|key, tx| {
        if key.starts_with(&payload.mac) {
            if tx.send(false).is_ok() {
                info!("Sent stop signal to proxy for MAC/key: {}", key);
            }
            false // Remove the entry
        } else {
            true // Keep the entry
        }
    });

    let mut machines = state.machines.lock().unwrap();
    machines.retain(|m| m.mac != payload.mac);
    if let Err(e) = web::save_machines(&machines) {
        error!("Error saving machines: {}", e);
    }
    Redirect::to("/")
}

async fn turn_off_remote_machine(
    State(state): State<AppState>,
    Form(payload): Form<RemoteTurnOffForm>,
) -> (axum::http::StatusCode, String) {
    let machine = {
        let machines = state.machines.lock().unwrap();
        machines.iter().find(|m| m.mac == payload.mac).cloned()
    };

    if let Some(machine) = machine {
        if let Some(port) = machine.turn_off_port {
            let url = format!("http://{}:{}/machines/turn-off", machine.ip, port);
            info!("Sending turn-off request to {}", url);
            let response = reqwest::Client::new().post(&url).send().await;
            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return (
                            axum::http::StatusCode::OK,
                            format!("Sent turn-off request to {}", payload.mac),
                        );
                    } else {
                        return (
                            axum::http::StatusCode::BAD_GATEWAY,
                            format!(
                                "Turn-off request to {} failed with status {}",
                                payload.mac,
                                resp.status()
                            ),
                        );
                    }
                }
                Err(e) => {
                    return (
                        axum::http::StatusCode::BAD_GATEWAY,
                        format!("Failed to send turn-off request: {}", e),
                    );
                }
            }
        } else {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("No turn-off port configured for {}", payload.mac),
            );
        }
    }

    (
        axum::http::StatusCode::NOT_FOUND,
        format!("Machine {} not found", payload.mac),
    )
}

async fn add_port_forward(
    State(state): State<AppState>,
    Form(payload): Form<web::AddPortForwardForm>,
) -> Redirect {
    let mut machines_guard = state.machines.lock().unwrap();
    if let Some(machine) = machines_guard.iter_mut().find(|m| m.mac == payload.mac) {
        let new_port_forward = web::PortForward {
            name: payload.name,
            local_port: payload.local_port,
            target_port: payload.target_port,
        };
        machine.port_forwards.push(new_port_forward.clone());

        let machine_clone_for_proxy = machine.clone();

        if let Err(e) = web::save_machines(&machines_guard) {
            error!("Error saving machines: {}", e);
        }

        drop(machines_guard); // Explicitly drop the mutex guard to end the mutable borrow

        // Start the new proxy
        web::start_proxy_if_configured(&machine_clone_for_proxy, &state);
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

    let bcast = std::net::Ipv4Addr::new(255, 255, 255, 255);
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
