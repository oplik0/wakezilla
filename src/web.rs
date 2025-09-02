use axum::{
    Router,
    extract::{Form, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

use crate::forward;
use crate::wol;

const DB_PATH: &str = "machines.json";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Machine {
    mac: String,
    ip: Ipv4Addr,
    port: u16,
    forward_target_port: Option<u16>,
    forward_local_port: Option<u16>,
}

#[derive(Deserialize)]
pub struct WakeForm {
    mac: String,
    port: u16,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    mac: String,
}

#[derive(Clone)]
struct AppState {
    machines: Arc<Mutex<Vec<Machine>>>,
}

fn load_machines() -> Result<Vec<Machine>, std::io::Error> {
    let data = fs::read_to_string(DB_PATH)?;
    serde_json::from_str(&data).map_err(|e| e.into())
}

fn save_machines(machines: &[Machine]) -> Result<(), std::io::Error> {
    let data = serde_json::to_string_pretty(machines)?;
    fs::write(DB_PATH, data)
}

pub async fn run() {
    let initial_machines = load_machines().unwrap_or_default();

    for machine in &initial_machines {
        if let (Some(local_port), Some(target_port)) =
            (machine.forward_local_port, machine.forward_target_port)
        {
            let remote_addr = SocketAddr::new(machine.ip.into(), target_port);
            let mac_str = machine.mac.clone();
            let wol_port = machine.port;

            tokio::spawn(async move {
                if let Err(e) = forward::proxy(local_port, remote_addr, mac_str, wol_port).await {
                    eprintln!(
                        "Forwarder for {} -> {} failed: {}",
                        local_port, remote_addr, e
                    );
                }
            });
        }
    }

    let state = AppState {
        machines: Arc::new(Mutex::new(initial_machines)),
    };

    let app = Router::new()
        .route("/", get(show_machines))
        .route("/machines", post(add_machine))
        .route("/machines/delete", post(delete_machine))
        .route("/wol", post(wake_machine))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn show_machines(State(state): State<AppState>) -> Html<String> {
    let machines = state.machines.lock().unwrap();
    let machine_rows: String = machines
        .iter()
        .map(|m| {
            format!(
                r#"<tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>
                        <form action="/wol" method="post" style="display: inline;">
                            <input type="hidden" name="mac" value="{}">
                            <input type="hidden" name="port" value="{}">
                            <button type="submit">Wake Up</button>
                        </form>
                        <form action="/machines/delete" method="post" style="display: inline;">
                            <input type="hidden" name="mac" value="{}">
                            <button type="submit">Remove</button>
                        </form>
                    </td>
                </tr>"#,
                m.mac,
                m.ip,
                m.port,
                m.forward_local_port
                    .map_or("".to_string(), |p| p.to_string()),
                m.forward_target_port
                    .map_or("".to_string(), |p| p.to_string()),
                m.mac,
                m.port,
                m.mac
            )
        })
        .collect();

    let html_body = format!(
        r#"
        <!doctype html>
        <html>
            <head>
                <title>WOL Manager</title>
            </head>
            <body>
                <h1>Registered Machines</h1>
                <table>
                    <thead>
                        <tr>
                            <th>MAC Address</th>
                            <th>IP Address</th>
                            <th>WOL Port</th>
                            <th>Fwd Local Port</th>
                            <th>Fwd Target Port</th>
                            <th>Action</th>
                        </tr>
                    </thead>
                    <tbody>
                        {}
                    </tbody>
                </table>
                <hr>
                <h2>Add New Machine</h2>
                <form action="/machines" method="post">
                    <label for="mac">MAC Address:</label><br>
                    <input type="text" id="mac" name="mac" required size="50"><br><br>

                    <label for="ip">Machine IP Address:</label><br>
                    <input type="text" id="ip" name="ip" required size="50"><br><br>

                    <label for="port">WOL Port:</label><br>
                    <input type="number" id="port" name="port" required value="9"><br><br>

                    <hr>
                    <h3>TCP Forwarding (optional)</h3>
                    <label for="forward_local_port">Local Port:</label><br>
                    <input type="number" id="forward_local_port" name="forward_local_port"><br><br>
                    <label for="forward_target_port">Target Port:</label><br>
                    <input type="number" id="forward_target_port" name="forward_target_port"><br><br>

                    <input type="submit" value="Add Machine">
                </form>
            </body>
        </html>
    "#,
        machine_rows
    );

    Html(html_body)
}

async fn add_machine(State(state): State<AppState>, Form(new_machine): Form<Machine>) -> Redirect {
    let mut machines = state.machines.lock().unwrap();
    machines.push(new_machine);
    if let Err(e) = save_machines(&machines) {
        eprintln!("Error saving machines: {}", e);
    }
    Redirect::to("/")
}

async fn delete_machine(
    State(state): State<AppState>,
    Form(payload): Form<DeleteForm>,
) -> Redirect {
    let mut machines = state.machines.lock().unwrap();
    machines.retain(|m| m.mac != payload.mac);
    if let Err(e) = save_machines(&machines) {
        eprintln!("Error saving machines: {}", e);
    }
    Redirect::to("/")
}

async fn wake_machine(Form(payload): Form<WakeForm>) -> impl IntoResponse {
    let mac = match wol::parse_mac(&payload.mac) {
        Ok(mac) => mac,
        Err(e) => {
            return format!("Invalid MAC address '{}': {}", payload.mac, e);
        }
    };

    let bcast = Ipv4Addr::new(255, 255, 255, 255);
    let port = payload.port;
    let count = 3;

    match wol::send_packets(&mac, bcast, port, count) {
        Ok(_) => format!("Sent WOL packet to {}", payload.mac),
        Err(e) => format!("Failed to send WOL packet: {}", e),
    }
}
