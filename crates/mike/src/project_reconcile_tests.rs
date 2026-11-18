use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

use super::{ReconcileReport, reconcile_unplaced_projects};
use crate::machine_identity::MachineIdentity;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

const REMOTE: &str = "github.com/temet-ai/traceroot";

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    anyhow::ensure!(output.status.success(), "git {args:?} failed");
    Ok(())
}

fn checkout(parent: &Path, name: &str, remote: Option<&str>) -> anyhow::Result<PathBuf> {
    let path = fs::canonicalize(parent)?.join(name);
    fs::create_dir_all(&path)?;
    git(&path, &["init", "-q"])?;
    if let Some(remote) = remote {
        git(&path, &["remote", "add", "origin", remote])?;
    }
    Ok(path)
}

fn machine() -> MachineIdentity {
    MachineIdentity {
        machine_uuid: Uuid::new_v4(),
        machine_name: "omit".into(),
    }
}

async fn located_project(handle: &PersistHandle, path: &str) -> anyhow::Result<i64> {
    let project = seed_project(handle.pool(), path).await?;
    let location = handle
        .knowledge()
        .add_checkout_location(Uuid::new_v4(), "mac", path, project)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the location is free"))?;
    handle
        .knowledge()
        .set_location_origin(location, REMOTE)
        .await?;
    Ok(project)
}

async fn insert_lesson(handle: &PersistHandle, project: i64) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, 'a lesson written under the phantom', ls.id, lsrc.id, 'reconcile'
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'
         RETURNING id",
    )
    .bind(project)
    .fetch_one(handle.pool())
    .await?)
}

async fn merged_into(handle: &PersistHandle, project: i64) -> anyhow::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT merged_into_id FROM projects.projects WHERE id = $1")
            .bind(project)
            .fetch_one(handle.pool())
            .await?,
    )
}

async fn location_on(
    handle: &PersistHandle,
    machine: Uuid,
) -> anyhow::Result<Vec<(i64, Option<String>)>> {
    Ok(sqlx::query_as(
        "SELECT project_id, origin_url FROM projects.locations WHERE machine_uuid = $1",
    )
    .bind(machine)
    .fetch_all(handle.pool())
    .await?)
}

#[tokio::test]
async fn unplaced_project_sharing_a_located_origin_is_merged_and_its_items_move()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let live = located_project(&handle, "/Users/dawid/projects/traceroot").await?;
    let tmp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR"))?;
    let path = checkout(tmp.path(), "traceroot", Some(REMOTE))?;
    let phantom = seed_project(handle.pool(), &path.to_string_lossy()).await?;
    let lesson = insert_lesson(&handle, phantom).await?;
    let here = machine();

    let report = reconcile_unplaced_projects(&handle, &here).await?;

    assert_eq!(
        report,
        ReconcileReport {
            merged: 1,
            registered: 0
        }
    );
    assert_eq!(merged_into(&handle, phantom).await?, Some(live));
    assert_eq!(
        location_on(&handle, here.machine_uuid).await?,
        vec![(live, Some(REMOTE.to_string()))]
    );
    let lesson_project: i64 =
        sqlx::query_scalar("SELECT project_id FROM memory.lessons WHERE id = $1")
            .bind(lesson)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(lesson_project, live);
    let item_project: Option<i64> = sqlx::query_scalar(
        "SELECT project_id FROM memory.knowledge_items WHERE kind = 'lesson' AND source_id = $1",
    )
    .bind(lesson)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(item_project, Some(live));
    assert_eq!(
        reconcile_unplaced_projects(&handle, &here).await?,
        ReconcileReport::default()
    );

    db.cleanup().await
}

#[tokio::test]
async fn unplaced_project_with_a_unique_origin_gets_its_own_location() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let tmp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR"))?;
    let path = checkout(
        tmp.path(),
        "omarchy",
        Some("github.com/dawiddutoit/omarchy"),
    )?;
    let phantom = seed_project(handle.pool(), &path.to_string_lossy()).await?;
    let here = machine();

    let report = reconcile_unplaced_projects(&handle, &here).await?;

    assert_eq!(
        report,
        ReconcileReport {
            merged: 0,
            registered: 1
        }
    );
    assert_eq!(merged_into(&handle, phantom).await?, None);
    assert_eq!(
        location_on(&handle, here.machine_uuid).await?,
        vec![(phantom, Some("github.com/dawiddutoit/omarchy".to_string()))]
    );
    assert_eq!(
        reconcile_unplaced_projects(&handle, &here).await?,
        ReconcileReport::default()
    );

    db.cleanup().await
}

#[tokio::test]
async fn unplaced_project_in_a_scratch_folder_is_left_alone() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    located_project(&handle, "/Users/dawid/projects/traceroot").await?;
    let tmp = tempfile::tempdir_in("/tmp")?;
    let path = checkout(tmp.path(), "traceroot", Some(REMOTE))?;
    let phantom = seed_project(handle.pool(), &path.to_string_lossy()).await?;
    let here = machine();

    let report = reconcile_unplaced_projects(&handle, &here).await?;

    assert_eq!(report, ReconcileReport::default());
    assert_eq!(merged_into(&handle, phantom).await?, None);
    assert!(location_on(&handle, here.machine_uuid).await?.is_empty());

    db.cleanup().await
}

#[tokio::test]
async fn unplaced_project_of_another_machines_path_is_left_alone() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    located_project(&handle, "/Users/dawid/projects/traceroot").await?;
    let phantom = seed_project(handle.pool(), "/Users/nobody/elsewhere/traceroot").await?;
    let here = machine();

    assert_eq!(
        reconcile_unplaced_projects(&handle, &here).await?,
        ReconcileReport::default()
    );
    assert_eq!(merged_into(&handle, phantom).await?, None);

    db.cleanup().await
}

#[tokio::test]
async fn unplaced_project_with_sessions_is_not_merged_by_origin_alone() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    located_project(&handle, "/Users/dawid/projects/traceroot").await?;
    let tmp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR"))?;
    let path = checkout(tmp.path(), "traceroot", Some(REMOTE))?;
    let phantom = seed_project(handle.pool(), &path.to_string_lossy()).await?;
    sqlx::query("INSERT INTO sessions.sessions (session_uuid, project_id) VALUES ($1, $2)")
        .bind(uuid::Uuid::new_v4())
        .bind(phantom)
        .execute(handle.pool())
        .await?;

    let report = reconcile_unplaced_projects(&handle, &machine()).await?;

    assert_eq!(report, ReconcileReport::default());
    assert_eq!(merged_into(&handle, phantom).await?, None);

    db.cleanup().await
}
