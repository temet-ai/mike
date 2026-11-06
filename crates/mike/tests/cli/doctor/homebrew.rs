use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

fn prepare_mike(prefix: &Path) -> Result<PathBuf> {
    let binary = prefix.join("Cellar/mike/1.0/bin/mike");
    std::fs::create_dir_all(binary.parent().context("mike parent")?)?;
    if std::fs::hard_link(env!("CARGO_BIN_EXE_mike"), &binary).is_err() {
        std::fs::copy(env!("CARGO_BIN_EXE_mike"), &binary)?;
    }
    Ok(binary)
}

fn prepare_companion(prefix: &Path, instance_id: &str) -> Result<PathBuf> {
    let binary = prefix.join("Cellar/mike-mcp/1.0/bin/mike-mcp");
    std::fs::create_dir_all(binary.parent().context("companion parent")?)?;
    let initialize = json!({"jsonrpc":"2.0","id":1,"result":{
        "protocolVersion":"2024-11-05","capabilities":{"tools":{}},
        "serverInfo":{"name":"mike-mcp","version":env!("CARGO_PKG_VERSION")}
    }});
    let list = json!({"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"get_stats"}]}});
    let stats = json!({"stats":{"health":{"instance":{
        "instance_id":instance_id,"boot_generation":uuid::Uuid::nil()
    }}}});
    let reply = json!({"jsonrpc":"2.0","id":3,"result":{
        "content":[{"type":"text","text":stats.to_string()}],"isError":false
    }});
    let script = format!(
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' 'mike-mcp {}'; exit 0; fi
[ "$1" = "--core" ] || exit 8
IFS= read -r initialize
printf '%s\n' '{}'
IFS= read -r initialized
IFS= read -r list
printf '%s\n' '{}'
IFS= read -r call
printf '%s\n' '{}'
while IFS= read -r remaining; do :; done
"#,
        env!("CARGO_PKG_VERSION"),
        initialize,
        list,
        reply
    );
    std::fs::write(&binary, script)?;
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
    Ok(binary)
}

async fn doctor(binary: &Path, home: &Path, config: &Path) -> Result<Vec<Value>> {
    let mut command = tokio::process::Command::new(binary);
    command
        .arg("--config")
        .arg(config)
        .args(["doctor", "--json"])
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("MIKE_INSPECT_ADDR", "127.0.0.1:1")
        .current_dir(home)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output()).await??;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["read_only"], true);
    let checks = report["checks"]
        .as_array()
        .context("missing doctor checks")?;
    let failed = checks
        .iter()
        .any(|check| check["required"] == true && check["status"] != "pass");
    assert_eq!(!output.status.success(), failed);
    assert!(!home.join("state").exists());
    assert!(!home.join("data").exists());
    Ok(checks.clone())
}

fn assert_check(checks: &[Value], id: &str, code: &str, status: &str) -> Result<()> {
    let check = checks
        .iter()
        .find(|check| check["id"] == id)
        .context("missing diagnostic check")?;
    assert_eq!(check["code"], code);
    assert_eq!(check["status"], status);
    Ok(())
}

#[tokio::test]
async fn doctor_split_homebrew_companion_without_path_uses_shared_resolution() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let prefix = directory.path().join("homebrew prefix");
    let config = directory.path().join("selected.toml");
    std::fs::write(&config, "persist_enabled=false\n")?;
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600))?;
    let paths = mike::config::paths::InstallationPaths::resolve(Some(&config))?;
    let binary = prepare_mike(&prefix)?;
    let companion = prepare_companion(&prefix, &paths.installation_id)?;
    assert!(!binary.with_file_name("mike-mcp").exists());
    for location in ["bin/mike-mcp", "opt/mike-mcp/bin/mike-mcp"] {
        let stable = prefix.join(location);
        std::fs::create_dir_all(stable.parent().context("stable companion parent")?)?;
        symlink(&companion, &stable)?;
        let checks = doctor(&binary, directory.path(), &config).await?;
        assert_check(&checks, "mcp_binary", "MCP_BINARY_FOUND", "pass")?;
        assert_check(&checks, "mcp_connection", "MCP_READY", "pass")?;
        std::fs::remove_file(stable)?;
    }
    let missing = doctor(&binary, directory.path(), &config).await?;
    assert_check(&missing, "mcp_binary", "MCP_BINARY_MISSING", "fail")?;
    let stable = prefix.join("bin/mike-mcp");
    symlink(&companion, &stable)?;
    std::fs::set_permissions(&companion, std::fs::Permissions::from_mode(0o600))?;
    let nonexecutable = doctor(&binary, directory.path(), &config).await?;
    assert_check(&nonexecutable, "mcp_binary", "MCP_BINARY_MISSING", "fail")?;
    assert!(!paths.state_dir.exists());
    assert!(!paths.data_dir.exists());
    Ok(())
}
