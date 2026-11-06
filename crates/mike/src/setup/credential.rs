use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};

pub(super) fn prepare_directory(path: &Path) -> Result<()> {
    crate::config::write::ensure_private_directory(path)
}

pub(super) fn lock(directory: &Path) -> Result<std::fs::File> {
    prepare_directory(directory)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let lock = options
        .open(directory.join("operation.lock"))
        .context("cannot open the private database operation lock")?;
    lock.try_lock().map_err(|_| {
        anyhow::anyhow!(
            "another database operation is using this installation; wait for it to finish and retry"
        )
    })?;
    Ok(lock)
}

pub(super) fn load_or_create(path: &Path, resources_exist: bool) -> Result<String> {
    if path
        .try_exists()
        .context("cannot inspect the database credential")?
    {
        return read(path);
    }
    ensure!(
        !resources_exist,
        "managed database resources already exist but their private credential is missing; restore the credential file before retrying"
    );
    let password = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(password.as_bytes())
                .context("cannot save the private database credential")?;
            file.sync_all()
                .context("cannot sync the private database credential")?;
            Ok(password)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read(path),
        Err(_) => anyhow::bail!(
            "cannot create the private database credential; check directory permissions"
        ),
    }
}

fn read(path: &Path) -> Result<String> {
    let metadata = std::fs::symlink_metadata(path)
        .context("cannot inspect the private database credential")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "database credential must be a regular private file, not a symlink"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() }
                && metadata.permissions().mode() & 0o077 == 0,
            "database credential must be owned by the current user and readable by its owner only"
        );
    }
    let value =
        std::fs::read_to_string(path).context("cannot read the private database credential")?;
    ensure!(
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "database credential file is invalid; restore the original private credential instead of rotating an existing database"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_credential_is_reused_and_private() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("password");
        let first = load_or_create(&path, false)?;
        assert_eq!(first.len(), 64);
        assert_eq!(load_or_create(&path, true)?, first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
        }
        Ok(())
    }

    #[test]
    fn existing_database_never_gets_a_replacement_credential() -> Result<()> {
        let dir = tempfile::tempdir()?;
        assert!(load_or_create(&dir.path().join("missing"), true).is_err());
        Ok(())
    }

    #[test]
    fn invalid_credential_is_rejected_without_echoing_contents() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("password");
        crate::owner_only_file::write(&path, "private-invalid-credential")?;
        let error = load_or_create(&path, true)
            .err()
            .ok_or_else(|| anyhow::anyhow!("invalid credential"))?;
        assert!(!error.to_string().contains("private-invalid"));
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod private_path_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn symlink_and_world_readable_credentials_are_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("private");
        load_or_create(&file, false)?;
        let link = dir.path().join("link");
        symlink(&file, &link)?;
        assert!(load_or_create(&link, true).is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
        assert!(load_or_create(&file, true).is_err());
        Ok(())
    }

    #[test]
    fn operation_lock_refuses_an_overlapping_setup() -> Result<()> {
        let dir = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()?;
        let path = dir.path().canonicalize()?;
        let first = lock(&path)?;
        assert!(lock(&path).is_err());
        drop(first);
        lock(&path)?;
        Ok(())
    }
}
