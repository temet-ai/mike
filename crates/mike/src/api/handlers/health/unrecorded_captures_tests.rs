use axum::extract::State;

use crate::config::{AppState, ProxyConfig};
use crate::store::models::RequestId;
use crate::store::persistence::capture_retry;

fn rows_for<'a>(list: &'a serde_json::Value, session: &str) -> Vec<&'a serde_json::Value> {
    list.as_array()
        .map(|rows| {
            rows.iter()
                .filter(|row| row["session_id"] == session)
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn health_lists_a_kept_and_an_unrecorded_request_with_session_and_time() -> anyhow::Result<()>
{
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let waiting_session = uuid::Uuid::new_v4().to_string();
    let lost_session = uuid::Uuid::new_v4().to_string();
    let kept = crate::store::tests::make_test_request(None, &waiting_session);
    capture_retry::defer(
        RequestId::fallback(),
        &kept,
        "lock timeout (55P03): canceling statement due to lock timeout",
    );
    let lost_at = chrono::Utc::now();
    capture_retry::note_unrecorded(
        &lost_session,
        lost_at,
        "value refused by the database (22P05)",
    );

    let body = serde_json::to_value(super::health(State(state)).await.0)?;
    let section = &body["unrecorded_captures"];

    let waiting = rows_for(&section["waiting"], &waiting_session);
    assert_eq!(waiting.len(), 1, "{section}");
    assert_eq!(
        waiting[0]["request_at"],
        serde_json::to_value(kept.created_at)?
    );
    assert!(
        waiting[0]["reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("lock timeout"))
    );

    let unrecorded = rows_for(&section["unrecorded"], &lost_session);
    assert_eq!(unrecorded.len(), 1, "{section}");
    assert_eq!(unrecorded[0]["request_at"], serde_json::to_value(lost_at)?);
    assert!(section["waiting_count"].as_u64().is_some_and(|n| n >= 1));
    assert!(section["unrecorded_count"].as_u64().is_some_and(|n| n >= 1));
    Ok(())
}
