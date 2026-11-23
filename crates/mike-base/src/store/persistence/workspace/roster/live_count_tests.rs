use std::sync::Arc;

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::super::roster_test_support::{fed, insert_channel, register, unique_project};
use super::{Held, LiveCount};
use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::workspace::{Liveness, RosterScope};

async fn live_in_full_roster(
    handle: &PersistHandle,
    project_id: i64,
    exclude: Uuid,
) -> anyhow::Result<usize> {
    Ok(handle
        .fetch_workspace_roster(project_id, RosterScope::Live)
        .await?
        .iter()
        .filter(|row| row.liveness == Liveness::Live && row.session_uuid != exclude)
        .count())
}

async fn lapse(handle: &PersistHandle, session: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE workspace.session_registrations SET lease_expires_at = now() - interval '20 minutes'
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(session)
    .execute(&handle.pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn live_count_over_mixed_members_equals_the_live_scope_roster() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = unique_project("live-count-mixed");
    let me = Uuid::new_v4();
    let quiet_channel = Uuid::new_v4();
    let leased = Uuid::new_v4();
    let released = Uuid::new_v4();
    let lapsed = Uuid::new_v4();
    let project_id = register(&handle, me, &project).await?;
    for session in [quiet_channel, leased, released, lapsed] {
        register(&handle, session, &project).await?;
    }
    insert_channel(&handle, &format!("cse_me_{me}"), Some(me), 0).await?;
    insert_channel(
        &handle,
        &format!("cse_quiet_{quiet_channel}"),
        Some(quiet_channel),
        300,
    )
    .await?;
    handle.end_workspace_registration(released, None).await?;
    lapse(&handle, lapsed).await?;

    let full = live_in_full_roster(&handle, project_id, me).await?;
    assert_eq!(full, 1);
    assert_eq!(
        handle.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: full }
    );
    assert_eq!(
        handle.count_live_peers(project_id, None).await?,
        LiveCount { live: 2 }
    );

    let _feed = fed(&handle);
    handle.registrations_changed(None);
    assert_eq!(
        handle.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: full }
    );
    db.cleanup().await
}

#[tokio::test]
async fn live_count_without_a_notice_is_held_and_after_one_is_read_again() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let project = unique_project("live-count-held");
    let me = Uuid::new_v4();
    let peer = Uuid::new_v4();
    let project_id = register(&here, me, &project).await?;
    register(&here, peer, &project).await?;
    assert_eq!(
        here.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: 1 }
    );

    other.end_workspace_registration(peer, None).await?;
    assert_eq!(
        here.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: 1 },
        "the release has not reached the feed"
    );

    feed.record(
        RowChangedTable::WorkspaceSessionRegistrations,
        Some(&peer.to_string()),
    );
    assert_eq!(
        here.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: 0 }
    );
    assert_eq!(live_in_full_roster(&here, project_id, me).await?, 0);
    db.cleanup().await
}

#[tokio::test]
async fn live_count_for_a_stale_peer_is_read_again_after_its_request_notice() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let project = unique_project("live-count-revive");
    let me = Uuid::new_v4();
    let peer = Uuid::new_v4();
    let project_id = register(&here, me, &project).await?;
    register(&here, peer, &project).await?;
    lapse(&here, peer).await?;
    here.registrations_changed(Some(peer));
    assert_eq!(
        here.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: 0 }
    );

    sqlx::query(
        "UPDATE workspace.session_registrations SET lease_expires_at = now() + interval '15 minutes'
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(peer)
    .execute(&here.pool)
    .await?;
    assert_eq!(
        here.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: 0 },
        "the renewal has not reached the feed"
    );

    feed.record(RowChangedTable::Requests, Some(&peer.to_string()));
    assert_eq!(
        here.count_live_peers(project_id, Some(me)).await?,
        LiveCount { live: 1 }
    );
    db.cleanup().await
}

fn held(feed: &ChangeFeed, until: Option<chrono::DateTime<Utc>>) -> Held {
    let mut held = Held {
        live: Arc::new(vec![Uuid::from_u128(1)]),
        watched: vec![Uuid::from_u128(2).to_string()],
        stamp: Vec::new(),
        until,
    };
    held.stamp = held.generations(feed);
    held
}

#[test]
fn held_count_past_its_deadline_does_not_hold() {
    let feed = ChangeFeed::new();
    let now = Utc::now();
    assert!(held(&feed, Some(now + Duration::seconds(5))).holds(&feed, now));
    assert!(!held(&feed, Some(now)).holds(&feed, now));
    assert!(held(&feed, None).holds(&feed, now));
}

#[test]
fn held_count_after_a_notice_for_a_watched_or_any_member_table_does_not_hold() {
    let feed = ChangeFeed::new();
    let now = Utc::now();
    let watched = held(&feed, None);
    feed.record(
        RowChangedTable::Requests,
        Some(&Uuid::from_u128(3).to_string()),
    );
    assert!(
        watched.holds(&feed, now),
        "a request of a session not watched"
    );
    feed.record(
        RowChangedTable::Requests,
        Some(&Uuid::from_u128(2).to_string()),
    );
    assert!(!watched.holds(&feed, now));

    let facts = held(&feed, None);
    feed.record(RowChangedTable::RuntimeSessionFacts, None);
    assert!(!facts.holds(&feed, now));
    let channels = held(&feed, None);
    feed.record(RowChangedTable::RemoteControlChannels, None);
    assert!(!channels.holds(&feed, now));
}
