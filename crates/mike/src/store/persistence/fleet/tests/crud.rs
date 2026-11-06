use serde_json::json;
use uuid::Uuid;

use super::seed::seed_run;
use super::{fixture, new_fleet};
use crate::store::persistence::test_support::seed_fleet_running;

#[tokio::test]
async fn insert_returns_the_draft_row_with_defaults() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let creator = Uuid::new_v4();
    let mut new = new_fleet("night-build", "/tmp/fleet-a");
    new.created_by = Some(creator);
    let row = f.handle.insert_fleet(&new).await?;

    assert_eq!(row.slug, "night-build");
    assert_eq!(row.project_path, "/tmp/fleet-a");
    assert_eq!(row.state, "draft");
    assert_eq!(row.definition, new.definition);
    assert_eq!(row.missing, new.missing);
    assert_eq!(row.created_by, Some(creator));
    assert_eq!(row.run_id, None);
    assert_eq!(row.start_report, None);
    assert_eq!(row.created_at, row.updated_at);
    assert_eq!(row.ready_at, None);
    assert_eq!(row.started_at, None);
    assert_eq!(row.finished_at, None);

    let other = f.handle.insert_fleet(&new).await?;
    assert_ne!(other.fleet_uuid, row.fleet_uuid);
    f.db.cleanup().await
}

#[tokio::test]
async fn find_resolves_a_uuid_and_the_newest_row_for_a_slug() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let older = f
        .handle
        .insert_fleet(&new_fleet("same-slug", "/tmp/fleet-a"))
        .await?;
    let newer = f
        .handle
        .insert_fleet(&new_fleet("same-slug", "/tmp/fleet-a"))
        .await?;

    let by_uuid = f.handle.find_fleet(&older.fleet_uuid.to_string()).await?;
    assert_eq!(by_uuid, Some(older.clone()));

    let by_slug = f.handle.find_fleet("same-slug").await?;
    assert_eq!(by_slug, Some(newer));

    assert_eq!(f.handle.find_fleet("no-such-slug").await?, None);
    assert_eq!(
        f.handle.find_fleet(&Uuid::new_v4().to_string()).await?,
        None
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn list_filters_by_project_and_returns_newest_first_within_the_limit() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = f
        .handle
        .insert_fleet(&new_fleet("one", "/tmp/fleet-a"))
        .await?;
    let second = f
        .handle
        .insert_fleet(&new_fleet("two", "/tmp/fleet-a"))
        .await?;
    let third = f
        .handle
        .insert_fleet(&new_fleet("three", "/tmp/fleet-a"))
        .await?;
    let elsewhere = f
        .handle
        .insert_fleet(&new_fleet("four", "/tmp/fleet-b"))
        .await?;

    let scoped = f
        .handle
        .list_fleet_headers(Some("/tmp/fleet-a"), 10)
        .await?;
    let ids: Vec<_> = scoped.iter().map(|r| r.fleet_uuid).collect();
    assert_eq!(
        ids,
        vec![third.fleet_uuid, second.fleet_uuid, first.fleet_uuid]
    );

    let limited = f.handle.list_fleet_headers(Some("/tmp/fleet-a"), 2).await?;
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].fleet_uuid, third.fleet_uuid);

    let all = f.handle.list_fleet_headers(None, 10).await?;
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].fleet_uuid, elsewhere.fleet_uuid);

    assert!(
        f.handle
            .list_fleet_headers(Some("/tmp/nowhere"), 10)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn save_definition_writes_when_unchanged_and_refuses_a_lost_update() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let row = f
        .handle
        .insert_fleet(&new_fleet("save", "/tmp/fleet-a"))
        .await?;
    let seen = row.updated_at;

    let mine = json!({"seats": ["coordinator", "acceptance"]});
    let none_missing = json!([]);
    let saved_at = f
        .handle
        .save_fleet_definition(row.fleet_uuid, &mine, &none_missing, seen)
        .await?
        .ok_or_else(|| anyhow::anyhow!("a save against the unchanged row must write"))?;
    assert!(saved_at > seen);
    let saved = row.saved(mine.clone(), none_missing.clone(), saved_at);
    assert_eq!(saved.state, "draft");

    let theirs = json!({"seats": ["someone-elses-edit"]});
    let lost = f
        .handle
        .save_fleet_definition(row.fleet_uuid, &theirs, &none_missing, seen)
        .await?;
    assert_eq!(lost, None, "a writer holding a stale updated_at must lose");

    let stored = f.handle.find_fleet(&row.fleet_uuid.to_string()).await?;
    assert_eq!(stored, Some(saved.clone()));

    f.handle
        .save_fleet_definition(row.fleet_uuid, &theirs, &none_missing, saved.updated_at)
        .await?
        .ok_or_else(|| anyhow::anyhow!("a retry with the fresh updated_at must write"))?;
    let again = f.handle.find_fleet(&row.fleet_uuid.to_string()).await?;
    assert_eq!(again.map(|r| r.definition), Some(theirs.clone()));

    let unknown = f
        .handle
        .save_fleet_definition(Uuid::new_v4(), &theirs, &none_missing, seen)
        .await?;
    assert_eq!(unknown, None);
    f.db.cleanup().await
}

#[tokio::test]
async fn run_lookup_and_state_list_find_a_running_fleet_behind_newer_ones() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let draft = f
        .handle
        .insert_fleet(&new_fleet("by-run-draft", "/tmp/fleet-a"))
        .await?;
    let launched = f
        .handle
        .insert_fleet(&new_fleet("by-run", "/tmp/fleet-a"))
        .await?;
    let launched = seed_fleet_running(&f.handle, launched.fleet_uuid, &run).await?;
    for n in 0..25 {
        f.handle
            .insert_fleet(&new_fleet(&format!("newer-{n}"), "/tmp/fleet-a"))
            .await?;
    }
    assert_eq!(
        f.handle.find_fleet_by_run(&run).await?,
        Some(launched.clone())
    );
    assert_eq!(
        f.handle
            .find_fleet_by_run(&Uuid::new_v4().to_string())
            .await?,
        None
    );

    let running = f.handle.list_fleets_in_states(&["running"]).await?;
    assert!(running.iter().all(|r| r.state == "running"));
    assert!(running.iter().any(|r| r.fleet_uuid == launched.fleet_uuid));
    assert!(!running.iter().any(|r| r.fleet_uuid == draft.fleet_uuid));
    f.db.cleanup().await
}
