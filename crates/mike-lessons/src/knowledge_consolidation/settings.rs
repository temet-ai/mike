use crate::knowledge_settings::meta_knowledge_consolidation as keys;
use crate::store::persistence::PersistHandle;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub interval: Duration,
    pub items_per_tick: i64,
    pub subjects_per_tick: i64,
    pub settle: chrono::Duration,
    pub max_judgements_per_day: i64,
    pub model: String,
    pub grouping_similarity: f32,
    pub edit_files: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_rows(&[])
    }
}

type Rows = [(String, serde_json::Value)];

fn find<'a>(rows: &'a Rows, key: &str) -> Option<&'a serde_json::Value> {
    rows.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn int(rows: &Rows, key: &str, default: i64, min: i64, max: i64) -> i64 {
    find(rows, key)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(default)
        .clamp(min, max)
}

impl Settings {
    pub fn from_rows(rows: &Rows) -> Self {
        Self {
            interval: Duration::from_secs(
                int(rows, keys::INTERVAL_SECS, keys::DEFAULT_INTERVAL_SECS, 10, 3_600) as u64,
            ),
            items_per_tick: int(rows, keys::ITEMS_PER_TICK, keys::DEFAULT_ITEMS_PER_TICK, 1, 64),
            subjects_per_tick: int(
                rows,
                keys::SUBJECTS_PER_TICK,
                keys::DEFAULT_SUBJECTS_PER_TICK,
                1,
                32,
            ),
            settle: chrono::Duration::seconds(int(
                rows,
                keys::SETTLE_SECS,
                keys::DEFAULT_SETTLE_SECS,
                0,
                86_400,
            )),
            max_judgements_per_day: int(
                rows,
                keys::MAX_JUDGEMENTS_PER_DAY,
                keys::DEFAULT_MAX_JUDGEMENTS_PER_DAY,
                0,
                100_000,
            ),
            model: find(rows, keys::MODEL)
                .and_then(serde_json::Value::as_str)
                .map(|m| m.trim().to_owned())
                .unwrap_or_default(),
            grouping_similarity: find(rows, keys::GROUPING_SIMILARITY)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(keys::DEFAULT_GROUPING_SIMILARITY)
                .clamp(0.5, 0.95) as f32,
            edit_files: find(rows, keys::EDIT_FILES)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }
    }

    pub async fn load(persist: &PersistHandle) -> Self {
        match persist.fetch_settings().await {
            Ok(rows) => Self::from_rows(&rows),
            Err(error) => {
                tracing::warn!(%error, "knowledge consolidation settings unreadable; using defaults");
                Self::default()
            }
        }
    }
}
