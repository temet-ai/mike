use super::*;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};

fn directory(path: &Path) -> anyhow_result::Result<()> {
    std::fs::create_dir(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

mod anyhow_result {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}

fn fixture() -> anyhow_result::Result<(tempfile::TempDir, PathBuf, ShimDirectory)> {
    let temporary = tempfile::tempdir()?;
    std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o700))?;
    let root = temporary.path().canonicalize()?;
    let path = root.join("shim");
    directory(&path)?;
    std::fs::write(path.join(MARKER_NAME), PROTOCOL_MARKER)?;
    std::fs::set_permissions(
        path.join(MARKER_NAME),
        std::fs::Permissions::from_mode(0o600),
    )?;
    script(&path.join("cargo"))?;
    let shim = ShimDirectory::verify(&root, &path)?;
    Ok((temporary, root, shim))
}

fn script(path: &Path) -> anyhow_result::Result<()> {
    std::fs::write(path, b"#!/bin/sh\nexit 7\n")?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub fn relative_empty_and_non_utf8_path_entries_keep_their_original_bytes()
-> anyhow_result::Result<()> {
    let (_temporary, root, shim) = fixture()?;
    let opaque = OsString::from_vec(vec![b'b', 0xff]);
    script(&root.join("cargo"))?;
    let path = std::env::join_paths([shim.path().as_os_str(), OsStr::new(""), &opaque])?;
    let result = resolve_cargo(&path, &root, &[shim])?;
    assert_eq!(result.executable, root.join("cargo"));
    assert_eq!(
        result.child_path,
        Some(std::env::join_paths([OsStr::new(""), &opaque])?)
    );
    Ok(())
}

pub fn rustup_named_symlink_keeps_the_cargo_path_instead_of_its_target() -> anyhow_result::Result<()>
{
    let (_temporary, root, shim) = fixture()?;
    let real = root.join("real");
    directory(&real)?;
    script(&real.join("rustup"))?;
    symlink("rustup", real.join("cargo"))?;
    let path = std::env::join_paths([shim.path(), &real])?;
    let result = resolve_cargo(&path, &root, &[shim])?;
    assert_eq!(result.executable, real.join("cargo"));
    assert_ne!(result.executable, real.join("rustup"));
    Ok(())
}

pub fn aliases_of_verified_shim_directories_are_removed_but_other_wrappers_remain()
-> anyhow_result::Result<()> {
    let (_temporary, root, shim) = fixture()?;
    symlink(shim.path(), root.join("alias"))?;
    let other = root.join("wrapper");
    directory(&other)?;
    script(&other.join("cargo"))?;
    let path =
        std::env::join_paths([root.join("alias"), shim.path().to_path_buf(), other.clone()])?;
    let result = resolve_cargo(&path, &root, &[shim])?;
    assert_eq!(result.executable, other.join("cargo"));
    assert_eq!(result.child_path, Some(other.into_os_string()));
    Ok(())
}

pub fn removed_all_entries_is_distinct_from_a_preserved_empty_entry() -> anyhow_result::Result<()> {
    let (_temporary, root, shim) = fixture()?;
    assert_eq!(
        without_mike_shims(shim.path().as_os_str(), &root, std::slice::from_ref(&shim))?,
        None
    );
    let with_empty = std::env::join_paths([shim.path().as_os_str(), OsStr::new("")])?;
    assert_eq!(
        without_mike_shims(&with_empty, &root, std::slice::from_ref(&shim))?,
        Some(OsString::new())
    );
    assert!(matches!(
        resolve_cargo(shim.path().as_os_str(), &root, std::slice::from_ref(&shim)),
        Err(PathError::MissingCargo)
    ));
    Ok(())
}

pub fn replaced_or_unmarked_shim_is_refused() -> anyhow_result::Result<()> {
    let (_temporary, root, shim) = fixture()?;
    std::fs::remove_file(shim.path().join(MARKER_NAME))?;
    assert!(without_mike_shims(OsStr::new("/usr/bin"), &root, &[shim]).is_err());
    let unmarked = root.join("unmarked");
    directory(&unmarked)?;
    assert!(ShimDirectory::verify(&root, &unmarked).is_err());
    Ok(())
}

pub fn a_hardlinked_launcher_outside_the_shim_cannot_recurse() -> anyhow_result::Result<()> {
    let (_temporary, root, shim) = fixture()?;
    let other = root.join("other");
    directory(&other)?;
    std::fs::hard_link(shim.path().join("cargo"), other.join("cargo"))?;
    let path = std::env::join_paths([shim.path(), &other])?;
    assert!(resolve_cargo(&path, &root, &[shim]).is_err());
    Ok(())
}

pub fn missing_or_non_executable_cargo_is_not_selected() -> anyhow_result::Result<()> {
    let (_temporary, root, shim) = fixture()?;
    let real = root.join("real");
    directory(&real)?;
    std::fs::write(real.join("cargo"), "not executable")?;
    let path = std::env::join_paths([shim.path(), &real])?;
    assert!(matches!(
        resolve_cargo(&path, &root, &[shim]),
        Err(PathError::MissingCargo)
    ));
    Ok(())
}
