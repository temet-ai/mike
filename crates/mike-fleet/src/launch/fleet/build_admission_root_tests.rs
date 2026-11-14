use std::os::unix::fs::{MetadataExt, PermissionsExt};

use super::*;

#[test]
fn ordinary_public_state_parent_gets_one_private_stable_child() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755))?;
    let first = prepare_in(root.path()).map_err(anyhow::Error::msg)?;
    let second = prepare_in(root.path()).map_err(anyhow::Error::msg)?;
    assert_eq!(first, second);
    assert_eq!(std::fs::metadata(&first)?.mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(root.path())?.mode() & 0o777, 0o755);
    Ok(())
}

#[test]
fn configured_parent_alias_canonicalizes_but_dedicated_child_alias_is_refused() -> anyhow::Result<()>
{
    let parent = tempfile::tempdir()?;
    let alias_root = tempfile::tempdir()?;
    let alias = alias_root.path().join("state");
    std::os::unix::fs::symlink(parent.path(), &alias)?;
    let first = prepare_in(&alias).map_err(anyhow::Error::msg)?;
    assert_eq!(
        first,
        prepare_in(parent.path()).map_err(anyhow::Error::msg)?
    );
    let other = tempfile::tempdir()?;
    let target = tempfile::tempdir()?;
    std::os::unix::fs::symlink(target.path(), other.path().join(DIRECTORY))?;
    assert!(prepare_in(other.path()).is_err());
    Ok(())
}

#[test]
fn unsafe_existing_directory_is_never_chmodded() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let child = root.path().join(DIRECTORY);
    std::fs::create_dir(&child)?;
    std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o755))?;
    assert!(prepare_in(root.path()).is_err());
    assert_eq!(std::fs::metadata(child)?.mode() & 0o777, 0o755);
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o777))?;
    assert!(prepare_in(root.path()).is_err());
    Ok(())
}

#[test]
fn missing_parent_and_non_directory_child_return_concrete_failures() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    assert!(prepare_in(&root.path().join("missing")).is_err());
    std::fs::write(root.path().join(DIRECTORY), b"do not modify")?;
    assert!(prepare_in(root.path()).is_err());
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY))?,
        b"do not modify"
    );
    Ok(())
}
