use crate::{web::Machine, wol};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

pub async fn turn_off_remote_machine(
    remote_ip: &str,
    turn_off_port: u16,
) -> Result<(), reqwest::Error> {
    let url = format!("http://{}:{}/machines/turn-off", remote_ip, turn_off_port);
    info!("Sending turn-off signal to {}", url);
    let response = reqwest::Client::new().post(&url).send().await?;
    if response.status().is_success() {
        info!(
            "Successfully sent turn-off signal to {}:{}",
            remote_ip, turn_off_port
        );
    } else {
        error!(
            "Failed to send turn-off signal to {}:{}, status: {}",
            remote_ip,
            turn_off_port,
            response.status()
        );
    }
    Ok(())
}
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
        "TCP Forwarder listening on {}, proxying to {}, rate limit: {}/{}min",
        listen_addr,
        remote_addr,
        machine.request_rate.max_requests,
        machine.request_rate.period_minutes
    );

    let last_request_time = Arc::new(Mutex::new(Instant::now()));

    if machine.request_rate.max_requests > 0 {
        let last_request_time = Arc::clone(&last_request_time);
        if let Some(port) = machine.turn_off_port {
            let turn_off_port = port;
            let remote_ip = machine.ip;
            let mac = machine.mac.clone();
            let amount_req = machine.request_rate.max_requests;
            let per_minutes = machine.request_rate.period_minutes;

            tokio::spawn(async move {
                let mut count = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let elapsed = {
                        let last_time = last_request_time.lock().unwrap();
                        last_time.elapsed()
                    };
                    debug!(
                        "checking for inactivity for machine {} ({}), elapsed: {:?}, per_minutes: {}, amount_req: {}",
                        remote_ip, mac, elapsed, per_minutes, amount_req
                    );
                    if elapsed > Duration::from_secs(per_minutes as u64 * 60) {
                        count += 1;
                        if count >= amount_req {
                            if let Err(e) =
                                turn_off_remote_machine(&remote_ip.to_string(), turn_off_port).await
                            {
                                error!("Failed to send turn-off signal: {}", e);
                            }
                            break;
                        }
                    } else {
                        count = 0;
                    }
                }
            });
        }
    } else {
        info!("Machine {} cannot be turned off automatically.", machine.ip);
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

                if machine.request_rate.max_requests > 0 {
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
