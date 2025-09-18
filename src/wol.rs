use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::{info, warn, instrument, debug};

/// Send WOL magic packets.
#[instrument(name = "send_wol_packets", skip(mac, config))]
pub async fn send_packets(mac: &[u8; 6], bcast: Ipv4Addr, port: u16, count: u32, config: &crate::config::Config) -> Result<()> {
    let packet = build_magic_packet(mac);
    debug!("Built WOL magic packet for MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
           mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

    // Use a UDP socket with broadcast enabled
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .context("Failed to bind UDP socket")?;
    sock.set_broadcast(true)
        .context("Failed to enable broadcast on socket")?;

    let addr = SocketAddrV4::new(bcast, port);
    info!("Sending {} WOL packets to {}:{}", count, bcast, port);

    for i in 0..count {
        debug!("Sending WOL packet {}/{}", i + 1, count);
        sock.send_to(&packet, addr)
            .await
            .context("Failed to send WOL packet")?;
        tokio::time::sleep(config.wol_packet_sleeptime()).await;
    }
    
    info!("Successfully sent {} WOL packets to {}:{}", count, bcast, port);
    Ok(())
}

/// Poll a TCP port on a host until it becomes reachable or a timeout is hit.
#[instrument(name = "check_host_reachability", skip(ip))]
pub fn check_host(
    ip: IpAddr,
    check_tcp_port: u16,
    wait_secs: u64,
    interval_ms: u64,
    connect_timeout_ms: u64,
    _config: &crate::config::Config,
) -> bool {
    let poll_every = Duration::from_millis(interval_ms);
    let connect_timeout = Duration::from_millis(connect_timeout_ms);
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let target = SocketAddr::new(ip, check_tcp_port);

    info!(
        "Waiting up to {}s for {}:{} ...",
        wait_secs, ip, check_tcp_port
    );

    loop {
        debug!("Checking if {}:{} is reachable", ip, check_tcp_port);
        if tcp_check(target, connect_timeout) {
            info!("Host {}:{} is UP ✅", ip, check_tcp_port);
            return true;
        }

        if Instant::now() >= deadline {
            warn!("TIMEOUT ❌ waiting for {}:{}", ip, check_tcp_port);
            return false;
        }

        debug!("Host {}:{} not reachable, waiting {:?} before next check", 
               ip, check_tcp_port, poll_every);
        std::thread::sleep(poll_every);
    }
}

/// One-shot TCP "ping": returns true if connect succeeds within timeout.
pub fn tcp_check(addr: SocketAddr, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Parse MAC address from common string formats.
#[instrument(name = "parse_mac", skip(s))]
pub fn parse_mac(s: &str) -> Result<[u8; 6]> {
    // Keep only hex digits
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        anyhow::bail!("expected 12 hex digits, got {}", hex.len());
    }
    let mut mac = [0u8; 6];
    for i in 0..6 {
        mac[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
            .with_context(|| {
                format!(
                    "invalid hex in MAC at position {}: '{}'",
                    i,
                    &hex[2 * i..2 * i + 2]
                )
            })?;
    }
    debug!("Successfully parsed MAC address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
           mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    Ok(mac)
}

/// Build WOL magic packet: 6 x 0xFF + 16 repetitions of the MAC.
fn build_magic_packet(mac: &[u8; 6]) -> [u8; 102] {
    let mut pkt = [0u8; 102];
    // 6 bytes of 0xFF
    for byte in pkt.iter_mut().take(6) {
        *byte = 0xFF;
    }
    // 16 repetitions of MAC
    for i in 0..16 {
        let start = 6 + i * 6;
        pkt[start..start + 6].copy_from_slice(mac);
    }
    pkt
}
