use pnet::datalink;
use std::process::Command;
use tracing::warn;

#[allow(dead_code)]
pub fn get_local_mac_addresses() -> Vec<String> {
    datalink::interfaces()
        .into_iter()
        .filter_map(|iface| iface.mac.map(|mac| mac.to_string()))
        .collect()
}

#[allow(dead_code)]
pub fn shutdown_machine() {
    warn!("SHUTTING DOWN THE MACHINE IN 5 SECONDS!");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let status = if cfg!(target_os = "windows") {
        Command::new("shutdown").args(["/s", "/t", "0"]).status()
    } else {
        Command::new("shutdown").args(["-h", "now"]).status()
    };

    match status {
        Ok(status) => {
            if !status.success() {
                warn!("Shutdown command failed with status: {}", status);
            }
        }
        Err(e) => {
            warn!("Failed to execute shutdown command: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_local_mac_addresses_returns_vector() {
        let addrs = get_local_mac_addresses();
        // Ensure any discovered MAC addresses are non-empty strings.
        assert!(addrs.iter().all(|addr| !addr.is_empty()));
    }
}
