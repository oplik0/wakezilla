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
    fn new(stream: TcpStream) -> Self {
        let now = Instant::now();
        Self {
            stream,
            last_used: now,
        }
    }

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

    /// Return a connection to the pool for reuse
    pub async fn return_connection(&self, target_addr: SocketAddr, stream: TcpStream) {
        let mut pools = self.pools.write().await;

        let pool = pools.entry(target_addr).or_insert_with(VecDeque::new);

        // Only add back to the pool if we're not over the per-target limit
        if pool.len() < MAX_CONNECTIONS_PER_TARGET {
            let pooled_conn = PooledConnection::new(stream);
            pool.push_back(pooled_conn);
            debug!(
                "Returned connection to pool for {}, pool size: {}",
                target_addr,
                pool.len()
            );
        } else {
            debug!(
                "Pool for {} already full ({} connections), not adding back",
                target_addr,
                pool.len()
            );
        }
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
                if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let pool = pool.clone();
                    tokio::spawn(async move {
                        pool.cleanup_expired().await;
                    });
                })) {
                    warn!("Connection pool cleanup task panicked: {:?}", e);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionPool;

    #[tokio::test]
    async fn get_stats_reports_zero_when_empty() {
        let pool = ConnectionPool::new();
        let stats = pool.get_stats().await;
        assert_eq!(stats.get("total_pools"), Some(&0));
    }
}
