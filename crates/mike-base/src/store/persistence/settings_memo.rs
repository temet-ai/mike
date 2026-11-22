//! Single setting values read from `config.settings`, kept per key until the
//! config listener reloads. Every write to `config.settings` fires the
//! `mike.config_changed` NOTIFY (a statement trigger), and the listener calls
//! [`PersistHandle::settings_changed`] on each one and whenever it connects
//! again, so a kept value is never older than the overlay itself. Until the
//! listener first reloads, every read goes to the database.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dashmap::DashMap;

use super::{PersistError, PersistHandle};

const CAPACITY: usize = 1024;

#[derive(Default)]
struct SettingValues {
    live: AtomicBool,
    generation: AtomicU64,
    values: DashMap<String, Option<serde_json::Value>>,
}

impl PersistHandle {
    /// Drops every kept setting value; the config listener calls this on each
    /// reload, which also turns the memo on.
    pub fn settings_changed(&self) {
        let memo = self.memo::<SettingValues>();
        memo.generation.fetch_add(1, Ordering::AcqRel);
        memo.values.clear();
        memo.live.store(true, Ordering::Release);
    }

    pub async fn setting_value(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, PersistError> {
        let memo = self.memo::<SettingValues>();
        let live = memo.live.load(Ordering::Acquire);
        if live && let Some(value) = memo.values.get(key) {
            return Ok(value.clone());
        }
        let generation = memo.generation.load(Ordering::Acquire);
        let value = self.read_setting_value(key).await?;
        if live && memo.generation.load(Ordering::Acquire) == generation {
            if memo.values.len() >= CAPACITY {
                memo.values.clear();
            }
            memo.values.insert(key.to_owned(), value.clone());
        }
        Ok(value)
    }

    async fn read_setting_value(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, PersistError> {
        crate::metrics::time_db("setting_value", async move {
            let value =
                sqlx::query_scalar!("SELECT value FROM config.settings WHERE key = $1", key)
                    .fetch_optional(&self.pool)
                    .await?;
            Ok(value)
        })
        .await
    }
}

#[cfg(test)]
#[path = "settings_memo_tests.rs"]
mod tests;
