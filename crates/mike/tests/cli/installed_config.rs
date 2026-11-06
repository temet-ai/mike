use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result};

fn run(home: &Path, cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mike"));
    command
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .current_dir(cwd)
        .args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    Ok(command.output()?)
}

fn default_config_dir(home: &Path) -> std::path::PathBuf {
    home.join("config/mike")
}

fn text(output: Output) -> Result<String> {
    anyhow::ensure!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn selected_file_process_env_and_cli_precedence_is_independent_of_cwd() -> Result<()> {
    let home = tempfile::tempdir()?;
    let installation = tempfile::tempdir()?;
    let cwd = tempfile::tempdir()?;
    let config = installation.path().join("selected config.toml");
    std::fs::write(
        &config,
        "proxy_listen_addr='127.0.0.1:7000'\npersist_enabled=false\n",
    )?;
    std::fs::write(cwd.path().join(".env"), "MIKE_LISTEN_ADDR=127.0.0.1:9999\n")?;
    let path = config.to_str().context("config path")?;
    let body = text(run(
        home.path(),
        cwd.path(),
        &["--config", path, "print-config", "--shell"],
        &[],
    )?)?;
    assert!(body.contains("MIKE_LISTEN_ADDR='127.0.0.1:7000'"));
    let body = text(run(
        home.path(),
        cwd.path(),
        &[
            "--config",
            path,
            "--proxy-listen-addr",
            "127.0.0.1:7002",
            "print-config",
            "--shell",
        ],
        &[("MIKE_LISTEN_ADDR", "127.0.0.1:7001")],
    )?)?;
    assert!(body.contains("MIKE_LISTEN_ADDR='127.0.0.1:7002'"));
    let body = text(run(
        home.path(),
        cwd.path(),
        &["print-config", "--shell", "--config", path],
        &[("MIKE_LISTEN_ADDR", "127.0.0.1:7001")],
    )?)?;
    assert!(body.contains("MIKE_LISTEN_ADDR='127.0.0.1:7001'"));
    assert_eq!(
        std::fs::read_to_string(cwd.path().join(".env"))?,
        "MIKE_LISTEN_ADDR=127.0.0.1:9999\n"
    );
    Ok(())
}

#[test]
fn default_user_configuration_ignores_project_and_ancestor_dotenv() -> Result<()> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let cwd = project.path().join("nested");
    std::fs::create_dir(&cwd)?;
    let config_dir = default_config_dir(home.path());
    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(
        config_dir.join("config.toml"),
        "proxy_listen_addr='127.0.0.1:7123'\npersist_enabled=false\n",
    )?;
    std::fs::write(project.path().join(".env"), "MIKE_LISTEN_ADDR=invalid\n")?;
    let body = text(run(home.path(), &cwd, &["print-config", "--shell"], &[])?)?;
    assert!(body.contains("MIKE_LISTEN_ADDR='127.0.0.1:7123'"));
    assert!(!cwd.join("config.toml").exists());
    Ok(())
}

#[test]
fn explicit_dotenv_import_preserves_source_and_literal_secret() -> Result<()> {
    let home = tempfile::tempdir()?;
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("legacy.env");
    let target = dir.path().join("private/config.toml");
    let contents = "MIKE_STEERER_OPENROUTER_API_KEY='s quotes \" $HOME `id` \\ # λ'\nMIKE_PERSIST_ENABLED=0\nMIKE_LISTEN_ADDR=127.0.0.1:7555\n";
    std::fs::write(&source, contents)?;
    let args = [
        "--config",
        target.to_str().context("target")?,
        "import-dotenv",
        source.to_str().context("source")?,
    ];
    let printed = text(run(home.path(), dir.path(), &args, &[])?)?;
    assert!(!printed.contains("$HOME"));
    assert_eq!(std::fs::read_to_string(&source)?, contents);
    let saved: toml::Table = std::fs::read_to_string(&target)?.parse()?;
    assert_eq!(
        saved["steerer"]["openrouter_api_key"].as_str(),
        Some("s quotes \" $HOME `id` \\ # λ")
    );
    let report = text(run(
        home.path(),
        dir.path(),
        &[
            "--config",
            target.to_str().context("target")?,
            "print-config",
        ],
        &[],
    )?)?;
    assert!(!report.contains("$HOME"));
    text(run(home.path(), dir.path(), &args, &[])?)?;
    assert_eq!(
        std::fs::read_dir(target.parent().context("parent")?)?.count(),
        3
    );
    Ok(())
}

#[test]
fn bad_import_and_bad_toml_do_not_echo_secret_or_replace_working_configuration() -> Result<()> {
    let home = tempfile::tempdir()?;
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("config.toml");
    let source = dir.path().join("legacy.env");
    std::fs::write(&target, "persist_enabled=false\n")?;
    std::fs::write(&source, "MIKE_DATABASE_URL=$(planted-secret)\n")?;
    let result = run(
        home.path(),
        dir.path(),
        &[
            "--config",
            target.to_str().context("target")?,
            "import-dotenv",
            source.to_str().context("source")?,
        ],
        &[],
    )?;
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("planted-secret"));
    assert_eq!(std::fs::read_to_string(&target)?, "persist_enabled=false\n");
    std::fs::write(&target, "database_url='planted-secret\n")?;
    let result = run(
        home.path(),
        dir.path(),
        &[
            "--config",
            target.to_str().context("target")?,
            "print-config",
            "--shell",
        ],
        &[],
    )?;
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("planted-secret"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn selected_installation_reads_from_a_read_only_unrelated_cwd() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir()?;
    let installation = tempfile::tempdir()?;
    let cwd = tempfile::tempdir()?;
    let config = installation.path().join("config.toml");
    std::fs::write(
        &config,
        "persist_enabled=false\nproxy_listen_addr='127.0.0.1:7666'",
    )?;
    std::fs::set_permissions(cwd.path(), std::fs::Permissions::from_mode(0o500))?;
    let result = run(
        home.path(),
        cwd.path(),
        &[
            "--config",
            config.to_str().context("config")?,
            "print-config",
            "--shell",
        ],
        &[],
    );
    std::fs::set_permissions(cwd.path(), std::fs::Permissions::from_mode(0o700))?;
    let body = text(result?)?;
    assert!(body.contains("MIKE_LISTEN_ADDR='127.0.0.1:7666'"));
    assert_eq!(std::fs::read_dir(cwd.path())?.count(), 0);
    Ok(())
}

#[test]
fn paths_report_matches_selected_identity_without_writes_or_database_access() -> Result<()> {
    let home = tempfile::tempdir()?;
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("chosen.toml");
    let source = "data_dir='relative-data'\ndatabase_url='postgres://secret@invalid.invalid/db'\n";
    std::fs::write(&target, source)?;
    let expected = mike::config::paths::InstallationPaths::resolve(Some(&target))?;
    let body = text(run(
        home.path(),
        dir.path(),
        &[
            "--config",
            target.to_str().context("selected path")?,
            "print-config",
            "--paths",
        ],
        &[],
    )?)?;
    let report: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(report["installation_id"], expected.installation_id);
    assert_eq!(report["config_file"], serde_json::to_value(&target)?);
    assert_eq!(
        report["state_dir"],
        serde_json::to_value(&expected.state_dir)?
    );
    assert_eq!(
        report["data_dir"],
        serde_json::to_value(dir.path().join("relative-data"))?
    );
    assert_eq!(report.as_object().context("object")?.len(), 5);
    assert!(!body.contains("secret"));
    assert_eq!(std::fs::read_to_string(&target)?, source);
    assert_eq!(std::fs::read_dir(dir.path())?.count(), 1);
    assert_eq!(std::fs::read_dir(home.path())?.count(), 0);
    Ok(())
}

#[test]
fn onboarding_metadata_legacy_values_and_environment_references_load_together() -> Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join("selected.toml");
    std::fs::write(
        &config,
        "setup='unfinished'\nsetup_features='recommended'\nclaude_route='plain'\npersist_enabled='false'\nlisten_addr='127.0.0.1:7012'\nlaunch_claude_args=['--chrome','--verbose']\nmax_requests_from_env='MIKE_FIXTURE_LIMIT'\n",
    )?;
    let body = text(run(
        home.path(),
        home.path(),
        &[
            "--config",
            config.to_str().context("path")?,
            "print-config",
            "--shell",
        ],
        &[("MIKE_FIXTURE_LIMIT", "37")],
    )?)?;
    assert!(body.contains("MIKE_LISTEN_ADDR='127.0.0.1:7012'"));
    assert!(body.contains("MIKE_MAX_REQUESTS='37'"));

    assert!(!body.contains("MIKE_SETUP_FEATURES"));
    Ok(())
}
