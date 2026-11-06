use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use figment::{
    Figment,
    providers::{Env, Serialized},
};
use serde::{Deserialize, Serialize};

use crate::config::{ProxyConfig, paths::InstallationPaths};

pub const TRIAL_RESTART_WARNING: &str = "Reconfiguration may restart the selected server. Existing trial captures are ephemeral and are not transferred into PostgreSQL. Existing database history and managed volumes are retained.";

pub enum StorageChoice {
    Trial,
    Existing(String),
    Docker { port: u16, image: Option<String> },
}

#[derive(Debug, Serialize)]
pub struct SetupReport {
    pub persistent: bool,
    pub config_file: PathBuf,
    pub previous_config_backup: Option<PathBuf>,
    pub verified_running: bool,
}

pub async fn apply(config: &mut ProxyConfig, choice: StorageChoice) -> Result<SetupReport> {
    let mut report = configure(config, choice).await?;
    reconcile_selected(config).await?;
    report.verified_running = true;
    Ok(report)
}

pub(crate) async fn configure(
    config: &mut ProxyConfig,
    choice: StorageChoice,
) -> Result<SetupReport> {
    ensure!(
        config.config_path.is_absolute(),
        "setup requires an absolute selected config path"
    );
    let persistent = !matches!(choice, StorageChoice::Trial);
    validate_overrides(config, persistent, None)?;
    let database_url = match choice {
        StorageChoice::Trial => None,
        StorageChoice::Existing(url) => {
            let url = url.trim().to_string();
            validate_overrides(config, true, Some(&url))?;
            super::database::validate_existing(&url).await?;
            Some(url)
        }
        StorageChoice::Docker { port, image } => {
            let paths = InstallationPaths::resolve(Some(&config.config_path))?;
            let url = super::docker::provision(&paths, port, image.as_deref()).await?;
            validate_overrides(config, true, Some(&url))?;
            Some(url)
        }
    };
    let backup =
        super::persistence::save_database_mode_at(&config.config_path, database_url.as_deref())?;
    let reloaded = crate::config::reload_selected(config)
        .context("setup saved the selected file but reload failed; restore its private backup or correct the overrides and retry")?;
    ensure!(
        reloaded.persist_enabled == persistent
            && database_url
                .as_ref()
                .is_none_or(|url| reloaded.database_url.as_ref() == Some(url))
            && (!persistent || !reloaded.db_fallback_enabled),
        "setup saved the selected mode but an environment or CLI override changed it; setup is incomplete, restore the private backup or remove the override and retry"
    );
    *config = reloaded;
    Ok(SetupReport {
        persistent,
        config_file: config.config_path.clone(),
        previous_config_backup: backup,
        verified_running: false,
    })
}

pub(crate) async fn reconcile_selected(config: &ProxyConfig) -> Result<()> {
    crate::stack::reconcile(config).await
        .context("setup saved configuration but could not verify its running state; setup is incomplete, inspect the owned server, restore the private backup if needed, and retry")?;
    ensure!(
        crate::stack::probe_selected(config).await,
        "setup configuration was saved but the selected owned server did not verify; previous configuration backup was retained"
    );
    Ok(())
}

#[derive(Deserialize)]
struct EffectiveMode {
    #[serde(deserialize_with = "crate::config::deserialize_permissive_bool")]
    persist_enabled: bool,
    database_url: Option<String>,
    #[serde(deserialize_with = "crate::config::deserialize_permissive_bool")]
    db_fallback_enabled: bool,
}

pub(super) fn validate_overrides(
    config: &ProxyConfig,
    persistent: bool,
    database_url: Option<&str>,
) -> Result<()> {
    let wanted = serde_json::json!({"persist_enabled": persistent,
        "database_url": database_url, "db_fallback_enabled": false});
    let mode: EffectiveMode = Figment::from(Serialized::defaults(wanted))
        .merge(Env::prefixed("MIKE_").split("__"))
        .merge(Serialized::defaults(&config.boot_overrides))
        .extract()
        .map_err(|_| {
            anyhow::anyhow!("invalid setup environment or CLI override; values omitted for privacy")
        })?;
    ensure!(
        mode.persist_enabled == persistent
            && database_url.is_none_or(|url| mode.database_url.as_deref() == Some(url))
            && (!persistent || !mode.db_fallback_enabled),
        "environment or CLI flags override the requested storage mode or URL; remove the conflicting override before setup; no configuration was changed"
    );
    Ok(())
}
