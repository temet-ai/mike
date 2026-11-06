use super::*;

#[test]
fn keys_map_to_proxy_environment_names_and_nested_tables_use_double_underscores()
-> anyhow::Result<()> {
    let entries = entries_of(
        "database_url = \"postgres://x\"\nlisten_addr = \"127.0.0.1:1\"\n\n[embedding]\nurl = \"http://e\"\n\n[a]\nb = { c = 1 }\n",
    )?;
    let mut names: Vec<_> = entries.iter().map(|e| e.env_name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "MIKE_A__B__C",
            "MIKE_DATABASE_URL",
            "MIKE_EMBEDDING__URL",
            "MIKE_LISTEN_ADDR"
        ]
    );
    Ok(())
}

#[test]
fn scalars_and_arrays_become_the_text_the_environment_would_hold() -> anyhow::Result<()> {
    let entries = entries_of(
        "persist_enabled = false\nmax_requests = 250\nspa_origins = [\"http://a\", \"http://b\"]\nlaunch_claude_args = [\"--model\", \"opus\"]\n",
    )?;
    let value = |name: &str| {
        entries
            .iter()
            .find(|e| e.env_name == name)
            .map(|e| e.setting.clone())
    };
    assert_eq!(
        value("MIKE_PERSIST_ENABLED"),
        Some(Setting::Literal("false".into()))
    );
    assert_eq!(
        value("MIKE_MAX_REQUESTS"),
        Some(Setting::Literal("250".into()))
    );
    assert_eq!(
        value("MIKE_SPA_ORIGINS"),
        Some(Setting::Literal("http://a,http://b".into()))
    );
    assert_eq!(
        value("MIKE_LAUNCH_CLAUDE_ARGS"),
        Some(Setting::Literal("--model opus".into()))
    );
    Ok(())
}

#[test]
fn a_key_ending_in_from_env_names_the_variable_that_holds_the_value() -> anyhow::Result<()> {
    let entries = entries_of("steerer_openrouter_api_key_from_env = \"OPENROUTER_API_KEY\"\n")?;
    assert_eq!(
        entries,
        [from_env(
            "MIKE_STEERER_OPENROUTER_API_KEY",
            "OPENROUTER_API_KEY"
        )]
    );
    Ok(())
}

#[test]
fn upper_case_keys_are_accepted_and_normalised() -> anyhow::Result<()> {
    assert_eq!(
        entries_of("DATABASE_URL = \"x\"\n")?,
        [literal("MIKE_DATABASE_URL", "x")]
    );
    Ok(())
}

#[test]
fn a_proxy_prefixed_or_oddly_spelled_key_is_rejected_with_the_fix() -> anyhow::Result<()> {
    for contents in ["MIKE_DATABASE_URL = \"x\"\n", "\"data-base\" = \"x\"\n"] {
        let (_dir, path) = temp_file(contents)?;
        let error = ConfigFile::open(&path)?
            .entries()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            error.contains("MIKE_ prefix") || error.contains("letters"),
            "{error}"
        );
        assert!(error.contains(&path.display().to_string()), "{error}");
    }
    Ok(())
}

#[test]
fn a_date_or_table_array_value_is_rejected_naming_the_key() -> anyhow::Result<()> {
    for contents in ["since = 2026-10-16\n", "[[hooks]]\nname = \"x\"\n"] {
        let (_dir, path) = temp_file(contents)?;
        let error = ConfigFile::open(&path)?.entries().err();
        assert!(
            matches!(error, Some(UserConfigError::UnsupportedValue { .. })),
            "{contents}"
        );
    }
    Ok(())
}

#[test]
fn invalid_toml_is_a_parse_error_that_names_the_file() -> anyhow::Result<()> {
    let (_dir, path) = temp_file("database_url = \n")?;
    let error = ConfigFile::open(&path).err();
    let text = error.as_ref().map(ToString::to_string).unwrap_or_default();
    assert!(matches!(error, Some(UserConfigError::Parse { .. })));
    assert!(text.contains(&path.display().to_string()), "{text}");
    Ok(())
}

#[test]
fn a_missing_file_reads_as_an_empty_one() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = ConfigFile::open(&dir.path().join("nope.toml"))?;
    assert!(file.entries()?.is_empty());
    Ok(())
}

#[test]
fn a_value_with_dollar_signs_quotes_and_backslashes_survives_a_save_and_reload()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mike").join("config.toml");
    let url = "postgres://mike:pa$$w\"o\\rd${HOME}$(id)@db/mike";
    let mut file = ConfigFile::open(&path)?;
    file.set("MIKE_DATABASE_URL", SettingValue::Text(url))?;
    file.save()?;
    let reloaded = ConfigFile::open(&path)?.entries()?;
    assert_eq!(reloaded, [literal("MIKE_DATABASE_URL", url)]);
    Ok(())
}

#[test]
fn setting_a_key_keeps_comments_and_the_other_keys() -> anyhow::Result<()> {
    let (_dir, path) = temp_file("# my notes\nlisten_addr = \"127.0.0.1:1\"\n")?;
    let mut file = ConfigFile::open(&path)?;
    file.set("launch_backend", SettingValue::Text("tmux"))?;
    file.set("persist_enabled", SettingValue::Bool(false))?;
    file.save()?;
    let text = std::fs::read_to_string(&path)?;
    assert!(text.starts_with("# my notes\n"), "{text}");
    assert!(text.contains("launch_backend = \"tmux\""), "{text}");
    assert!(text.contains("persist_enabled = false"), "{text}");
    assert!(text.contains("listen_addr = \"127.0.0.1:1\""), "{text}");
    Ok(())
}

#[test]
fn removing_a_key_leaves_the_rest() -> anyhow::Result<()> {
    let (_dir, path) = temp_file("a = 1\nb = 2\n")?;
    let mut file = ConfigFile::open(&path)?;
    file.remove("MIKE_A");
    file.save()?;
    assert_eq!(std::fs::read_to_string(&path)?, "b = 2\n");
    Ok(())
}

#[cfg(unix)]
#[test]
fn saving_creates_a_private_file_in_a_private_directory() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mike").join("config.toml");
    let mut file = ConfigFile::open(&path)?;
    file.set("database_url", SettingValue::Text("x"))?;
    file.save()?;
    let mode = |p: &Path| std::fs::metadata(p).map(|m| m.permissions().mode() & 0o777);
    assert_eq!(mode(&path)?, 0o600);
    assert_eq!(mode(path.parent().unwrap_or(dir.path()))?, 0o700);
    Ok(())
}

#[cfg(unix)]
#[test]
fn saving_through_a_symlink_keeps_the_link_and_updates_its_target() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("dotfiles").join("mike.toml");
    std::fs::create_dir_all(real.parent().unwrap_or(dir.path()))?;
    std::fs::write(&real, "a = 1\n")?;
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&real, &link)?;
    let mut file = ConfigFile::open(&link)?;
    file.set("b", SettingValue::Text("2"))?;
    file.save()?;
    assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
    let text = std::fs::read_to_string(&real)?;
    assert!(
        text.contains("a = 1") && text.contains("b = \"2\""),
        "{text}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn saving_through_a_dangling_symlink_creates_the_target() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("target.toml");
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&real, &link)?;
    let mut file = ConfigFile::open(&link)?;
    file.set("b", SettingValue::Text("2"))?;
    file.save()?;
    assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
    assert!(std::fs::read_to_string(&real)?.contains("b = \"2\""));
    Ok(())
}

#[test]
fn saving_leaves_no_temporary_file_beside_the_config() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    let mut file = ConfigFile::open(&path)?;
    file.set("a", SettingValue::Text("1"))?;
    file.save()?;
    let names: Vec<_> = std::fs::read_dir(dir.path())?
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from("config.toml")]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn saving_into_a_read_only_directory_is_a_write_error() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked)?;
    let path = locked.join("config.toml");
    let mut file = ConfigFile::open(&path)?;
    file.set("a", SettingValue::Text("1"))?;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500))?;
    let error = file.save().err();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700))?;
    assert!(
        matches!(error, Some(UserConfigError::Write { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn reading_a_path_below_a_regular_file_is_a_read_error() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x")?;
    let error = ConfigFile::open(&blocker.join("config.toml")).err();
    assert!(
        matches!(error, Some(UserConfigError::Read { .. })),
        "{error:?}"
    );
    Ok(())
}
