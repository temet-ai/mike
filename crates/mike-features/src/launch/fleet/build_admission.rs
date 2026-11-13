use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

const LOCK_NAME: &str = "managed-build.lock";
const RETRY: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error("managed build admission was cancelled before execution")]
    Cancelled,
    #[error("cooperative managed build admission is unsupported on this platform or filesystem")]
    Unsupported,
    #[error("unsafe managed build admission path `{path}`: {reason}")]
    UnsafePath { path: PathBuf, reason: &'static str },
    #[error("managed build admission could not {action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug)]
#[must_use = "retain this permit until the owned process group and output cleanup have finished"]
pub enum Permit {
    Lock { _directory: File, _lock: File },
    Resources { _permits: Vec<Permit> },
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn acquire(
    private_state_root: &Path,
    cancellation: &CancellationToken,
) -> Result<Permit, AdmissionError> {
    if cancellation.is_cancelled() {
        return Err(AdmissionError::Cancelled);
    }
    let mut pending = Pending::open_slot(private_state_root, 0)?;
    loop {
        match try_acquire(pending, cancellation)? {
            Ok(permit) => return Ok(permit),
            Err(back) => pending = back,
        }
        pause(cancellation).await?;
    }
}

fn unsafe_path(path: &Path, reason: &'static str) -> AdmissionError {
    AdmissionError::UnsafePath {
        path: path.to_path_buf(),
        reason,
    }
}

fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> AdmissionError {
    if source.kind() == std::io::ErrorKind::Unsupported {
        return AdmissionError::Unsupported;
    }
    AdmissionError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(unix)]
fn open_directory(path: &Path) -> Result<File, AdmissionError> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Component;

    if !path.is_absolute() {
        return Err(unsafe_path(
            path,
            "an absolute private state directory is required",
        ));
    }
    let flags = libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open("/")
        .map_err(|error| io_error("open directory", path, error))?;
    for component in path.components() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => name,
            _ => {
                return Err(unsafe_path(
                    path,
                    "parent and relative path components are refused",
                ));
            }
        };
        let name = std::ffi::CString::new(name.as_bytes())
            .map_err(|_| unsafe_path(path, "path contains NUL"))?;
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        directory = from_descriptor(descriptor, "open directory without symlinks", path)?;
    }
    Ok(directory)
}

#[cfg(unix)]
fn from_descriptor(
    descriptor: libc::c_int,
    action: &'static str,
    path: &Path,
) -> Result<File, AdmissionError> {
    use std::os::fd::FromRawFd;

    if descriptor < 0 {
        return Err(io_error(action, path, std::io::Error::last_os_error()));
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(unix)]
fn validate_private(file: &File, path: &Path, directory: bool) -> Result<(), AdmissionError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file
        .metadata()
        .map_err(|error| io_error("inspect file", path, error))?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(unsafe_path(
            path,
            "expected an ordinary directory or regular lock file",
        ));
    }
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err(unsafe_path(
            path,
            "must be owned by the current user with no group or other access",
        ));
    }
    if !directory && metadata.nlink() != 1 {
        return Err(unsafe_path(
            path,
            "lock file must have exactly one hard link",
        ));
    }
    Ok(())
}

fn slot_name(slot: usize) -> String {
    if slot == 0 {
        LOCK_NAME.to_string()
    } else {
        format!("managed-build-{slot}.lock")
    }
}

#[cfg(unix)]
fn open_lock(
    directory: &File,
    root: &Path,
    name: &str,
    create: bool,
) -> Result<File, AdmissionError> {
    use std::os::fd::AsRawFd;

    let path = root.join(name);
    let c_name =
        std::ffi::CString::new(name).map_err(|_| unsafe_path(&path, "name contains NUL"))?;
    let mut flags = libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    if create {
        flags |= libc::O_CREAT;
    }
    let descriptor = unsafe { libc::openat(directory.as_raw_fd(), c_name.as_ptr(), flags, 0o600) };
    let file = from_descriptor(descriptor, "open regular lock file", &path)?;
    validate_private(&file, &path, false)?;
    Ok(file)
}

#[cfg(unix)]
fn same_file(first: &File, second: &File, path: &Path) -> Result<(), AdmissionError> {
    use std::os::unix::fs::MetadataExt;

    let first = first
        .metadata()
        .map_err(|error| io_error("inspect identity", path, error))?;
    let second = second
        .metadata()
        .map_err(|error| io_error("inspect identity", path, error))?;
    if (first.dev(), first.ino()) != (second.dev(), second.ino()) {
        return Err(unsafe_path(
            path,
            "admission directory or lock changed while waiting",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn verify_entry(
    root: &Path,
    directory: &File,
    lock: &File,
    name: &str,
) -> Result<(), AdmissionError> {
    let current = open_directory(root)?;
    validate_private(&current, root, true)?;
    same_file(directory, &current, root)?;
    let current_lock = open_lock(&current, root, name, false)?;
    same_file(lock, &current_lock, &root.join(name))
}

pub struct Pending {
    root: std::path::PathBuf,
    name: String,
    directory: File,
    lock: File,
}

impl Pending {
    pub fn open_slot(private_state_root: &Path, slot: usize) -> Result<Self, AdmissionError> {
        #[cfg(unix)]
        {
            let name = slot_name(slot);
            let directory = open_directory(private_state_root)?;
            validate_private(&directory, private_state_root, true)?;
            let lock = open_lock(&directory, private_state_root, &name, true)?;
            Ok(Self {
                root: private_state_root.to_path_buf(),
                name,
                directory,
                lock,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = (private_state_root, slot);
            Err(AdmissionError::Unsupported)
        }
    }

    pub fn try_take(&self, cancellation: &CancellationToken) -> Result<bool, AdmissionError> {
        #[cfg(unix)]
        {
            try_lock_once(
                &self.root,
                &self.directory,
                &self.lock,
                &self.name,
                cancellation,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = cancellation;
            Err(AdmissionError::Unsupported)
        }
    }

    pub fn release(&self) {
        let _ = self.lock.unlock();
    }

    pub fn into_permit(self) -> Permit {
        Permit::Lock {
            _directory: self.directory,
            _lock: self.lock,
        }
    }
}

#[cfg(any(test, feature = "test-seams"))]
fn try_acquire(
    pending: Pending,
    cancellation: &CancellationToken,
) -> Result<Result<Permit, Pending>, AdmissionError> {
    if pending.try_take(cancellation)? {
        Ok(Ok(pending.into_permit()))
    } else {
        Ok(Err(pending))
    }
}

pub async fn pause(cancellation: &CancellationToken) -> Result<(), AdmissionError> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(AdmissionError::Cancelled),
        _ = tokio::time::sleep(RETRY) => Ok(()),
    }
}

#[cfg(unix)]
fn try_lock_once(
    root: &Path,
    directory: &File,
    lock: &File,
    name: &str,
    cancellation: &CancellationToken,
) -> Result<bool, AdmissionError> {
    if cancellation.is_cancelled() {
        return Err(AdmissionError::Cancelled);
    }
    match lock.try_lock() {
        Ok(()) => {
            if cancellation.is_cancelled() {
                return Err(AdmissionError::Cancelled);
            }
            verify_entry(root, directory, lock, name)?;
            if cancellation.is_cancelled() {
                return Err(AdmissionError::Cancelled);
            }
            Ok(true)
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(error)) => Err(io_error(
            "acquire cooperative lock",
            &root.join(name),
            error,
        )),
    }
}

#[cfg(feature = "test-seams")]
#[path = "build_admission_test_seams.rs"]
pub mod test_seams;

#[cfg(all(test, unix))]
#[path = "build_admission_tests.rs"]
mod tests;
