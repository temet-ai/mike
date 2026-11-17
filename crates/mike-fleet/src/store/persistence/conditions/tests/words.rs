use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{fixture, new_condition, seeded_session};
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::test_support::seed_session_turns;

#[tokio::test]
async fn words_gate_opens_on_a_new_turn_and_closes_once_judged() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = seeded_session(&f.handle).await?;
    let mut new = new_condition(
        "words",
        json!({"text": "the build is green", "session_uuid": session.uuid}),
    );
    new.not_before = Utc::now() - Duration::hours(1);
    let row = f.handle.fleet().insert_condition(&new).await?;
    let since = row.not_before;

    let before = f
        .handle
        .fleet()
        .words_gate(row.id, session.uuid, since)
        .await?;
    assert_eq!(
        before.map(|g| (g.session_id, g.turn_landed)),
        Some((session.row_id, false))
    );

    seed_session_turns(
        f.handle.pool(),
        &session.uuid.to_string(),
        session.project_id,
        1,
    )
    .await?;
    let after = f
        .handle
        .fleet()
        .words_gate(row.id, session.uuid, since)
        .await?;
    assert_eq!(after.map(|g| g.turn_landed), Some(true));

    f.handle
        .fleet()
        .mark_condition_judged(row.id, Utc::now() + Duration::seconds(5))
        .await?;
    let judged = f
        .handle
        .fleet()
        .words_gate(row.id, session.uuid, since)
        .await?;
    assert_eq!(judged.map(|g| g.turn_landed), Some(false));

    let stranger = f
        .handle
        .fleet()
        .words_gate(row.id, Uuid::new_v4(), since)
        .await?;
    assert!(stranger.is_none());
    f.db.cleanup().await
}

#[tokio::test]
async fn mark_condition_asked_is_true_only_the_first_time() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let new = new_condition(
        "words",
        json!({"text": "the deploy finished", "session_uuid": Uuid::new_v4()}),
    );
    let row = f.handle.fleet().insert_condition(&new).await?;
    assert!(f.handle.fleet().mark_condition_asked(row.id).await?);
    assert!(!f.handle.fleet().mark_condition_asked(row.id).await?);
    f.db.cleanup().await
}
