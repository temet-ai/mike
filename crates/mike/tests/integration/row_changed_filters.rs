use std::time::Duration;

use serde_json::Value;
use sqlx::postgres::{PgListener, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};

use crate::seed::{SeedRequest, seed_project};
use crate::test_db::{TestDb, TestDbHandle};

const SESSION: &str = "7d3f6a52-1c0e-4c8e-9a51-5f0d2b9e4a01";
const REQUEST_ID: i64 = 9_310_001;

struct Fixture {
    handle: TestDbHandle,
    pool: PgPool,
    listener: PgListener,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;
    let project_id = seed_project(&pool, "/tmp/row-changed-filters").await?;
    SeedRequest::new(REQUEST_ID, project_id)
        .session_uuid_ensure(SESSION)
        .status("pending")
        .insert(&pool)
        .await?;
    let mut listener = PgListener::connect(&handle.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    Ok(Some(Fixture {
        handle,
        pool,
        listener,
    }))
}

impl Fixture {
    async fn exec(&self, sql: &str) -> anyhow::Result<()> {
        sqlx::query(AssertSqlSafe(sql.to_owned()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn notifications_until_marker(&mut self) -> anyhow::Result<Vec<Value>> {
        let marker: i64 = sqlx::query_scalar(
            "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at)
             SELECT k.id, t.id, s.id, now()
               FROM memory.consolidation_run_kinds k,
                    memory.consolidation_triggers t,
                    memory.consolidation_run_statuses s
              WHERE k.code = 'session' AND t.code = 'manual' AND s.code = 'running'
             RETURNING id",
        )
        .fetch_one(&self.pool)
        .await?;
        let marker = marker.to_string();
        let mut seen = Vec::new();
        loop {
            let notification = tokio::time::timeout(Duration::from_secs(5), self.listener.recv())
                .await
                .map_err(|_| anyhow::anyhow!("timed out waiting for the marker notification"))??;
            let payload: Value = serde_json::from_str(notification.payload())?;
            if payload["table"] == "consolidation_runs" && payload["id"] == marker.as_str() {
                return Ok(seen);
            }
            seen.push(payload);
        }
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.pool.close().await;
        self.handle.cleanup().await
    }
}

#[tokio::test]
async fn sessions_update_last_seen_or_no_op_sends_nothing() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    fx.notifications_until_marker().await?;
    fx.exec(&format!(
        "UPDATE sessions.sessions SET last_seen = last_seen + interval '1 second'
          WHERE session_uuid = '{SESSION}'"
    ))
    .await?;
    fx.exec(&format!(
        "UPDATE sessions.sessions SET title = title, last_status = last_status
          WHERE session_uuid = '{SESSION}'"
    ))
    .await?;
    let seen = fx.notifications_until_marker().await?;
    assert!(seen.is_empty(), "unexpected notifications: {seen:?}");
    fx.cleanup().await
}

#[tokio::test]
async fn sessions_update_last_status_sends_one_scoped_notification() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    fx.notifications_until_marker().await?;
    fx.exec(&format!(
        "UPDATE sessions.sessions SET last_status = 'Complete', last_seen = now()
          WHERE session_uuid = '{SESSION}'"
    ))
    .await?;
    let seen = fx.notifications_until_marker().await?;
    assert_eq!(seen.len(), 1, "notifications: {seen:?}");
    assert_eq!(seen[0]["table"], "sessions");
    assert_eq!(seen[0]["op"], "UPDATE");
    assert_eq!(seen[0]["id"], SESSION);
    assert_eq!(seen[0]["session_id"], SESSION);
    fx.cleanup().await
}

#[tokio::test]
async fn requests_update_body_project_or_no_op_sends_nothing() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    fx.notifications_until_marker().await?;
    let other_project = seed_project(&fx.pool, "/tmp/row-changed-filters-other").await?;
    fx.exec(&format!(
        "UPDATE requests.requests SET request_body = NULL WHERE id = {REQUEST_ID}"
    ))
    .await?;
    fx.exec(&format!(
        "UPDATE requests.requests SET project_id = {other_project} WHERE id = {REQUEST_ID}"
    ))
    .await?;
    fx.exec(&format!(
        "UPDATE requests.requests SET status_id = status_id WHERE id = {REQUEST_ID}"
    ))
    .await?;
    let seen = fx.notifications_until_marker().await?;
    assert!(seen.is_empty(), "unexpected notifications: {seen:?}");
    fx.cleanup().await
}

#[tokio::test]
async fn requests_update_completion_sends_one_scoped_notification() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    fx.notifications_until_marker().await?;
    fx.exec(&format!(
        "UPDATE requests.requests r
            SET status_id = st.id, completed_at = now()
           FROM requests.statuses st
          WHERE st.code = 'complete' AND r.id = {REQUEST_ID}"
    ))
    .await?;
    let seen = fx.notifications_until_marker().await?;
    assert_eq!(seen.len(), 1, "notifications: {seen:?}");
    assert_eq!(seen[0]["table"], "requests");
    assert_eq!(seen[0]["op"], "UPDATE");
    assert_eq!(seen[0]["id"], REQUEST_ID.to_string());
    assert_eq!(seen[0]["session_id"], SESSION);
    fx.cleanup().await
}
