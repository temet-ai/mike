use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn update_selected(path: &Path, updates: toml::Table) -> Result<Option<PathBuf>> {
    edit_selected(path, |table| merge(table, updates))
}

#[cfg(unix)]
pub fn edit_selected(path: &Path, edit: impl FnOnce(&mut toml::Table)) -> Result<Option<PathBuf>> {
    let transaction = super::write_transaction::Transaction::begin(path)?;
    let mut table = transaction.table()?;
    edit(&mut table);
    super::load::file::validate(&table)?;
    let contents = toml::to_string_pretty(&table)
        .map_err(|_| anyhow::anyhow!("cannot serialize private configuration"))?;
    transaction.commit(&contents)
}

#[cfg(not(unix))]
pub fn edit_selected(
    _path: &Path,
    _edit: impl FnOnce(&mut toml::Table),
) -> Result<Option<PathBuf>> {
    anyhow::bail!("private selected configuration transactions require Unix")
}

pub fn ensure_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        for ancestor in path.ancestors() {
            if let Ok(metadata) = std::fs::symlink_metadata(ancestor) {
                anyhow::ensure!(
                    !metadata.file_type().is_symlink(),
                    "installation directory ancestry must not contain symlinks"
                );
            }
        }
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .context("cannot create private installation directory")?;
        let metadata =
            std::fs::symlink_metadata(path).context("cannot inspect installation directory")?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink() && metadata.is_dir(),
            "installation directory must be a real directory"
        );
        anyhow::ensure!(
            metadata.permissions().mode() & 0o077 == 0
                && metadata.uid() == rustix::process::geteuid().as_raw(),
            "installation directory must be private (chmod 700 the selected directory)"
        );
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(path).context("cannot create installation directory")?;
    Ok(())
}

fn merge(target: &mut toml::Table, updates: toml::Table) {
    for (key, value) in updates {
        match (target.get_mut(&key), value) {
            (Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
                merge(existing, incoming)
            }
            (_, value) => {
                target.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
