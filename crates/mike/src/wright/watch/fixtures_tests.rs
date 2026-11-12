use serde_json::{Value, json};
use uuid::Uuid;

use super::catalogue::{WatchContext, probe};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};
use crate::wright::{CheckOutcome, Finding};

pub(super) struct Db {
    pub(super) db: TestDbHandle,
    pub(super) handle: PersistHandle,
    pub(super) project: i64,
}

impl Db {
    pub(super) async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project = seed_project(handle.pool(), "/wright/project").await?;
        Ok(Self {
            db,
            handle,
            project,
        })
    }

    pub(super) fn ctx(&self) -> WatchContext {
        WatchContext {
            persist: self.handle.clone(),
            log_dir: None,
            machine: None,
        }
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }

    pub(super) async fn session(&self, uuid: Uuid, last_seen_ago: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, now() - interval '2 days', now() - $3::interval)
             RETURNING id",
        )
        .bind(uuid)
        .bind(self.project)
        .bind(last_seen_ago)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(super) async fn request(
        &self,
        session: i64,
        ago: &str,
        origin: &str,
    ) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO requests.requests (
                 session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
                 status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
                 created_at, completed_at)
             SELECT $1, $2, rm.id, '/v1/messages', true, 1, false, rs.id, '{}'::jsonb, '', '',
                    rtt.id, rto.id, now() - $3::interval, now() - $3::interval
               FROM requests.methods rm, requests.statuses rs, requests.turn_types rtt,
                    requests.turn_origins rto
              WHERE rm.code = 'POST' AND rs.code = 'complete' AND rtt.code = 'user_prompt'
                AND rto.code = $4
             RETURNING id",
        )
        .bind(session)
        .bind(self.project)
        .bind(ago)
        .bind(origin)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(super) async fn request_error(&self, request: i64, message: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO requests.errors (request_id, message) VALUES ($1, $2)")
            .bind(request)
            .bind(message)
            .execute(self.handle.pool())
            .await?;
        Ok(())
    }

    pub(super) async fn tool_error(
        &self,
        request: i64,
        session: i64,
        tool: &str,
        body: &str,
    ) -> anyhow::Result<()> {
        let content = Value::String(body.to_owned());
        sqlx::query(
            "WITH u AS (
                 INSERT INTO tools.uses (wire_tool_use_id, request_id, session_id, ord, tool_name, input)
                 VALUES ('toolu_' || gen_random_uuid(), $1, $2,
                         (SELECT count(*) FROM tools.uses WHERE request_id = $1), $3, '{}'::jsonb)
                 RETURNING id)
             INSERT INTO tools.results (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes)
             SELECT $1, $2, u.id, 0, true, $4, repeat('0', 64), length($5) FROM u",
        )
        .bind(request)
        .bind(session)
        .bind(tool)
        .bind(&content)
        .bind(body)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn response(
        &self,
        request: i64,
        cache_read: i64,
        input: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO responses.responses (request_id, response_status, token_usage, completed_at)
             VALUES ($1, 200, $2, now())",
        )
        .bind(request)
        .bind(json!({"input_tokens": input, "output_tokens": 10, "cache_read_input_tokens": cache_read}))
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn findings(&self, check: &str) -> anyhow::Result<Vec<Finding>> {
        let row = self
            .handle
            .wright_check(check)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no check named {check}"))?;
        match probe(&row, &self.ctx()).await {
            CheckOutcome::Complete(findings) => Ok(findings),
            CheckOutcome::Unknown(reason) => anyhow::bail!("{check} could not run: {reason}"),
        }
    }

    pub(super) async fn mail(
        &self,
        to: i64,
        queued_ago: &str,
        sent: bool,
        channel: bool,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO mailbox.messages (message_uuid, to_session_id, body, queued_at, sent_at,
                                           claimed_at, channel_delivered_at, delivery_attempts)
             VALUES (gen_random_uuid(), $1, 'hello', now() - $2::interval,
                     CASE WHEN $3 THEN now() - $2::interval + interval '1 minute' END,
                     CASE WHEN $3 THEN now() - $2::interval + interval '1 minute' END,
                     CASE WHEN $4 THEN now() - $2::interval + interval '1 minute' END,
                     CASE WHEN $3 THEN 1 ELSE 0 END)",
        )
        .bind(to)
        .bind(queued_ago)
        .bind(sent)
        .bind(channel)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }
}

#[tokio::test]
async fn undelivered_mail_counts_only_unsent_rows_not_read_but_unacknowledged_ones()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let busy = Uuid::new_v4();
    let busy_id = db.session(busy, "1 minute").await?;
    for _ in 0..5 {
        db.mail(busy_id, "2 hours", true, false).await?;
        db.mail(busy_id, "2 hours", false, true).await?;
    }
    assert_eq!(db.findings("undelivered_mail").await?, Vec::new());

    for _ in 0..3 {
        db.mail(busy_id, "2 hours", false, false).await?;
    }
    let findings = db.findings("undelivered_mail").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, busy.to_string());
    assert_eq!(findings[0].evidence["undelivered"], 3);
    db.cleanup().await
}

#[tokio::test]
async fn undelivered_mail_ignores_a_recipient_that_stopped_before_the_mail_came()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let gone = db.session(Uuid::new_v4(), "3 hours").await?;
    for _ in 0..4 {
        db.mail(gone, "2 hours", false, false).await?;
    }
    assert_eq!(db.findings("undelivered_mail").await?, Vec::new());
    db.cleanup().await
}
