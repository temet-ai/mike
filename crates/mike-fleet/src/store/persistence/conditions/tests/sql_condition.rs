use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{fixture, new_condition};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::conditions::holds::{expectation_met, sql_check_due};
use crate::store::persistence::conditions::{
    ConditionError, ConditionFilter, ConditionRow, SqlExpect,
};
use crate::store::persistence::fleet_repo::FleetPersist as _;

fn sql_condition(query: &str, expect: &str) -> crate::store::persistence::conditions::NewCondition {
    new_condition("sql", json!({"query": query, "expect": expect}))
}

#[tokio::test]
async fn sql_condition_holds_by_its_expectation() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(holds_for(&f.handle, "SELECT 1", "rows").await?);
    assert!(!holds_for(&f.handle, "SELECT 1 WHERE false", "rows").await?);
    assert!(!holds_for(&f.handle, "SELECT 1", "no_rows").await?);
    assert!(holds_for(&f.handle, "SELECT 1 WHERE false", "no_rows").await?);
    f.db.cleanup().await
}

async fn holds_for(handle: &PersistHandle, query: &str, expect: &str) -> anyhow::Result<bool> {
    let row = handle
        .fleet()
        .insert_condition(&sql_condition(query, expect))
        .await?;
    Ok(handle.fleet().condition_holds(&row, Utc::now()).await?)
}

#[tokio::test]
async fn sql_condition_that_writes_is_refused_by_the_read_only_transaction() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let query = "SELECT 1 FROM launch.conditions FOR UPDATE";
    let direct = f.handle.fleet().run_read_only_exists(query).await;
    let Err(error) = direct else {
        anyhow::bail!("a write inside the condition query must fail");
    };
    assert!(error.to_string().contains("read-only"), "{error}");

    let row = f
        .handle
        .fleet()
        .insert_condition(&sql_condition(query, "rows"))
        .await?;
    let held = f.handle.fleet().condition_holds(&row, Utc::now()).await;
    let Err(ConditionError::Invalid(message)) = held else {
        anyhow::bail!("the evaluation must fail with the query in the message");
    };
    assert!(message.contains("read-only"), "{message}");
    assert!(message.contains("FOR UPDATE"), "{message}");
    f.db.cleanup().await
}

#[tokio::test]
async fn sql_condition_is_checked_at_most_once_per_interval() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let slug = format!("thread-{}", Uuid::new_v4());
    let mut new = sql_condition("SELECT 1", "rows");
    new.thread_slug = Some(slug.clone());
    let row = f.handle.fleet().insert_condition(&new).await?;
    assert!(f.handle.fleet().condition_holds(&row, Utc::now()).await?);

    let filter = ConditionFilter {
        thread_slug: Some(slug),
        limit: 5,
        ..ConditionFilter::default()
    };
    let rows = f.handle.fleet().list_conditions(&filter).await?;
    let stamped: &ConditionRow = rows.first().ok_or_else(|| anyhow::anyhow!("row missing"))?;
    assert!(stamped.last_checked_at.is_some());
    assert!(
        !f.handle
            .fleet()
            .condition_holds(stamped, Utc::now())
            .await?
    );
    let much_later = Utc::now() + Duration::seconds(i64::from(stamped.check_every_secs) + 5);
    assert!(
        f.handle
            .fleet()
            .condition_holds(stamped, much_later)
            .await?
    );
    f.db.cleanup().await
}

#[test]
fn sql_check_due_follows_last_checked_at_and_the_interval() -> anyhow::Result<()> {
    let mut row = sample_row();
    let now = Utc::now();
    assert!(sql_check_due(&row, now));
    row.last_checked_at = Some(now - Duration::seconds(299));
    assert!(!sql_check_due(&row, now));
    row.last_checked_at = Some(now - Duration::seconds(300));
    assert!(sql_check_due(&row, now));
    Ok(())
}

#[test]
fn expectation_met_inverts_for_no_rows() {
    assert!(expectation_met(SqlExpect::Rows, true));
    assert!(!expectation_met(SqlExpect::Rows, false));
    assert!(expectation_met(SqlExpect::NoRows, false));
    assert!(!expectation_met(SqlExpect::NoRows, true));
}

pub fn sample_row() -> ConditionRow {
    let now = Utc::now();
    ConditionRow {
        id: 1,
        condition_uuid: Uuid::new_v4(),
        created_at: now,
        machine_uuid: Uuid::new_v4(),
        created_by_session: None,
        project_id: None,
        thread_slug: None,
        kind: "sql".to_owned(),
        spec: json!({"query": "SELECT 1"}),
        not_before: now,
        deadline: None,
        check_every_secs: 300,
        last_checked_at: None,
        action: "message".to_owned(),
        target_session: None,
        folder: None,
        brief: "b".to_owned(),
        state: "armed".to_owned(),
        fired_at: None,
        outcome: None,
        attempts: 0,
        next_attempt_at: None,
        last_error: None,
    }
}
