use std::process::Command;

#[test]
fn version_and_help_do_not_load_configuration() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    std::fs::write(home.path().join(".env"), "MIKE_MAX_REQUESTS=not-a-number\n")?;
    for arg in ["--version", "-V", "--help"] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike"))
            .arg(arg)
            .env_clear()
            .env("HOME", home.path())
            .env("MIKE_DATABASE_URL", "not-a-database-url")
            .env("MIKE_MAX_REQUESTS", "not-a-number")
            .current_dir(home.path())
            .output()?;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout)?;
        if arg == "--help" {
            assert!(text.contains("Usage:"));
        } else {
            assert!(text.starts_with(&format!("mike {} (", env!("CARGO_PKG_VERSION"))));
            assert!(text.contains(", built "));
        }
    }
    assert_eq!(std::fs::read_dir(home.path())?.count(), 1);
    Ok(())
}

#[test]
fn build_info_ignores_invalid_configuration_and_is_read_only() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join("config/mike/config.toml");
    std::fs::create_dir_all(
        config
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config parent"))?,
    )?;
    let poison = "planted-private-value = [invalid TOML\n";
    std::fs::write(&config, poison)?;
    std::fs::write(home.path().join(".env"), poison)?;
    let output = Command::new(env!("CARGO_BIN_EXE_mike"))
        .arg("--build-info")
        .env_clear()
        .env("PATH", "")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("MIKE_DATABASE_URL", "planted-private-invalid-database")
        .env("MIKE_MAX_REQUESTS", "planted-private-invalid-number")
        .env("MIKE_BUILD_SHA", "planted-runtime-identity-override")
        .current_dir(home.path())
        .output()?;
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let text = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        value,
        serde_json::json!({
            "schema_version": 1, "product": "mike", "version": env!("CARGO_PKG_VERSION"),
            "source_sha": env!("MIKE_BUILD_SHA"), "source_dirty": env!("MIKE_BUILD_DIRTY") != "false",
            "built_at": env!("MIKE_BUILD_TIME"),
        })
    );
    assert!(!text.contains("planted-"));
    assert!(!text.contains(&home.path().to_string_lossy().to_string()));
    assert_eq!(std::fs::read_to_string(config)?, poison);
    assert_eq!(std::fs::read_to_string(home.path().join(".env"))?, poison);
    assert_eq!(std::fs::read_dir(home.path())?.count(), 2);
    assert!(!home.path().join("state").exists());
    assert!(!home.path().join("data").exists());
    Ok(())
}

#[test]
fn build_info_rejects_extra_arguments_before_loading_configuration() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    for args in [["--build-info", "serve"], ["--build-info", "--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike"))
            .args(args)
            .env_clear()
            .env("HOME", home.path())
            .env("MIKE_MAX_REQUESTS", "planted-invalid-number")
            .current_dir(home.path())
            .output()?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8(output.stderr)?.contains("planted-invalid-number"));
    }
    assert_eq!(std::fs::read_dir(home.path())?.count(), 0);
    Ok(())
}
