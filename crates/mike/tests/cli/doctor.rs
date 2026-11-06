use std::process::Command;

#[test]
fn doctor_support_bundle_is_allowlisted_and_read_only() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config_dir = home.path().join("config");
    let state_dir = home.path().join("state");
    for arg in ["--json", "--support-bundle"] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike"))
            .args(["doctor", arg])
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", &config_dir)
            .env("XDG_STATE_HOME", &state_dir)
            .env("MIKE_PERSIST_ENABLED", "0")
            .env("MIKE_INSPECT_ADDR", "127.0.0.1:1")
            .env(
                "MIKE_DATABASE_URL",
                "postgres://planted-user:planted-secret@127.0.0.1:1/private-database",
            )
            .env("OPENROUTER_API_KEY", "planted-provider-secret")
            .env("PRIVATE_PROMPT", "planted-conversation-content")
            .current_dir(home.path())
            .output()?;
        let text = String::from_utf8(output.stdout)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(value["read_only"], true);
        assert_eq!(value["product"], "mike");
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        for secret in [
            "planted-",
            "postgres://",
            "private-database",
            &home.path().to_string_lossy(),
        ] {
            assert!(
                !text.contains(secret),
                "diagnostic output exposed sensitive input"
            );
        }
        assert!(!config_dir.exists());
        assert!(!state_dir.exists());
        let checks = value["checks"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing checks"))?;
        assert!(checks.iter().any(|check| check["code"] == "TRIAL_MODE"));
        let required_failed = checks
            .iter()
            .any(|check| check["required"] == true && check["status"] != "pass");
        assert_eq!(!output.status.success(), required_failed);
    }
    assert_eq!(std::fs::read_dir(home.path())?.count(), 0);
    Ok(())
}

#[test]
fn doctor_invalid_configuration_reports_code_without_values() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let output = Command::new(env!("CARGO_BIN_EXE_mike"))
        .args(["doctor", "--json"])
        .env_clear()
        .env("HOME", home.path())
        .env("MIKE_MAX_REQUESTS", "planted-invalid-secret")
        .current_dir(home.path())
        .output()?;
    assert!(!output.status.success());
    let text = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(value["checks"][0]["code"], "CONFIG_INVALID");
    assert!(!text.contains("planted-invalid-secret"));
    assert!(!String::from_utf8(output.stderr)?.contains("planted-invalid-secret"));
    assert_eq!(std::fs::read_dir(home.path())?.count(), 0);
    Ok(())
}

#[cfg(unix)]
#[path = "doctor/homebrew.rs"]
mod homebrew;
