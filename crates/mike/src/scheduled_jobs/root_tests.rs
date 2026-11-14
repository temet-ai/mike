use std::path::{Path, PathBuf};

use super::super::jobs::{LESSONS_COMMIT, PROMOTE};
use super::{Presence, locate_in};

fn write_script(root: &Path, script: &str) -> anyhow::Result<()> {
    let path = root.join(script);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, "#!/bin/sh\n")?;
    Ok(())
}

fn package_with_binary(base: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    let prefix = std::fs::canonicalize(base)?.join("prefix");
    std::fs::create_dir_all(prefix.join("bin"))?;
    let exe = prefix.join("bin").join("mike");
    std::fs::write(&exe, "x")?;
    Ok((prefix.join("share").join("mike"), exe))
}

#[test]
fn a_package_install_runs_shipped_jobs_from_its_share_folder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (share, exe) = package_with_binary(dir.path())?;
    write_script(&share, PROMOTE.script)?;
    let elsewhere = tempfile::tempdir()?;
    let located = locate_in(&PROMOTE, Some(&exe), Some(elsewhere.path()));
    assert_eq!(located.root, Some(share.clone()));
    assert_eq!(located.presence, Presence::Present);
    assert!(located.runs_here());
    assert!(share.join(PROMOTE.script).is_file());
    Ok(())
}

#[test]
fn a_source_checkout_wins_over_the_package_folder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (share, exe) = package_with_binary(dir.path())?;
    write_script(&share, PROMOTE.script)?;
    let checkout = tempfile::tempdir()?;
    write_script(checkout.path(), PROMOTE.script)?;
    let located = locate_in(&PROMOTE, Some(&exe), Some(checkout.path()));
    assert_eq!(located.root.as_deref(), Some(checkout.path()));
    assert_eq!(located.presence, Presence::Present);
    Ok(())
}

#[test]
fn a_checkout_is_found_from_a_subfolder_of_it() -> anyhow::Result<()> {
    let checkout = tempfile::tempdir()?;
    write_script(checkout.path(), PROMOTE.script)?;
    let inner = checkout.path().join("crates").join("mike");
    std::fs::create_dir_all(&inner)?;
    let located = locate_in(&PROMOTE, None, Some(&inner));
    assert_eq!(located.root.as_deref(), Some(checkout.path()));
    Ok(())
}

#[test]
fn a_checkout_only_job_needs_a_checkout_even_when_the_package_has_its_script() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let (share, exe) = package_with_binary(dir.path())?;
    write_script(&share, LESSONS_COMMIT.script)?;
    let elsewhere = tempfile::tempdir()?;
    let located = locate_in(&LESSONS_COMMIT, Some(&exe), Some(elsewhere.path()));
    assert_eq!(located.presence, Presence::NeedsCheckout);
    assert!(!located.runs_here());
    Ok(())
}

#[test]
fn a_shipped_job_with_no_script_anywhere_is_missing_and_keeps_the_working_directory()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (_share, exe) = package_with_binary(dir.path())?;
    let elsewhere = tempfile::tempdir()?;
    let located = locate_in(&PROMOTE, Some(&exe), Some(elsewhere.path()));
    assert_eq!(located.presence, Presence::Missing);
    assert_eq!(located.root.as_deref(), Some(elsewhere.path()));
    Ok(())
}
