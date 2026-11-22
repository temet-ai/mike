use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{fixture, time_condition};
use crate::store::persistence::conditions::Fired;
use crate::store::persistence::fleet_repo::FleetPersist as _;

#[tokio::test]
async fn next_armed_condition_at_without_armed_rows_is_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let machine = Uuid::new_v4();
    let mut fired = time_condition();
    fired.machine_uuid = machine;
    let fleet = f.handle.fleet();
    let row = fleet.insert_condition(&fired).await?;
    fleet
        .mark_condition_fired(row.id, Fired::Held, &serde_json::json!({}))
        .await?;
    let mut elsewhere = time_condition();
    elsewhere.machine_uuid = Uuid::new_v4();
    fleet.insert_condition(&elsewhere).await?;

    assert_eq!(fleet.next_armed_condition_at(machine).await?, None);
    f.db.cleanup().await
}

#[tokio::test]
async fn next_armed_condition_at_names_the_earliest_start_including_retries() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let machine = Uuid::new_v4();
    let fleet = f.handle.fleet();
    let mut later = time_condition();
    later.machine_uuid = machine;
    later.not_before = Utc::now() + Duration::hours(2);
    fleet.insert_condition(&later).await?;
    let mut retrying = time_condition();
    retrying.machine_uuid = machine;
    let retrying = fleet.insert_condition(&retrying).await?;
    fleet.mark_condition_failed_attempt(retrying.id, "x").await?;
    fleet.mark_condition_failed_attempt(retrying.id, "x").await?;

    let at = fleet
        .next_armed_condition_at(machine)
        .await?
        .ok_or_else(|| anyhow::anyhow!("armed rows gave no start"))?;
    let minutes = (at - Utc::now()).num_minutes();
    assert!((3..=5).contains(&minutes), "next start in {minutes} min");
    f.db.cleanup().await
}
