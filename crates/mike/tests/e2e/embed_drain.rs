use crate::db_helpers;

use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::Value;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn infinity_response(count: usize) -> serde_json::Value {
    let vec: Vec<f32> = vec![0.0_f32; 768];
    let data: Vec<serde_json::Value> = (0..count)
        .map(|i| {
            serde_json::json!({
                "embedding": vec,
                "index": i,
                "object": "embedding"
            })
        })
        .collect();
    serde_json::json!({ "data": data })
}

fn echo_count_responder() -> impl wiremock::Respond {
    |req: &wiremock::Request| {
        let count = serde_json::from_slice::<serde_json::Value>(&req.body)
            .ok()
            .and_then(|v| v["input"].as_array().map(|a| a.len()))
            .unwrap_or(1);
        ResponseTemplate::new(200).set_body_json(infinity_response(count))
    }
}

async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, $2, $2
             WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind(project_path)
    .bind(now)
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

async fn insert_closed_turn(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    user_prompt: &str,
    final_assistant_text: &str,
) -> anyhow::Result<i64> {
    insert_closed_turn_at(
        pool,
        session_id,
        project_id,
        user_prompt,
        final_assistant_text,
        chrono::Utc::now(),
    )
    .await
}

async fn insert_closed_turn_at(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    user_prompt: &str,
    final_assistant_text: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<i64> {
    let empty_obj = serde_json::json!({});
    let (req_id, _): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id,
            method_id, path, is_streaming, message_count,
            is_tool_loop, status_id,
            params, last_user_message, human_prompt,
            turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2,
            rm.id, '/v1/messages', false, 0,
            false, rs.id,
            $3, $4, $4,
            rtt.id, rto.id,
            $5, $5
         FROM requests.methods rm,
              requests.statuses rs,
              requests.turn_types rtt,
              requests.turn_origins rto
         WHERE rm.code  = 'POST'
           AND rs.code  = 'complete'
           AND rtt.code = 'user_prompt'
           AND rto.code = 'primary'
         RETURNING id, created_at",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(user_prompt)
    .bind(now)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO responses.stream_blocks
            (request_id, session_id, seq_ord, content,
             byte_size, content_sha256, arrived_at, block_type_id)
         VALUES ($1,
             (SELECT id FROM sessions.sessions WHERE session_uuid = $4::text::uuid),
             0, $2, $3, repeat('0', 64), NOW(),
             (SELECT id FROM responses.stream_block_types WHERE code = 'text'))",
    )
    .bind(req_id)
    .bind(final_assistant_text)
    .bind(final_assistant_text.len() as i64)
    .bind(session_id)
    .execute(pool)
    .await?;
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            outcome_id, classified_at, closed_at,
            user_prompt, final_assistant_text
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, $3, $3,
            (SELECT id FROM memory.turn_outcomes WHERE code = 'shipped'),
            NOW(), NOW(),
            $4, $5
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(req_id)
    .bind(user_prompt)
    .bind(final_assistant_text)
    .fetch_one(pool)
    .await?;
    Ok(turn_id)
}

async fn patch_embedding_setting(
    inspect_url: &str,
    key: &str,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("{inspect_url}/api/settings/{key}"))
        .json(&serde_json::json!({ "value": value }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "failed to patch {key}: {}",
        resp.status()
    );
    Ok(())
}

async fn start_proxy_with_state(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(String, String, CancellationToken, AppState)> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });

    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    Ok((
        format!("http://{proxy_addr}"),
        format!("http://{inspect_addr}"),
        shutdown,
        state,
    ))
}

#[tokio::test]
async fn embed_drain_clears_closed_unembedded_turns_when_infinity_up() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream_mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let infinity_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(echo_count_responder())
        .mount(&infinity_mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [{"id": "nomic-embed-text-v1.5"}]
        })))
        .mount(&infinity_mock)
        .await;

    let project_path = "test-embed-drain-up";
    let session_id = "12345678-d001-4d01-8d01-d01d01d01d01";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    insert_closed_turn(
        &pool,
        session_id,
        project_id,
        "how does binary search work?",
        "binary search halves the search space on each step",
    )
    .await?;

    let (_proxy_url2, inspect_url2, shutdown2, state) =
        start_proxy_with_state(&upstream_mock.uri(), &test_db.database_url).await?;
    patch_embedding_setting(&inspect_url2, "embedding.enabled", serde_json::json!(true)).await?;
    patch_embedding_setting(
        &inspect_url2,
        "embedding.url",
        serde_json::json!(infinity_mock.uri()),
    )
    .await?;

    let client = reqwest::Client::new();
    let health_url = format!("{inspect_url2}/api/health");
    mike::api::handlers::refresh_health_now(&state).await;
    let resp = client.get(&health_url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK, "health must be 200");
    let health: Value = resp.json().await?;
    let pending_before = health["embedding"]["closed_unembedded_turns"]
        .as_i64()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "closed_unembedded_turns missing from health response (persistence enabled): {health}"
            )
        })?;
    assert!(
        pending_before >= 1,
        "expected at least 1 pending turn before drain, got: {pending_before}"
    );

    let drained = mike::embed_worker::run_drain_once(&state.knowledge_state()).await?;
    assert!(
        drained >= 1,
        "run_drain_once must embed at least 1 turn, got {drained}"
    );

    mike::api::handlers::refresh_health_now(&state).await;
    let resp = client.get(&health_url).send().await?;
    let health: Value = resp.json().await?;
    let pending_after = health["embedding"]["closed_unembedded_turns"]
        .as_i64()
        .unwrap_or(-1);
    assert_eq!(
        pending_after, 0,
        "closed_unembedded_turns must be 0 after successful drain, got: {health}"
    );
    assert!(
        health["embedding"]["reachable"].as_bool().unwrap_or(false),
        "Infinity must be reported as reachable: {health}"
    );

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn embed_drain_leaves_pending_when_infinity_down() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream_mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let infinity_url = "http://127.0.0.1:1";

    let project_path = "test-embed-drain-down";
    let session_id = "12345678-d002-4d02-8d02-d02d02d02d02";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    insert_closed_turn(
        &pool,
        session_id,
        project_id,
        "configure docker compose volumes",
        "updated compose.yaml bind mount",
    )
    .await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory.operator_turns WHERE closed_at IS NOT NULL AND embedded_at IS NULL",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        count >= 1,
        "must have at least 1 pending turn in DB before drain, got {count}"
    );

    let (_proxy_url2, inspect_url2, shutdown2, state) =
        start_proxy_with_state(&upstream_mock.uri(), &test_db.database_url).await?;
    patch_embedding_setting(&inspect_url2, "embedding.enabled", serde_json::json!(true)).await?;
    patch_embedding_setting(
        &inspect_url2,
        "embedding.url",
        serde_json::json!(infinity_url),
    )
    .await?;

    let drained = mike::embed_worker::run_drain_once(&state.knowledge_state()).await?;
    assert_eq!(
        drained, 0,
        "drain with Infinity down must return 0 (graceful, not Err), got {drained}"
    );

    let client = reqwest::Client::new();
    let health_url = format!("{inspect_url2}/api/health");
    mike::api::handlers::refresh_health_now(&state).await;
    let resp = client.get(&health_url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let health: Value = resp.json().await?;
    assert!(
        !health["embedding"]["reachable"].as_bool().unwrap_or(true),
        "Infinity must be reported as unreachable: {health}"
    );
    let pending = health["embedding"]["closed_unembedded_turns"]
        .as_i64()
        .unwrap_or(0);
    assert!(
        pending >= 1,
        "closed_unembedded_turns must remain > 0 when drain fails, got: {health}"
    );

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[path = "embed_drain/catchup.rs"]
mod catchup;
