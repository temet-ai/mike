use super::*;

const COLLECTOR_FILE: &str = "{\n  \"machine_uuid\": \"6f1c5c1e-3a52-4f0b-8a0d-0f8e6a4b9c11\",\n  \"machine_name\": \"desktop\"\n}\n";

#[test]
fn state_dir_prefers_xdg_state_home() -> anyhow::Result<()> {
    let dir = state_dir(Some("/xdg"), Some("/home/u"))?;
    assert_eq!(dir, PathBuf::from("/xdg/mike"));
    Ok(())
}

#[test]
fn state_dir_falls_back_to_home_when_xdg_is_empty() -> anyhow::Result<()> {
    let dir = state_dir(Some(""), Some("/home/u"))?;
    assert_eq!(dir, PathBuf::from("/home/u/.local/state/mike"));
    Ok(())
}

#[test]
fn state_dir_without_xdg_or_home_is_an_error() {
    assert!(matches!(
        state_dir(None, Some("")),
        Err(MachineIdentityError::NoStateDir)
    ));
}

#[tokio::test]
async fn read_file_parses_the_file_the_python_collector_writes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = file_in(dir.path());
    tokio::fs::write(&path, COLLECTOR_FILE).await?;
    let identity = read_file(&path)
        .await?
        .ok_or_else(|| anyhow::anyhow!("identity file was not read"))?;
    assert_eq!(identity.machine_name, "desktop");
    assert_eq!(
        identity.machine_uuid.to_string(),
        "6f1c5c1e-3a52-4f0b-8a0d-0f8e6a4b9c11"
    );
    Ok(())
}

#[tokio::test]
async fn read_file_missing_is_none() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(read_file(&file_in(dir.path())).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn read_file_malformed_names_the_path() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = file_in(dir.path());
    tokio::fs::write(&path, "{not json").await?;
    let error = read_file(&path)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("malformed file was accepted"))?;
    assert!(matches!(error, MachineIdentityError::Malformed { .. }));
    assert!(error.to_string().contains(FILE_NAME));
    Ok(())
}

#[tokio::test]
async fn read_file_unreadable_path_is_an_io_error() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let error = read_file(dir.path())
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("a directory was read as a file"))?;
    assert!(matches!(error, MachineIdentityError::Io { .. }));
    Ok(())
}

#[tokio::test]
async fn load_or_create_in_writes_an_identity_the_collector_can_read() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let state = dir.path().join("mike");
    let made = load_or_create_in(&state).await?;
    assert!(!made.machine_name.is_empty());
    let text = tokio::fs::read_to_string(file_in(&state)).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed["machine_uuid"], made.machine_uuid.to_string());
    assert_eq!(load_or_create_in(&state).await?, made);
    Ok(())
}

#[tokio::test]
async fn load_or_create_in_keeps_an_existing_collector_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    tokio::fs::write(file_in(dir.path()), COLLECTOR_FILE).await?;
    let identity = load_or_create_in(dir.path()).await?;
    assert_eq!(identity.machine_name, "desktop");
    Ok(())
}

#[tokio::test]
async fn concurrent_load_or_create_in_agrees_on_one_identity() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (a, b) = tokio::join!(load_or_create_in(dir.path()), load_or_create_in(dir.path()));
    assert_eq!(a?, b?);
    Ok(())
}
