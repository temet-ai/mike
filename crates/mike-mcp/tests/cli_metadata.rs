use std::process::Command;

#[test]
fn version_and_help_ignore_broken_runtime_configuration() -> anyhow::Result<()> {
    for arg in ["--version", "-V", "--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike-mcp"))
            .arg(arg)
            .env_clear()
            .env("MIKE_MCP_PROFILE", "not-a-profile")
            .env("MIKE_INSPECT_URL", "not-a-url")
            .current_dir(std::env::temp_dir())
            .output()?;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout)?;
        assert!(text.starts_with(&format!("mike-mcp {}\n", env!("CARGO_PKG_VERSION"))));
        assert_eq!(text.contains("Usage:"), matches!(arg, "--help" | "-h"));
    }
    Ok(())
}

#[test]
fn metadata_options_reject_unexpected_arguments() -> anyhow::Result<()> {
    for arg in ["--version", "--help"] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike-mcp"))
            .args([arg, "extra"])
            .env_clear()
            .output()?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr)?.contains("unexpected extra argument"));
    }
    Ok(())
}

#[test]
fn build_info_ignores_runtime_configuration_and_is_read_only() -> anyhow::Result<()> {
    let home = MetadataHome::new()?;
    let poison = "planted-private-value = [invalid TOML\n";
    std::fs::write(home.0.join("config.toml"), poison)?;
    let output = Command::new(env!("CARGO_BIN_EXE_mike-mcp"))
        .arg("--build-info")
        .env_clear()
        .env("PATH", "")
        .env("HOME", &home.0)
        .env("XDG_CONFIG_HOME", home.0.join("config"))
        .env("XDG_STATE_HOME", home.0.join("state"))
        .env("MIKE_MCP_PROFILE", "planted-private-invalid-profile")
        .env("MIKE_INSPECT_URL", "planted-private-invalid-url")
        .env("MIKE_BUILD_SHA", "planted-runtime-identity-override")
        .current_dir(&home.0)
        .output()?;
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let text = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        value,
        serde_json::json!({
            "schema_version": 1, "product": "mike-mcp", "version": env!("CARGO_PKG_VERSION"),
            "source_sha": env!("MIKE_BUILD_SHA"), "source_dirty": env!("MIKE_BUILD_DIRTY") != "false",
            "built_at": env!("MIKE_BUILD_TIME"),
        })
    );
    assert!(!text.contains("planted-"));
    assert!(!text.contains(&home.0.to_string_lossy().to_string()));
    assert_eq!(std::fs::read_to_string(home.0.join("config.toml"))?, poison);
    assert_eq!(std::fs::read_dir(&home.0)?.count(), 1);
    Ok(())
}

#[test]
fn build_info_rejects_extra_arguments_before_runtime_startup() -> anyhow::Result<()> {
    for args in [["--build-info", "serve"], ["--build-info", "--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike-mcp"))
            .args(args)
            .env_clear()
            .env("MIKE_MCP_PROFILE", "planted-private-invalid-profile")
            .env("MIKE_INSPECT_URL", "planted-private-invalid-url")
            .output()?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8(output.stderr)?.contains("planted-private-"));
    }
    Ok(())
}

struct MetadataHome(std::path::PathBuf);

impl MetadataHome {
    fn new() -> anyhow::Result<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mike-mcp-metadata-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for MetadataHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
