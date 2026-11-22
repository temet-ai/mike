use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::{PersistHandle, test_support};

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

async fn pending_wake(handle: &PersistHandle) -> anyhow::Result<i64> {
    let project = test_support::seed_project(
        handle.pool(),
        &format!("/pending-wakes/{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now()) RETURNING id",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(project)
    .fetch_one(handle.pool())
    .await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO steering.interventions (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid,
                                             expires_at, payload_text, delivery_channel, deliver_after, wake_state)
         SELECT st.id, $1, v.id, m.id, a.id, $2, now() + interval '1 hour', 'nudge', 'wake', now(), 'pending'
           FROM steering.steerers st
           CROSS JOIN steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          WHERE st.name = 'turn-guard' AND v.code = 'fired' AND m.code = 'drive' AND a.code = 'command'
         RETURNING id",
    )
    .bind(session)
    .bind(uuid::Uuid::new_v4())
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn disable_all_pending_wakes_after_finding_none_stays_quiet_until_interventions_change()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&handle);
    assert!(
        handle
            .steerer()
            .disable_all_pending_wakes()
            .await?
            .is_empty()
    );
    let wake = pending_wake(&handle).await?;
    assert!(
        handle
            .steerer()
            .disable_all_pending_wakes()
            .await?
            .is_empty(),
        "no interventions change reached the feed, so the update is not sent"
    );
    feed.record(RowChangedTable::Requests, None);
    assert!(
        handle
            .steerer()
            .disable_all_pending_wakes()
            .await?
            .is_empty()
    );
    feed.record(RowChangedTable::Interventions, None);
    assert_eq!(
        handle.steerer().disable_all_pending_wakes().await?,
        vec![wake]
    );
    let state: Option<String> =
        sqlx::query_scalar("SELECT wake_state FROM steering.interventions WHERE id = $1")
            .bind(wake)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(state.as_deref(), Some("disabled"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn disable_all_pending_wakes_after_a_resync_runs_again() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&handle);
    assert!(
        handle
            .steerer()
            .disable_all_pending_wakes()
            .await?
            .is_empty()
    );
    let wake = pending_wake(&handle).await?;
    feed.resync();
    assert_eq!(
        handle.steerer().disable_all_pending_wakes().await?,
        vec![wake]
    );
    db.cleanup().await?;
    Ok(())
}
