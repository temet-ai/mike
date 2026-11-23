use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::{SetupError, install};

async fn installer(home: &Path, body: &str) -> anyhow::Result<PathBuf> {
    let path = home.join("installer");
    tokio::fs::write(&path, format!("#!/bin/sh\n{body}\n")).await?;
    tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(path)
}

#[tokio::test]
async fn setup_keeps_home_and_endpoint_as_literal_arguments() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = installer(
        root.path(),
        "printf '%s\\n' \"$@\" >&2; printf '{\"notice\":\"\",\"mike_otel_exporter\":true}'",
    )
    .await?;
    let home = root.path().join("Codex home's; literal");
    let url = "http://localhost:9989/path?literal=$value";
    let result = install(&path, &home, url).await?;
    assert_eq!(
        result.notice,
        format!(
            "--codex-home\n{}\n--mike-url\n{url}\n--require-mcp\n--proxy-only\n--report-json",
            home.display()
        )
    );
    assert!(result.mike_otel_exporter);
    Ok(())
}

#[tokio::test]
async fn setup_errors_preserve_failure_without_accepting_partial_installation() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let path = installer(root.path(), "printf 'missing mike-mcp' >&2; exit 2").await?;
    let result = install(&path, root.path(), "http://localhost:9989").await;
    assert!(
        matches!(result, Err(SetupError::Failed { code: Some(2), detail }) if detail == "missing mike-mcp")
    );
    let result = install(
        &root.path().join("absent"),
        root.path(),
        "http://localhost:9989",
    )
    .await;
    assert!(matches!(result, Err(SetupError::Io(_))));
    Ok(())
}

#[tokio::test]
async fn installer_output_is_bounded_even_when_it_does_not_exit() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = installer(
        root.path(),
        "while :; do printf '0123456789abcdef' >&2; done",
    )
    .await?;
    let result = install(&path, root.path(), "http://localhost:9989").await;
    assert!(matches!(result, Err(SetupError::OutputLimit)));
    Ok(())
}
