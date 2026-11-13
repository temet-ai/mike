use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::RegistrationPlan;

pub(super) struct Prepared {
    path: PathBuf,
    original: Option<Vec<u8>>,
    replacement: Option<Vec<u8>>,
    _lock: RegistrationLock,
}

struct RegistrationLock {
    file: File,
}

impl Drop for RegistrationLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(super) fn prepare(plan: &RegistrationPlan, replace: bool) -> Result<Prepared> {
    prepare_edit(plan, |document| merge(document, plan.entry(), replace))
}

pub(super) fn prepare_removal(plan: &RegistrationPlan) -> Result<Prepared> {
    prepare_edit(plan, |document| remove_owned(document, &plan.entry()))
}

fn prepare_edit(
    plan: &RegistrationPlan,
    edit: impl FnOnce(&mut Value) -> Result<bool>,
) -> Result<Prepared> {
    let path = super::absolute(&plan.client_config)?;
    let lock = acquire_lock(&path)?;
    let original = read(&path)?;
    let mut document = match &original {
        Some(bytes) => serde_json::from_slice(bytes)
            .context("Claude config is malformed JSON; refusing to overwrite it")?,
        None => json!({}),
    };
    let changed = edit(&mut document)?;
    let replacement = if changed {
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        Some(bytes)
    } else {
        None
    };
    Ok(Prepared {
        path,
        original,
        replacement,
        _lock: lock,
    })
}

fn remove_owned(document: &mut Value, entry: &Value) -> Result<bool> {
    let root = document
        .as_object_mut()
        .context("Claude config is not a JSON object; refusing to overwrite it")?;
    let Some(servers) = root.get_mut("mcpServers") else {
        return Ok(false);
    };
    let servers = servers
        .as_object_mut()
        .context("Claude config mcpServers is not a JSON object; refusing to overwrite it")?;
    let Some(existing) = servers.get("mike") else {
        return Ok(false);
    };
    anyhow::ensure!(
        existing == entry,
        "Claude has a changed or foreign mike registration; refusing to remove it because it does not exactly match the selected installation identity, stable companion, and inspect endpoint; legacy entries without an ownership marker require manual review"
    );
    servers.remove("mike");
    Ok(true)
}

fn merge(document: &mut Value, entry: Value, replace: bool) -> Result<bool> {
    let root = document
        .as_object_mut()
        .context("Claude config is not a JSON object; refusing to overwrite it")?;
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .context("Claude config mcpServers is not a JSON object; refusing to overwrite it")?;
    if let Some(existing) = servers.get("mike") {
        if existing == &entry {
            return Ok(false);
        }
        anyhow::ensure!(
            replace,
            "Claude already has a different mike registration; review it and use mike mcp register --replace to replace only that entry with a backup"
        );
    }
    servers.insert("mike".to_string(), entry);
    Ok(true)
}

impl Prepared {
    pub(super) fn commit(self) -> Result<(bool, Option<PathBuf>)> {
        anyhow::ensure!(
            read(&self.path)? == self.original,
            "Claude config changed while preparing MCP registration update; no registration was changed, retry after the other writer finishes"
        );
        let Some(bytes) = self.replacement else {
            return Ok((false, None));
        };
        let parent = self
            .path
            .parent()
            .context("Claude config needs a parent directory")?;
        let backup = self
            .original
            .as_deref()
            .map(|bytes| backup(&self.path, bytes))
            .transpose()?;
        File::open(parent)?.sync_all()?;
        let mut pending = tempfile::Builder::new()
            .prefix(".mike-registration-")
            .tempfile_in(parent)?;
        private_permissions(pending.as_file())?;
        pending.write_all(&bytes)?;
        pending.as_file().sync_all()?;
        anyhow::ensure!(
            read(&self.path)? == self.original,
            "Claude config changed before atomic write; original and backup preserved, retry registration update"
        );
        if self.original.is_some() {
            pending
                .persist(&self.path)
                .context("could not atomically replace Claude config; backup preserved")?;
        } else {
            pending
                .persist_noclobber(&self.path)
                .context("Claude config appeared during registration; refusing to overwrite it")?;
        }
        File::open(parent)?.sync_all()?;
        Ok((true, backup))
    }
}

fn acquire_lock(path: &Path) -> Result<RegistrationLock> {
    let name = path
        .file_name()
        .context("Claude config requires a file name")?
        .to_string_lossy();
    let lock_path = path.with_file_name(format!(".{name}.mike-registration.lock"));
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let lock = options
        .open(lock_path)
        .context("cannot open safe Claude registration lock")?;
    validate_file(&lock)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        anyhow::ensure!(
            lock.metadata()?.permissions().mode() & 0o077 == 0,
            "Claude registration lock has unsafe permissions"
        );
    }
    lock.try_lock().map_err(|_| {
        anyhow::anyhow!("another MCP registration is running; retry after it finishes")
    })?;
    Ok(RegistrationLock { file: lock })
}

fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "Claude config is a symlink; refusing to read or overwrite it"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot inspect Claude config"),
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .context("cannot safely open Claude config")?;
    validate_file(&file)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .context("cannot read Claude config")?;
    Ok(Some(bytes))
}

fn validate_file(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file(),
        "Claude config and registration lock must be regular files"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        anyhow::ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw() && metadata.nlink() == 1,
            "Claude config and registration lock must be owned by this user with no hard links"
        );
    }
    Ok(())
}

fn backup(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let name = path
        .file_name()
        .context("Claude config requires a file name")?
        .to_string_lossy();
    let destination = path.with_file_name(format!("{name}.mike-backup-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&destination)
        .context("could not create private Claude config backup")?;
    private_permissions(&file)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(destination)
}

fn private_permissions(file: &File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
