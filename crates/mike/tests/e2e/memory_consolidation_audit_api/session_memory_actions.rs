use crate::db_helpers;

use serde_json::Value;

use super::support::{
    ActionFixture, bootstrap_project_session, insert_action, insert_lesson, start_consolidation_run,
};

#[tokio::test]
async fn list_session_memory_actions_returns_rows_or_404_for_unknown_session() -> anyhow::Result<()>
{
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-session-memory-actions";
    let session_id = "77777777-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let other_session_id = "88888888-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    bootstrap_project_session(&pool, project_path, other_session_id).await?;
    let now = chrono::Utc::now();
    let run_id = start_consolidation_run(&pool, "session", "manual", "completed", now).await?;
    let (lesson_id, lesson_uuid) =
        insert_lesson(&pool, project_id, "session-scoped lesson", "daily").await?;

    insert_action(
        &pool,
        ActionFixture {
            run_id,
            lesson_id,
            lesson_uuid,
            project_id: Some(project_id),
            session_uuid: Some(session_id),
            files_touched: None,
            lesson_text: "session-scoped lesson",
            action_kind: "evaluated",
            outcome: "applied",
            error: None,
            distance: None,
            threshold: None,
            counterpart_lesson_id: None,
            counterpart_uuid: None,
            counterpart_text: None,
            runners_up: serde_json::json!([]),
            created_at: now,
        },
    )
    .await?;

    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/memory-actions"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rows: Vec<Value> = resp.json().await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["lesson_id"], lesson_id);

    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{other_session_id}/memory-actions"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rows: Vec<Value> = resp.json().await?;
    assert!(rows.is_empty());

    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/99999999-aaaa-4aaa-8aaa-aaaaaaaaaaaa/memory-actions"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    pool.close().await;
    Ok(())
}
