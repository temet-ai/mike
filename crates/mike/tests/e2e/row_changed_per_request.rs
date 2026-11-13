use crate::anthropic_fixtures::RequestBody;
use crate::test_db;
use crate::upstream::sse_upstream;

use std::collections::BTreeMap;
use std::time::Duration;

use mike::app::proxy_router;
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use sqlx::postgres::{PgListener, PgPoolOptions};
use tokio::net::TcpListener;

const SESSION: &str = "5b1e7c3a-9d24-4f61-8a0e-2c7f4d9b6e13";
const REQUESTS: usize = 4;
const BUDGET_PER_REQUEST: usize = 6;

fn streaming_body() -> String {
    let events = [
        json!({"type": "message_start", "message": {
            "id": "msg_row_changed", "type": "message", "role": "assistant", "content": [],
            "model": "claude-sonnet-4-20250514", "stop_reason": null,
            "usage": {"input_tokens": 7, "output_tokens": 0}}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "ok"}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 4}}),
        json!({"type": "message_stop"}),
    ];
    events
        .iter()
        .map(|e| {
            format!(
                "event: {}\ndata: {e}\n\n",
                e["type"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

async fn start_proxy(upstream: &str, database_url: &str) -> anyhow::Result<(String, AppState)> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream,
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
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let app = proxy_router(state.clone());
    let shutdown = state.shutdown();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await;
    });
    Ok((format!("http://{addr}"), state))
}

async fn wait_completed(pool: &sqlx::PgPool, count: i64) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let done: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid AND r.completed_at IS NOT NULL",
        )
        .bind(SESSION)
        .fetch_one(pool)
        .await?;
        if done >= count {
            return Ok(());
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "only {done} of {count} requests completed"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn drain_until_marker(
    pool: &sqlx::PgPool,
    listener: &mut PgListener,
) -> anyhow::Result<Vec<Value>> {
    let marker: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at)
         SELECT k.id, t.id, s.id, now()
           FROM memory.consolidation_run_kinds k,
                memory.consolidation_triggers t,
                memory.consolidation_run_statuses s
          WHERE k.code = 'session' AND t.code = 'manual' AND s.code = 'running'
         RETURNING id",
    )
    .fetch_one(pool)
    .await?;
    let marker = marker.to_string();
    let mut seen = Vec::new();
    loop {
        let notification = tokio::time::timeout(Duration::from_secs(5), listener.recv())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for the marker notification"))??;
        let payload: Value = serde_json::from_str(notification.payload())?;
        if payload["table"] == "consolidation_runs" && payload["id"] == marker.as_str() {
            return Ok(seen);
        }
        seen.push(payload);
    }
}

fn tally(seen: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for payload in seen {
        let key = format!(
            "{} {}",
            payload["table"].as_str().unwrap_or("?"),
            payload["op"].as_str().unwrap_or("?")
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

#[tokio::test]
async fn proxied_requests_notify_within_budget_per_request() -> anyhow::Result<()> {
    let Some(factory) = test_db::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;

    let upstream = sse_upstream(&streaming_body()).await;
    let (proxy_url, state) = start_proxy(&upstream.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();
    for n in 0..REQUESTS {
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-api-key", "sk-test")
            .header("anthropic-version", "2023-06-01")
            .header("x-claude-code-session-id", SESSION)
            .json(
                &RequestBody::default()
                    .content(format!("prompt {n}"))
                    .max_tokens(16)
                    .stream(true)
                    .build(),
            )
            .send()
            .await?;
        anyhow::ensure!(
            resp.status() == 200,
            "request {n} returned {}",
            resp.status()
        );
        let _ = resp.bytes().await?;
        wait_completed(&pool, i64::try_from(n + 1)?).await?;
    }
    let seen = drain_until_marker(&pool, &mut listener).await?;
    let counts = tally(&seen);
    eprintln!(
        "row_changed: {} notifications for {REQUESTS} requests: {counts:?}",
        seen.len()
    );
    assert!(
        seen.len() <= REQUESTS * BUDGET_PER_REQUEST,
        "{} notifications for {REQUESTS} requests: {counts:?}",
        seen.len()
    );

    state.shutdown().cancel();
    pool.close().await;
    db.cleanup().await
}
