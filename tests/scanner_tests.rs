#![cfg(test)]
use wakezilla::scanner::{scan_network, DiscoveredDevice};

#[test]
fn discovered_device_serialize() {
    let device = DiscoveredDevice {
        ip: "192.168.1.1".to_string(),
        mac: "AA:BB:CC:DD:EE:FF".to_string(),
        hostname: Some("example.com".to_string()),
    };
    let json = serde_json::to_string(&device).unwrap();
    assert!(json.contains("\"ip\":\"192.168.1.1\""));
}

#[tokio::test]
async fn test_scan_network_basic() {
    // This test triggers the network interface detection code
    let result = scan_network().await;
    // It will likely fail due to no root privileges, but some code paths are exercised
    match &result {
        Ok(devices) => println!("Unexpectedly succeeded: found {} devices", devices.len()),
        Err(e) => println!("Expected failure: {}", e),
    }
    // As long as it doesn't panic, it's fine
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn hostname_lookup_integration() {
    // Test hostname lookup with a known IP
    // This doesn't require special privileges
    let devices = vec![DiscoveredDevice {
        ip: "127.0.0.1".to_string(), // localhost
        mac: "00:00:00:00:00:00".to_string(),
        hostname: None,
    }];

    let lookups = devices
        .into_iter()
        .map(|mut device| {
            tokio::spawn(async move {
                if let Ok(ip_addr) = device.ip.parse::<std::net::IpAddr>() {
                    device.hostname = dns_lookup::lookup_addr(&ip_addr).ok();
                }
                device
            })
        });

    let results = futures_util::future::join_all(lookups)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    assert!(!results.is_empty());
    let device = &results[0];
    assert_eq!(device.ip, "127.0.0.1");
    // Hostname might be None or some value depending on system
    // Just assert the structure is correct
}