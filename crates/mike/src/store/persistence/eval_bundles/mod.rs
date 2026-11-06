mod read;
mod swap;
#[cfg(test)]
mod swap_tests;
mod write;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use read::BundleQuery;
pub use swap::{LiveModelSwap, SwapRequest};
pub use write::{BuildOrigin, EnsuredBundle, NewBundleRow, StatusChange};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct BundleRow {
    pub id: i64,
    pub role: String,
    pub subject_key: String,
    pub origin: String,
    pub status: String,
    pub components: serde_json::Value,
    pub content_sha256: String,
    pub build_sha: String,
    pub build_dirty: bool,
    pub cause: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    pub superseded_by_id: Option<i64>,
    pub gate_run_id: Option<i64>,
    pub promoted_at: Option<DateTime<Utc>>,
    pub override_reason: Option<String>,
}

pub(crate) const BUNDLE_COLUMNS: &str = "id, role, subject_key, origin, status, components, \
     content_sha256, build_sha, build_dirty, cause, created_at, superseded_at, superseded_by_id, \
     gate_run_id, promoted_at, override_reason";
