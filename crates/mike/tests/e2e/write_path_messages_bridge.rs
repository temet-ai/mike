use crate::db_helpers;

use crate::upstream;

use crate::anthropic_fixtures;

use std::time::Duration;

use anthropic_fixtures::{RequestBody, assistant_message, user_message};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use db_helpers::{ProxyWithDb, start_proxy_with_db};

async fn send_request(proxy_url: &str, session_id: &str, body: &Value) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "expected 200 OK, got {}",
        resp.status()
    );
    let _ = resp.bytes().await?;
    Ok(())
}

async fn wait_for_requests(
    pool: &PgPool,
    session_id: &str,
    expected: usize,
) -> anyhow::Result<Vec<(i64, chrono::DateTime<chrono::Utc>)>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let rows: Vec<(i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT r.id, r.created_at FROM requests.requests r
              JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid
              ORDER BY r.created_at ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?;
        if rows.len() == expected {
            return Ok(rows);
        }
        if tokio::time::Instant::now() >= deadline {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM requests.requests")
                .fetch_one(pool)
                .await
                .unwrap_or(-1);
            let recent_sessions: Vec<(String, i64)> = sqlx::query_as(
                "SELECT session_id, COUNT(*) FROM requests.requests
                  GROUP BY session_id LIMIT 10",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            anyhow::bail!(
                "timed out waiting for {expected} request rows for session {session_id}; \
                 saw {} for that session, {total} total in table, recent sessions = {recent_sessions:?}",
                rows.len()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn seed_two_overlapping_requests(proxy_url: &str, session_id: &str) -> anyhow::Result<()> {
    let body_1 = RequestBody::default()
        .max_tokens(4)
        .messages(json!([
            {"role": "user", "content": "first turn", "cache_control": {"type": "ephemeral"}}
        ]))
        .build();
    send_request(proxy_url, session_id, &body_1).await?;

    let body_2 = RequestBody::default()
        .max_tokens(4)
        .messages(json!([
            user_message("first turn"),
            assistant_message("ok"),
            {"role": "user", "content": "second turn", "cache_control": {"type": "ephemeral"}}
        ]))
        .build();
    send_request(proxy_url, session_id, &body_2).await?;
    Ok(())
}

async fn wait_for_bridge_rows_settled(
    pool: &PgPool,
    req1_id: i64,
    req2_id: i64,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let bridge_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM requests.messages
              WHERE request_id IN ($1, $2)",
        )
        .bind(req1_id)
        .bind(req2_id)
        .fetch_one(pool)
        .await?;
        if bridge_count >= 4 {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for bridge rows; saw {bridge_count}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn assert_bridge_row_counts_per_request(
    pool: &PgPool,
    req1_id: i64,
    req2_id: i64,
) -> anyhow::Result<()> {
    let req1_bridge: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages
          WHERE request_id = $1",
    )
    .bind(req1_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        req1_bridge, 1,
        "request 1 must have 1 bridge row (one user message)"
    );
    let req2_bridge: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages
          WHERE request_id = $1",
    )
    .bind(req2_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        req2_bridge, 3,
        "request 2 must have 3 bridge rows (one shared with req1 + two new)"
    );
    Ok(())
}

async fn assert_dedup_produces_three_unique_messages(
    pool: &PgPool,
    req1_id: i64,
    req2_id: i64,
) -> anyhow::Result<()> {
    let unique_messages: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT message_id)
           FROM requests.messages
          WHERE request_id IN ($1, $2)",
    )
    .bind(req1_id)
    .bind(req2_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        unique_messages, 3,
        "expected 3 unique canonical messages across the 4 bridge rows; \
         {unique_messages} unique implies the cache_control strip in the \
         live snapshot path does NOT match the unit-test contract"
    );
    Ok(())
}

async fn assert_no_orphan_bridge_rows(
    pool: &PgPool,
    req1_id: i64,
    req2_id: i64,
) -> anyhow::Result<()> {
    let orphan_bridge: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages rm
          LEFT JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id IN ($1, $2)
            AND m.id IS NULL",
    )
    .bind(req1_id)
    .bind(req2_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(orphan_bridge, 0, "no bridge row may dangle in messages");
    Ok(())
}

async fn assert_canonical_body_strips_cache_control(
    pool: &PgPool,
    req1_id: i64,
) -> anyhow::Result<()> {
    let first_user_body: Value = sqlx::query_scalar(
        "SELECT m.body FROM messages.messages m
          JOIN requests.messages rm ON rm.message_id = m.id
          WHERE rm.request_id = $1 AND rm.ord = 0",
    )
    .bind(req1_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        first_user_body.get("role").and_then(Value::as_str),
        Some("user")
    );
    assert!(
        first_user_body.get("cache_control").is_none(),
        "stored canonical body must have cache_control stripped; got {first_user_body:?}"
    );
    assert_eq!(
        first_user_body.get("content").and_then(Value::as_str),
        Some("first turn"),
        "stored canonical body must preserve full content"
    );
    Ok(())
}

async fn assert_wire_body_persisted_independent_of_bridge(
    pool: &PgPool,
    req1_id: i64,
    req1_created: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    let req1_body: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT request_body FROM requests.requests
          WHERE id = $1 AND created_at = $2",
    )
    .bind(req1_id)
    .bind(req1_created)
    .fetch_one(pool)
    .await?;
    assert!(
        req1_body.is_some(),
        "wire-form body bytes must still be persisted alongside the bridge"
    );
    Ok(())
}

async fn assert_isolated_db_has_exactly_expected_messages(
    pool: &PgPool,
    expected: i64,
) -> anyhow::Result<()> {
    let messages_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages.messages")
        .fetch_one(pool)
        .await?;
    assert_eq!(
        messages_total, expected,
        "per-test ephemeral DB must hold exactly the {expected} unique messages this test produced"
    );
    Ok(())
}

#[tokio::test]
async fn snapshot_request_populates_messages_bridge_with_dedup_across_requests()
-> anyhow::Result<()> {
    let mock = upstream::canned_json_upstream("msg_w23").await;

    let Some(proxy): Option<ProxyWithDb> = start_proxy_with_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&proxy.test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();

    seed_two_overlapping_requests(&proxy.proxy_url, &session_id).await?;

    let request_rows = wait_for_requests(&pool, &session_id, 2).await?;
    assert_eq!(request_rows.len(), 2, "expected 2 captured requests");
    let (req1_id, req1_created) = request_rows[0];
    let (req2_id, _req2_created) = request_rows[1];

    wait_for_bridge_rows_settled(&pool, req1_id, req2_id).await?;
    assert_bridge_row_counts_per_request(&pool, req1_id, req2_id).await?;
    assert_dedup_produces_three_unique_messages(&pool, req1_id, req2_id).await?;
    assert_no_orphan_bridge_rows(&pool, req1_id, req2_id).await?;
    assert_canonical_body_strips_cache_control(&pool, req1_id).await?;
    assert_wire_body_persisted_independent_of_bridge(&pool, req1_id, req1_created).await?;
    assert_isolated_db_has_exactly_expected_messages(&pool, 3).await?;

    pool.close().await;
    proxy.shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_request_skips_bridge_when_messages_missing() -> anyhow::Result<()> {
    let mock = upstream::canned_json_upstream("msg_w23_skip").await;

    let Some(proxy): Option<ProxyWithDb> = start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&proxy.test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();

    let body = RequestBody::default()
        .max_tokens(4)
        .messages(json!([]))
        .build();
    send_request(&proxy.proxy_url, &session_id, &body).await?;

    let request_rows = wait_for_requests(&pool, &session_id, 1).await?;
    let (req_id, req_created) = request_rows[0];

    tokio::time::sleep(Duration::from_millis(300)).await;

    let req_bridge: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages
          WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        req_bridge, 0,
        "empty messages[] must result in 0 bridge rows; the request row itself still persisted"
    );

    let body_bytes: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT request_body FROM requests.requests
          WHERE id = $1 AND created_at = $2",
    )
    .bind(req_id)
    .bind(req_created)
    .fetch_one(&pool)
    .await?;
    assert!(body_bytes.is_some());
    let _ = req_id;
    let _ = req_created;

    pool.close().await;
    proxy.shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    proxy.test_db.cleanup().await?;
    Ok(())
}
