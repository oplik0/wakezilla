use crate::models::{DiscoveredDevice, Machine, NetworkInterface};

use leptos::leptos_dom::logging::console_log;

use gloo_net::http::Request;
use web_sys::window;
const DEFAULT_API_PORT: u16 = 3000;

// Function to get the API base URL dynamically from the current window location
fn get_api_base() -> String {
    if let Some(window) = window() {
        let location = window.location();
        if let (Ok(protocol), Ok(hostname), Ok(_port)) =
            (location.protocol(), location.hostname(), location.port())
        {
            format!("{}//{}:{}{}", protocol, hostname, DEFAULT_API_PORT, "/api")
        } else {
            // Fallback to default if location properties are not available
            String::from("http://localhost:3000/api")
        }
    } else {
        String::from("http://localhost:3000/api")
    }
}

pub async fn create_machine(machine: Machine) -> Result<(), String> {
    let api_base = get_api_base();
    Request::post(&format!("{}/machines", api_base))
        .json(&machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
pub async fn get_details_machine(mac: &str) -> Result<Machine, String> {
    let api_base = get_api_base();
    Request::get(&format!("{}/machines/{}", api_base, mac))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_machine(mac: &str, machine: &Machine) -> Result<(), String> {
    let api_base = get_api_base();
    Request::put(&format!("{}/machines/{}", api_base, mac))
        .json(machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn delete_machine(mac: &str) -> Result<(), String> {
    let api_base = get_api_base();
    let payload = serde_json::json!({ "mac": mac });
    Request::delete(&format!("{}/machines/delete", api_base))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn fetch_machines() -> Result<Vec<Machine>, String> {
    let api_base = get_api_base();
    Request::get(&format!("{}/machines", api_base))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_interfaces() -> Result<Vec<NetworkInterface>, String> {
    let api_base = get_api_base();
    Request::get(&format!("{}/interfaces", api_base))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_scan_network(device: String) -> Result<Vec<DiscoveredDevice>, String> {
    let api_base = get_api_base();
    let url = if device.is_empty() {
        format!("{}/scan", api_base)
    } else {
        format!("{}/scan?interface={}", api_base, device)
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
    let api_base = get_api_base();
    let response = Request::post(&format!("{}/machines/{}/remote-turn-off", api_base, mac))
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

pub async fn wake_machine(mac: &str) -> Result<String, String> {
    let api_base = get_api_base();
    let response = Request::post(&format!("{}/machines/{}/wake", api_base, mac))
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
