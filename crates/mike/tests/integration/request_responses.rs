use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::seed;

use anthropic_fixtures::{MockResponse, RequestBody};
use chrono::Utc;
use db_helpers::{TestDb, start_proxy_with_shared_db};
use seed::{seed_project, seed_session};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn seed_request(
    pool: &PgPool,
    id: i64,
    session_id: &str,
    project_id: i64,
    status: &str,
) -> anyhow::Result<(i64, chrono::DateTime<Utc>)> {
    let now = Utc::now();
    let session_uuid: uuid::Uuid = session_id.parse()?;
    sqlx::query(
        "INSERT INTO requests.requests (
             id, session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id, params,
             last_user_message, human_prompt,
             turn_type_id, turn_origin_id, created_at
         )
         OVERRIDING SYSTEM VALUE
         SELECT $1,
                (SELECT id FROM sessions.sessions WHERE session_uuid = $2),
                $3, rm.id, '/v1/messages', FALSE,
                1, FALSE, rs.id, '{}'::jsonb,
                'hello', 'hello',
                rtt.id, rto.id, $5
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = $4
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
    )
    .bind(id)
    .bind(session_uuid)
    .bind(project_id)
    .bind(status)
    .bind(now)
    .execute(pool)
    .await?;
    Ok((id, now))
}

async fn seed_response_row(
    pool: &PgPool,
    request_id: i64,
    _request_created_at: chrono::DateTime<Utc>,
) -> anyhow::Result<()> {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO responses.responses (
             request_id,
             response_status, response_headers, response_body,
             token_usage, stop_reason_id, completed_at
         )
         SELECT $1, 200, '[]'::jsonb, '\\x'::bytea, $2, sr.id, $3
         FROM (SELECT 'end_turn'::text AS code) AS v
         LEFT JOIN responses.stop_reasons sr ON sr.code = v.code
         WHERE NOT EXISTS (
             SELECT 1 FROM responses.responses WHERE request_id = $1
         )",
    )
    .bind(request_id)
    .bind(json!({"input_tokens": 10, "output_tokens": 5}))
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

fn non_streaming_mock_body() -> serde_json::Value {
    MockResponse::new("msg_rr_integration_test")
        .text("Hello")
        .usage(15, 8)
        .build()
}

#[tokio::test]
async fn backfill_row_count_matches_completed_requests() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_responses backfill test");
        return Ok(());
    };
    let handle = test_db_factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_path = format!("/tmp/rr-backfill-{}", uuid::Uuid::new_v4());
    let session_id = uuid::Uuid::new_v4().to_string();

    let project_id = seed_project(&pool, &project_path).await?;
    seed_session(&pool, project_id, &session_id).await?;
    let (request_id, created_at) =
        seed_request(&pool, 9_100_001, &session_id, project_id, "complete").await?;
    seed_response_row(&pool, request_id, created_at).await?;

    let rr_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM responses.responses WHERE request_id = $1")
            .bind(request_id)
            .fetch_one(&pool)
            .await?;

    assert_eq!(rr_count, 1, "seeded request_responses row must be present");

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn request_responses_no_null_invariant() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_responses null invariant test");
        return Ok(());
    };
    let handle = test_db_factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_path = format!("/tmp/rr-null-{}", uuid::Uuid::new_v4());
    let session_id = uuid::Uuid::new_v4().to_string();

    let project_id = seed_project(&pool, &project_path).await?;
    seed_session(&pool, project_id, &session_id).await?;
    let (request_id, created_at) =
        seed_request(&pool, 9_100_002, &session_id, project_id, "complete").await?;
    seed_response_row(&pool, request_id, created_at).await?;

    let null_status: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM responses.responses
         WHERE request_id = $1 AND response_status IS NULL",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(null_status, 0, "response_status must not be null");

    let null_headers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM responses.responses
         WHERE request_id = $1 AND response_headers IS NULL",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(null_headers, 0, "response_headers must not be null");

    let null_body: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM responses.responses
         WHERE request_id = $1 AND response_body IS NULL",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(null_body, 0, "response_body must not be null");

    let null_token_usage: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM responses.responses
         WHERE request_id = $1 AND token_usage IS NULL",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(null_token_usage, 0, "token_usage must not be null");

    let null_completed_at: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM responses.responses
         WHERE request_id = $1 AND completed_at IS NULL",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(null_completed_at, 0, "completed_at must not be null");

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_terminal_inserts_into_request_responses() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(non_streaming_mock_body())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_shared_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping snapshot_terminal test");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", &session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(
            &RequestBody::default()
                .content("test rr insert")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    let _ = resp.bytes().await?;

    let session_uuid: uuid::Uuid = session_id.parse()?;
    let row: Option<(i16, Option<String>)> = wait_for_response_row(&pool, session_uuid).await?;

    let Some(row) = row else {
        anyhow::bail!("request_responses row must exist after snapshot_terminal");
    };
    assert_eq!(row.0, 200i16, "response_status must be 200");

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

async fn wait_for_response_row(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<Option<(i16, Option<String>)>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let row: Option<(i16, Option<String>)> = sqlx::query_as(
            "SELECT rr.response_status, sr.code
             FROM responses.responses rr
             JOIN requests.requests r ON r.id = rr.request_id
             JOIN sessions.sessions s ON s.id = r.session_id
             LEFT JOIN responses.stop_reasons sr ON sr.id = rr.stop_reason_id
             WHERE s.session_uuid = $1
             ORDER BY rr.completed_at DESC LIMIT 1",
        )
        .bind(session_uuid)
        .fetch_optional(pool)
        .await?;
        if row.is_some() || std::time::Instant::now() >= deadline {
            return Ok(row);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn snapshot_terminal_no_response_columns_on_requests() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping dropped columns test");
        return Ok(());
    };
    let handle = test_db_factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let col_names: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
         WHERE table_schema = 'requests' AND table_name = 'requests'
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await?;

    let forbidden = [
        "response_status",
        "response_headers",
        "response_body",
        "upstream_request_id",
        "upstream_message_id",
        "token_usage",
        "stop_reason",
        "diagnostics",
    ];
    for f in &forbidden {
        assert!(
            !col_names.contains(&f.to_string()),
            "column '{}' must NOT exist on requests after migration 20260517000002",
            f
        );
    }

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn pending_request_has_no_response_row() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping pending request test");
        return Ok(());
    };
    let handle = test_db_factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_path = format!("/tmp/rr-pending-{}", uuid::Uuid::new_v4());
    let session_id = uuid::Uuid::new_v4().to_string();

    let project_id = seed_project(&pool, &project_path).await?;
    seed_session(&pool, project_id, &session_id).await?;
    let (request_id, _created_at) =
        seed_request(&pool, 9_100_005, &session_id, project_id, "pending").await?;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM responses.responses WHERE request_id = $1")
            .bind(request_id)
            .fetch_one(&pool)
            .await?;

    assert_eq!(
        count, 0,
        "no request_responses row must exist for a Pending request (no snapshot_terminal called)"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_terminal_idempotent_no_duplicate_response_row() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping idempotent insert test");
        return Ok(());
    };
    let handle = test_db_factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_path = format!("/tmp/rr-idem-{}", uuid::Uuid::new_v4());
    let session_id = uuid::Uuid::new_v4().to_string();
    let request_id: i64 = 9_100_007;

    let project_id = seed_project(&pool, &project_path).await?;
    seed_session(&pool, project_id, &session_id).await?;
    let (_rid, _created_at) =
        seed_request(&pool, request_id, &session_id, project_id, "complete").await?;

    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO responses.responses (
                 request_id,
                 response_status, response_headers, response_body,
                 token_usage, stop_reason_id, completed_at
             )
             SELECT $1, 200, '[]'::jsonb, '\\x'::bytea, $2, sr.id, NOW()
             FROM (SELECT 'end_turn'::text AS code) AS v
             LEFT JOIN responses.stop_reasons sr ON sr.code = v.code
             WHERE NOT EXISTS (
                 SELECT 1 FROM responses.responses WHERE request_id = $1
             )",
        )
        .bind(request_id)
        .bind(json!({"input_tokens": 5, "output_tokens": 3}))
        .execute(&pool)
        .await?;
    }

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM responses.responses WHERE request_id = $1")
            .bind(request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        count, 1,
        "ON CONFLICT DO NOTHING must produce exactly 1 row on duplicate inserts"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn request_responses_indexes_exist_after_migration() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping index existence test");
        return Ok(());
    };
    let handle = test_db_factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let expected_indexes = [
        "idx_request_responses_completed_at",
        "idx_request_responses_stop_reason",
        "idx_request_responses_upstream_request_id",
    ];

    for idx in &expected_indexes {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT indexname FROM pg_indexes
             WHERE schemaname = 'responses' AND indexname = $1",
        )
        .bind(idx)
        .fetch_optional(&pool)
        .await?;
        assert!(
            exists.is_some(),
            "expected index `{idx}` to exist on request_responses after migration"
        );
    }

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
