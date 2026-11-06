use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::json;

use super::super::{probe, register};

fn executable(directory: &Path, name: &str, body: &str) -> Result<PathBuf> {
    let path = directory.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

fn fixture(directory: &Path, reply: &str) -> Result<PathBuf> {
    let initialize = json!({"jsonrpc":"2.0","id":1,"result":{
        "protocolVersion":"2024-11-05","capabilities":{"tools":{}},
        "serverInfo":{"name":"mike-mcp","version":env!("CARGO_PKG_VERSION")}
    }});
    let list = json!({"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"get_stats"}]}});
    executable(
        directory,
        "mcp fixture",
        &format!(
            r#"
if [ "$1" = "--version" ]; then printf '%s\n' 'mike-mcp {}'; exit 0; fi
[ "$1" = "--core" ] || exit 8
[ "$MIKE_INSPECT_URL" = '{}' ] || exit 9
[ -z "$CLAUDE_CODE_SESSION_ID$MIKE_SESSION_ID" ] || exit 10
printf '%s\n' "$$" > '{}'
IFS= read -r initialize
printf '%s\n' '{}'
IFS= read -r initialized
IFS= read -r list
printf '%s\n' '{}'
IFS= read -r read
{}
while IFS= read -r remaining; do :; done
"#,
            env!("CARGO_PKG_VERSION"),
            "http://127.0.0.1:14589",
            directory.join("pid").display(),
            initialize,
            list,
            reply
        ),
    )
}

fn good_reply() -> String {
    let body = json!({"stats":{"runtime":{},"diagnostic":{},"health":{"instance":{
        "instance_id":"test-instance", "boot_generation":uuid::Uuid::nil()
    }}}});
    format!(
        "printf '%s\\n' '{}'",
        json!({"jsonrpc":"2.0","id":3,"result":{
            "content":[{"type":"text","text":body.to_string()}],"isError":false
        }})
    )
}

fn assert_reaped(directory: &Path) -> Result<()> {
    let pid: libc::pid_t = std::fs::read_to_string(directory.join("pid"))?
        .trim()
        .parse()?;
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    Ok(())
}

#[tokio::test]
async fn probe_real_stdio_exchange_verifies_endpoint_and_reaps_child() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let binary = fixture(dir.path(), &good_reply())?;
    let report = probe::run(&binary, "http://127.0.0.1:14589", "test-instance").await?;
    assert!(report.handshake);
    assert!(report.backend_read);
    assert_eq!(report.code, "MCP_READY");
    assert_reaped(dir.path())?;
    Ok(())
}

#[tokio::test]
async fn probe_backend_unavailable_differs_from_handshake_failure() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let binary = fixture(
        dir.path(),
        "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"error\":{\"code\":-32603,\"message\":\"backend offline\"}}'",
    )?;
    let report = probe::run(&binary, "http://127.0.0.1:14589", "test-instance").await?;
    assert!(report.handshake);
    assert!(!report.backend_read);
    assert_eq!(report.code, "MCP_BACKEND_UNAVAILABLE");
    assert_reaped(dir.path())?;
    let bad = executable(
        dir.path(),
        "bad",
        &format!(
            "if [ \"$1\" = --version ]; then echo 'mike-mcp {}'; else echo 'not protocol'; fi",
            env!("CARGO_PKG_VERSION")
        ),
    )?;
    let report = probe::run(&bad, "http://127.0.0.1:14589", "test-instance").await?;
    assert!(!report.handshake);
    assert_eq!(report.code, "MCP_HANDSHAKE_FAILED");
    Ok(())
}

#[tokio::test]
async fn probe_invalid_read_protocol_is_not_backend_readiness() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let binary = fixture(dir.path(), "echo 'diagnostic on stdout'")?;
    let report = probe::run(&binary, "http://127.0.0.1:14589", "test-instance").await?;
    assert!(report.handshake);
    assert!(!report.backend_read);
    assert_eq!(report.code, "MCP_READ_PROTOCOL_FAILED");
    assert_reaped(dir.path())?;
    Ok(())
}

#[tokio::test]
async fn probe_hung_backend_read_is_bounded_and_reaped() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let binary = fixture(dir.path(), "IFS= read -r never_sent")?;
    let started = std::time::Instant::now();
    let report = probe::run(&binary, "http://127.0.0.1:14589", "test-instance").await?;
    assert!(started.elapsed() < std::time::Duration::from_secs(7));
    assert_eq!(report.code, "MCP_BACKEND_UNAVAILABLE");
    assert_reaped(dir.path())?;
    Ok(())
}

#[tokio::test]
async fn registration_wrong_version_or_handshake_never_changes_client() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut plan = super::plan(dir.path());
    std::fs::write(&plan.client_config, "{}")?;
    plan.binary = executable(dir.path(), "wrong", "echo 'mike-mcp 0.0.0-unrelated'")?;
    assert!(register(plan.clone(), false).await.is_err());
    plan.binary = executable(
        dir.path(),
        "bad",
        &format!(
            "if [ \"$1\" = --version ]; then echo 'mike-mcp {}'; else echo bad; fi",
            env!("CARGO_PKG_VERSION")
        ),
    )?;
    assert!(register(plan.clone(), false).await.is_err());
    assert_eq!(std::fs::read_to_string(plan.client_config)?, "{}");
    Ok(())
}

#[tokio::test]
async fn registration_verified_connection_writes_only_synthetic_destination() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut plan = super::plan(dir.path());
    plan.binary = fixture(dir.path(), &good_reply())?;
    let report = register(plan.clone(), false).await?;
    assert!(report.probe.backend_read);
    assert!(report.changed);
    assert!(report.backup.is_none());
    let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(plan.client_config)?)?;
    assert_eq!(
        actual["mcpServers"]["mike"]["env"]["MIKE_INSPECT_URL"],
        "http://127.0.0.1:14589"
    );
    Ok(())
}

#[tokio::test]
async fn registration_rejects_successful_read_from_foreign_instance() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut plan = super::plan(dir.path());
    plan.binary = fixture(dir.path(), &good_reply())?;
    plan.instance_id = "selected-other-config".into();
    let report = probe::run(&plan.binary, &plan.inspect_url, &plan.instance_id).await?;
    assert!(report.handshake);
    assert!(!report.backend_read);
    assert_eq!(report.code, "MCP_INSTANCE_MISMATCH");
    assert!(register(plan.clone(), false).await.is_err());
    assert!(!plan.client_config.exists());
    Ok(())
}
