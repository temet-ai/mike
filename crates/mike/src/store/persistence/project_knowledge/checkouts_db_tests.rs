use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};
use crate::workspace::checkouts::{place_folder, record_origins};

const REMOTE: &str = "github.com/temet-ai/tether";

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    anyhow::ensure!(output.status.success(), "git {args:?} failed");
    Ok(())
}

fn repo(parent: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let path = fs::canonicalize(parent)?.join(name);
    fs::create_dir_all(path.join("src"))?;
    git(&path, &["init", "-q"])?;
    git(
        &path,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/temet-ai/tether.git",
        ],
    )?;
    Ok(path)
}

async fn location(
    handle: &PersistHandle,
    machine: Uuid,
    root: &str,
    project: i64,
) -> anyhow::Result<Uuid> {
    handle
        .add_checkout_location(machine, "host", root, project)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the location is free"))
}

async fn origin_of(handle: &PersistHandle, location: Uuid) -> anyhow::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT origin_url FROM projects.locations WHERE location_uuid = $1")
            .bind(location)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn a_checkout_finds_the_project_of_its_remote_on_another_machine() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/mnt/data/projects/tether").await?;
    let (omit, mac) = (Uuid::new_v4(), Uuid::new_v4());
    let elsewhere = location(&handle, omit, "/mnt/data/projects/tether", project).await?;
    handle.set_location_origin(elsewhere, REMOTE).await?;

    let tmp = tempfile::tempdir()?;
    let checkout = repo(tmp.path(), "tether")?;
    let inside = checkout.join("src").to_string_lossy().into_owned();

    assert_eq!(
        place_folder(&handle, mac, "mac", &inside).await?,
        Some(project)
    );
    assert_eq!(handle.locations_on_machine(mac).await?.len(), 1);
    assert_eq!(
        handle.project_id_enclosing(mac, &inside).await?,
        Some(project)
    );

    db.cleanup().await
}

#[tokio::test]
async fn a_remote_on_two_projects_places_nothing() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let machine = Uuid::new_v4();
    for name in ["/one", "/two"] {
        let project = seed_project(handle.pool(), name).await?;
        let at = location(&handle, Uuid::new_v4(), name, project).await?;
        handle.set_location_origin(at, REMOTE).await?;
    }
    let tmp = tempfile::tempdir()?;
    let checkout = repo(tmp.path(), "tether")?;

    assert_eq!(
        place_folder(&handle, machine, "mac", &checkout.to_string_lossy()).await?,
        None
    );
    assert!(handle.locations_on_machine(machine).await?.is_empty());

    db.cleanup().await
}

#[tokio::test]
async fn a_repository_no_project_holds_gets_one_project_across_machines() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (mac, omit) = (Uuid::new_v4(), Uuid::new_v4());
    let tmp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR"))?;
    let here = repo(tmp.path(), "here")?;
    let there = repo(tmp.path(), "there")?;

    let first = place_folder(&handle, mac, "mac", &here.to_string_lossy()).await?;
    let second = place_folder(&handle, omit, "omit", &there.to_string_lossy()).await?;

    assert!(first.is_some());
    assert_eq!(first, second);
    assert_eq!(handle.project_ids_for_origin(REMOTE).await?.len(), 1);

    db.cleanup().await
}

#[tokio::test]
async fn a_clone_in_a_scratch_folder_makes_no_project() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let tmp = tempfile::tempdir_in("/tmp")?;
    let clone = repo(tmp.path(), "clone")?;

    assert_eq!(
        place_folder(&handle, Uuid::new_v4(), "mac", &clone.to_string_lossy()).await?,
        None
    );

    db.cleanup().await
}

#[tokio::test]
async fn a_repository_with_no_remote_makes_no_project() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let tmp = tempfile::tempdir()?;
    let path = fs::canonicalize(tmp.path())?.join("local");
    fs::create_dir_all(&path)?;
    git(&path, &["init", "-q"])?;

    assert_eq!(
        place_folder(&handle, Uuid::new_v4(), "mac", &path.to_string_lossy()).await?,
        None
    );

    db.cleanup().await
}

#[tokio::test]
async fn a_folder_outside_a_repository_takes_the_location_above_it() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/any").await?;
    let machine = Uuid::new_v4();
    let tmp = tempfile::tempdir()?;
    let root = fs::canonicalize(tmp.path())?;
    let inner = root.join("notes");
    fs::create_dir_all(&inner)?;
    location(&handle, machine, &root.to_string_lossy(), project).await?;

    assert_eq!(
        place_folder(&handle, machine, "mac", &inner.to_string_lossy()).await?,
        Some(project)
    );

    db.cleanup().await
}

#[tokio::test]
async fn only_a_checkout_root_records_its_remote() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/any").await?;
    let machine = Uuid::new_v4();
    let tmp = tempfile::tempdir()?;
    let checkout = repo(tmp.path(), "tether")?;
    let root = location(&handle, machine, &checkout.to_string_lossy(), project).await?;
    let inner = checkout.join("src");
    let folder = location(&handle, machine, &inner.to_string_lossy(), project).await?;

    record_origins(&handle, machine).await?;

    assert_eq!(origin_of(&handle, root).await?.as_deref(), Some(REMOTE));
    assert_eq!(origin_of(&handle, folder).await?, None);

    db.cleanup().await
}
