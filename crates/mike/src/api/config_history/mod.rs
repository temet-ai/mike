use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::AppState;
use crate::store::persistence::change_log::{ChangeLogFilter, ChangeLogRow};

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;
const SNAPSHOT_KINDS: [&str; 3] = ["setting", "toggle", "steerer"];

#[derive(Debug, Deserialize)]
pub(crate) struct ChangeLogQuery {
    pub(crate) subject_kind: Option<String>,
    pub(crate) subject_key: Option<String>,
    pub(crate) subject_key_prefix: Option<String>,
    pub(crate) via: Option<String>,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) until: Option<DateTime<Utc>>,
    pub(crate) before_id: Option<i64>,
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChangeLogPage {
    pub(crate) changes: Vec<ChangeLogRow>,
    pub(crate) next_before_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SnapshotQuery {
    pub(crate) at: DateTime<Utc>,
    pub(crate) subject_kind: Option<String>,
}

fn bad_request(message: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "session persistence is disabled; the change log is not available"
        })),
    )
        .into_response()
}

fn persist_failed(e: &crate::store::persistence::PersistError) -> Response {
    tracing::warn!(error = %e, "config history read failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": format!("persistence error: {e}") })),
    )
        .into_response()
}

pub(crate) fn page_of(mut rows: Vec<ChangeLogRow>, limit: i64) -> ChangeLogPage {
    let limit = usize::try_from(limit).unwrap_or(0);
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_before_id = if has_more {
        rows.last().map(|row| row.id)
    } else {
        None
    };
    ChangeLogPage {
        changes: rows,
        next_before_id,
    }
}

pub(crate) async fn list_config_changes(
    State(state): State<AppState>,
    Query(query): Query<ChangeLogQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return bad_request(format!(
            "limit must be between 1 and {MAX_LIMIT}, got {limit}"
        ));
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = ChangeLogFilter {
        subject_kind: query.subject_kind,
        subject_key: query.subject_key,
        subject_key_prefix: query.subject_key_prefix,
        via: query.via,
        since: query.since,
        until: query.until,
        before_id: query.before_id,
        limit: limit + 1,
    };
    match persist.list_config_changes(&filter).await {
        Ok(rows) => Json(page_of(rows, limit)).into_response(),
        Err(e) => persist_failed(&e),
    }
}

pub(crate) async fn get_config_snapshot(
    State(state): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> Response {
    if let Some(kind) = query.subject_kind.as_deref()
        && !SNAPSHOT_KINDS.contains(&kind)
    {
        return bad_request(format!(
            "subject_kind must be one of setting, toggle or steerer, got {kind:?}"
        ));
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist
        .config_snapshot_at(query.at, query.subject_kind.as_deref())
        .await
    {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(e) => persist_failed(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64) -> ChangeLogRow {
        ChangeLogRow {
            id,
            changed_at: Utc::now(),
            xact_id: "1".to_owned(),
            subject_kind: "setting".to_owned(),
            subject_key: "k".to_owned(),
            op: "update".to_owned(),
            old_value: None,
            new_value: None,
            redacted: false,
            via: "api".to_owned(),
            actor: None,
            request_id: None,
            build_sha: None,
            db_user: "u".to_owned(),
            application_name: None,
            detail: serde_json::json!({}),
        }
    }

    #[test]
    fn page_of_sets_next_before_id_only_when_more_rows_exist() {
        let full = page_of(vec![row(5), row(4), row(3)], 2);
        assert_eq!(full.changes.len(), 2);
        assert_eq!(full.next_before_id, Some(4));
        let last = page_of(vec![row(2), row(1)], 2);
        assert_eq!(last.next_before_id, None);
    }
}
