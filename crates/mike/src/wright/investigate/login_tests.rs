use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};

use super::{home, login, test_support};

fn renewing_claude(root: &Path, credentials: &Path) -> anyhow::Result<PathBuf> {
    let fresh = root.join("fresh.json");
    test_support::credentials(&fresh, Duration::days(1))?;
    let script = root.join("claude");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho ran >> {marker}\ncp {fresh} {credentials}\n",
            marker = root.join("ran").display(),
            fresh = fresh.display(),
            credentials = credentials.display(),
        ),
    )?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
    Ok(script)
}

#[tokio::test]
async fn a_login_that_expires_before_the_run_ends_is_renewed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let credentials = root.path().join("credentials.json");
    test_support::credentials(&credentials, Duration::hours(-15))?;
    let claude = renewing_claude(root.path(), &credentials)?;
    let deadline = Utc::now() + Duration::minutes(30);

    login::renew_if_short(&claude, &credentials, deadline).await;

    let raw = std::fs::read(&credentials)?;
    assert!(!home::login_is_short(&raw, deadline));
    assert!(home::claude_credentials(&raw, deadline).is_ok());
    Ok(())
}

#[tokio::test]
async fn a_login_that_outlasts_the_run_is_left_alone() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let credentials = root.path().join("credentials.json");
    test_support::credentials(&credentials, Duration::hours(6))?;
    let claude = renewing_claude(root.path(), &credentials)?;

    login::renew_if_short(&claude, &credentials, Utc::now() + Duration::minutes(30)).await;

    assert!(!root.path().join("ran").exists());
    Ok(())
}

#[tokio::test]
async fn a_renewal_that_cannot_start_leaves_the_short_login_to_be_reported() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let credentials = root.path().join("credentials.json");
    test_support::credentials(&credentials, Duration::hours(-15))?;
    let deadline = Utc::now() + Duration::minutes(30);

    login::renew_if_short(&root.path().join("missing"), &credentials, deadline).await;

    let error = home::claude_credentials(&std::fs::read(&credentials)?, deadline)
        .err()
        .unwrap_or_default();
    assert!(error.contains("log in"), "{error}");
    Ok(())
}
