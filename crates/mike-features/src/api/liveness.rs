use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::steerer::structural::wiring::last_pass;
use crate::steerer::structural::wiring_store::{StoredFinding, list_findings};

const DEFAULT_RESOLVED_DAYS: i64 = 7;
const MAX_RESOLVED_DAYS: i64 = 90;

#[derive(Debug, Default, Deserialize)]
pub struct LivenessQuery {
    #[serde(default)]
    pub resolved_days: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LivenessEntry {
    #[serde(flatten)]
    pub finding: StoredFinding,
    pub status: &'static str,
    pub age_days: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LivenessReport {
    pub generated_at: DateTime<Utc>,
    pub last_pass_at: Option<DateTime<Utc>>,
    pub open: usize,
    pub resolved: usize,
    pub findings: Vec<LivenessEntry>,
}

pub fn build_report(
    findings: Vec<StoredFinding>,
    now: DateTime<Utc>,
    last_pass_at: Option<DateTime<Utc>>,
) -> LivenessReport {
    let entries: Vec<LivenessEntry> = findings
        .into_iter()
        .map(|finding| {
            let ended = finding.resolved_at.unwrap_or(now);
            LivenessEntry {
                status: if finding.resolved_at.is_some() {
                    "resolved"
                } else {
                    "open"
                },
                age_days: (ended - finding.first_seen).num_days(),
                finding,
            }
        })
        .collect();
    let resolved = entries
        .iter()
        .filter(|entry| entry.status == "resolved")
        .count();
    LivenessReport {
        generated_at: now,
        last_pass_at,
        open: entries.len() - resolved,
        resolved,
        findings: entries,
    }
}

pub async fn get_liveness(
    State(state): State<AppState>,
    Query(query): Query<LivenessQuery>,
) -> Result<Json<LivenessReport>, StatusCode> {
    let Some(persist) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let days = query
        .resolved_days
        .unwrap_or(DEFAULT_RESOLVED_DAYS)
        .clamp(0, MAX_RESOLVED_DAYS);
    let now = Utc::now();
    let findings = list_findings(persist.pool(), now - Duration::days(days))
        .await
        .map_err(|error| {
            tracing::warn!(%error, "liveness: could not read the findings");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(build_report(findings, now, last_pass())))
}

#[cfg(test)]
#[path = "liveness_tests.rs"]
mod liveness_tests;
