use super::*;

fn parse(text: &str) -> Result<toml::Table> {
    Ok(text.parse()?)
}

#[test]
fn updates_preserve_other_settings_and_back_up_exact_previous_bytes() -> Result<()> {
    let dir = private_tempdir()?;
    let file = dir.path().join("installation space/config.toml");
    assert!(
        update_selected(
            &file,
            parse("persist_enabled = true\ndatabase_url = 'postgres://u:password@localhost/db'\n")?
        )?
        .is_none()
    );
    let original = std::fs::read_to_string(&file)?;
    let backup =
        update_selected(&file, parse("persist_enabled = false\n")?)?.context("backup missing")?;
    assert_eq!(std::fs::read_to_string(backup)?, original);
    let saved = super::super::load::file::read(&file)?;
    assert_eq!(saved["persist_enabled"].as_bool(), Some(false));
    assert_eq!(
        saved["database_url"].as_str(),
        Some("postgres://u:password@localhost/db")
    );
    Ok(())
}

#[test]
fn literal_secret_round_trip_has_no_shell_interpretation() -> Result<()> {
    let dir = private_tempdir()?;
    let file = dir.path().join("config.toml");
    let key = "s spaces ' \" $HOME `id` \\ # λ";
    let updates = toml::Table::from_iter([(
        "steerer".into(),
        toml::Value::Table(toml::Table::from_iter([(
            "openrouter_api_key".into(),
            toml::Value::String(key.into()),
        )])),
    )]);
    update_selected(&file, updates)?;
    let saved = super::super::load::file::read(&file)?;
    assert_eq!(saved["steerer"]["openrouter_api_key"].as_str(), Some(key));
    Ok(())
}

#[test]
fn invalid_update_does_not_modify_previous_valid_file_or_echo_values() -> Result<()> {
    let dir = private_tempdir()?;
    let file = dir.path().join("config.toml");
    update_selected(&file, parse("persist_enabled = false")?)?;
    let before = std::fs::read(&file)?;
    let error = update_selected(&file, parse("max_requests = 'planted-secret'")?)
        .err()
        .context("invalid type was accepted")?;
    assert!(!error.to_string().contains("planted-secret"));
    assert_eq!(std::fs::read(&file)?, before);
    assert_eq!(std::fs::read_dir(dir.path())?.count(), 2);
    Ok(())
}

#[cfg(unix)]
#[test]
fn private_modes_apply_to_directories_replacements_and_backups() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = private_tempdir()?;
    let file = dir.path().join("nested/private/config.toml");
    update_selected(&file, parse("persist_enabled = false")?)?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
    let backup =
        update_selected(&file, parse("persist_enabled = true")?)?.context("backup missing")?;
    for path in [&file, &backup] {
        assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(
        std::fs::metadata(file.parent().context("parent")?)?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_or_shared_parent_is_rejected_without_writing() -> Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = private_tempdir()?;
    let destination = dir.path().join("shared");
    std::fs::create_dir(&destination)?;
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755))?;
    assert!(
        update_selected(
            &destination.join("config.toml"),
            parse("persist_enabled=false")?
        )
        .is_err()
    );
    symlink(&destination, dir.path().join("link"))?;
    assert!(
        update_selected(
            &dir.path().join("link/config.toml"),
            parse("persist_enabled=false")?
        )
        .is_err()
    );
    assert!(!destination.join("config.toml").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn read_only_configuration_is_not_replaced() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = private_tempdir()?;
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "persist_enabled=false")?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400))?;
    assert!(update_selected(&file, parse("persist_enabled=true")?).is_err());
    assert_eq!(std::fs::read_to_string(&file)?, "persist_enabled=false");
    Ok(())
}

fn private_tempdir() -> Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}
