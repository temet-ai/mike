use std::path::{Path, PathBuf};

const DIRECTORY: &str = "managed-builds";

pub fn prepare() -> Result<PathBuf, String> {
    let state = crate::machine_identity::default_state_dir().map_err(|error| error.to_string())?;
    prepare_in(&state)
}

#[cfg(unix)]
pub fn prepare_in(state: &Path) -> Result<PathBuf, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let canonical = state.canonicalize().map_err(|error| {
        format!(
            "Mike state directory `{}` is unavailable: {error}",
            state.display()
        )
    })?;
    let parent = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&canonical)
        .map_err(|error| error.to_string())?;
    let metadata = parent.metadata().map_err(|error| error.to_string())?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err("Mike state directory must belong to this user and not be group/other writable; permissions were not changed".into());
    }
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), c"managed-builds".as_ptr(), 0o700) };
    if result != 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
        return Err(format!(
            "private managed build directory could not be created: {}",
            std::io::Error::last_os_error()
        ));
    }
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c"managed-builds".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(format!(
            "private managed build directory could not be opened without following symlinks: {}",
            std::io::Error::last_os_error()
        ));
    }
    let directory = unsafe { std::fs::File::from_raw_fd(descriptor) };
    let private = directory.metadata().map_err(|error| error.to_string())?;
    if private.uid() != unsafe { libc::geteuid() } || private.mode() & 0o077 != 0 {
        return Err("existing managed build directory is not private to this user; permissions were not changed".into());
    }
    let path = canonical.join(DIRECTORY);
    let resolved = path.canonicalize().map_err(|error| error.to_string())?;
    let reopened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&resolved)
        .map_err(|error| error.to_string())?;
    let actual = reopened.metadata().map_err(|error| error.to_string())?;
    if (private.dev(), private.ino()) != (actual.dev(), actual.ino()) {
        return Err("private managed build directory changed during preparation".into());
    }
    Ok(resolved)
}

#[cfg(not(unix))]
pub fn prepare_in(_state: &Path) -> Result<PathBuf, String> {
    Err("cooperative managed build admission is unsupported on this platform".into())
}

#[cfg(all(test, unix))]
#[path = "build_admission_root_tests.rs"]
pub mod tests;
