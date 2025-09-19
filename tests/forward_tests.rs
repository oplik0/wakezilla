use std::io::ErrorKind;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use wakezilla::forward;

#[tokio::test]
async fn turn_off_remote_machine_sends_post_request() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == ErrorKind::PermissionDenied => {
            eprintln!(
                "skipping forward test because binding TCP sockets is not permitted: {}",
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

    forward::turn_off_remote_machine(&addr.ip().to_string(), addr.port())
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
