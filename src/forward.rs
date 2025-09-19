use crate::connection_pool::ConnectionPool;
use crate::{web::Machine, wol};
use anyhow::{Context, Result};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

fn turn_off_url(remote_ip: &str, turn_off_port: u16) -> String {
    format!("http://{}:{}/machines/turn-off", remote_ip, turn_off_port)
}

pub async fn turn_off_remote_machine(
    remote_ip: &str,
    turn_off_port: u16,
) -> Result<(), reqwest::Error> {
    let url = turn_off_url(remote_ip, turn_off_port);
    info!("Sending turn-off signal to {}", url);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?;

    let response = client.post(&url).send().await?;
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
    connection_pool: ConnectionPool,
) -> Result<()> {
    let listen_addr = format!("0.0.0.0:{}", local_port);
    let listener = TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("Failed to bind TCP listener on {}", listen_addr))?;
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
                let (mut inbound, client_addr) = result
                    .context("Failed to accept incoming connection")?;
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

                let connection_pool_clone = connection_pool.clone();
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

                        let broadcast_addr = Ipv4Addr::new(255, 255, 255, 255);
                        let wol_config = Default::default();
                        if let Err(e) = crate::wol::send_packets(&mac, broadcast_addr, wol_port, 3, &wol_config).await {
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

                    let mut outbound = match connection_pool_clone.get_connection(remote_addr_clone).await {
                        Ok(stream) => {
                            debug!("Successfully obtained or created connection to {}", remote_addr_clone);
                            stream
                        }
                        Err(e) => {
                            error!("Failed to obtain connection to remote {}: {}", remote_addr_clone, e);
                            return;
                        }
                    };

                    if let Err(e) = copy_bidirectional(&mut inbound, &mut outbound).await {
                        connection_pool_clone.return_connection(remote_addr_clone, outbound).await;
                        warn!(
                            "Error forwarding data between {} and {}: {}",
                            client_addr, remote_addr_clone, e
                        );
                    } else {
                        connection_pool_clone.return_connection(remote_addr_clone, outbound).await;
                        debug!("Successfully completed data transfer for {}", remote_addr_clone);
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[test]
    fn turn_off_url_formats_expected_path() {
        let url = super::turn_off_url("192.168.1.10", 8080);
        assert_eq!(url, "http://192.168.1.10:8080/machines/turn-off");
    }

    #[tokio::test]
    async fn turn_off_remote_machine_sends_expected_request() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::PermissionDenied | ErrorKind::AddrNotAvailable
                ) =>
            {
                eprintln!(
                    "skipping test because binding TCP sockets is not permitted: {}",
                    err
                );
                return;
            }
            Err(err) => panic!("failed to bind http test listener: {err}"),
        };
        let addr = listener.local_addr().expect("failed to read listener addr");

        let received = Arc::new(Mutex::new(None));
        let received_clone = received.clone();

        let server_task = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                if let Ok(n) = socket.read(&mut buf).await {
                    if n > 0 {
                        let request = String::from_utf8_lossy(&buf[..n]).to_string();
                        *received_clone.lock().await = Some(request);
                    }
                }
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            }
        });

        turn_off_remote_machine(&addr.ip().to_string(), addr.port())
            .await
            .expect("turn_off_remote_machine should succeed");

        server_task.await.expect("server task panicked");

        let request = received.lock().await.clone().expect("no request captured");
        assert!(request.starts_with("POST /machines/turn-off"));

        let host_line = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("host:"))
            .unwrap_or_else(|| panic!("Host header missing in request: {request}"));

        let host_value = host_line.split_once(':').map(|(_, value)| value.trim());
        let expected_ip = addr.ip().to_string();
        let expected_with_port = format!("{}:{}", expected_ip, addr.port());
        assert!(
            matches!(host_value, Some(value) if value.eq_ignore_ascii_case(&expected_ip) || value.eq_ignore_ascii_case(&expected_with_port)),
            "unexpected host header: {host_line}"
        );
    }
}
