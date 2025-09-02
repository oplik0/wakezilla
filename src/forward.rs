use std::io;
use std::net::SocketAddr;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};

pub async fn proxy(local_port: u16, remote_addr: SocketAddr) -> io::Result<()> {
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
        tokio::spawn(async move {
            let mut outbound = match TcpStream::connect(remote_addr_clone).await {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!(
                        "Failed to connect to remote {}: {}",
                        remote_addr_clone, e
                    );
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
