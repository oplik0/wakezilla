use crate::{web::Machine, wol};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::{error, info, warn};

pub async fn proxy(
    local_port: u16,
    remote_addr: SocketAddr,
    machine: Machine,
    wol_port: u16,
    mut rx: watch::Receiver<bool>,
) -> io::Result<()> {
    let listen_addr = format!("0.0.0.0:{}", local_port);
    let listener = TcpListener::bind(&listen_addr).await?;
    info!(
        "TCP Forwarder listening on {}, proxying to {}",
        listen_addr, remote_addr
    );

    let last_request_time = Arc::new(Mutex::new(Instant::now()));

    if machine.can_be_turned_off {
        let last_request_time = Arc::clone(&last_request_time);
        let turn_off_port = machine.turn_off_port;
        let remote_ip = machine.ip;
        let mac = machine.mac.clone();

        tokio::spawn(async move {
            loop {
                let check_interval = machine.requests_per_minute.unwrap_or(0);
                if check_interval == 0 {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
                tokio::time::sleep(Duration::from_secs(check_interval as u64 * 60)).await;
                let elapsed = {
                    let last_time = last_request_time.lock().unwrap();
                    last_time.elapsed()
                };

                if elapsed > Duration::from_secs(check_interval as u64 * 60) {
                    if let Some(port) = turn_off_port {
                        let url = format!("http://{}:{}/machines/turn-off", remote_ip, port);
                        info!("No requests for {}, sending turn-off signal to {}", mac, url);
                        let _ = reqwest::Client::new().post(&url).send().await;
                    }
                }
            }
        });
    }

    loop {
        tokio::select! {
            result = rx.changed() => {
                if result.is_err() || !*rx.borrow() {
                    info!("Proxy for {} on port {} cancelled.", remote_addr, local_port);
                    return Ok(());
                }
            }
            result = listener.accept() => {
                let (mut inbound, client_addr) = result?;
                info!(
                    "Accepted connection from {} to forward to {}",
                    client_addr, remote_addr
                );

                if machine.can_be_turned_off {
                    let mut last_time = last_request_time.lock().unwrap();
                    *last_time = Instant::now();
                }

                let remote_addr_clone = remote_addr;
                let mac_str_clone = machine.mac.clone();

                tokio::spawn(async move {
                    let connect_timeout = Duration::from_millis(1000);
                    if !wol::tcp_check(remote_addr_clone, connect_timeout) {
                        info!(
                            "Host {} seems to be down. Sending WOL packet to MAC {}.",
                            remote_addr_clone, mac_str_clone
                        );

                        let mac = match wol::parse_mac(&mac_str_clone) {
                            Ok(m) => m,
                            Err(e) => {
                                error!("Invalid MAC for WOL on proxy: {}: {}", mac_str_clone, e);
                                return;
                            }
                        };

                        if let Err(e) =
                            wol::send_packets(&mac, Ipv4Addr::new(255, 255, 255, 255), wol_port, 3)
                        {
                            error!("Failed to send WOL packet for {}: {}", mac_str_clone, e);
                            return;
                        }

                        info!(
                            "WOL packet sent. Waiting up to 60s for {} to become reachable...",
                            remote_addr_clone
                        );

                        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
                        let mut host_up = false;
                        while tokio::time::Instant::now() < deadline {
                            if wol::tcp_check(remote_addr_clone, connect_timeout) {
                                info!("Host {} is now up.", remote_addr_clone);
                                host_up = true;
                                break;
                            }
                            tokio::time::sleep(Duration::from_secs(2)).await;
                        }

                        if !host_up {
                            warn!(
                                "Timeout waiting for host {} to come up. Dropping connection from {}.",
                                remote_addr_clone, client_addr
                            );
                            return;
                        }
                    }

                    let mut outbound = match TcpStream::connect(remote_addr_clone).await {
                        Ok(stream) => stream,
                        Err(e) => {
                            error!("Failed to connect to remote {}: {}", remote_addr_clone, e);
                            return;
                        }
                    };

                    if let Err(e) = copy_bidirectional(&mut inbound, &mut outbound).await {
                        warn!(
                            "Error forwarding data between {} and {}: {}",
                            client_addr, remote_addr_clone, e
                        );
                    }
                });
            }
        }
    }
}
