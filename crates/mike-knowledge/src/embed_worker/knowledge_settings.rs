use crate::knowledge_settings::meta_knowledge_embedding as keys;
use crate::store::persistence::PersistHandle;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeSweepSettings {
    pub interval: Duration,
    pub budget: Duration,
    pub batch_sources: i64,
    pub concurrency: usize,
    pub chunks_per_call: usize,
}

impl Default for KnowledgeSweepSettings {
    fn default() -> Self {
        Self::from_rows(&[])
    }
}

fn int(rows: &[(String, serde_json::Value)], key: &str, default: i64, min: i64, max: i64) -> i64 {
    rows.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_i64())
        .unwrap_or(default)
        .clamp(min, max)
}

impl KnowledgeSweepSettings {
    pub fn from_rows(rows: &[(String, serde_json::Value)]) -> Self {
        let secs = |key, default: u64, min, max| {
            Duration::from_secs(int(rows, key, default as i64, min, max) as u64)
        };
        Self {
            interval: secs(keys::INTERVAL_SECS, keys::DEFAULT_INTERVAL_SECS, 5, 3_600),
            budget: secs(keys::BUDGET_SECS, keys::DEFAULT_BUDGET_SECS, 1, 600),
            batch_sources: int(
                rows,
                keys::BATCH_SOURCES,
                keys::DEFAULT_BATCH_SOURCES,
                16,
                256,
            ),
            concurrency: int(
                rows,
                keys::CONCURRENCY,
                keys::DEFAULT_CONCURRENCY as i64,
                1,
                16,
            ) as usize,
            chunks_per_call: int(
                rows,
                keys::CHUNKS_PER_CALL,
                keys::DEFAULT_CHUNKS_PER_CALL as i64,
                1,
                64,
            ) as usize,
        }
    }

    pub async fn load(persist: &PersistHandle) -> Self {
        match persist.fetch_settings().await {
            Ok(rows) => Self::from_rows(&rows),
            Err(error) => {
                tracing::warn!(%error, "knowledge embedding settings unreadable; using defaults");
                Self::default()
            }
        }
    }
}

pub const IDLE_WAIT_CEILING: Duration = Duration::from_secs(120);

pub fn next_wait(configured: Duration, previous: Duration, found_work: bool) -> Duration {
    if found_work {
        return configured;
    }
    previous
        .max(configured)
        .saturating_mul(2)
        .min(configured.max(IDLE_WAIT_CEILING))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn next_wait_empty_sweeps_double_up_to_the_ceiling() {
        let configured = Duration::from_secs(30);
        let second = next_wait(configured, configured, false);
        let third = next_wait(configured, second, false);
        assert_eq!(second, Duration::from_secs(60));
        assert_eq!(third, IDLE_WAIT_CEILING);
        assert_eq!(next_wait(configured, third, false), IDLE_WAIT_CEILING);
    }

    #[test]
    fn next_wait_found_work_returns_to_the_configured_interval() {
        let configured = Duration::from_secs(30);
        assert_eq!(next_wait(configured, IDLE_WAIT_CEILING, true), configured);
    }

    #[test]
    fn next_wait_configured_above_ceiling_is_kept() {
        let configured = Duration::from_secs(600);
        assert_eq!(next_wait(configured, configured, false), configured);
    }

    fn row(key: &str, value: serde_json::Value) -> (String, serde_json::Value) {
        (key.to_owned(), value)
    }

    #[test]
    fn no_rows_keep_the_behaviour_before_the_settings_existed() {
        let s = KnowledgeSweepSettings::default();
        assert_eq!(s.interval, Duration::from_secs(30));
        assert_eq!(s.budget, Duration::from_secs(10));
        assert_eq!(s.batch_sources, 16);
        assert_eq!(s.concurrency, 1);
        assert_eq!(s.chunks_per_call, 16);
    }

    #[test]
    fn stored_values_apply_and_out_of_range_values_are_clamped() {
        let s = KnowledgeSweepSettings::from_rows(&[
            row(keys::INTERVAL_SECS, json!(5)),
            row(keys::BUDGET_SECS, json!(100_000)),
            row(keys::BATCH_SOURCES, json!(64)),
            row(keys::CONCURRENCY, json!(0)),
            row(keys::CHUNKS_PER_CALL, json!("many")),
        ]);
        assert_eq!(s.interval, Duration::from_secs(5));
        assert_eq!(s.budget, Duration::from_secs(600));
        assert_eq!(s.batch_sources, 64);
        assert_eq!(s.concurrency, 1);
        assert_eq!(s.chunks_per_call, 16);
    }
}
