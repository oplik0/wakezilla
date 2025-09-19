use std::io::ErrorKind;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration};

use wakezilla::connection_pool::ConnectionPool;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_pool_reuses_and_removes_connections() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == ErrorKind::PermissionDenied => {
            eprintln!(
                "skipping connection pool test because binding TCP sockets is not permitted: {}",
                err
            );
            return;
        }
        Err(err) => panic!("failed to bind listener for connection pool test: {err}"),
    };
    let addr = listener.local_addr().expect("failed to read listener addr");

    let accept_count = Arc::new(AtomicUsize::new(0));
    let sockets = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let acceptor_sockets = sockets.clone();
    let acceptor_count = accept_count.clone();
    let accept_task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((socket, _)) => {
                    acceptor_count.fetch_add(1, Ordering::SeqCst);
                    acceptor_sockets.lock().await.push(socket);
                }
                Err(_) => break,
            }
        }
    });

    let pool = ConnectionPool::new();

    let stream = pool
        .get_connection(addr)
        .await
        .expect("failed to establish initial connection");
    expect_accepts(&accept_count, 1).await;

    pool.return_connection(addr, stream).await;

    let reused = pool
        .get_connection(addr)
        .await
        .expect("failed to reuse connection from pool");
    expect_accepts(&accept_count, 1).await;

    pool.return_connection(addr, reused).await;

    let stats = pool.get_stats().await;
    let addr_key = addr.to_string();
    assert_eq!(stats.get(&addr_key), Some(&1));
    assert_eq!(stats.get("total_pools"), Some(&1));

    pool.remove_target(addr).await;

    let _fresh = pool
        .get_connection(addr)
        .await
        .expect("failed to establish new connection after removal");
    expect_accepts(&accept_count, 2).await;

    let stats_after_remove = pool.get_stats().await;
    assert_eq!(stats_after_remove.get(&addr_key), None);
    assert_eq!(stats_after_remove.get("total_pools"), Some(&0));

    accept_task.abort();
}

async fn expect_accepts(count: &AtomicUsize, expected: usize) {
    let wait_until = async {
        loop {
            if count.load(Ordering::SeqCst) == expected {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
    };

    if timeout(Duration::from_secs(2), wait_until).await.is_err() {
        let actual = count.load(Ordering::SeqCst);
        panic!("timed out waiting for accept count to reach {expected}, last observed {actual}");
    }
}
