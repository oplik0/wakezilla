use regex::Regex;
use serde::Serialize;
use std::process::Command;
use tracing::{info, warn};

#[derive(Serialize, Debug, Clone)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub mac: String,
}

pub async fn scan_network() -> Result<Vec<DiscoveredDevice>, String> {
    info!("Starting network scan using 'arp -a'...");

    let discovered_devices = tokio::task::spawn_blocking(|| {
        let output = Command::new("arp")
            .arg("-a")
            .output()
            .map_err(|e| format!("Failed to execute 'arp -a': {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("'arp -a' command finished with non-zero status: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_arp_output(&stdout)
    })
    .await
    .map_err(|e| e.to_string())??;

    info!(
        "Network scan finished. Found {} devices.",
        discovered_devices.len()
    );
    Ok(discovered_devices)
}

fn parse_arp_output(output: &str) -> Result<Vec<DiscoveredDevice>, String> {
    // Regex for macOS/Linux: `(IP_ADDRESS) at MAC_ADDRESS`
    // Example: `? (192.168.1.1) at 1c:2d:3e:4f:5a:6b on en0 ifscope [ethernet]`
    let re = Regex::new(r"\((?P<ip>[\d\.]+)\) at (?P<mac>([0-9a-fA-F]{1,2}:){5}[0-9a-fA-F]{1,2})")
        .map_err(|e| e.to_string())?;

    let mut devices = Vec::new();
    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            let ip = caps.name("ip").unwrap().as_str().to_string();
            let mac = caps.name("mac").unwrap().as_str().to_string().to_uppercase();
            // Avoid adding incomplete entries
            if !mac.contains("(incomplete)") {
                devices.push(DiscoveredDevice { ip, mac });
            }
        }
    }
    Ok(devices)
}
