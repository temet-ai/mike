use crate::db_helpers;

use serde_json::{Value, json};

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};

#[tokio::test]
async fn lessons_create_with_session_id_resolves_session_and_latest_turn() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-lessons-session-provenance";
    let session_id = "12345678-dddd-4ddd-8ddd-dddddddddddd";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let r1 = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "first turn",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let turn_id = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r1,
            end_request_id: r1,
            user_prompt: "first turn",
            final_assistant_text: "done",
            files_touched: &json!([]),
            outcome: "unknown",
            outcome_evidence: None,
        },
    )
    .await?;

    let client = reqwest::Client::new();
    let create_resp = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": project_path,
            "kind": "correction",
            "text": "always pass session_id from record_lesson",
            "session_id": session_id,
        }))
        .send()
        .await?;
    assert_eq!(create_resp.status(), reqwest::StatusCode::CREATED);
    let created: Value = create_resp.json().await?;
    assert_eq!(
        created["source_turn_id"].as_i64(),
        Some(turn_id),
        "source_turn_id must resolve to the session's latest operator turn: {created}"
    );
    let lesson_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id must be an integer: {created}"))?;

    let (session_db_id, stored_session_id): (i64, Option<i64>) = sqlx::query_as(
        "SELECT s.id, l.session_id
           FROM memory.lessons l
           JOIN sessions.sessions s ON s.session_uuid = $2::text::uuid
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .bind(session_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stored_session_id,
        Some(session_db_id),
        "lesson must record the resolved bigint session_id"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lessons_create_with_session_id_no_turn_yet_leaves_source_turn_id_null()
-> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-lessons-session-no-turn";
    let session_id = "12345678-eeee-4eee-8eee-eeeeeeeeeeee";
    bootstrap_project_session(&pool, project_path, session_id).await?;

    let client = reqwest::Client::new();
    let create_resp = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": project_path,
            "kind": "convention",
            "text": "session with no operator turn yet",
            "session_id": session_id,
        }))
        .send()
        .await?;
    assert_eq!(create_resp.status(), reqwest::StatusCode::CREATED);
    let created: Value = create_resp.json().await?;
    assert!(
        created["source_turn_id"].is_null(),
        "source_turn_id must stay NULL when the session has no operator turn: {created}"
    );
    let lesson_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id must be an integer: {created}"))?;

    let (session_db_id, stored_session_id): (i64, Option<i64>) = sqlx::query_as(
        "SELECT s.id, l.session_id
           FROM memory.lessons l
           JOIN sessions.sessions s ON s.session_uuid = $2::text::uuid
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .bind(session_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stored_session_id,
        Some(session_db_id),
        "lesson must still record the session even with no turn yet"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lessons_create_with_unknown_session_id_returns_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": "test-lessons-unknown-session",
            "kind": "warning",
            "text": "should be rejected",
            "session_id": "00000000-0000-4000-8000-000000000000",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    let body: Value = resp.json().await?;
    let error = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error body must carry a string message: {body}"))?;
    assert!(
        error.contains("session_id"),
        "error must name the offending field so a caller can self-correct: {error}"
    );
    test_db.cleanup().await?;
    Ok(())
}
