#[cfg(unix)]
#[test]
fn persist_env_var_creates_the_config_file_readable_by_its_owner_only() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    let config_file = dir.path().join("mike").join("config.toml");
    super::super::persist::persist_env_var_at(&config_file, "MIKE_DATABASE_URL", "sk-test")?;
    assert_eq!(
        std::fs::read_to_string(&config_file)?,
        "database_url = \"sk-test\"\n"
    );
    assert_eq!(
        std::fs::metadata(&config_file)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(dir.path().join("mike"))?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn persist_env_var_tightens_a_config_file_that_was_world_readable() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    let config_file = dir.path().join("mike/config.toml");
    std::fs::create_dir_all(
        config_file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("parent"))?,
    )?;
    std::fs::set_permissions(
        config_file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("parent"))?,
        std::fs::Permissions::from_mode(0o700),
    )?;
    let original = "persist_enabled = false\n";
    std::fs::write(&config_file, original)?;
    std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(0o644))?;
    super::super::persist::persist_env_var_at(&config_file, "MIKE_DATABASE_URL", "sk-test")?;
    let saved: toml::Table = std::fs::read_to_string(&config_file)?.parse()?;
    assert_eq!(saved["persist_enabled"].as_bool(), Some(false));
    assert_eq!(saved["database_url"].as_str(), Some("sk-test"));
    assert_eq!(
        std::fs::metadata(&config_file)?.permissions().mode() & 0o777,
        0o600
    );
    Ok(())
}

#[test]
fn persist_env_var_under_a_regular_file_fails() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x")?;
    let target = blocker.join("config.toml");
    assert!(
        super::super::persist::persist_env_var_at(&target, "MIKE_TEST_OWNER_ONLY_ERR", "x")
            .is_err()
    );
    Ok(())
}

#[test]
fn persist_with_no_known_config_location_names_the_missing_home() {
    let error =
        super::super::persist::persist_env_var_at(std::path::Path::new(""), "MIKE_TEST_X", "x")
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
    assert!(error.contains("XDG_CONFIG_HOME"), "{error}");
}

#[test]
fn a_dollar_sign_in_a_saved_value_round_trips() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let config_file = dir.path().join("mike/config.toml");
    let url = "postgres://mike:pa$$word${HOME}@db.example/mike";
    super::super::persist::persist_env_var_at(&config_file, "MIKE_DATABASE_URL", url)?;
    let file = crate::user_config::ConfigFile::open(&config_file)?;
    let entries = file.entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].env_name, "MIKE_DATABASE_URL");
    assert_eq!(
        entries[0].setting,
        crate::user_config::Setting::Literal(url.to_string())
    );
    Ok(())
}

#[test]
fn only_all_caps_names_count_as_environment_variable_names() {
    use super::super::persist::names_an_environment_variable as names;
    assert!(names("OPENROUTER_API_KEY"));
    assert!(names("_KEY2"));
    assert!(!names("sk-or-v1-abcdef"));
    assert!(!names("Sk_Or"));
    assert!(!names("AB"));
    assert!(!names(""));
}

#[test]
fn the_openrouter_key_is_stored_by_variable_name_when_the_input_names_one() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let config_file = dir.path().join("mike/config.toml");
    persist_provider(&config_file, "sk-or-v1-secret")?;
    persist_provider(&config_file, "MY_OPENROUTER_KEY")?;
    let text = std::fs::read_to_string(&config_file)?;
    assert!(
        text.contains("openrouter_api_key_from_env = \"MY_OPENROUTER_KEY\""),
        "{text}"
    );
    assert!(
        !text.contains("sk-or-v1-secret"),
        "the old literal key must be removed: {text}"
    );
    Ok(())
}

#[test]
fn a_pasted_openrouter_key_is_stored_in_the_config_file_and_replaces_a_variable_name()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let config_file = dir.path().join("mike/config.toml");
    persist_provider(&config_file, "MY_OPENROUTER_KEY")?;
    persist_provider(&config_file, "sk-or-v1-secret")?;
    let text = std::fs::read_to_string(&config_file)?;
    assert!(
        text.contains("openrouter_api_key = \"sk-or-v1-secret\""),
        "{text}"
    );
    assert!(!text.contains("MY_OPENROUTER_KEY"), "{text}");
    Ok(())
}

fn persist_provider(path: &std::path::Path, value: &str) -> anyhow::Result<()> {
    crate::setup::persistence::save_provider_setting_at(
        path,
        value,
        super::super::persist::names_an_environment_variable(value),
    )
}
