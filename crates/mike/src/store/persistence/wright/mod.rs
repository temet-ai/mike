use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

mod checks;
mod problems;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct CheckRow {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) cadence: String,
    pub(crate) expects: String,
    pub(crate) probe: String,
    pub(crate) query: Option<String>,
    pub(crate) params: Value,
    pub(crate) enabled: bool,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewCheck {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) cadence: String,
    pub(crate) expects: String,
    pub(crate) probe: String,
    pub(crate) query: Option<String>,
    pub(crate) params: Value,
    pub(crate) enabled: bool,
}

impl From<CheckRow> for NewCheck {
    fn from(row: CheckRow) -> Self {
        Self {
            name: row.name,
            source: row.source,
            cadence: row.cadence,
            expects: row.expects,
            probe: row.probe,
            query: row.query,
            params: row.params,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub(crate) struct RecordedRun {
    pub(crate) run_id: i64,
    pub(crate) opened: i64,
    pub(crate) recurred: i64,
    pub(crate) still_open: i64,
    pub(crate) healthy: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProblemFilter {
    pub(crate) state: Option<String>,
    pub(crate) check: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct ProblemRow {
    pub(crate) id: i64,
    pub(crate) check_name: String,
    pub(crate) source: String,
    pub(crate) scope: String,
    pub(crate) resource: String,
    pub(crate) state: String,
    pub(crate) opened_at: DateTime<Utc>,
    pub(crate) last_seen_at: DateTime<Utc>,
    pub(crate) healthy_at: Option<DateTime<Utc>>,
    pub(crate) episodes: i64,
    pub(crate) latest_evidence: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct EpisodeRow {
    pub(crate) id: i64,
    pub(crate) opened_at: DateTime<Utc>,
    pub(crate) last_seen_at: DateTime<Utc>,
    pub(crate) closed_at: Option<DateTime<Utc>>,
    pub(crate) opened_by_run: i64,
    pub(crate) last_seen_run: i64,
    pub(crate) closed_by_run: Option<i64>,
    pub(crate) observations: i32,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct CheckRunRow {
    pub(crate) id: i64,
    pub(crate) check_name: String,
    pub(crate) source: String,
    pub(crate) cadence: String,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) finished_at: DateTime<Utc>,
    pub(crate) outcome: String,
    pub(crate) findings: i32,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ProblemPage {
    pub(crate) problems: Vec<ProblemRow>,
    pub(crate) total: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct NewClientError {
    pub(crate) route: String,
    pub(crate) kind: String,
    pub(crate) message: String,
    pub(crate) detail: Value,
}
