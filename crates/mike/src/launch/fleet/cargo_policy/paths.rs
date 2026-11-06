use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::launch::fleet::cargo_policy::cargo_resolution::{
    ShimDirectory, resolve_cargo, without_mike_shims,
};

pub const POLICY_MARKER: &str = ".mike-cargo-policy";
pub const POLICY_VERSION: &[u8] = b"mike-fleet-cargo-policy-v1\n";

pub fn verify(directory: &Path) -> Result<ShimDirectory, String> {
    let root = directory
        .parent()
        .ok_or("Cargo policy directory has no private parent")?;
    let marker = directory.join(POLICY_MARKER);
    let mut file = private_file(&marker)?;
    let mut contents = Vec::new();
    (&mut file)
        .take(128)
        .read_to_end(&mut contents)
        .map_err(|_| "Cargo policy marker unreadable")?;
    if contents != POLICY_VERSION {
        return Err("Cargo policy marker is unsupported".into());
    }
    ShimDirectory::verify(root, directory)
        .map_err(|_| "Cargo policy directory is unsafe or replaced".into())
}

pub fn discovered(path: &OsStr, cwd: &Path) -> Result<Vec<ShimDirectory>, String> {
    let mut directories = Vec::new();
    for entry in std::env::split_paths(path) {
        let candidate = if entry.is_absolute() {
            entry
        } else {
            cwd.join(entry)
        };
        if candidate.join(POLICY_MARKER).symlink_metadata().is_err() {
            continue;
        }
        let canonical = candidate
            .canonicalize()
            .map_err(|_| "Cargo policy directory no longer resolves")?;
        directories.push(verify(&canonical)?);
    }
    Ok(directories)
}

pub fn managed_path(path: &OsStr, cwd: &Path) -> Result<Option<OsString>, String> {
    let directories = discovered(path, cwd)?;
    without_mike_shims(path, cwd, &directories)
        .map_err(|_| "Cargo policy PATH sanitation failed".into())
}

pub fn real_cargo(
    directory: &Path,
    path: &OsStr,
    cwd: &Path,
) -> Result<(PathBuf, OsString), String> {
    let current = verify(directory)?;
    let mut directories = discovered(path, cwd)?;
    if !directories
        .iter()
        .any(|entry| entry.path() == current.path())
    {
        directories.push(current);
    }
    let resolved = resolve_cargo(path, cwd, &directories)
        .map_err(|_| "real Cargo executable unavailable or recursive")?;
    Ok((
        resolved.executable,
        resolved.child_path.ok_or("real Cargo PATH is empty")?,
    ))
}

fn private_file(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Cargo policy marker cannot be opened safely")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Cargo policy marker cannot be inspected")?;
    if metadata.nlink() != 1
        || !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err("Cargo policy marker is not private to this user".into());
    }
    Ok(file)
}
