use std::path::Path;

use uuid::Uuid;

use super::{MergeError, legacy_state_dir, merge_legacy_identity};
use crate::machine_identity::{MachineIdentity, file_in};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

async fn handle() -> anyhow::Result<(TestDbHandle, PersistHandle)> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok((db, handle))
}

fn identity(machine_uuid: Uuid) -> MachineIdentity {
    MachineIdentity {
        machine_uuid,
        machine_name: "host".to_owned(),
    }
}

fn write_identity(dir: &Path, machine_uuid: Uuid) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        file_in(dir),
        format!("{{\"machine_uuid\": \"{machine_uuid}\", \"machine_name\": \"host\"}}\n"),
    )?;
    Ok(())
}

async fn register_legacy_location(
    handle: &PersistHandle,
    legacy: Uuid,
    root: &str,
) -> anyhow::Result<()> {
    let project = seed_project(handle.pool(), "/test/legacy-file").await?;
    sqlx::query("INSERT INTO projects.machines(machine_uuid, name) VALUES ($1, 'host')")
        .bind(legacy)
        .execute(handle.pool())
        .await?;
    sqlx::query(
        "INSERT INTO projects.locations(project_id, machine_uuid, root_path) VALUES ($1, $2, $3)",
    )
    .bind(project)
    .bind(legacy)
    .bind(root)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn locations_of(handle: &PersistHandle, machine: Uuid) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM projects.locations WHERE machine_uuid = $1")
            .bind(machine)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[test]
fn legacy_state_dir_is_the_mike_knowledge_folder_under_home() {
    assert_eq!(
        legacy_state_dir(Some("/home/u")),
        Some("/home/u/.local/state/mike-knowledge".into())
    );
    assert_eq!(legacy_state_dir(Some("")), None);
    assert_eq!(legacy_state_dir(None), None);
}

#[tokio::test]
async fn merge_legacy_identity_with_a_missing_file_does_nothing() -> anyhow::Result<()> {
    let (_db, handle) = handle().await?;
    let dir = tempfile::tempdir()?;
    let merged = merge_legacy_identity(&handle, &identity(Uuid::new_v4()), dir.path()).await?;
    assert!(merged.is_none());
    assert!(!dir.path().join("machine.json.merged").exists());
    Ok(())
}

#[tokio::test]
async fn merge_legacy_identity_with_the_current_uuid_leaves_the_file() -> anyhow::Result<()> {
    let (_db, handle) = handle().await?;
    let dir = tempfile::tempdir()?;
    let current = Uuid::new_v4();
    write_identity(dir.path(), current)?;
    let merged = merge_legacy_identity(&handle, &identity(current), dir.path()).await?;
    assert!(merged.is_none());
    assert!(file_in(dir.path()).exists());
    Ok(())
}

#[tokio::test]
async fn merge_legacy_identity_repoints_the_legacy_uuid_then_renames_the_file() -> anyhow::Result<()>
{
    let (_db, handle) = handle().await?;
    let dir = tempfile::tempdir()?;
    let (legacy, current) = (Uuid::new_v4(), Uuid::new_v4());
    write_identity(dir.path(), legacy)?;
    register_legacy_location(&handle, legacy, "/legacy/checkout").await?;
    let merged = merge_legacy_identity(&handle, &identity(current), dir.path())
        .await?
        .ok_or_else(|| anyhow::anyhow!("a differing legacy identity was not merged"))?;
    assert_eq!(merged.0, legacy);
    assert_eq!(merged.1.locations_moved, 1);
    assert_eq!(locations_of(&handle, legacy).await?, 0);
    assert_eq!(locations_of(&handle, current).await?, 1);
    assert!(!file_in(dir.path()).exists());
    assert!(dir.path().join("machine.json.merged").exists());
    let again = merge_legacy_identity(&handle, &identity(current), dir.path()).await?;
    assert!(again.is_none());
    Ok(())
}

#[tokio::test]
async fn merge_legacy_identity_with_a_malformed_file_names_the_file_and_is_not_retried()
-> anyhow::Result<()> {
    let (_db, handle) = handle().await?;
    let dir = tempfile::tempdir()?;
    std::fs::write(file_in(dir.path()), "{not json")?;
    let error = merge_legacy_identity(&handle, &identity(Uuid::new_v4()), dir.path())
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("a malformed legacy file was accepted"))?;
    assert!(matches!(error, MergeError::Identity(_)));
    assert!(error.to_string().contains("machine.json"));
    assert!(!error.is_retryable());
    Ok(())
}
