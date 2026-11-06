use super::{persist_boot_setting_at, save_database_mode_at, save_provider_key_at};

#[test]
fn terminal_choice_is_saved_in_the_selected_toml() -> anyhow::Result<()> {
    let dir = private_tempdir()?;
    let path = dir.path().join("config.toml");
    persist_boot_setting_at(&path, "launch_backend", "none")?;
    let saved: toml::Table = std::fs::read_to_string(&path)?.parse()?;
    assert_eq!(saved["launch_backend"].as_str(), Some("none"));
    Ok(())
}

#[test]
fn trial_to_persistent_writes_flag_and_url_together() -> anyhow::Result<()> {
    let dir = private_tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "persist_enabled = false\ndb_fallback_enabled = true\nlaunch_backend = \"none\"\n",
    )?;
    let database_url = "postgres://user:fake-password@localhost/mike";
    save_database_mode_at(&path, Some(database_url))?;
    let saved: toml::Table = std::fs::read_to_string(&path)?.parse()?;
    assert_eq!(saved["persist_enabled"].as_bool(), Some(true));
    assert_eq!(saved["db_fallback_enabled"].as_bool(), Some(false));
    assert_eq!(saved["database_url"].as_str(), Some(database_url));
    assert_eq!(saved["launch_backend"].as_str(), Some("none"));
    Ok(())
}

#[test]
fn choosing_trial_retains_the_durable_url_for_later_reconnection() -> anyhow::Result<()> {
    let dir = private_tempdir()?;
    let path = dir.path().join("config.toml");
    let database_url = "postgres://user:fake-password@localhost/mike";
    save_database_mode_at(&path, Some(database_url))?;
    save_database_mode_at(&path, None)?;
    let saved: toml::Table = std::fs::read_to_string(&path)?.parse()?;
    assert_eq!(saved["persist_enabled"].as_bool(), Some(false));
    assert_eq!(saved["database_url"].as_str(), Some(database_url));
    Ok(())
}

#[test]
fn special_character_credentials_round_trip_as_toml_data() -> anyhow::Result<()> {
    let dir = private_tempdir()?;
    let path = dir.path().join("config.toml");
    let key = "sq? space 'double\" dollar$ backtick` backslash\\ hash# 雪";
    let database_url = "postgres://user:fake ' \" $ ` \\ # 雪@localhost/mike";
    save_provider_key_at(&path, key)?;
    save_database_mode_at(&path, Some(database_url))?;
    let saved: toml::Table = std::fs::read_to_string(&path)?.parse()?;
    assert_eq!(saved["steerer"]["openrouter_api_key"].as_str(), Some(key));
    assert_eq!(saved["database_url"].as_str(), Some(database_url));
    Ok(())
}

#[test]
fn malformed_existing_config_is_not_overwritten_or_partially_updated() -> anyhow::Result<()> {
    let dir = private_tempdir()?;
    let path = dir.path().join("config.toml");
    let previous = "persist_enabled = false\n[broken\n";
    std::fs::write(&path, previous)?;
    assert!(save_database_mode_at(&path, Some("postgres://host/mike")).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, previous);
    Ok(())
}

fn private_tempdir() -> anyhow::Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}
