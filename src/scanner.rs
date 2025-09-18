use anyhow::{bail, Context, Result};
use futures_util::future::join_all;
use ipnetwork::IpNetwork;
use pnet::datalink::{self, Channel, Config, NetworkInterface as PnetNetworkInterface};
use pnet::packet::arp::{ArpHardwareTypes, ArpOperations, ArpPacket, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet::packet::Packet;
use pnet::util::MacAddr;
use serde::Serialize;
use std::net::IpAddr;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Serialize, Debug, Clone)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub mac: String,
    pub hostname: Option<String>,
}

pub async fn scan_network() -> Result<Vec<DiscoveredDevice>> {
    info!("Starting network scan...");

    let pnet_iface = datalink::interfaces()
        .into_iter()
        .find(|iface| {
            iface.is_up()
                && !iface.is_loopback()
                && iface.mac.is_some()
                && iface.ips.iter().any(|ip| ip.is_ipv4())
        })
        .ok_or_else(|| anyhow::anyhow!("No suitable network interface found for scanning."))?;

    let ip_network = pnet_iface
        .ips
        .iter()
        .find(|ip| ip.is_ipv4())
        .ok_or_else(|| anyhow::anyhow!("Selected interface has no IPv4 address."))?;

    let source_ip = ip_network.ip();
    let network = IpNetwork::new(ip_network.ip(), ip_network.prefix())
        .context("Failed to create IP network")?;

    info!(
        "Found network interface to scan: {} on {}",
        network, pnet_iface.name
    );

    let source_mac = pnet_iface
        .mac
        .ok_or_else(|| anyhow::anyhow!("Interface has no MAC address"))?;

    let discovered_devices_no_hostname = tokio::task::spawn_blocking(move || {
        scan_with_pnet(pnet_iface, network, source_ip, source_mac)
    })
    .await
    .context("Failed to join scanning task")?
    .context("Network scanning failed")?;

    let lookups = discovered_devices_no_hostname
        .into_iter()
        .map(|mut device| {
            tokio::spawn(async move {
                if let Ok(ip_addr) = device.ip.parse::<IpAddr>() {
                    device.hostname = dns_lookup::lookup_addr(&ip_addr).ok();
                }
                device
            })
        });

    let discovered_devices = join_all(lookups)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    info!(
        "Network scan finished. Found {} devices.",
        discovered_devices.len()
    );
    Ok(discovered_devices)
}

fn scan_with_pnet(
    interface: PnetNetworkInterface,
    network: IpNetwork,
    source_ip: IpAddr,
    source_mac: MacAddr,
) -> Result<Vec<DiscoveredDevice>> {
    let source_ipv4 = match source_ip {
        IpAddr::V4(ip) => ip,
        _ => bail!("Only IPv4 is supported"),
    };

    let config = Config {
        read_timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    let (mut tx, mut rx) = match datalink::channel(&interface, config) {
        Ok(Channel::Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => bail!("Unsupported channel type"),
        Err(e) => bail!("Failed to create channel: {}", e),
    };

    for target_ip in network.iter() {
        let target_ipv4 = match target_ip {
            IpAddr::V4(ip) => ip,
            _ => continue,
        };

        if target_ipv4 == source_ipv4 {
            continue;
        }

        let mut ethernet_buffer = [0u8; 42];
        let mut ethernet_packet = MutableEthernetPacket::new(&mut ethernet_buffer).unwrap();

        ethernet_packet.set_destination(MacAddr::broadcast());
        ethernet_packet.set_source(source_mac);
        ethernet_packet.set_ethertype(EtherTypes::Arp);

        let mut arp_buffer = [0u8; 28];
        let mut arp_packet = MutableArpPacket::new(&mut arp_buffer).unwrap();

        arp_packet.set_hardware_type(ArpHardwareTypes::Ethernet);
        arp_packet.set_protocol_type(EtherTypes::Ipv4);
        arp_packet.set_hw_addr_len(6);
        arp_packet.set_proto_addr_len(4);
        arp_packet.set_operation(ArpOperations::Request);
        arp_packet.set_sender_hw_addr(source_mac);
        arp_packet.set_sender_proto_addr(source_ipv4);
        arp_packet.set_target_hw_addr(MacAddr::zero());
        arp_packet.set_target_proto_addr(target_ipv4);

        ethernet_packet.set_payload(arp_packet.packet());

        tx.send_to(ethernet_packet.packet(), None);
    }

    // Drop sender to allow receiver to unblock on some platforms
    drop(tx);

    let mut devices = Vec::new();
    let start_time = std::time::Instant::now();
    let scan_duration = Duration::from_secs(5);

    while start_time.elapsed() < scan_duration {
        match rx.next() {
            Ok(packet) => {
                if let Some(ethernet_packet) = pnet::packet::ethernet::EthernetPacket::new(packet) {
                    if ethernet_packet.get_ethertype() == EtherTypes::Arp {
                        if let Some(arp_packet) = ArpPacket::new(ethernet_packet.payload()) {
                            if arp_packet.get_operation() == ArpOperations::Reply {
                                let device = DiscoveredDevice {
                                    ip: arp_packet.get_sender_proto_addr().to_string(),
                                    mac: arp_packet.get_sender_hw_addr().to_string().to_uppercase(),
                                    hostname: None,
                                };
                                if !devices
                                    .iter()
                                    .any(|d: &DiscoveredDevice| d.mac == device.mac)
                                {
                                    devices.push(device);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                warn!("Error receiving packet: {}", e);
                break;
            }
        }
    }

    Ok(devices)
}
