//! The newest captured request's `created_at`, kept in memory. `/api/health`
//! reads it on every call (proxy-forward probes health about once a second),
//! so it is read from `requests.requests` only until a value is known; after
//! that each request this process commits moves it forward.
//!
//! Once known, it counts only this process's captures, which is what the
//! capture-stall check means: requests reached this proxy but none was written.
//! Another machine writing to the shared database no longer hides a stall here.

use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{PersistError, PersistHandle};

#[derive(Default)]
struct LatestCaptureWrite(Mutex<Option<DateTime<Utc>>>);

impl LatestCaptureWrite {
    fn get(&self) -> Option<DateTime<Utc>> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn advance(&self, at: DateTime<Utc>) {
        let mut kept = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if kept.is_none_or(|current| current < at) {
            *kept = Some(at);
        }
    }
}

impl PersistHandle {
    /// Records that a request created at `at` was committed to `requests.requests`.
    pub fn note_capture_write(&self, at: DateTime<Utc>) {
        self.memo::<LatestCaptureWrite>().advance(at);
    }

    pub async fn latest_capture_write_at(&self) -> Result<Option<DateTime<Utc>>, PersistError> {
        let memo = self.memo::<LatestCaptureWrite>();
        if let Some(at) = memo.get() {
            return Ok(Some(at));
        }
        let newest = self.read_latest_capture_write_at().await?;
        if let Some(at) = newest {
            memo.advance(at);
        }
        Ok(memo.get())
    }

    async fn read_latest_capture_write_at(&self) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("latest_capture_write_at", async move {
            let ts = sqlx::query_scalar!("SELECT MAX(created_at) FROM requests.requests")
                .fetch_one(&self.pool)
                .await?;
            Ok(ts)
        })
        .await
    }
}
