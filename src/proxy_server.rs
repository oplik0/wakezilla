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
use crate::forward;
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
        .route("/machines/:mac", get(machine_detail))
        .route("/machines/update-ports", post(update_ports))
        .route("/machines/update-config", post(update_machine_config))
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

#[derive(Template)]
#[template(path = "machine_detail.html")]
struct MachineDetailTemplate {
    machine: Machine,
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

use axum::extract::Path;

async fn machine_detail(State(state): State<AppState>, Path(mac): Path<String>) -> impl IntoResponse {
    let machines = state.machines.lock().unwrap();
    if let Some(machine) = machines.iter().find(|m| m.mac == mac).cloned() {
        MachineDetailTemplate { machine }.into_response()
    } else {
        axum::http::StatusCode::NOT_FOUND.into_response()
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
            period_minutes: new_machine_form.period_minutes.unwrap_or(60),
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
            info!("Sending turn-off request to {}:{}", machine.ip, port);
            match forward::turn_off_remote_machine(&machine.ip.to_string(), port).await {
                Ok(_) => {
                    return (
                        axum::http::StatusCode::OK,
                        format!("Sent turn-off request to {}", payload.mac),
                    );
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

async fn update_machine_config(State(state): State<AppState>, Form(payload): Form<std::collections::HashMap<String, String>>) -> Redirect {
    let mac = payload.get("mac").cloned().unwrap_or_default();
    let mut machines = state.machines.lock().unwrap();
    if let Some(machine) = machines.iter_mut().find(|m| m.mac == mac) {
        if let Some(new_name) = payload.get("name") {
            machine.name = new_name.clone();
        }
        machine.description = payload.get("description").map(|v| if v.trim().is_empty() { None } else { Some(v.clone()) }).flatten();
        // If the box is present in the form (checked), the value is "true".
        machine.can_be_turned_off = payload.get("can_be_turned_off").is_some();
        if let Some(rph) = payload.get("requests_per_hour") {
            if let Ok(num) = rph.parse() {
                machine.request_rate.max_requests = num;
            }
        }
        if let Some(pm) = payload.get("period_minutes") {
            if let Ok(num) = pm.parse() {
                if num > 0 {
                    machine.request_rate.period_minutes = num;
                }
            }
        }
        let _ = web::save_machines(&machines);
        return Redirect::to(&format!("/machines/{}", mac));
    }
    Redirect::to("/")
}

async fn update_ports(State(state): State<AppState>, Form(payload): Form<std::collections::HashMap<String, String>>) -> Redirect {
    let mac = payload.get("mac").cloned().unwrap_or_default();
    let mut machines = state.machines.lock().unwrap();
    if let Some(machine) = machines.iter_mut().find(|m| m.mac == mac) {
        let mut ports = Vec::new();
        let mut idx = 0;
        loop {
            let name_key = format!("pf_name_{}", idx);
            let local_key = format!("pf_local_{}", idx);
            let target_key = format!("pf_target_{}", idx);
            match (payload.get(&name_key), payload.get(&local_key), payload.get(&target_key)) {
                (Some(name), Some(local), Some(target)) => {
                    let remove_key = format!("pf_remove_{}", idx);
                    let remove_checked = payload.get(&remove_key).is_some();
                    if !remove_checked && !name.trim().is_empty() && !local.trim().is_empty() && !target.trim().is_empty() {
                        if let (Ok(local), Ok(target)) = (local.parse(), target.parse()) {
                            ports.push(web::PortForward {
                                name: name.clone(),
                                local_port: local,
                                target_port: target,
                            });
                        }
                    }
                },
                _ => break,
            }
            idx += 1;
        }
        // Add new port if submitted
        if let (Some(name), Some(local), Some(target)) = (
            payload.get("pf_name_new"),
            payload.get("pf_local_new"),
            payload.get("pf_target_new")
        ) {
            if !name.trim().is_empty() && !local.trim().is_empty() && !target.trim().is_empty() {
                if let (Ok(local), Ok(target)) = (local.parse(), target.parse()) {
                    ports.push(web::PortForward {
                        name: name.clone(),
                        local_port: local,
                        target_port: target,
                    });
                }
            }
        }
        machine.port_forwards = ports;
        let machine_clone = machine.clone();
        let _ = web::save_machines(&machines);
        // Remove proxies for this mac
        state.proxies.lock().unwrap().retain(|key, tx| {
            let should_remove = key.starts_with(&mac);
            if should_remove {
                let _ = tx.send(false);
            }
            !should_remove
        });
        // Restart with new ports
        web::start_proxy_if_configured(&machine_clone, &state);
        return Redirect::to(&format!("/machines/{}", mac));
    }
    Redirect::to("/")
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
