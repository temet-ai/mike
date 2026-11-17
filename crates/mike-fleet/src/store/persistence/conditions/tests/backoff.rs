use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{fixture, time_condition};
use crate::store::persistence::conditions::Fired;
use crate::store::persistence::fleet_repo::FleetPersist as _;

fn minutes_from_now(at: Option<DateTime<Utc>>) -> anyhow::Result<i64> {
    let at = at.ok_or_else(|| anyhow::anyhow!("next_attempt_at is not set"))?;
    Ok((at - Utc::now()).num_minutes())
}

#[tokio::test]
async fn failed_attempts_back_off_one_five_thirty_minutes_then_fail() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let row = f.handle.fleet().insert_condition(&time_condition()).await?;
    let fleet = f.handle.fleet();

    let first = fleet
        .mark_condition_failed_attempt(row.id, "launch refused")
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed row not updated"))?;
    assert_eq!((first.attempts, first.state.as_str()), (1, "armed"));
    assert_eq!(minutes_from_now(first.next_attempt_at)?, 0);

    let second = fleet
        .mark_condition_failed_attempt(row.id, "launch refused")
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed row not updated"))?;
    assert_eq!((second.attempts, second.state.as_str()), (2, "armed"));
    assert_eq!(minutes_from_now(second.next_attempt_at)?, 4);

    let third = fleet
        .mark_condition_failed_attempt(row.id, "launch refused")
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed row not updated"))?;
    assert_eq!((third.attempts, third.state.as_str()), (3, "armed"));
    assert_eq!(minutes_from_now(third.next_attempt_at)?, 29);

    let fourth = fleet
        .mark_condition_failed_attempt(row.id, "last words")
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed row not updated"))?;
    assert_eq!((fourth.attempts, fourth.state.as_str()), (4, "failed"));
    assert_eq!(fourth.last_error.as_deref(), Some("last words"));

    let after_failure = fleet.mark_condition_failed_attempt(row.id, "again").await?;
    assert!(after_failure.is_none());
    f.db.cleanup().await
}

#[tokio::test]
async fn claimed_firing_is_taken_once_and_a_failed_action_rearms_it() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let row = f.handle.fleet().insert_condition(&time_condition()).await?;
    let fleet = f.handle.fleet();
    let claim = json!({"via": "claimed"});

    let first = fleet
        .mark_condition_fired(row.id, Fired::Held, &claim)
        .await?;
    assert!(first.is_some_and(|r| r.state == "fired"));
    let second = fleet
        .mark_condition_fired(row.id, Fired::Held, &claim)
        .await?;
    assert!(second.is_none());

    let rearmed = fleet
        .mark_condition_failed_attempt(row.id, "launch refused")
        .await?
        .ok_or_else(|| anyhow::anyhow!("claimed row not re-armed"))?;
    assert_eq!((rearmed.attempts, rearmed.state.as_str()), (1, "armed"));
    assert!(rearmed.fired_at.is_none() && rearmed.outcome.is_none());

    fleet
        .mark_condition_fired(row.id, Fired::Held, &claim)
        .await?;
    fleet
        .set_condition_outcome(row.id, &json!({"via": "push"}))
        .await?;
    let listed = fleet
        .list_conditions(&Default::default())
        .await?
        .into_iter()
        .find(|r| r.id == row.id)
        .ok_or_else(|| anyhow::anyhow!("row missing from list"))?;
    assert_eq!(listed.outcome, Some(json!({"via": "push"})));
    f.db.cleanup().await
}

#[tokio::test]
async fn due_conditions_are_this_machines_armed_rows_whose_time_has_come() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let machine = Uuid::new_v4();
    let mut ready = time_condition();
    ready.machine_uuid = machine;
    let mut later = time_condition();
    later.machine_uuid = machine;
    later.not_before = Utc::now() + Duration::hours(1);
    let mut elsewhere = time_condition();
    elsewhere.machine_uuid = Uuid::new_v4();
    let mut waiting = time_condition();
    waiting.machine_uuid = machine;
    let fleet = f.handle.fleet();
    let ready = fleet.insert_condition(&ready).await?;
    fleet.insert_condition(&later).await?;
    fleet.insert_condition(&elsewhere).await?;
    let waiting = fleet.insert_condition(&waiting).await?;
    fleet
        .mark_condition_failed_attempt(waiting.id, "try later")
        .await?;

    let due = fleet.due_conditions(machine).await?;
    let ids: Vec<Uuid> = due.iter().map(|r| r.condition_uuid).collect();
    assert_eq!(ids, vec![ready.condition_uuid]);
    f.db.cleanup().await
}

#[tokio::test]
async fn mark_fired_records_the_outcome_once_and_removes_the_row_from_due() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let machine = Uuid::new_v4();
    let mut new = time_condition();
    new.machine_uuid = machine;
    let fleet = f.handle.fleet();
    let row = fleet.insert_condition(&new).await?;

    let outcome = json!({"via": "mail", "missed": false});
    let fired = fleet
        .mark_condition_fired(row.id, Fired::Held, &outcome)
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed row not fired"))?;
    assert_eq!(fired.state, "fired");
    assert!(fired.fired_at.is_some());
    assert_eq!(fired.outcome, Some(outcome.clone()));
    assert!(fleet.due_conditions(machine).await?.is_empty());
    assert!(
        fleet
            .mark_condition_fired(row.id, Fired::Missed, &outcome)
            .await?
            .is_none()
    );

    let mut missed = time_condition();
    missed.machine_uuid = machine;
    let missed_row = fleet.insert_condition(&missed).await?;
    let missed_fired = fleet
        .mark_condition_fired(missed_row.id, Fired::Missed, &json!({"missed": true}))
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed row not marked missed"))?;
    assert_eq!(missed_fired.state, "missed");
    f.db.cleanup().await
}
