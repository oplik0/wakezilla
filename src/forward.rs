use crate::connection_pool::ConnectionPool;
use crate::{web::Machine, wol};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
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

#[derive(Clone)]
struct TurnOffLimiter {
    request_times: Arc<Mutex<VecDeque<Instant>>>,
    max_requests: usize,
    window: Duration,
    turn_off_port: u16,
    remote_ip: Ipv4Addr,
    mac: String,
    triggered: Arc<AtomicBool>,
}

impl TurnOffLimiter {
    fn new(machine: &Machine, turn_off_port: u16) -> Self {
        let window_minutes = machine.request_rate.period_minutes.max(1);
        let window_secs = window_minutes.saturating_mul(60);
        Self {
            request_times: Arc::new(Mutex::new(VecDeque::new())),
            max_requests: machine.request_rate.max_requests as usize,
            window: Duration::from_secs(window_secs as u64),
            turn_off_port,
            remote_ip: machine.ip,
            mac: machine.mac.clone(),
            triggered: Arc::new(AtomicBool::new(false)),
        }
    }

    fn record_request(&self) -> Option<usize> {
        if self.max_requests == 0 {
            return None;
        }

        let now = Instant::now();
        let mut times = self.request_times.lock().unwrap();
        times.push_back(now);

        while let Some(oldest) = times.front() {
            if now.duration_since(*oldest) > self.window {
                times.pop_front();
            } else {
                break;
            }
        }

        let current = times.len();
        if current >= self.max_requests && !self.triggered.swap(true, Ordering::SeqCst) {
            Some(current)
        } else {
            None
        }
    }
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

    let rate_limiter = if machine.can_be_turned_off {
        if let Some(port) = machine.turn_off_port {
            if machine.request_rate.max_requests > 0 {
                Some(TurnOffLimiter::new(&machine, port))
            } else {
                info!(
                    "Machine {} has rate limit disabled (max_requests = 0)",
                    machine.mac
                );
                None
            }
        } else {
            debug!(
                "Turn off port not configured for {}, skipping rate-based shutdown",
                machine.mac
            );
            None
        }
    } else {
        info!(
            "Machine {} cannot be turned off automatically (feature disabled)",
            machine.mac
        );
        None
    };

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

                let remote_addr_clone = remote_addr;
                let mac_str_clone = machine.mac.clone();
                let rate_limiter = rate_limiter.clone();

                let connection_pool_clone = connection_pool.clone();
                tokio::spawn(async move {
                    if let Some(limiter) = rate_limiter.clone() {
                        if let Some(hit_count) = limiter.record_request() {
                            let remote_ip = limiter.remote_ip.to_string();
                            let turn_off_port = limiter.turn_off_port;
                            let mac = limiter.mac.clone();
                            let window = limiter.window;
                            tokio::spawn(async move {
                                info!(
                                    "Request limit reached for {}: {} requests within {:?}, sending turn-off signal",
                                    mac, hit_count, window
                                );
                                if let Err(e) = turn_off_remote_machine(&remote_ip, turn_off_port).await {
                                    error!(
                                        "Failed to send turn-off signal for {} on {}:{}: {}",
                                        mac, remote_ip, turn_off_port, e
                                    );
                                }
                            });
                        }
                    }

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

                    match copy_bidirectional(&mut inbound, &mut outbound).await {
                        Ok(_) => {
                            // Most targets close the connection after each request.
                            // Drop the stream instead of reusing a socket that is very
                            // likely already shut down by the remote endpoint.
                            drop(outbound);
                            debug!(
                                "Completed data transfer for {} (connection closed)",
                                remote_addr_clone
                            );
                        }
                        Err(e) => {
                            // Drop the broken connection so it isn't re-used from the pool.
                            drop(outbound);
                            connection_pool_clone.remove_target(remote_addr_clone).await;
                            warn!(
                                "Error forwarding data between {} and {}: {}",
                                client_addr, remote_addr_clone, e
                            );
                        }
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
