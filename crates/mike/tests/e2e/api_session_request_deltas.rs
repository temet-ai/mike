use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_pk))
}

async fn insert_request(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    created_at_offset_ms: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(created_at_offset_ms);
    let empty_obj = json!({"thinking_enabled": false});
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2, 2, '/v1/messages', false, 0,
            false, 3, $3,
            '', '', 1, 1,
            $4
         ) RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(request_id)
}

async fn insert_added_delta_row(
    pool: &PgPool,
    session_pk: i64,
    request_id: i64,
    predecessor_request_id: Option<i64>,
    sha: &str,
) -> anyhow::Result<()> {
    let body = json!({"role": "user", "content": "hello"});
    let message_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
             SELECT $1, $2, $3, r.id
             FROM messages.roles r
             WHERE r.code = 'user'
               AND NOT EXISTS (SELECT 1 FROM messages.messages WHERE message_sha256 = $1)
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM messages.messages WHERE message_sha256 = $1
         LIMIT 1",
    )
    .bind(sha)
    .bind(&body)
    .bind(body.to_string().len() as i32)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.deltas (
            request_id, predecessor_request_id, session_id,
            change_kind_id, ord, message_id
         )
         SELECT $1, $2, $3, ck.id, 0, $4
         FROM requests.delta_change_kinds ck
         WHERE ck.code = 'added'",
    )
    .bind(request_id)
    .bind(predecessor_request_id)
    .bind(session_pk)
    .bind(message_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn get_request_deltas_returns_rows_for_own_session() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "c0000000-0000-4000-8000-000000000001";
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, "test-session-deltas-1", session_id).await?;

    let pred = insert_request(&pool, session_pk, project_id, 0).await?;
    let cur = insert_request(&pool, session_pk, project_id, 100).await?;
    insert_added_delta_row(&pool, session_pk, cur, Some(pred), &"a".repeat(64)).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/requests/{cur}/deltas"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["request_id"].as_i64(), Some(cur));
    assert_eq!(body["predecessor_request_id"].as_i64(), Some(pred));
    let deltas = body["deltas"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("deltas must be a JSON array"))?;
    assert_eq!(deltas.len(), 1, "one added delta row");
    assert_eq!(deltas[0]["change_kind"].as_str(), Some("added"));

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_request_deltas_unknown_session_returns_404() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/ffffffff-ffff-4fff-8fff-ffffffffffff/requests/1/deltas"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_request_deltas_unknown_request_returns_404() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "c0000000-0000-4000-8000-000000000002";
    bootstrap_project_session(&pool, "test-session-deltas-2", session_id).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/requests/999999999/deltas"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_request_deltas_cross_session_request_returns_404() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let owning_session_id = "c0000000-0000-4000-8000-000000000003";
    let (project_id, owning_session_pk) =
        bootstrap_project_session(&pool, "test-session-deltas-3-owner", owning_session_id).await?;
    let other_session_id = "c0000000-0000-4000-8000-000000000004";
    bootstrap_project_session(&pool, "test-session-deltas-3-other", other_session_id).await?;

    let req = insert_request(&pool, owning_session_pk, project_id, 0).await?;
    insert_added_delta_row(&pool, owning_session_pk, req, None, &"b".repeat(64)).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{other_session_id}/requests/{req}/deltas"
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "a request id from a different session must 404, never leak its deltas"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
