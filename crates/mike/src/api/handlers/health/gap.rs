use super::stored::{Stored, StoredSections};
use super::*;
use crate::store::persistence::PersistHandle;
use chrono::{DateTime, Utc};

pub(crate) const CAPTURE_GAP_SILENCE_SECS: i64 = 300;
pub(crate) const CAPTURE_GAP_SIDECAR_RECENT_SECS: i64 = 120;
pub(crate) const CAPTURE_GAP_LOOKBACK_SECS: i64 = 1800;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionActivity {
    pub(crate) session_id: String,
    pub(crate) first_seen_at: DateTime<Utc>,
    pub(crate) last_work_at: Option<DateTime<Utc>>,
    pub(crate) last_sidecar_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct CaptureGap {
    pub(crate) session_id: String,
    pub(crate) last_work_at: Option<String>,
    pub(crate) last_sidecar_at: String,
    pub(crate) work_silent_secs: i64,
}

#[derive(Serialize)]
pub(crate) struct CaptureGapHealth {
    pub(crate) silence_threshold_secs: i64,
    pub(crate) sidecar_recent_secs: i64,
    pub(crate) lookback_secs: i64,
    pub(crate) alarm: bool,
    pub(crate) sessions: Vec<CaptureGap>,
    pub(crate) reading_age_secs: Option<i64>,
}

fn secs_between(earlier: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - earlier).num_seconds().max(0)
}

fn gap_of(activity: &SessionActivity, now: DateTime<Utc>) -> Option<CaptureGap> {
    let sidecar_at = activity.last_sidecar_at?;
    if secs_between(sidecar_at, now) > CAPTURE_GAP_SIDECAR_RECENT_SECS {
        return None;
    }
    let silent_since = activity.last_work_at.unwrap_or(activity.first_seen_at);
    let silent_secs = secs_between(silent_since, now);
    if silent_secs <= CAPTURE_GAP_SILENCE_SECS {
        return None;
    }
    Some(CaptureGap {
        session_id: activity.session_id.clone(),
        last_work_at: activity.last_work_at.map(|at| at.to_rfc3339()),
        last_sidecar_at: sidecar_at.to_rfc3339(),
        work_silent_secs: silent_secs,
    })
}

pub(crate) fn find_gaps(activity: &[SessionActivity], now: DateTime<Utc>) -> Vec<CaptureGap> {
    let mut gaps: Vec<CaptureGap> = activity
        .iter()
        .filter_map(|session| gap_of(session, now))
        .collect();
    gaps.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    gaps
}

pub(crate) fn capture_gap_health(
    reading: Option<&Stored<Vec<SessionActivity>>>,
    now: DateTime<Utc>,
) -> CaptureGapHealth {
    let sessions = reading
        .map(|section| find_gaps(&section.value, now))
        .unwrap_or_default();
    CaptureGapHealth {
        silence_threshold_secs: CAPTURE_GAP_SILENCE_SECS,
        sidecar_recent_secs: CAPTURE_GAP_SIDECAR_RECENT_SECS,
        lookback_secs: CAPTURE_GAP_LOOKBACK_SECS,
        alarm: !sessions.is_empty(),
        sessions,
        reading_age_secs: reading.map(|section| section.age_secs(now)),
    }
}

pub(super) fn current(stored: &StoredSections, now: DateTime<Utc>) -> CaptureGapHealth {
    capture_gap_health(stored.capture_activity.as_ref(), now)
}

pub(crate) fn apply_gap_alarm(status: &'static str, alarm: bool) -> &'static str {
    if alarm { "degraded" } else { status }
}

pub(super) async fn read_activity(
    persist: &PersistHandle,
    now: DateTime<Utc>,
) -> Option<Vec<SessionActivity>> {
    let since = now - chrono::Duration::seconds(CAPTURE_GAP_LOOKBACK_SECS);
    let rows = sqlx::query!(
        r#"SELECT session_uuid::text AS "session_id!",
                  MIN(created_at) AS "first_seen_at!",
                  MAX(created_at) FILTER (
                      WHERE turn_origin IN ('primary', 'primary_first', 'subagent', 'workflow_subagent')
                  ) AS last_work_at,
                  MAX(created_at) FILTER (WHERE turn_origin LIKE '%\_sidecar') AS last_sidecar_at
           FROM requests.requests_v
           WHERE created_at > $1 AND session_uuid IS NOT NULL
           GROUP BY session_uuid
           HAVING bool_or(turn_origin LIKE '%\_sidecar')"#,
        since
    )
    .fetch_all(persist.pool())
    .await
    .map_err(|error| {
        tracing::warn!(%error, "health: failed to read per-session capture activity");
    })
    .ok()?;
    Some(
        rows.into_iter()
            .map(|row| SessionActivity {
                session_id: row.session_id,
                first_seen_at: row.first_seen_at,
                last_work_at: row.last_work_at,
                last_sidecar_at: row.last_sidecar_at,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
