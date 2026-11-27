use crate::store::persistence::web_repo::WebPersist as _;
use std::time::Duration;

use sqlx::postgres::PgListener;
use uuid::Uuid;

use super::followups_tests::{CHECK, Env};
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

async fn filed(env: &Env) -> anyhow::Result<i64> {
    let (problem, episode) = env.problem(CHECK, true).await?;
    let finding = env.finding(problem, episode).await?;
    let id = env.record("hardening", Some("wire"), finding).await?;
    env.file(id).await?;
    Ok(id)
}

#[tokio::test]
async fn live_handoffs_with_a_fed_feed_keep_an_empty_answer_until_a_followup_changes()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let id = filed(&env).await?;
    let here = PersistHandle::connect(&env.db.database_url).await?;
    let feed = fed(&here);
    assert!(here.web().live_wright_handoffs().await?.is_empty());

    env.hand_off(id).await?;
    assert!(here.web().live_wright_handoffs().await?.is_empty());

    feed.record(RowChangedTable::WrightRepairs, None);
    assert!(here.web().live_wright_handoffs().await?.is_empty());

    feed.record(RowChangedTable::WrightFollowups, None);
    assert_eq!(here.web().live_wright_handoffs().await?.len(), 1);
    env.db.cleanup().await
}

#[tokio::test]
async fn handing_off_a_followup_notifies_the_change_feed() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let id = filed(&env).await?;
    let mut listener = PgListener::connect(&env.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    env.persist
        .web()
        .mark_wright_followup_handed_off(id, Uuid::new_v4(), "omit")
        .await?;
    let payload: serde_json::Value = loop {
        let note = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(note.payload())?;
        if payload["table"] == "wright_followups" {
            break payload;
        }
    };
    assert_eq!(payload["id"], id.to_string());
    assert_eq!(payload["op"], "UPDATE");
    env.db.cleanup().await
}

fn filed_limit(daily_filed: i32) -> super::FollowupLimitsPatch {
    super::FollowupLimitsPatch {
        daily_filed: Some(daily_filed),
        ..super::FollowupLimitsPatch::default()
    }
}

#[tokio::test]
async fn followup_limits_with_a_fed_feed_are_kept_until_the_limits_change() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let here = PersistHandle::connect(&env.db.database_url).await?;
    let feed = fed(&here);
    let first = here.web().wright_followup_limits().await?.daily_filed;

    env.persist
        .web()
        .patch_wright_followup_limits(&filed_limit(first + 1))
        .await?;
    assert_eq!(here.web().wright_followup_limits().await?.daily_filed, first);

    feed.record(RowChangedTable::WrightFollowups, None);
    assert_eq!(here.web().wright_followup_limits().await?.daily_filed, first);

    feed.record(RowChangedTable::WrightFollowupLimits, None);
    assert_eq!(
        here.web().wright_followup_limits().await?.daily_filed,
        first + 1
    );
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_limits_patched_through_the_same_handle_are_read_fresh() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let here = PersistHandle::connect(&env.db.database_url).await?;
    let _feed = fed(&here);
    let first = here.web().wright_followup_limits().await?.daily_filed;
    here.web()
        .patch_wright_followup_limits(&filed_limit(first + 1))
        .await?;
    assert_eq!(
        here.web().wright_followup_limits().await?.daily_filed,
        first + 1
    );
    env.db.cleanup().await
}

#[tokio::test]
async fn patching_followup_limits_notifies_the_change_feed() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let mut listener = PgListener::connect(&env.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    env.persist
        .web()
        .patch_wright_followup_limits(&filed_limit(4))
        .await?;
    let payload: serde_json::Value = loop {
        let note = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(note.payload())?;
        if payload["table"] == "wright_followup_limits" {
            break payload;
        }
    };
    assert_eq!(payload["op"], "UPDATE");
    env.db.cleanup().await
}
