use clap::Parser;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

/// Simple Wake-on-LAN sender + post-WOL reachability check.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Target MAC address (formats: 00:11:22:33:44:55 or 001122334455, etc.)
    mac: String,

    /// Broadcast IP to use (default 255.255.255.255)
    #[arg(short, long)]
    broadcast: Option<Ipv4Addr>,

    /// UDP port (common: 9 or 7). Default: 9
    #[arg(short, long, default_value_t = 9)]
    port: u16,

    /// Number of times to send the packet (helps with flaky networks)
    #[arg(short = 'n', long, default_value_t = 3)]
    count: u32,

    /// Optional: IP/host to check after WOL (e.g., 192.168.0.200)
    #[arg(long, value_name = "IP")]
    check_ip: Option<IpAddr>,

    /// Optional: TCP port to check on the target host (default 22)
    #[arg(long, default_value_t = 22)]
    check_tcp_port: u16,

    /// Max time to wait (seconds) for the host to come up
    #[arg(long, default_value_t = 90)]
    wait_secs: u64,

    /// Poll interval (milliseconds) between checks
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,

    /// Per-attempt TCP connect timeout (milliseconds)
    #[arg(long, default_value_t = 700)]
    connect_timeout_ms: u64,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let mac = parse_mac(&args.mac).unwrap_or_else(|e| {
        eprintln!("Invalid MAC '{}': {}", args.mac, e);
        std::process::exit(2);
    });

    let packet = build_magic_packet(&mac);

    // Use a UDP socket with broadcast enabled
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    sock.set_broadcast(true)?;

    let bcast = args.broadcast.unwrap_or(Ipv4Addr::new(255, 255, 255, 255));
    let addr = SocketAddrV4::new(bcast, args.port);

    for _ in 0..args.count {
        sock.send_to(&packet, addr)?;
        std::thread::sleep(Duration::from_millis(50));
    }

    println!(
        "Sent WOL magic packet to {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} via {}:{}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], bcast, args.port
    );

    // ---- Optional post-WOL reachability check ----
    if let Some(ip) = args.check_ip {
        let poll_every = Duration::from_millis(args.interval_ms);
        let connect_timeout = Duration::from_millis(args.connect_timeout_ms);
        let deadline = Instant::now() + Duration::from_secs(args.wait_secs);
        let target = SocketAddr::new(ip, args.check_tcp_port);

        eprint!(
            "Waiting up to {}s for {}:{} ... ",
            args.wait_secs, ip, args.check_tcp_port
        );

        loop {
            // attempt a TCP connect with timeout
            if tcp_check(target, connect_timeout) {
                eprintln!("UP ✅");
                return Ok(());
            }

            if Instant::now() >= deadline {
                eprintln!("TIMEOUT ❌ (host did not become reachable)");
                // Non-zero exit to indicate failure to callers
                std::process::exit(3);
            }

            std::thread::sleep(poll_every);
            eprint!("."); // progress dots
        }
    }

    Ok(())
}

/// One-shot TCP "ping": returns true if connect succeeds within timeout.
fn tcp_check(addr: SocketAddr, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Parse MAC address from common string formats.
fn parse_mac(s: &str) -> Result<[u8; 6], String> {
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
