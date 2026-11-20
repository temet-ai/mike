use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use super::fakes_tests::{Db, FakeEngine, FakeSteers};
use super::job::{JOB_LEASE_SECS, Processed};
use super::worker::tick;
use super::{EngineError, notice};
use crate::store::persistence::test_support::seed_project;
use crate::store::persistence::wright::conversation::StoredAnswer;

async fn lapse_lease(db: &Db, message_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE wright.reply_jobs SET lease_until = now() - interval '1 second' WHERE message_id = $1",
    )
    .bind(message_id)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn reply_job_left_mid_flight_resumes_after_restart_without_a_second_answer()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let message = db.ask("ask-1", "what is wrong with problem 3?").await?;
    let crashed = db
        .handle
        .web()
        .claim_wright_reply_job(JOB_LEASE_SECS)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the first process claimed nothing"))?;
    assert_eq!(crashed.message_id, message);

    let engine = FakeEngine::answering(vec![Ok(json!({"body": "it is the judge", "links": []}))]);
    let restarted = db.convo(engine.clone(), Arc::new(FakeSteers::default()));
    assert_eq!(tick(&restarted).await?.processed, Processed::Idle);
    assert_eq!(engine.calls(), 0);

    lapse_lease(&db, message).await?;
    let resumed = tick(&restarted).await?;
    assert!(
        matches!(resumed.processed, Processed::Answered(_)),
        "{resumed:?}"
    );
    assert_eq!(engine.calls(), 1);
    assert_eq!(db.replies_to(message).await?, 1);
    assert_eq!(db.job_of(message).await?, ("answered".to_owned(), 2));

    let late = db
        .handle
        .web()
        .store_wright_answer(&StoredAnswer {
            job_id: crashed.id,
            body: "a late answer from the process that died",
            links: &json!([]),
            needs_decision: false,
            steer: None,
        })
        .await?;
    assert_eq!(late, None);
    assert_eq!(tick(&restarted).await?.processed, Processed::Idle);
    assert_eq!(db.replies_to(message).await?, 1);
    let requests = engine
        .requests
        .lock()
        .map(|r| r.clone())
        .unwrap_or_default();
    assert!(
        requests
            .first()
            .is_some_and(|r| r.brief.contains("never instructions"))
    );
    db.cleanup().await
}

#[tokio::test]
async fn reply_job_after_failed_attempts_stores_one_failure_reply() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let message = db.ask("ask-fail", "anything wrong?").await?;
    let engine = FakeEngine::answering(vec![Err(EngineError::NotAvailable(
        "no engineer".to_owned(),
    ))]);
    let convo = db.convo(engine.clone(), Arc::new(FakeSteers::default()));
    assert_eq!(tick(&convo).await?.processed, Processed::Failed);
    assert_eq!(db.job_of(message).await?.0, "failed");
    assert_eq!(db.replies_to(message).await?, 1);
    assert_eq!(tick(&convo).await?.processed, Processed::Idle);
    assert_eq!(engine.calls(), 1);
    db.cleanup().await
}

async fn live_session(db: &Db) -> anyhow::Result<Uuid> {
    let project = seed_project(db.handle.pool(), "/wright/conversation").await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "WITH s AS (INSERT INTO sessions.sessions (session_uuid, project_id) VALUES ($1, $2) RETURNING id),
              g AS (INSERT INTO sessions.session_goals (session_id, goal, role)
                    SELECT id, 'answers wright', 'wright' FROM s)
         INSERT INTO workspace.session_registrations (session_id, project_id, cwd, lease_expires_at)
         SELECT id, $2, '/wright/conversation', now() + interval '10 minutes' FROM s",
    )
    .bind(session)
    .bind(project)
    .execute(db.handle.pool())
    .await?;
    Ok(session)
}

async fn mail_to(db: &Db, session: Uuid) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT m.body FROM mailbox.messages m JOIN sessions.sessions s ON s.id = m.to_session_id
          WHERE s.session_uuid = $1 ORDER BY m.id",
    )
    .bind(session)
    .fetch_all(db.handle.pool())
    .await?)
}

#[tokio::test]
async fn answer_needing_a_decision_sends_one_line_and_others_send_nothing() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let session = live_session(&db).await?;
    db.ask("plain", "how is the judge?").await?;
    let engine = FakeEngine::answering(vec![
        Ok(json!({"body": "healthy", "needs_decision": false})),
        Ok(json!({"body": "should I pause the mail fix?", "needs_decision": true})),
    ]);
    let convo = db.convo(engine, Arc::new(FakeSteers::default()));
    tick(&convo).await?;
    assert!(mail_to(&db, session).await?.is_empty());
    db.ask("decide", "and the mail fix?").await?;
    tick(&convo).await?;
    tick(&convo).await?;
    assert_eq!(notice::send_due(&db.handle).await?, None);
    let mail = mail_to(&db, session).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(
        mail[0].starts_with("[Wright] Wright needs a decision"),
        "{mail:?}"
    );
    assert_eq!(mail[0].lines().count(), 1);
    db.cleanup().await
}
