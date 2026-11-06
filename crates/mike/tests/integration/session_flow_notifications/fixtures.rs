use std::time::Duration;

use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::PgListener;
use tokio::time::timeout;

pub(super) const RECV_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) async fn open_listener(database_url: &str) -> anyhow::Result<PgListener> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen("proxy.row_changed").await?;
    Ok(listener)
}

pub(super) async fn recv_payload(listener: &mut PgListener) -> anyhow::Result<Value> {
    let n = timeout(RECV_TIMEOUT, listener.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for notification (>500ms)"))??;
    Ok(serde_json::from_str(n.payload())?)
}

pub(super) async fn recv_matching(listener: &mut PgListener, table: &str) -> anyhow::Result<Value> {
    loop {
        let payload = recv_payload(listener).await?;
        if payload["table"] == table {
            return Ok(payload);
        }
    }
}

pub(super) async fn assert_no_more(listener: &mut PgListener) -> anyhow::Result<()> {
    match timeout(Duration::from_millis(100), listener.recv()).await {
        Err(_) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Ok(Ok(n)) => Err(anyhow::anyhow!(
            "expected no further notification, but got payload {:?}",
            n.payload()
        )),
    }
}

pub(super) async fn seed_project_and_session(
    pool: &sqlx::PgPool,
) -> anyhow::Result<(i64, i64, String)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/sfn-{}", uuid::Uuid::new_v4());
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

    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         RETURNING id",
    )
    .bind(&session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    Ok((project_id, session_pk, session_id))
}

pub(super) async fn insert_request(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_pk: i64,
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
         VALUES (
            $1, $2,
            (SELECT id FROM requests.methods  WHERE code = 'POST'),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            '{}'::jsonb,
            'hello', 'hello',
            (SELECT id FROM requests.turn_types   WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $3
         )
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.get(0))
}

pub(super) fn zero_vector_literal() -> String {
    let mut s = String::from("[");
    for i in 0..768 {
        if i > 0 {
            s.push(',');
        }
        s.push('0');
    }
    s.push(']');
    s
}
