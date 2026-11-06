use std::path::Path;
use std::process::{Command, Output};

fn isolated_home() -> anyhow::Result<tempfile::TempDir> {
    let home = tempfile::tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(home)
}

fn setup(home: &Path, args: &[&str]) -> anyhow::Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_mike"))
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .current_dir(home)
        .arg("--config")
        .arg(home.join("config.toml"))
        .arg("setup")
        .args(args)
        .output()?)
}

#[test]
fn scriptable_setup_requires_restart_acknowledgement_before_touching_config() -> anyhow::Result<()>
{
    let home = isolated_home()?;
    let output = setup(home.path(), &["--mode", "trial"])?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--accept-trial-loss"), "{stderr}");
    assert!(!home.path().join("config.toml").exists());
    Ok(())
}

#[test]
fn invalid_protected_database_input_leaves_the_working_mode_unchanged() -> anyhow::Result<()> {
    let home = isolated_home()?;
    let previous = "persist_enabled = false\n";
    std::fs::write(home.path().join("config.toml"), previous)?;
    let url_file = home.path().join("database-url");
    std::fs::write(&url_file, "not-a-url-with-private-secret")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&url_file, std::fs::Permissions::from_mode(0o600))?;
    }
    let output = setup(
        home.path(),
        &[
            "--mode",
            "existing",
            "--accept-trial-loss",
            "--database-url-file",
            "database-url",
        ],
    )?;
    assert!(!output.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !text.contains("private-secret"),
        "secret was repeated in setup diagnostics"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("config.toml"))?,
        previous
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn setup_refuses_readable_by_others_or_symlinked_credential_input() -> anyhow::Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let home = isolated_home()?;
    let url_file = home.path().join("url");
    std::fs::write(&url_file, "private-sentinel")?;
    std::fs::set_permissions(&url_file, std::fs::Permissions::from_mode(0o644))?;
    symlink(&url_file, home.path().join("linked-url"))?;
    for file in ["url", "linked-url"] {
        let output = setup(
            home.path(),
            &[
                "--mode",
                "existing",
                "--accept-trial-loss",
                "--database-url-file",
                file,
            ],
        )?;
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-sentinel"));
        assert!(!home.path().join("config.toml").exists());
    }
    Ok(())
}

#[test]
fn database_provision_rejects_conflicting_modes_before_docker_or_config_changes()
-> anyhow::Result<()> {
    for (key, value) in [
        ("MIKE_DB_FALLBACK_ENABLED", "true"),
        ("MIKE_PERSIST_ENABLED", "false"),
    ] {
        let home = isolated_home()?;
        let previous = "persist_enabled = false\ndb_fallback_enabled = true\n";
        let config = home.path().join("config.toml");
        std::fs::write(&config, previous)?;
        let output = Command::new(env!("CARGO_BIN_EXE_mike"))
            .env_clear()
            .env("PATH", "")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env(key, value)
            .current_dir(home.path())
            .arg("--config")
            .arg(&config)
            .args(["database", "provision"])
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains("override the requested storage mode"),
            "{stderr}"
        );
        assert_eq!(std::fs::read_to_string(config)?, previous);
        assert!(!home.path().join("state").exists());
    }
    Ok(())
}
