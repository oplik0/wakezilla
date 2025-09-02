use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

/// Send WOL magic packets.
pub fn send_packets(
    mac: &[u8; 6],
    bcast: Ipv4Addr,
    port: u16,
    count: u32,
) -> io::Result<()> {
    let packet = build_magic_packet(mac);

    // Use a UDP socket with broadcast enabled
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    sock.set_broadcast(true)?;

    let addr = SocketAddrV4::new(bcast, port);

    for _ in 0..count {
        sock.send_to(&packet, addr)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

/// Poll a TCP port on a host until it becomes reachable or a timeout is hit.
pub fn check_host(
    ip: IpAddr,
    check_tcp_port: u16,
    wait_secs: u64,
    interval_ms: u64,
    connect_timeout_ms: u64,
) -> bool {
    let poll_every = Duration::from_millis(interval_ms);
    let connect_timeout = Duration::from_millis(connect_timeout_ms);
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let target = SocketAddr::new(ip, check_tcp_port);

    eprint!(
        "Waiting up to {}s for {}:{} ... ",
        wait_secs, ip, check_tcp_port
    );

    loop {
        if tcp_check(target, connect_timeout) {
            eprintln!("UP ✅");
            return true;
        }

        if Instant::now() >= deadline {
            eprintln!("TIMEOUT ❌ (host did not become reachable)");
            return false;
        }

        std::thread::sleep(poll_every);
        eprint!("."); // progress dots
    }
}

/// One-shot TCP "ping": returns true if connect succeeds within timeout.
pub fn tcp_check(addr: SocketAddr, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Parse MAC address from common string formats.
pub fn parse_mac(s: &str) -> Result<[u8; 6], String> {
    // Keep only hex digits
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return Err("expected 12 hex digits".into());
    }
    let mut mac = [0u8; 6];
    for i in 0..6 {
        mac[i] =
            u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| "invalid hex in MAC")?;
    }
    Ok(mac)
}

/// Build WOL magic packet: 6 x 0xFF + 16 repetitions of the MAC.
fn build_magic_packet(mac: &[u8; 6]) -> [u8; 102] {
    let mut pkt = [0u8; 102];
    // 6 bytes of 0xFF
    for i in 0..6 {
        pkt[i] = 0xFF;
    }
    // 16 repetitions of MAC
    for i in 0..16 {
        let start = 6 + i * 6;
        pkt[start..start + 6].copy_from_slice(mac);
    }
    pkt
}
