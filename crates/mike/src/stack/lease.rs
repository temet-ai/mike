use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

pub(crate) struct FileLease {
    _file: std::fs::File,
}

impl Drop for FileLease {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

impl FileLease {
    #[cfg(unix)]
    pub(crate) fn try_acquire(path: &Path, create: bool) -> Result<Option<Self>> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .context("cannot open private installation lock")?;
        let metadata = file
            .metadata()
            .context("cannot inspect installation lock")?;
        anyhow::ensure!(
            metadata.is_file()
                && metadata.uid() == rustix::process::geteuid().as_raw()
                && metadata.permissions().mode() & 0o077 == 0,
            "installation lock must be a private file owned by the current user"
        );
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => {
                Err(error).context("cannot acquire installation ownership lock")
            }
        }
    }

    #[cfg(not(unix))]
    pub(crate) fn try_acquire(_path: &Path, _create: bool) -> Result<Option<Self>> {
        anyhow::bail!("private daemon ownership is supported only on Unix")
    }

    pub(crate) async fn acquire_start(path: &Path, deadline: Duration) -> Result<Self> {
        let start = std::time::Instant::now();
        loop {
            if let Some(lease) = Self::try_acquire(path, true)? {
                return Ok(lease);
            }
            anyhow::ensure!(
                start.elapsed() < deadline,
                "another start operation still owns this installation; retry after it finishes"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

pub(crate) fn ownership_held(path: &Path) -> Result<bool> {
    if !path
        .try_exists()
        .context("cannot inspect installation ownership")?
    {
        return Ok(false);
    }
    Ok(FileLease::try_acquire(path, false)?.is_none())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn operating_system_lease_lives_until_its_owner_is_dropped() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("daemon.lock");
        let first = FileLease::try_acquire(&path, true)?.context("first lease")?;
        assert!(ownership_held(&path)?);
        assert!(FileLease::try_acquire(&path, true)?.is_none());
        drop(first);
        assert!(!ownership_held(&path)?);
        Ok(())
    }

    #[test]
    fn lock_symlinks_and_shared_permissions_are_rejected() -> Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("lock");
        std::fs::write(&path, "unrelated")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        assert!(FileLease::try_acquire(&path, true).is_err());
        symlink(&path, dir.path().join("link"))?;
        assert!(FileLease::try_acquire(&dir.path().join("link"), true).is_err());
        assert!(!ownership_held(&dir.path().join("missing"))?);
        Ok(())
    }
    #[tokio::test]
    async fn start_mutex_is_bounded_and_recovers_after_release() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("start.lock");
        let held = FileLease::try_acquire(&path, true)?.context("held lock")?;
        assert!(
            FileLease::acquire_start(&path, Duration::ZERO)
                .await
                .is_err()
        );
        drop(held);
        let _lease = FileLease::acquire_start(&path, Duration::from_secs(1)).await?;
        Ok(())
    }
}
