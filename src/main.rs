use clap::{Parser, Subcommand};
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use tracing::{error, info};

mod forward;
mod proxy_server;
mod client_server;
mod scanner;
mod system;
mod web;
mod wol;

/// Simple Wake-on-LAN sender + post-WOL reachability check.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Send WOL packet via CLI
    Send(SendArgs),
    /// Start proxy server
    ProxyServer(ServeArgs),
    /// Start a client server
    ClientServer(ClientServerArgs),
}

#[derive(Parser, Debug)]
#[command()]
struct ServeArgs {
    /// Port to listen on for the web server
    #[arg(short, long, default_value_t = 3000)]
    port: u16,
}

#[derive(Parser, Debug)]
#[command()]
struct ClientServerArgs {
    /// Port to listen on for the hello server
    #[arg(short, long, default_value_t = 3001)]
    port: u16,
}

#[derive(Parser, Debug)]
#[command()]
struct SendArgs {
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

#[tokio::main]
async fn main() -> io::Result<()> {
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_env_filter(env_filter)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Send(args) => {
            let mac = wol::parse_mac(&args.mac).unwrap_or_else(|e| {
                error!("Invalid MAC '{}': {}", args.mac, e);
                std::process::exit(2);
            });

            let bcast = args.broadcast.unwrap_or(Ipv4Addr::new(255, 255, 255, 255));

            wol::send_packets(&mac, bcast, args.port, args.count)?;

            info!(
                "Sent WOL magic packet to {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} via {}:{}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], bcast, args.port
            );

            // ---- Optional post-WOL reachability check ----
            if let Some(ip) = args.check_ip {
                if !wol::check_host(
                    ip,
                    args.check_tcp_port,
                    args.wait_secs,
                    args.interval_ms,
                    args.connect_timeout_ms,
                ) {
                    // Non-zero exit to indicate failure to callers
                    std::process::exit(3);
                }
            }
        }
        Commands::ProxyServer(args) => {
            proxy_server::start(args.port).await;
        }
        Commands::ClientServer(args) => {
            client_server::start(args.port).await;
        }
    }

    Ok(())
}
