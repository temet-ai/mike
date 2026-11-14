use std::path::{Path, PathBuf};

const DIRECTORY: &str = "managed-builds";

pub fn prepare() -> Result<PathBuf, String> {
    let state = crate::machine_identity::default_state_dir().map_err(|error| error.to_string())?;
    prepare_in(&state)
}

#[cfg(unix)]
pub fn prepare_in(state: &Path) -> Result<PathBuf, String> {
    use rustix::fs::{Mode, OFlags};
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
    let user = rustix::process::geteuid().as_raw();
    if metadata.uid() != user || metadata.mode() & 0o022 != 0 {
        return Err("Mike state directory must belong to this user and not be group/other writable; permissions were not changed".into());
    }
    match rustix::fs::mkdirat(&parent, c"managed-builds", Mode::RWXU) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
        Err(errno) => {
            return Err(format!(
                "private managed build directory could not be created: {}",
                std::io::Error::from(errno)
            ));
        }
    }
    let opened = rustix::fs::openat(
        &parent,
        c"managed-builds",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    );
    let directory = std::fs::File::from(opened.map_err(|errno| {
        format!(
            "private managed build directory could not be opened without following symlinks: {}",
            std::io::Error::from(errno)
        )
    })?);
    let private = directory.metadata().map_err(|error| error.to_string())?;
    if private.uid() != user || private.mode() & 0o077 != 0 {
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
mod tests;
