use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::error;

use serde::{Deserializer, Serializer};
use std::str::FromStr;

fn serialize_ipv4addr<S>(ip: &Ipv4Addr, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&ip.to_string())
}

fn deserialize_ipv4addr<'de, D>(deserializer: D) -> Result<Ipv4Addr, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ipv4Addr::from_str(&s).map_err(serde::de::Error::custom)
}

use crate::forward;

const DB_PATH: &str = "machines.json";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Machine {
    pub mac: String,
    #[serde(serialize_with = "serialize_ipv4addr", deserialize_with = "deserialize_ipv4addr")]
    pub ip: Ipv4Addr,
    pub name: String,
    pub description: Option<String>,
    pub turn_off_port: Option<u16>,
    pub can_be_turned_off: bool,
    pub requests_per_minute: Option<u32>,
    pub port_forwards: Vec<PortForward>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PortForward {
    pub name: String,
    pub local_port: u16,
    pub target_port: u16,
}

#[derive(Deserialize)]
pub struct WakeForm {
    pub mac: String,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    pub mac: String,
}

#[derive(Deserialize)]
pub struct RemoteTurnOffForm {
    pub mac: String,
}

#[derive(Deserialize)]
pub struct AddPortForwardForm {
    pub mac: String,
    pub name: String,
    pub local_port: u16,
    pub target_port: u16,
}

#[derive(Deserialize)]
pub struct AddMachineForm {
    pub mac: String,
    pub ip: Ipv4Addr,
    pub name: String,
    pub description: Option<String>,
    pub turn_off_port: Option<u16>,
    pub can_be_turned_off: bool,
    pub requests_per_minute: Option<u32>,
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
    for pf in &machine.port_forwards {
        let remote_addr = SocketAddr::new(machine.ip.into(), pf.target_port);
        let wol_port = 9; // Default WOL port
        let local_port = pf.local_port;
        let machine_clone = machine.clone();

        let (tx, rx) = watch::channel(true);
        // The key for the proxy should probably include the port to be unique
        let proxy_key = format!("{}-{}-{}", machine.mac, local_port, pf.target_port);
        state.proxies.lock().unwrap().insert(proxy_key.clone(), tx);

        tokio::spawn(async move {
            if let Err(e) = forward::proxy(local_port, remote_addr, machine_clone, wol_port, rx).await {
                error!(
                    "Forwarder for {} -> {} failed: {}",
                    local_port, remote_addr, e
                );
            }
        });
    }
}
