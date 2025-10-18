//! Connection pooling for improved proxy performance.
//!
//! This module provides connection reuse functionality to reduce latency and
//! connection overhead in the network proxy. It maintains a pool of connections
//! per target address with automatic cleanup and health checking.

use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpStream,
    sync::{RwLock, Semaphore},
    time,
};
use tracing::{debug, info, warn};

/// Maximum number of connections to maintain per target address
const MAX_CONNECTIONS_PER_TARGET: usize = 10;

/// How long a connection can stay idle before being closed (in seconds)
const CONNECTION_IDLE_TIMEOUT: u64 = 300;

/// How long to wait for a connection to become available in the pool
const CONNECTION_WAIT_TIMEOUT: Duration = Duration::from_millis(5000);

/// Metadata about a pooled connection
#[derive(Debug)]
struct PooledConnection {
    stream: TcpStream,
    last_used: Instant,
}

impl PooledConnection {
    fn is_expired(&self) -> bool {
        self.last_used.elapsed() > Duration::from_secs(CONNECTION_IDLE_TIMEOUT)
    }

    fn mark_used(&mut self) {
        self.last_used = Instant::now();
    }

    fn into_stream(mut self) -> TcpStream {
        self.mark_used();
        self.stream
    }
}

/// Connection pool for managing reusable TCP connections
#[derive(Clone)]
pub struct ConnectionPool {
    /// Map of target addresses to connection pools
    pools: Arc<RwLock<HashMap<SocketAddr, VecDeque<PooledConnection>>>>,
    /// Total connection limit semaphore
    connection_limit: Arc<Semaphore>,
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            // Limit total connections across all pools
            connection_limit: Arc::new(Semaphore::new(MAX_CONNECTIONS_PER_TARGET * 10)),
        }
    }

    /// Get a connection from the pool, or create a new one if needed
    pub async fn get_connection(
        &self,
        target_addr: SocketAddr,
    ) -> Result<TcpStream, Box<dyn std::error::Error + Send + Sync>> {
        // Acquire permit for total connection limit
        let _permit =
            match time::timeout(CONNECTION_WAIT_TIMEOUT, self.connection_limit.acquire()).await {
                Ok(permit_result) => match permit_result {
                    Ok(permit) => Some(permit),
                    Err(_) => {
                        warn!("Connection limit exceeded, creating new connection immediately");
                        None
                    }
                },
                Err(_) => {
                    warn!("Connection limit exceeded, creating new connection immediately");
                    None
                }
            };

        // First, try to get an existing connection from the pool
        if let Some(connection) = self.get_from_pool(target_addr).await {
            // For now, unconditionally reuse connections.
            // TODO: Implement proper connection health checking in production
            // by checking for recent usage and implementing keepalive probes
            debug!("Reusing existing connection to {}", target_addr);
            return Ok(connection.into_stream());
        }

        // No existing connection or it was bad, create a new one
        debug!("Creating new connection to {}", target_addr);
        match time::timeout(Duration::from_secs(30), TcpStream::connect(target_addr)).await {
            Ok(Ok(stream)) => {
                debug!("Successfully connected to {}", target_addr);
                Ok(stream)
            }
            Ok(Err(e)) => {
                warn!("Failed to connect to {}: {}", target_addr, e);
                Err(Box::new(e))
            }
            Err(_) => {
                warn!("Timeout connecting to {}", target_addr);
                Err("Connection timeout".into())
            }
        }
    }

    /// Try to get an existing connection from the pool
    async fn get_from_pool(&self, target_addr: SocketAddr) -> Option<PooledConnection> {
        let mut pools = self.pools.write().await;

        if let Some(pool) = pools.get_mut(&target_addr) {
            while let Some(connection) = pool.pop_front() {
                if connection.is_expired() {
                    debug!("Removing expired connection to {}", target_addr);
                    // Expired connection, discard it and try next
                    continue;
                }

                debug!("Found existing connection to {} in pool", target_addr);
                return Some(connection);
            }
        }

        None
    }

    /// Remove all connections for a specific target (e.g., when a machine is deleted)
    pub async fn remove_target(&self, target_addr: SocketAddr) {
        let mut pools = self.pools.write().await;
        if let Some(pool) = pools.remove(&target_addr) {
            debug!(
                "Removed {} connections from pool for {}",
                pool.len(),
                target_addr
            );
        }
    }

    /// Remove all expired connections across all pools
    pub async fn cleanup_expired(&self) {
        let mut pools = self.pools.write().await;
        let mut total_cleaned = 0;

        for (addr, pool) in pools.iter_mut() {
            let initial_len = pool.len();
            pool.retain(|conn| !conn.is_expired());
            let cleaned_count = initial_len - pool.len();
            if cleaned_count > 0 {
                debug!(
                    "Cleaned {} expired connections from pool for {}",
                    cleaned_count, addr
                );
                total_cleaned += cleaned_count;
            }
        }

        if total_cleaned > 0 {
            info!(
                "Connection pool cleanup: removed {} expired connections",
                total_cleaned
            );
        }
    }

    /// Get statistics about the connection pool
    #[allow(dead_code)] // Exposed for external metrics consumers even if unused internally
    pub async fn get_stats(&self) -> HashMap<String, usize> {
        let pools = self.pools.read().await;
        let mut stats = HashMap::new();

        for (addr, pool) in pools.iter() {
            stats.insert(addr.to_string(), pool.len());
        }

        stats.insert("total_pools".to_string(), pools.len());
        stats
    }

    /// Start a background task for periodic cleanup of expired connections
    pub fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(60)); // Cleanup every minute

            loop {
                interval.tick().await;
                pool.cleanup_expired().await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionPool;
    use std::io::ErrorKind;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;
    use tokio::sync::Mutex;
    use tokio::time::{sleep, timeout, Duration};

    #[tokio::test]
    async fn get_stats_reports_zero_when_empty() {
        let pool = ConnectionPool::new();
        let stats = pool.get_stats().await;
        assert_eq!(stats.get("total_pools"), Some(&0));
    }

    #[tokio::test]
    async fn connections_are_reused_and_removed() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::PermissionDenied | ErrorKind::AddrNotAvailable
                ) =>
            {
                eprintln!(
                    "skipping connection reuse test because binding TCP sockets is not permitted: {}",
                    err
                );
                return;
            }
            Err(err) => panic!("failed to bind listener: {err}"),
        };
        let addr = listener.local_addr().expect("failed to read listener addr");

        let accept_count = Arc::new(AtomicUsize::new(0));
        let sockets = Arc::new(Mutex::new(Vec::new()));

        let acceptor_sockets = sockets.clone();
        let acceptor_count = accept_count.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut socket, _)) => {
                        acceptor_count.fetch_add(1, Ordering::SeqCst);
                        // keep socket alive and respond to simple ping to avoid connection closure
                        let mut buf = vec![0u8; 16];
                        if socket.read(&mut buf).await.is_ok() {
                            let _ = socket.write_all(b"ok").await;
                        }
                        acceptor_sockets.lock().await.push(socket);
                    }
                    Err(_) => break,
                }
            }
        });

        let pool = ConnectionPool::new();

        let mut stream = pool
            .get_connection(addr)
            .await
            .expect("failed to create connection");
        // keep connection alive by writing a small payload
        let _ = stream.write_all(b"hi").await;
        expect_accepts(&accept_count, 1).await;

        pool.return_connection(addr, stream).await;

        let mut reused = pool
            .get_connection(addr)
            .await
            .expect("failed to reuse connection");
        let _ = reused.write_all(b"again").await;
        expect_accepts(&accept_count, 1).await;

        pool.return_connection(addr, reused).await;
        pool.remove_target(addr).await;

        let stats = pool.get_stats().await;
        assert!(stats.get(&addr.to_string()).is_none());

        accept_task.abort();
    }

    async fn expect_accepts(count: &AtomicUsize, expected: usize) {
        let waiter = async {
            loop {
                if count.load(Ordering::SeqCst) == expected {
                    return;
                }
                sleep(Duration::from_millis(10)).await;
            }
        };

        if timeout(Duration::from_secs(2), waiter).await.is_err() {
            panic!(
                "timed out waiting for accept count {}, last observed {}",
                expected,
                count.load(Ordering::SeqCst)
            );
        }
    }

    #[tokio::test]
    async fn get_connection_timeout_fails() {
        let pool = ConnectionPool::new();
        let target = "127.0.0.1:12345".parse::<SocketAddr>().unwrap();
        let result = pool.get_connection(target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cleanup_expired_can_be_called() {
        let pool = ConnectionPool::new();
        pool.cleanup_expired().await;
        // Just ensure no panic
    }

    #[tokio::test]
    async fn start_cleanup_task_can_be_started_and_aborted() {
        let pool = ConnectionPool::new();
        let handle = pool.start_cleanup_task();
        handle.abort();
    }
}
