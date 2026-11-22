use crate::store::persistence::web_repo::WebPersist as _;
use std::time::Duration;

use chrono::Utc;

use super::fakes_tests::Db;
use super::job::JOB_LEASE_SECS;
use super::worker::{IDLE_POLL, Next, idle_wait};

#[test]
fn an_idle_conversation_waits_for_a_new_job_rather_than_polling() {
    let wait = idle_wait(Next::Until(None), true, Utc::now());
    assert!(wait >= Duration::from_secs(600), "waited {wait:?}");
}

#[test]
fn a_requeued_job_wakes_the_loop_when_it_falls_due() {
    let now = Utc::now();
    let due = now + chrono::Duration::seconds(60);
    assert_eq!(
        idle_wait(Next::Until(Some(due)), true, now),
        Duration::from_secs(60)
    );
}

#[test]
fn a_job_already_due_is_looked_at_again_within_a_second() {
    let now = Utc::now();
    let past = now - chrono::Duration::seconds(30);
    assert_eq!(
        idle_wait(Next::Until(Some(past)), true, now),
        Duration::from_secs(1)
    );
}

#[test]
fn work_this_machine_cannot_take_yet_is_retried_on_the_poll() {
    assert_eq!(idle_wait(Next::Poll, true, Utc::now()), IDLE_POLL);
    assert_eq!(idle_wait(Next::Now, true, Utc::now()), Duration::ZERO);
}

#[test]
fn without_a_change_feed_the_loop_keeps_its_poll() {
    assert_eq!(idle_wait(Next::Until(None), false, Utc::now()), IDLE_POLL);
}

#[tokio::test]
async fn next_due_names_a_queued_job_and_then_its_lease() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let web = db.handle.web();
    assert_eq!(web.wright_reply_next_due().await?, None);
    db.ask("due-1", "is problem 3 known?").await?;
    let queued = web
        .wright_reply_next_due()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a queued job has no due time"))?;
    assert!(queued <= Utc::now());
    web.claim_wright_reply_job(JOB_LEASE_SECS).await?;
    let lease = web
        .wright_reply_next_due()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a running job has no lease expiry"))?;
    assert!(lease > Utc::now() + chrono::Duration::seconds(60));
    db.cleanup().await
}

#[tokio::test]
async fn a_new_reply_job_notifies_the_change_feed() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let mut listener = sqlx::postgres::PgListener::connect_with(db.handle.pool()).await?;
    listener.listen("proxy.row_changed").await?;
    db.ask("notify-1", "anything open?").await?;
    let note = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let note = listener.recv().await?;
            if note.payload().contains("wright_reply_jobs") {
                return anyhow::Ok(note.payload().to_owned());
            }
        }
    })
    .await??;
    let event: serde_json::Value = serde_json::from_str(&note)?;
    assert_eq!(event["op"], "INSERT");
    drop(listener);
    db.cleanup().await
}
