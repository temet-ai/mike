use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};

use sqlx::PgPool;

static POOLS: OnceLock<Vec<(&'static str, PgPool)>> = OnceLock::new();
static IN_FLIGHT: LazyLock<Mutex<BTreeMap<String, usize>>> = LazyLock::new(Mutex::default);

pub fn register_pools(pools: &[(&'static str, PgPool)]) {
    POOLS.get_or_init(|| pools.to_vec());
}

#[derive(Debug)]
pub struct InFlight(String);

impl InFlight {
    pub fn start(method: &str, route: &str) -> Self {
        let key = format!("{method} {route}");
        let mut running = IN_FLIGHT.lock().unwrap_or_else(PoisonError::into_inner);
        *running.entry(key.clone()).or_default() += 1;
        Self(key)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let mut running = IN_FLIGHT.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = running.get_mut(&self.0) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                running.remove(&self.0);
            }
        }
    }
}

pub fn describe() -> String {
    let pools = POOLS.get().map(Vec::as_slice).unwrap_or_default();
    let mut parts: Vec<String> = pools
        .iter()
        .map(|(class, pool)| {
            let max = pool.options().get_max_connections();
            format!(
                "{class}: {} of {max} open, {} idle",
                pool.size(),
                pool.num_idle()
            )
        })
        .collect();
    let running = IN_FLIGHT.lock().unwrap_or_else(PoisonError::into_inner);
    parts.extend(
        running
            .iter()
            .map(|(route, n)| format!("in flight {n}x {route}")),
    );
    parts.join("; ")
}

#[cfg(test)]
#[path = "db_pressure_tests.rs"]
mod tests;
