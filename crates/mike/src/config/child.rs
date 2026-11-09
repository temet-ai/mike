use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

use super::{CliOverrides, ProxyConfig, paths::InstallationPaths};
use crate::environment::{Environment, ProcessEnvironment};

#[derive(Deserialize, Serialize)]
pub(crate) struct BootSnapshot {
    pub(crate) config: toml::Table,
    pub(crate) overrides: CliOverrides,
}

pub fn configure_child(
    command: &mut std::process::Command,
    config: &ProxyConfig,
) -> Result<PathBuf> {
    configure_child_in(command, config, &ProcessEnvironment)
}

pub(crate) fn configure_child_in(
    command: &mut std::process::Command,
    config: &ProxyConfig,
    environment: &dyn Environment,
) -> Result<PathBuf> {
    let selection =
        (!config.config_path.as_os_str().is_empty()).then_some(config.config_path.as_path());
    let paths = InstallationPaths::resolve_in(selection, &crate::environment::lookup(environment))?;
    super::write::ensure_private_directory(&paths.state_dir)?;
    let path = paths
        .state_dir
        .join(format!("boot-{}.toml", uuid::Uuid::new_v4()));
    let snapshot = BootSnapshot {
        config: private_config_table(config)?,
        overrides: config.boot_overrides.clone(),
    };
    let contents = toml::to_string(&snapshot)
        .map_err(|_| anyhow::anyhow!("cannot serialize private child configuration"))?;
    crate::owner_only_file::write(&path, &contents)
        .context("cannot stage private child configuration")?;
    command
        .arg("--config")
        .arg(paths.config_file)
        .arg("--boot-config-snapshot")
        .arg(&path);
    Ok(path)
}

fn private_config_table(config: &ProxyConfig) -> Result<toml::Table> {
    let mut value = toml::Value::try_from(config)
        .map_err(|_| anyhow::anyhow!("cannot serialize private boot configuration"))?;
    if let Some(key) = &config.steerer.openrouter_api_key {
        let steerer = value
            .get_mut("steerer")
            .and_then(toml::Value::as_table_mut)
            .context("missing steerer table in private configuration")?;
        steerer.insert(
            "openrouter_api_key".into(),
            toml::Value::String(key.expose_secret().to_owned()),
        );
    }
    value
        .as_table()
        .cloned()
        .context("private configuration must be a table")
}

pub(crate) fn read_snapshot(path: Option<&Path>, state_dir: &Path) -> Result<Option<BootSnapshot>> {
    use std::io::Read;
    let Some(path) = path else {
        return Ok(None);
    };
    validate_snapshot_location(path, state_dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .context("cannot open private child configuration")?;
    let metadata = file
        .metadata()
        .context("cannot inspect private child configuration")?;
    validate_snapshot_metadata(&metadata)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .context("cannot read private child configuration")?;
    let snapshot: BootSnapshot = toml::from_str(&contents).map_err(|_| {
        anyhow::anyhow!("invalid private child configuration; values omitted for privacy")
    })?;
    super::load::file::validate(&snapshot.config)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let current =
            std::fs::symlink_metadata(path).context("private child configuration disappeared")?;
        anyhow::ensure!(
            current.dev() == metadata.dev() && current.ino() == metadata.ino(),
            "private child configuration changed while reading"
        );
    }
    std::fs::remove_file(path).context("cannot consume private child configuration")?;
    Ok(Some(snapshot))
}

fn validate_snapshot_location(path: &Path, state_dir: &Path) -> Result<()> {
    anyhow::ensure!(
        path.parent() == Some(state_dir),
        "child configuration must be inside this installation's state directory"
    );
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid child configuration filename")?;
    let id = name
        .strip_prefix("boot-")
        .and_then(|name| name.strip_suffix(".toml"));
    anyhow::ensure!(
        id.is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()),
        "invalid child configuration filename"
    );
    for ancestor in state_dir.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .context("cannot inspect child configuration directory")?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "child configuration directory must not be symlinked"
        );
    }
    let metadata =
        std::fs::metadata(state_dir).context("cannot inspect child configuration directory")?;
    validate_private_owner(&metadata)
}

fn validate_snapshot_metadata(metadata: &std::fs::Metadata) -> Result<()> {
    anyhow::ensure!(
        metadata.is_file(),
        "child configuration must be a regular private file"
    );
    validate_private_owner(metadata)
}

fn validate_private_owner(metadata: &std::fs::Metadata) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        anyhow::ensure!(
            metadata.permissions().mode() & 0o077 == 0
                && metadata.uid() == unsafe { libc::geteuid() },
            "child configuration must be owned by the current user and readable only by its owner"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_receives_private_values_without_secret_argv_and_consumes_snapshot() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut config = ProxyConfig {
            config_path: dir.path().join("config.toml"),
            database_url: Some("postgres://u:child-secret@localhost/db".into()),
            ..ProxyConfig::default()
        };
        config.set_openrouter_api_key("s $`\\\"#λ".into());
        config.boot_overrides.max_requests = Some(4321);
        let mut command = std::process::Command::new("mike");
        let path = configure_child(&mut command, &config)?;
        assert!(!format!("{command:?}").contains("child-secret"));
        let snapshot = read_snapshot(Some(&path), path.parent().context("state directory")?)?
            .context("snapshot missing")?;
        assert_eq!(
            snapshot.config["database_url"].as_str(),
            config.database_url.as_deref()
        );
        assert_eq!(
            snapshot.config["steerer"]["openrouter_api_key"].as_str(),
            Some("s $`\\\"#λ")
        );
        assert_eq!(snapshot.overrides.max_requests, Some(4321));
        assert!(!path.exists());
        assert!(read_snapshot(Some(&path), path.parent().context("state directory")?).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn public_snapshot_permissions_are_rejected() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let file = dir
            .path()
            .join(format!("boot-{}.toml", uuid::Uuid::new_v4()));
        std::fs::write(&file, "private")?;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
        assert!(read_snapshot(Some(&file), dir.path()).is_err());
        assert!(file.exists());
        Ok(())
    }
    #[test]
    fn child_snapshot_preserves_effective_overrides_and_selected_reload_source() -> Result<()> {
        use crate::config::load::{load_with, reload_selected_with};
        use crate::environment::MapEnvironment;
        use clap::Parser;
        let environment = MapEnvironment::isolated();
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "max_requests=11")?;
        let cli = CliOverrides {
            config: Some(file.clone()),
            max_requests: Some(55),
            ..CliOverrides::default()
        };
        let parent = load_with(cli, &environment)?;
        let mut command = std::process::Command::new("mike");
        let snapshot = configure_child_in(&mut command, &parent, &environment)?;
        std::fs::write(&file, "max_requests=22")?;
        let child_cli = CliOverrides::try_parse_from(
            std::iter::once(std::ffi::OsStr::new("mike")).chain(command.get_args()),
        )?;
        let child = load_with(child_cli, &environment)?;
        assert_eq!(child.max_requests, 55);
        assert_eq!(child.config_path, file);
        assert!(!snapshot.exists());
        assert_eq!(reload_selected_with(&child, &environment)?.max_requests, 55);
        Ok(())
    }
    #[test]
    fn arbitrary_or_malformed_private_files_are_never_deleted() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let arbitrary = dir.path().join("credentials.toml");
        crate::owner_only_file::write(&arbitrary, "private-value")?;
        assert!(read_snapshot(Some(&arbitrary), dir.path()).is_err());
        assert!(arbitrary.exists());
        let malformed = dir
            .path()
            .join(format!("boot-{}.toml", uuid::Uuid::new_v4()));
        crate::owner_only_file::write(&malformed, "planted-secret")?;
        assert!(read_snapshot(Some(&malformed), dir.path()).is_err());
        assert!(malformed.exists());
        Ok(())
    }
}
