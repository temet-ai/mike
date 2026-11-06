use crate::test_db;

use std::time::Duration;

use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::{PgListener, PgPoolOptions};
use test_db::TestDb;
use tokio::time::timeout;

const RECV_TIMEOUT: Duration = Duration::from_millis(500);

async fn open_listener(database_url: &str) -> anyhow::Result<PgListener> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen("proxy.row_changed").await?;
    Ok(listener)
}

async fn recv_payload(listener: &mut PgListener) -> anyhow::Result<Value> {
    let n = timeout(RECV_TIMEOUT, listener.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for notification (>500ms)"))??;
    let parsed: Value = serde_json::from_str(n.payload())?;
    Ok(parsed)
}

async fn assert_no_more(listener: &mut PgListener) -> anyhow::Result<()> {
    match timeout(Duration::from_millis(100), listener.recv()).await {
        Err(_) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Ok(Ok(n)) => Err(anyhow::anyhow!(
            "expected no further notification, but got payload {:?}",
            n.payload()
        )),
    }
}

async fn seed_project_and_session(pool: &sqlx::PgPool) -> anyhow::Result<(i64, i64, String)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/ln-{}", uuid::Uuid::new_v4());
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

async fn insert_request(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_pk: i64,
) -> anyhow::Result<(i64, chrono::DateTime<chrono::Utc>)> {
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
         RETURNING id, created_at",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let id: i64 = row.get(0);
    let created_at: chrono::DateTime<chrono::Utc> = row.get(1);
    Ok((id, created_at))
}

#[tokio::test]
async fn requests_insert_emits_notification() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping listen_notify smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let mut listener = open_listener(&handle.database_url).await?;

    let (project_id, session_pk, session_id) = seed_project_and_session(&pool).await?;
    let (req_id, _req_created_at) = insert_request(&pool, project_id, session_pk).await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "requests");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], req_id.to_string());
    assert_eq!(payload["session_id"], session_id);

    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sessions_update_emits_notification() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping listen_notify smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (_project_id, _session_pk, session_id) = seed_project_and_session(&pool).await?;

    let mut listener = open_listener(&handle.database_url).await?;

    sqlx::query(
        "UPDATE sessions.sessions
            SET last_seen = $2
          WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await?;
    sqlx::query(
        "UPDATE sessions.sessions
            SET title = 'listen notify smoke'
          WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .execute(&pool)
    .await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "sessions");
    assert_eq!(payload["op"], "UPDATE");
    assert_eq!(payload["id"], session_id);
    assert_eq!(
        payload["session_id"], session_id,
        "sessions notifications carry their own uuid; payload was {payload}"
    );

    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn filter_rules_insert_emits_notification() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping listen_notify smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let mut listener = open_listener(&handle.database_url).await?;

    let rule_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO rules.rules (uuid, kind_id, value, action_id, source_id)
         VALUES (
            $1,
            (SELECT id FROM rules.kinds   WHERE code = 'text_substring'),
            'noisy-banner',
            (SELECT id FROM rules.actions WHERE code = 'drop'),
            (SELECT id FROM rules.sources WHERE code = 'ui')
         )",
    )
    .bind(rule_id)
    .execute(&pool)
    .await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "filter_rules");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], rule_id.to_string());

    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn prompt_compositions_insert_emits_notification() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping listen_notify smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let prompt_sha = "a".repeat(64);
    let system_prompt_id: i64 = sqlx::query_scalar(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks)
         VALUES ($1, '[]'::jsonb)
         RETURNING id",
    )
    .bind(&prompt_sha)
    .fetch_one(&pool)
    .await?;

    let mut listener = open_listener(&handle.database_url).await?;

    let comp_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO overrides.overrides
            (prompt_sha256, system_prompt_id, label, blocks)
         VALUES ($1, $2, 'phase-b-test', '[]'::jsonb)
         RETURNING uuid",
    )
    .bind(&prompt_sha)
    .bind(system_prompt_id)
    .fetch_one(&pool)
    .await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "prompt_compositions");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], comp_id.to_string());

    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn request_with_diagnostics_emits_one_notification() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping listen_notify smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (project_id, session_pk, session_id) = seed_project_and_session(&pool).await?;
    let mut listener = open_listener(&handle.database_url).await?;

    let mut tx = pool.begin().await?;
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
            'hi', 'hi',
            (SELECT id FROM requests.turn_types   WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $3
         )
         RETURNING id, created_at",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    let req_id: i64 = row.get(0);

    sqlx::query(
        "INSERT INTO requests.diagnostics (
            request_id,
            category, analyzer, advice, matched_text, created_at
         )
         VALUES ($1, 'weasel', 'weasel-pattern', 'tone', 'maybe', $2)",
    )
    .bind(req_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "requests");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], req_id.to_string());
    assert_eq!(payload["session_id"], session_id);

    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
