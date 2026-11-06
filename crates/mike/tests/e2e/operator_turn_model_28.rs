use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::MockServer;

async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (
             SELECT 1 FROM projects.projects WHERE project_path = $1
         )",
    )
    .bind(project_path)
    .bind(now)
    .execute(pool)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "SELECT id FROM projects.projects WHERE project_path = $1 ORDER BY id LIMIT 1",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(project_id)
}

struct InsertRequestParams<'a> {
    session_id: &'a str,
    project_id: i64,
    model: &'a str,
    completed: bool,
    created_at_offset_ms: i64,
}

async fn insert_request(pool: &PgPool, p: InsertRequestParams<'_>) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(p.created_at_offset_ms);
    let completed_at = p.completed.then_some(now);
    let empty_obj = json!({});
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, model, is_streaming, message_count,
            is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
                $2, rm.id, '/v1/messages', $3, false, 0,
                false, rs.id, $4,
                '', 'test turn', rtt.id, rto.id,
                $6, $7
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = $5
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(p.model)
    .bind(&empty_obj)
    .bind(if p.completed { "complete" } else { "pending" })
    .bind(now)
    .bind(completed_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn insert_response(pool: &PgPool, request_id: i64, stop_reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, stop_reason_id, completed_at)
         SELECT $1, 200, sr.id, NOW()
         FROM responses.stop_reasons sr
         WHERE sr.code = $2",
    )
    .bind(request_id)
    .bind(stop_reason)
    .execute(pool)
    .await?;
    Ok(())
}

struct InsertTurnParams<'a> {
    session_id: &'a str,
    project_id: i64,
    start_request_id: i64,
    end_request_id: i64,
    outcome: &'a str,
    closed: bool,
}

async fn insert_turn(pool: &PgPool, p: InsertTurnParams<'_>) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            user_prompt, outcome_id, classified_at, closed_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid), $2,
                $3, $4,
                'test turn', toc.id,
                CASE WHEN $6 THEN NOW() END,
                CASE WHEN $6 THEN NOW() END
         FROM memory.turn_outcomes toc
         WHERE toc.code = $5
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(p.start_request_id)
    .bind(p.end_request_id)
    .bind(p.outcome)
    .bind(p.closed)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[tokio::test]
async fn operator_turns_and_overview_carry_serving_model_id() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "6a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "model-28-basic", session_id).await?;

    let model = "claude-opus-4-1-20260305";
    let r = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            model,
            completed: true,
            created_at_offset_ms: 0,
        },
    )
    .await?;
    insert_response(&pool, r, "end_turn").await?;
    insert_turn(
        &pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r,
            end_request_id: r,
            outcome: "accepted",
            closed: true,
        },
    )
    .await?;

    let client = reqwest::Client::new();

    let turns: Vec<Value> = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/operator-turns"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(turns.len(), 1, "expected exactly one turn: {turns:?}");
    assert_eq!(
        turns[0]["model"], model,
        "operator-turns row must carry the start request's model: {:?}",
        turns[0]
    );

    let overview: Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/overview"))
        .send()
        .await?
        .json()
        .await?;
    let overview_turns = overview["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns array: {overview}"))?;
    assert_eq!(overview_turns.len(), 1, "expected exactly one turn");
    assert_eq!(
        overview_turns[0]["model"], model,
        "overview row must carry the start request's model: {}",
        overview_turns[0]
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn model_present_for_open_turn_without_response() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "6a000000-0000-4000-8000-000000000002";
    let project_id = bootstrap_project_session(&pool, "model-28-no-response", session_id).await?;

    let model = "claude-sonnet-4-6-20260601";
    let r = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            model,
            completed: false,
            created_at_offset_ms: 0,
        },
    )
    .await?;
    insert_turn(
        &pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r,
            end_request_id: r,
            outcome: "unknown",
            closed: false,
        },
    )
    .await?;

    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/operator-turns"
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        200,
        "no error/panic for a turn with no response"
    );
    let turns: Vec<Value> = resp.json().await?;
    assert_eq!(turns.len(), 1, "expected exactly one open turn: {turns:?}");
    assert_eq!(
        turns[0]["model"], model,
        "model must still be present for a turn whose start request has no response: {:?}",
        turns[0]
    );

    let overview_resp = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/overview"))
        .send()
        .await?;
    assert_eq!(overview_resp.status(), 200);
    let overview: Value = overview_resp.json().await?;
    let overview_turns = overview["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns array: {overview}"))?;
    assert_eq!(overview_turns.len(), 1);
    assert_eq!(
        overview_turns[0]["model"], model,
        "overview: model must still be present for a turn with no response: {}",
        overview_turns[0]
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
