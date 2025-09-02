use crate::wol;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};

pub async fn proxy(
    local_port: u16,
    remote_addr: SocketAddr,
    mac_str: String,
    wol_port: u16,
) -> io::Result<()> {
    let listen_addr = format!("0.0.0.0:{}", local_port);
    let listener = TcpListener::bind(&listen_addr).await?;
    println!(
        "TCP Forwarder listening on {}, proxying to {}",
        listen_addr, remote_addr
    );

    loop {
        let (mut inbound, client_addr) = listener.accept().await?;
        println!(
            "Accepted connection from {} to forward to {}",
            client_addr, remote_addr
        );

        let remote_addr_clone = remote_addr;
        let mac_str_clone = mac_str.clone();

        tokio::spawn(async move {
            let connect_timeout = Duration::from_millis(1000);
            if !wol::tcp_check(remote_addr_clone, connect_timeout) {
                println!(
                    "Host {} seems to be down. Sending WOL packet to MAC {}.",
                    remote_addr_clone, mac_str_clone
                );

                let mac = match wol::parse_mac(&mac_str_clone) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Invalid MAC for WOL on proxy: {}: {}", mac_str_clone, e);
                        return;
                    }
                };

                if let Err(e) =
                    wol::send_packets(&mac, Ipv4Addr::new(255, 255, 255, 255), wol_port, 3)
                {
                    eprintln!("Failed to send WOL packet for {}: {}", mac_str_clone, e);
                    return;
                }

                println!(
                    "WOL packet sent. Waiting up to 60s for {} to become reachable...",
                    remote_addr_clone
                );

                let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
                let mut host_up = false;
                while tokio::time::Instant::now() < deadline {
                    if wol::tcp_check(remote_addr_clone, connect_timeout) {
                        println!("Host {} is now up.", remote_addr_clone);
                        host_up = true;
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }

                if !host_up {
                    eprintln!(
                        "Timeout waiting for host {} to come up. Dropping connection from {}.",
                        remote_addr_clone, client_addr
                    );
                    return;
                }
            }

            let mut outbound = match TcpStream::connect(remote_addr_clone).await {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("Failed to connect to remote {}: {}", remote_addr_clone, e);
                    return;
                }
            };

            if let Err(e) = copy_bidirectional(&mut inbound, &mut outbound).await {
                eprintln!(
                    "Error forwarding data between {} and {}: {}",
                    client_addr, remote_addr_clone, e
                );
            }
        });
    }
}
