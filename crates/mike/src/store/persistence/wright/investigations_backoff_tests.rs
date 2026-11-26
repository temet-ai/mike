use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_captured_request, seed_project,
};
use crate::store::persistence::web_repo::WebPersist as _;

const RESET_HEADER: &str = "anthropic-ratelimit-unified-reset";

struct Env {
    db: TestDbHandle,
    persist: PersistHandle,
}

impl Env {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        Ok(Self { db, persist })
    }

    async fn investigation(&self, state: &str) -> anyhow::Result<(Uuid, DateTime<Utc>)> {
        let session = Uuid::new_v4();
        let ended_at: DateTime<Utc> = sqlx::query_scalar(
            "INSERT INTO wright.investigations
                 (mode, session_uuid, state, host, deadline_at, ended_at, end_reason)
             VALUES ('investigate', $1, $2, 'test-host',
                     now() - interval '1 hour', now() - interval '30 minutes', 'fixture')
             RETURNING ended_at",
        )
        .bind(session)
        .bind(state)
        .fetch_one(self.persist.pool())
        .await?;
        Ok((session, ended_at))
    }

    async fn rate_limited(&self, session: Uuid, reset: DateTime<Utc>) -> anyhow::Result<()> {
        let project = seed_project(self.persist.pool(), "/test/wright-backoff").await?;
        let request = seed_captured_request(
            self.persist.pool(),
            &session.to_string(),
            project,
            "primary",
            b"{}",
        )
        .await?;
        let headers = json!([[RESET_HEADER, reset.timestamp().to_string()]]);
        sqlx::query(
            "INSERT INTO responses.responses
                 (request_id, response_status, response_headers, completed_at)
             VALUES ($1, 429, $2, now())",
        )
        .bind(request)
        .bind(headers)
        .execute(self.persist.pool())
        .await?;
        Ok(())
    }

    async fn blocked_until(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        Ok(self.persist.web().wright_engineer_blocked_until().await?)
    }
}

fn whole_seconds(at: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp(at.timestamp(), 0).unwrap_or(at)
}

#[tokio::test]
async fn blocked_until_weekly_reset_days_away_caps_at_six_hours() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (session, ended_at) = env.investigation("unavailable").await?;
    env.rate_limited(session, ended_at + Duration::days(3))
        .await?;

    let until = env.blocked_until().await?;

    assert_eq!(until, Some(ended_at + Duration::hours(6)));
    env.db.cleanup().await
}

#[tokio::test]
async fn blocked_until_reset_within_six_hours_waits_for_the_reset() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (session, ended_at) = env.investigation("unavailable").await?;
    let reset = whole_seconds(ended_at + Duration::hours(1));
    env.rate_limited(session, reset).await?;

    let until = env.blocked_until().await?;

    assert_eq!(until, Some(reset));
    env.db.cleanup().await
}

#[tokio::test]
async fn blocked_until_last_investigation_finished_is_none() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (session, ended_at) = env.investigation("finished").await?;
    env.rate_limited(session, ended_at + Duration::hours(1))
        .await?;

    let until = env.blocked_until().await?;

    assert_eq!(until, None);
    env.db.cleanup().await
}
