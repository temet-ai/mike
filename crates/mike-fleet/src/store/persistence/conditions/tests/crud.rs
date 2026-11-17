use serde_json::json;
use uuid::Uuid;

use super::{fixture, new_condition, time_condition};
use crate::store::persistence::conditions::ConditionFilter;
use crate::store::persistence::fleet_repo::FleetPersist as _;

fn thread_filter(slug: &str, state: Option<&str>) -> ConditionFilter {
    ConditionFilter {
        thread_slug: Some(slug.to_owned()),
        state: state.map(str::to_owned),
        created_by_session: None,
        limit: 50,
    }
}

#[tokio::test]
async fn condition_insert_list_cancel_round_trip() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let slug = format!("thread-{}", Uuid::new_v4());
    let mut new = time_condition();
    new.thread_slug = Some(slug.clone());
    new.brief = "start the follow-up".to_owned();
    let row = f.handle.fleet().insert_condition(&new).await?;
    assert_eq!(row.state, "armed");
    assert_eq!(row.kind, "time");
    assert_eq!(row.brief, "start the follow-up");
    assert_eq!(row.attempts, 0);
    assert_eq!(row.machine_uuid, new.machine_uuid);

    let listed = f
        .handle
        .fleet()
        .list_conditions(&thread_filter(&slug, Some("armed")))
        .await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].condition_uuid, row.condition_uuid);

    assert!(
        f.handle
            .fleet()
            .cancel_condition(row.condition_uuid)
            .await?
    );
    assert!(
        !f.handle
            .fleet()
            .cancel_condition(row.condition_uuid)
            .await?
    );

    let cancelled = f
        .handle
        .fleet()
        .list_conditions(&thread_filter(&slug, Some("cancelled")))
        .await?;
    assert_eq!(cancelled.len(), 1);
    let armed = f
        .handle
        .fleet()
        .list_conditions(&thread_filter(&slug, Some("armed")))
        .await?;
    assert!(armed.is_empty());
    f.db.cleanup().await
}

#[tokio::test]
async fn condition_list_filters_by_registering_session_and_orders_newest_first()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let mut first = time_condition();
    first.created_by_session = Some(session);
    let mut second = new_condition("time", json!({}));
    second.created_by_session = Some(session);
    let first = f.handle.fleet().insert_condition(&first).await?;
    let second = f.handle.fleet().insert_condition(&second).await?;
    f.handle.fleet().insert_condition(&time_condition()).await?;

    let filter = ConditionFilter {
        created_by_session: Some(session),
        limit: 50,
        ..ConditionFilter::default()
    };
    let rows = f.handle.fleet().list_conditions(&filter).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.condition_uuid).collect();
    assert_eq!(ids, vec![second.condition_uuid, first.condition_uuid]);
    f.db.cleanup().await
}

#[tokio::test]
async fn condition_insert_with_invalid_spec_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let slug = format!("thread-{}", Uuid::new_v4());
    let mut new = new_condition("sql", json!({"query": "SELECT 1; SELECT 2"}));
    new.thread_slug = Some(slug.clone());
    let result = f.handle.fleet().insert_condition(&new).await;
    assert!(result.is_err());
    let rows = f
        .handle
        .fleet()
        .list_conditions(&thread_filter(&slug, None))
        .await?;
    assert!(rows.is_empty());
    f.db.cleanup().await
}
