use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::rerank_page::{MAX_CANDIDATE_ROWS, QueryVector};
use crate::api::rerank_search::RerankReport;
use crate::config::AppState;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ProjectIdentity, search_page};
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) async fn search(
    state: &AppState,
    handle: &PersistHandle,
    project: &ProjectIdentity,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
) -> Result<(Value, RerankReport), PersistError> {
    let limit = input.limit.unwrap_or(20).clamp(1, 100);
    let offset = input.offset.unwrap_or(0).max(0);
    let window = i64::try_from(state.rerank_service().current_config().candidates)
        .unwrap_or(MAX_CANDIDATE_ROWS);
    let pool = window.max(limit).min(MAX_CANDIDATE_ROWS);
    let pool_rows = super::wide::Pool {
        project,
        input,
        wanted: pool,
        vector,
        reranked: true,
    };
    let mut rows = super::wide::candidate_rows(state, handle, pool_rows).await?;
    rows.truncate(usize::try_from(pool).unwrap_or(0));
    rows.sort_by(newest_first);
    let page: Vec<Value> = rows
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(0))
        .take(usize::try_from(limit + 1).unwrap_or(1))
        .collect();
    Ok((
        search_page(project.project_uuid, vector.is_some(), page, limit, offset),
        RerankReport::recent_order(),
    ))
}

fn newest_first(a: &Value, b: &Value) -> Ordering {
    updated_at(b)
        .cmp(&updated_at(a))
        .then_with(|| reference(a).cmp(reference(b)))
}

fn updated_at(row: &Value) -> Option<DateTime<Utc>> {
    row["updated_at"]
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|moment| moment.with_timezone(&Utc))
}

fn reference(row: &Value) -> &str {
    row["reference"].as_str().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn row(reference: &str, updated_at: Value) -> Value {
        json!({"reference": reference, "updated_at": updated_at})
    }

    #[test]
    fn newest_first_orders_by_time_then_reference_and_puts_undated_rows_last() {
        let mut rows = [
            row("b", json!("2026-11-01T10:00:00+00:00")),
            row("none", Value::Null),
            row("c", json!("2026-11-03T09:00:00.5+00:00")),
            row("a", json!("2026-11-01T10:00:00+00:00")),
        ];

        rows.sort_by(newest_first);

        let order: Vec<&str> = rows.iter().map(reference).collect();
        assert_eq!(order, vec!["c", "a", "b", "none"]);
    }
}
