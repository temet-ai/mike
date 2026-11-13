use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::RwLock;
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use super::names::{GAUGE_DB_POOL_IDLE, GAUGE_DB_POOL_SIZE, GAUGE_STORE_BYTES, GAUGE_STORE_SIZE};
use crate::store::RequestStore;

pub fn status_bucket(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

pub fn spawn_store_gauge_updater(
    store: Arc<RwLock<RequestStore>>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let guard = store.read().await;
                    let size = guard.len();
                    let bytes = guard.total_bytes();
                    drop(guard);
                    metrics::gauge!(GAUGE_STORE_SIZE).set(size as f64);
                    metrics::gauge!(GAUGE_STORE_BYTES).set(bytes as f64);
                }
                _ = cancel.cancelled() => {
                    tracing::debug!("store gauge updater cancelled");
                    break;
                }
            }
        }
    })
}

pub fn spawn_db_pool_gauge_updater(
    pools: Vec<(&'static str, PgPool)>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    for (class, pool) in &pools {
                        metrics::gauge!(GAUGE_DB_POOL_SIZE, "class" => *class).set(pool.size() as f64);
                        metrics::gauge!(GAUGE_DB_POOL_IDLE, "class" => *class).set(pool.num_idle() as f64);
                    }
                }
                _ = cancel.cancelled() => {
                    tracing::debug!("db pool gauge updater cancelled");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn status_bucket_maps_codes_to_expected_labels() {
        assert_eq!(status_bucket(200), "2xx");
        assert_eq!(status_bucket(204), "2xx");
        assert_eq!(status_bucket(301), "3xx");
        assert_eq!(status_bucket(404), "4xx");
        assert_eq!(status_bucket(429), "4xx");
        assert_eq!(status_bucket(500), "5xx");
        assert_eq!(status_bucket(529), "5xx");
        assert_eq!(status_bucket(100), "other");
        assert_eq!(status_bucket(999), "other");
    }
}
