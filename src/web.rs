use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::error;

use crate::forward;

const DB_PATH: &str = "machines.json";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Machine {
    pub mac: String,
    pub ip: Ipv4Addr,
    pub description: Option<String>,
    pub forward_target_port: Option<u16>,
    pub forward_local_port: Option<u16>,
}

#[derive(Deserialize)]
pub struct WakeForm {
    pub mac: String,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    pub mac: String,
}

#[derive(Clone)]
pub struct AppState {
    pub machines: Arc<Mutex<Vec<Machine>>>,
    pub proxies: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

pub fn load_machines() -> Result<Vec<Machine>, std::io::Error> {
    let data = fs::read_to_string(DB_PATH)?;
    serde_json::from_str(&data).map_err(|e| e.into())
}

pub fn save_machines(machines: &[Machine]) -> Result<(), std::io::Error> {
    let data = serde_json::to_string_pretty(machines)?;
    fs::write(DB_PATH, data)
}

pub fn start_proxy_if_configured(machine: &Machine, state: &AppState) {
    if let (Some(local_port), Some(target_port)) =
        (machine.forward_local_port, machine.forward_target_port)
    {
        let remote_addr = SocketAddr::new(machine.ip.into(), target_port);
        let mac_str = machine.mac.clone();
        let wol_port = 9; // Default WOL port

        let (tx, rx) = watch::channel(true);
        state.proxies.lock().unwrap().insert(mac_str.clone(), tx);

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