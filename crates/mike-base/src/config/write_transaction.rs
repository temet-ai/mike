use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::owner_only_file::anchored::{PrivateDirectory, SourceSnapshot};

pub struct Transaction {
    directory: PrivateDirectory,
    parent: PathBuf,
    name: OsString,
    lock_name: OsString,
    lock: WriteLease,
    original: Option<SourceSnapshot>,
}

impl Transaction {
    pub fn begin(path: &Path) -> Result<Self> {
        let paths = super::paths::InstallationPaths::resolve(Some(path))?;
        super::write::ensure_private_directory(&paths.config_dir)?;
        let directory = PrivateDirectory::open(&paths.config_dir)
            .context("cannot anchor private configuration directory")?;
        let lock_name = OsString::from(format!(".mike-config-{}.lock", paths.installation_id));
        let lock = directory
            .lock_file(&lock_name)
            .context("cannot open private configuration write lock")?;
        let lock = acquire(lock)?;
        directory.same_file(&lock_name, &lock.file)?;
        let name = paths
            .config_file
            .file_name()
            .context("selected configuration needs a file name")?
            .to_os_string();
        let original = directory
            .read(&name)
            .context("cannot safely read selected configuration")?;
        Ok(Self {
            directory,
            parent: paths.config_dir,
            name,
            lock_name,
            lock,
            original,
        })
    }

    pub fn table(&self) -> Result<toml::Table> {
        self.original
            .as_ref()
            .map_or(Ok(toml::Table::new()), |source| {
                source.contents.parse().map_err(|_| {
                    anyhow::anyhow!(
                        "invalid TOML in selected configuration; values omitted for privacy"
                    )
                })
            })
    }

    pub fn commit(&self, contents: &str) -> Result<Option<PathBuf>> {
        self.unchanged()?;
        let backup = self.backup()?;
        self.directory.replace_checked(&self.name, contents, || self.unchanged())
            .context("cannot replace selected configuration; previous configuration retained unless directory sync failed")?;
        Ok(backup)
    }

    fn unchanged(&self) -> std::io::Result<()> {
        self.directory.still_selected(&self.parent)?;
        self.directory.same_file(&self.lock_name, &self.lock.file)?;
        if self.directory.read(&self.name)? != self.original {
            return Err(std::io::Error::other(
                "selected configuration changed outside the writer lock; retry without overwriting it",
            ));
        }
        Ok(())
    }

    fn backup(&self) -> Result<Option<PathBuf>> {
        let Some(original) = &self.original else {
            return Ok(None);
        };
        let mut name = self.name.clone();
        name.push(format!(".backup-{}", uuid::Uuid::new_v4()));
        self.directory
            .create_new(&name, &original.contents)
            .context("cannot back up previous configuration")?;
        Ok(Some(self.parent.join(name)))
    }
}

struct WriteLease {
    file: File,
}

impl Drop for WriteLease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn acquire(file: File) -> Result<WriteLease> {
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(WriteLease { file }),
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(error).context("cannot acquire selected configuration write lock");
            }
        }
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(10),
            "another writer still owns the selected configuration; retry after it finishes"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
#[path = "write_transaction_tests.rs"]
mod tests;
