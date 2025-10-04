use crate::models::{DiscoveredDevice, Machine, NetworkInterface};

use leptos::leptos_dom::logging::console_log;

use gloo_net::http::Request;

const API_BASE: &str = "http://localhost:3000/api";

pub async fn create_machine(machine: Machine) -> Result<(), String> {
    Request::post(&format!("{}/machines", API_BASE))
        .json(&machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
pub async fn get_details_machine(mac: &str) -> Result<Machine, String> {
    Request::get(&format!("{}/machines/{}", API_BASE, mac))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_machine(mac: &str, machine: &Machine) -> Result<(), String> {
    Request::put(&format!("{}/machines/{}", API_BASE, mac))
        .json(machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn delete_machine(mac: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "mac": mac });
    Request::delete(&format!("{}/machines/delete", API_BASE))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn fetch_machines() -> Result<Vec<Machine>, String> {
    Request::get(&format!("{}/machines", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_interfaces() -> Result<Vec<NetworkInterface>, String> {
    Request::get(&format!("{}/interfaces", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_scan_network(device: String) -> Result<Vec<DiscoveredDevice>, String> {
    let url = if device.is_empty() {
        format!("{}/scan", API_BASE)
    } else {
        format!("{}/scan?interface={}", API_BASE, device)
    };
    Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn turn_off_machine(mac: &str) -> Result<String, String> {
    let response = Request::post(&format!("{}/machines/{}/remote-turn-off", API_BASE, mac))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let is_success = response.ok();
    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let message = body
        .get("message")
        .and_then(|value| value.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| body.to_string());

    if is_success {
        Ok(message)
    } else {
        Err(message)
    }
}

pub async fn is_machine_online(machine: &Machine) -> bool {
    let url = format!(
        "http://{}:{}/health",
        machine.ip,
        machine.turn_off_port.unwrap_or(3000)
    );
    let response = Request::get(&url).send().await;
    match response {
        Ok(res) => res.status() == 200,
        Err(e) => {
            console_log(&format!(
                "Network error for machine {}: {}",
                machine.name, e
            ));
            false // Mark as offline on network errors
        }
    }
}
