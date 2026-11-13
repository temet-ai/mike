use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use secrecy::ExposeSecret;
use sha2::Digest;

use crate::config::{ProxyConfig, paths::InstallationPaths};

use super::lease::{FileLease, ownership_held};
use super::protocol::InstanceIdentity;

const OWNER_FILE: &str = "instance.json";
const OWNER_LOCK: &str = "daemon.lock";
const START_LOCK: &str = "start.lock";
const CONTROL_SOCKET: &str = "control.sock";

pub(crate) struct InstanceSpec {
    pub(crate) paths: InstallationPaths,
    pub(crate) fingerprint: String,
    pub(crate) persist_enabled: bool,
}

impl InstanceSpec {
    pub(crate) fn from_config(config: &ProxyConfig) -> Result<Self> {
        let selection =
            (!config.config_path.as_os_str().is_empty()).then_some(config.config_path.as_path());
        let paths = InstallationPaths::resolve(selection)?;
        super::socket_path::validate(&paths.state_dir.join(CONTROL_SOCKET))?;
        let mut normalized = config.clone();
        normalized.data_dir.get_or_insert(paths.data_dir.clone());
        normalized.log_dir.get_or_insert(paths.log_dir.clone());
        Ok(Self {
            fingerprint: private_boot_fingerprint(&normalized)?,
            paths,
            persist_enabled: config.persist_enabled,
        })
    }

    pub(crate) fn socket_path(&self) -> PathBuf {
        self.paths.state_dir.join(CONTROL_SOCKET)
    }
    pub(crate) fn owner_lock(&self) -> PathBuf {
        self.paths.state_dir.join(OWNER_LOCK)
    }
    pub(crate) fn start_lock(&self) -> PathBuf {
        self.paths.state_dir.join(START_LOCK)
    }
    pub(crate) fn owner_file(&self) -> PathBuf {
        self.paths.state_dir.join(OWNER_FILE)
    }
}

fn private_boot_fingerprint(config: &ProxyConfig) -> Result<String> {
    let mut private =
        serde_json::to_value(config).context("cannot describe private boot configuration")?;
    if let Some(key) = &config.steerer.openrouter_api_key {
        private["steerer"]["openrouter_api_key"] =
            serde_json::Value::String(key.expose_secret().to_owned());
    }
    let bytes = serde_json::to_vec(&private).context("cannot encode private boot configuration")?;
    Ok(hex::encode(sha2::Sha256::digest(bytes)))
}

pub(crate) struct DaemonOwner {
    pub(crate) spec: InstanceSpec,
    pub(crate) identity: InstanceIdentity,
    _lease: Option<FileLease>,
    pub(crate) stale_socket_is_owned: bool,
    pub(crate) owns_installation: bool,
}

impl DaemonOwner {
    pub(crate) fn acquire(config: &ProxyConfig) -> Result<Self> {
        let spec = InstanceSpec::from_config(config)?;
        crate::config::write::ensure_private_directory(&spec.paths.state_dir)?;
        let lease = FileLease::try_acquire(&spec.owner_lock(), true)?
            .context("the selected installation already has a daemon owner; use its private control connection")?;
        let stale_socket_is_owned = check_existing_record(&spec)?;
        let identity = fresh_identity(&spec)?;
        crate::owner_only_file::write(&spec.owner_file(), &serde_json::to_string(&identity)?)
            .context("cannot publish private daemon identity")?;
        Ok(Self {
            spec,
            identity,
            _lease: Some(lease),
            stale_socket_is_owned,
            owns_installation: true,
        })
    }

    pub(crate) fn acquire_unless_smoke_run(config: &ProxyConfig) -> Result<Self> {
        if config.smoke_run {
            Self::observe_without_ownership(config)
        } else {
            Self::acquire(config)
        }
    }

    fn observe_without_ownership(config: &ProxyConfig) -> Result<Self> {
        let spec = InstanceSpec::from_config(config)?;
        let identity = fresh_identity(&spec)?;
        Ok(Self {
            spec,
            identity,
            _lease: None,
            stale_socket_is_owned: false,
            owns_installation: false,
        })
    }
}

fn fresh_identity(spec: &InstanceSpec) -> Result<InstanceIdentity> {
    Ok(InstanceIdentity {
        instance_id: spec.paths.installation_id.clone(),
        generation: uuid::Uuid::new_v4(),
        pid: std::process::id(),
        started_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    })
}

impl Drop for DaemonOwner {
    fn drop(&mut self) {
        if !self.owns_installation {
            return;
        }
        let record = read_record(&self.spec.owner_file()).ok().flatten();
        if record.as_ref() == Some(&self.identity) {
            let _ = std::fs::remove_file(self.spec.owner_file());
        }
    }
}

fn check_existing_record(spec: &InstanceSpec) -> Result<bool> {
    if let Some(record) = read_record(&spec.owner_file())? {
        anyhow::ensure!(
            record.instance_id == spec.paths.installation_id,
            "private daemon identity belongs to another installation; no state was overwritten"
        );
        return Ok(true);
    }
    Ok(false)
}

fn read_record(path: &Path) -> Result<Option<InstanceIdentity>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot inspect private daemon identity"),
    };
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "private daemon identity must be a regular file"
    );
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        anyhow::ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw()
                && metadata.permissions().mode() & 0o077 == 0,
            "daemon identity must be private and owned by this user"
        );
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .context("cannot open private daemon identity")?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text)
        .context("cannot read private daemon identity")?;
    serde_json::from_str(&text).map(Some).map_err(|_| {
        anyhow::anyhow!("invalid private daemon identity; archive the record before retrying")
    })
}

pub(crate) fn verify_owner(spec: &InstanceSpec, identity: &InstanceIdentity) -> Result<()> {
    identity.verify_owner(
        read_record(&spec.owner_file())?.as_ref(),
        ownership_held(&spec.owner_lock())?,
    )
}

pub(crate) fn running_process(config: &ProxyConfig) -> Option<(u32, SystemTime)> {
    let spec = InstanceSpec::from_config(config).ok()?;
    if !ownership_held(&spec.owner_lock()).ok()? {
        return None;
    }
    let record = read_record(&spec.owner_file()).ok()??;
    if record.instance_id != spec.paths.installation_id {
        return None;
    }
    Some((
        record.pid,
        UNIX_EPOCH + Duration::from_secs(record.started_unix_secs),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_credentials_and_every_boot_override_affect_private_matching() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut config = ProxyConfig::for_tests_in_memory();
        config.config_path = dir.path().join("config.toml");
        let original = InstanceSpec::from_config(&config)?;
        config.persist_enabled = true;
        assert_ne!(
            original.fingerprint,
            InstanceSpec::from_config(&config)?.fingerprint
        );
        config.persist_enabled = false;
        config.set_openrouter_api_key("private-key".into());
        assert_ne!(
            original.fingerprint,
            InstanceSpec::from_config(&config)?.fingerprint
        );
        assert_eq!(
            original.paths.installation_id,
            InstanceSpec::from_config(&config)?.paths.installation_id
        );
        Ok(())
    }

    #[test]
    fn daemon_lease_prevents_second_owner_and_releases_after_drop() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut config = ProxyConfig::for_tests_in_memory();
        config.config_path = dir.path().join("config.toml");
        let first = DaemonOwner::acquire(&config)?;
        assert!(DaemonOwner::acquire(&config).is_err());
        assert_eq!(
            running_process(&config).map(|p| p.0),
            Some(std::process::id())
        );
        let generation = first.identity.generation;
        drop(first);
        assert!(running_process(&config).is_none());
        let next = DaemonOwner::acquire(&config)?;
        assert_ne!(next.identity.generation, generation);
        Ok(())
    }
}
