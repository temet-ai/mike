use crate::test_db;

use std::time::Duration;

use futures::StreamExt;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use mike::db_listener;
use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

pub(super) const RECV_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn ws_url(inspect_url: &str) -> String {
    let authority = inspect_url.trim_start_matches("http://");
    format!("ws://{authority}/api/ws")
}

pub(super) struct ProxyHandle {
    pub(super) inspect_url: String,
    pub(super) pool: sqlx::PgPool,
    pub(super) shutdown: tokio_util::sync::CancellationToken,
    test_db: test_db::TestDbHandle,
}

impl ProxyHandle {
    pub(super) fn test_db_url(&self) -> &str {
        &self.test_db.database_url
    }
}

impl ProxyHandle {
    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.pool.close().await;
        self.test_db.cleanup().await?;
        Ok(())
    }
}

pub(super) async fn start_proxy_with_listener() -> anyhow::Result<Option<ProxyHandle>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "http://127.0.0.1:1",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &test_db.database_url,
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    db_listener::spawn_listener(&state);

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    wait_for_listener_pid(&pool).await?;

    Ok(Some(ProxyHandle {
        inspect_url: format!("http://{inspect_addr}"),
        pool,
        shutdown,
        test_db,
    }))
}

pub(super) async fn next_frame<S>(ws: &mut S) -> anyhow::Result<Value>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let deadline = tokio::time::Instant::now() + RECV_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("timed out waiting for WS frame");
        }
        let msg = match timeout(remaining, ws.next()).await {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(e))) => anyhow::bail!("ws stream error: {e}"),
            Ok(None) => anyhow::bail!("ws stream closed unexpectedly"),
            Err(_) => anyhow::bail!("timed out waiting for WS frame"),
        };
        match msg {
            Message::Text(t) => {
                let value: Value = serde_json::from_str(&t)?;
                return Ok(value);
            }
            Message::Close(_) => anyhow::bail!("ws closed before frame arrived"),
            _ => continue,
        }
    }
}

pub(super) async fn next_matching<S, F>(ws: &mut S, mut predicate: F) -> anyhow::Result<Value>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    F: FnMut(&Value) -> bool,
{
    loop {
        let value = next_frame(ws).await?;
        if predicate(&value) {
            return Ok(value);
        }
    }
}

pub(super) async fn seed_project_and_session(pool: &sqlx::PgPool) -> anyhow::Result<(i64, String)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/dl-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now();

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(&project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)",
    )
    .bind(&session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok((project_id, session_id))
}

pub(super) async fn insert_request(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let row = sqlx::query(
        "INSERT INTO requests.requests (
            session_id, project_id,
            method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id,
            created_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid), $2,
            rm.id, '/v1/messages', false, 1, false,
            rs.id, '{}'::jsonb,
            'hello', 'hello',
            rtt.id, rto.id,
            $3
         FROM requests.methods rm,
              requests.statuses rs,
              requests.turn_types rtt,
              requests.turn_origins rto
         WHERE rm.code = 'POST'
           AND rs.code = 'complete'
           AND rtt.code = 'user_prompt'
           AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.get(0))
}

pub(super) fn is_row_changed(table: &str, id: &str) -> impl Fn(&Value) -> bool {
    let table = table.to_owned();
    let id = id.to_owned();
    move |v: &Value| -> bool {
        v.get("type").and_then(Value::as_str) == Some("RowChanged")
            && v.get("table").and_then(Value::as_str) == Some(table.as_str())
            && v.get("id").and_then(Value::as_str) == Some(id.as_str())
    }
}

pub(super) fn is_resync_required(reason: &str) -> impl Fn(&Value) -> bool {
    let reason = reason.to_owned();
    move |v: &Value| -> bool {
        v.get("type").and_then(Value::as_str) == Some("ResyncRequired")
            && v.get("reason").and_then(Value::as_str) == Some(reason.as_str())
    }
}

pub(super) async fn wait_for_listener_pid(pool: &sqlx::PgPool) -> anyhow::Result<i32> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let pid: Option<i32> = sqlx::query_scalar(
            "SELECT pid FROM pg_stat_activity
             WHERE datname = current_database()
               AND state = 'idle'
               AND query LIKE 'LISTEN %row_changed%'
               AND pid <> pg_backend_pid()
             ORDER BY backend_start DESC
             LIMIT 1",
        )
        .fetch_optional(pool)
        .await?
        .flatten();
        if let Some(pid) = pid {
            return Ok(pid);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for db_listener LISTEN session to appear");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub(super) async fn wait_for_new_listener_pid(
    pool: &sqlx::PgPool,
    prior_pid: i32,
) -> anyhow::Result<i32> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let pid: Option<i32> = sqlx::query_scalar(
            "SELECT pid FROM pg_stat_activity
             WHERE datname = current_database()
               AND state = 'idle'
               AND query LIKE 'LISTEN %row_changed%'
               AND pid <> pg_backend_pid()
               AND pid <> $1
             ORDER BY backend_start DESC
             LIMIT 1",
        )
        .bind(prior_pid)
        .fetch_optional(pool)
        .await?
        .flatten();
        if let Some(pid) = pid {
            return Ok(pid);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for db_listener to reconnect on a fresh pg backend (prior pid {prior_pid})"
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
