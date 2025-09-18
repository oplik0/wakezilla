use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{RwLock, watch};
use tracing::{error, info};
use validator::{Validate, ValidationError};

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
    #[serde(
        serialize_with = "serialize_ipv4addr",
        deserialize_with = "deserialize_ipv4addr"
    )]
    pub ip: Ipv4Addr,
    pub name: String,
    pub description: Option<String>,
    pub turn_off_port: Option<u16>,
    pub can_be_turned_off: bool,
    #[serde(default = "get_default_request_rate")]
    pub request_rate: RequestRateConfig,

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

fn validate_ip(ip: &str) -> Result<(), ValidationError> {
    if ip.parse::<IpAddr>().is_ok() {
        Ok(())
    } else {
        Err(ValidationError::new("Invalid IP address"))
    }
}

static MAC_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$").unwrap());

fn validate_mac(mac: &str) -> Result<(), ValidationError> {
    if MAC_REGEX.is_match(mac) {
        Ok(())
    } else {
        Err(ValidationError::new("Invalid MAC address"))
    }
}
#[derive(Debug, Deserialize, Validate)]
pub struct AddMachineForm {
    #[validate(custom(function = "validate_mac"))]
    pub mac: String,
    #[validate(custom(function = "validate_ip"))]
    pub ip: String,
    pub name: String,
    pub description: Option<String>,
    pub turn_off_port: Option<u16>,
    #[serde(default = "default_can_be_turned_off")]
    pub can_be_turned_off: bool,
    pub requests_per_hour: Option<u32>,
    pub period_minutes: Option<u32>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RequestRateConfig {
    pub max_requests: u32,
    pub period_minutes: u32,
}

pub fn get_default_request_rate() -> RequestRateConfig {
    RequestRateConfig {
        max_requests: 10,
        period_minutes: 30,
    }
}

fn default_can_be_turned_off() -> bool {
    false
}

#[derive(Clone)]
pub struct AppState {
    pub machines: Arc<RwLock<Vec<Machine>>>,
    pub proxies: Arc<RwLock<HashMap<String, watch::Sender<bool>>>>,
}

/// Load machines using the configured database path
pub fn load_machines() -> Result<Vec<Machine>> {
    load_machines_from_path(DB_PATH)
}

/// Load machines from a specific path
pub fn load_machines_from_path(path: &str) -> Result<Vec<Machine>> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("Failed to read machines database from {}", path))?;
    
    let machines: Vec<Machine> = serde_json::from_str(&data)
        .with_context(|| "Failed to parse machines database")?;
    
    info!("Successfully loaded {} machines from database at {:?}", machines.len(), path);
    Ok(machines)
}

pub fn save_machines(machines: &[Machine]) -> Result<()> {
    let data =
        serde_json::to_string_pretty(machines).context("Failed to serialize machines data")?;
    fs::write(DB_PATH, data)
        .with_context(|| format!("Failed to write machines database to {}", DB_PATH))
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
        
        // We need to use tokio::spawn and async block to handle the async lock
        let proxies_clone = state.proxies.clone();
        tokio::spawn(async move {
            let mut proxies = proxies_clone.write().await;
            proxies.insert(proxy_key.clone(), tx);
            
            // We can't hold the lock across the await, so we need to drop it here
            drop(proxies);

            if let Err(e) = forward::proxy(local_port, remote_addr, machine_clone, wol_port, rx).await {
                error!("Forwarder for {} -> {} failed: {}", local_port, remote_addr, e);
            }
        });
    }
}
