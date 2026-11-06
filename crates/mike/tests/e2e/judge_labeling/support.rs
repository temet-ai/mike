use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(super) fn or_verdict_response(label: &str) -> serde_json::Value {
    let content = serde_json::json!({
        "label": label,
        "confidence": 0.9,
        "rationale": "delivered what the operator asked for",
        "unmet_ask": "",
        "evidence_quality": "verified"
    })
    .to_string();
    serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content
            },
            "finish_reason": "stop"
        }]
    })
}

pub(super) async fn mount_judge_stub(or_mock: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(or_verdict_response("accepted")))
        .mount(or_mock)
        .await;
}

pub(super) async fn raise_llm_limits(database_url: &str) -> anyhow::Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    for (key, value) in [
        ("llm.rate_limit_per_minute", "600"),
        ("llm.ceiling_max_calls", "100000"),
    ] {
        sqlx::query(
            "INSERT INTO config.settings (key, value) VALUES ($1, $2::text::jsonb)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await?;
    }
    pool.close().await;
    Ok(())
}

pub(super) async fn start_proxy_with_judge(
    upstream_url: &str,
    database_url: &str,
    or_base_url: &str,
) -> anyhow::Result<(String, CancellationToken, AppState)> {
    raise_llm_limits(database_url).await?;

    let mut config = ProxyConfig::try_parse_from([
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
    config.set_openrouter_base_url(or_base_url.to_owned());
    config.set_openrouter_api_key("test-api-key".to_owned());
    config.set_judge_enabled(true);

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
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

    Ok((format!("http://{proxy_addr}"), shutdown, state))
}

pub(super) async fn seed_unjudged_turns(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
    count: i64,
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

    let request_ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt,
             turn_type_id, turn_origin_id, created_at, completed_at
         )
         SELECT ss.id, $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb, 'fix the bug', 'fix the bug',
                rtt.id, rto.id,
                NOW() + (g.i || ' microseconds')::interval,
                NOW() + (g.i || ' microseconds')::interval
         FROM generate_series(1, $3::int) AS g(i),
              sessions.sessions ss,
              requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE ss.session_uuid = $1::text::uuid
           AND rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(count)
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(request_ids.len() as i64 == count, "request seed sanity");

    let ts_pairs: Vec<(i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, created_at FROM requests.requests WHERE id = ANY($1) ORDER BY id ASC",
    )
    .bind(&request_ids)
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(ts_pairs.len() as i64 == count, "ts pair sanity");

    let ids: Vec<i64> = ts_pairs.iter().map(|(id, _)| *id).collect();
    let tss: Vec<chrono::DateTime<chrono::Utc>> = ts_pairs.iter().map(|(_, ts)| *ts).collect();

    sqlx::query(
        "INSERT INTO memory.operator_turns (
             session_id, project_id,
             start_request_id, end_request_id,
             outcome_id, classified_at, closed_at, created_at,
             user_prompt, final_assistant_text
         )
         SELECT ss.id, $2, u.rid, u.rid, tot.id, NOW(), NOW(), u.rts,
                'fix the bug', 'I fixed the bug.'
         FROM UNNEST($3::bigint[], $4::timestamptz[]) AS u(rid, rts),
              sessions.sessions ss,
              memory.turn_outcomes tot
         WHERE ss.session_uuid = $1::text::uuid
           AND tot.code = 'unknown'",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&ids)
    .bind(&tss)
    .execute(pool)
    .await?;

    Ok(project_id)
}

pub(super) async fn count_judged_turns(pool: &PgPool, session_id: &str) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
            AND t.outcome_id <> (SELECT id FROM memory.turn_outcomes WHERE code = 'unknown')
            AND t.judge_rubric_version IS NOT NULL",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub(super) async fn count_with_judge_model(pool: &PgPool, session_id: &str) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
            AND t.judge_model IS NOT NULL",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub(super) async fn store_settings(
    database_url: &str,
    settings: &[(&str, serde_json::Value)],
) -> anyhow::Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    for (key, value) in settings {
        sqlx::query(
            "INSERT INTO config.settings (key, value) VALUES ($1, $2::text::jsonb)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(key)
        .bind(value.to_string())
        .execute(&pool)
        .await?;
    }
    pool.close().await;
    Ok(())
}

pub(super) async fn start_proxy_without_openrouter_key(
    upstream_url: &str,
    database_url: &str,
    settings: &[(&str, serde_json::Value)],
) -> anyhow::Result<(CancellationToken, AppState)> {
    raise_llm_limits(database_url).await?;
    store_settings(database_url, settings).await?;

    let mut config = ProxyConfig::try_parse_from([
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
    config.set_judge_enabled(true);

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();
    Ok((shutdown, state))
}
