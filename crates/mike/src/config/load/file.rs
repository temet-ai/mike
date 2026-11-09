use std::path::Path;

use anyhow::{Context, Result};
use figment::{Figment, providers::Serialized};

use crate::config::ProxyConfig;
use crate::environment::{Environment, ProcessEnvironment};

pub(crate) fn read(path: &Path) -> Result<toml::Table> {
    read_with(path, &ProcessEnvironment)
}

pub(crate) fn read_with(path: &Path, environment: &dyn Environment) -> Result<toml::Table> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(toml::Table::new()),
        Err(error) => return Err(error).context("cannot read the selected configuration file"),
    };
    let table = contents.parse().map_err(|_| {
        anyhow::anyhow!("invalid TOML in selected configuration; values omitted for privacy")
    })?;
    super::file_normalize::normalize(&table, environment)
}

pub(crate) fn validate(table: &toml::Table) -> Result<()> {
    validate_with(table, &ProcessEnvironment)
}

pub(crate) fn validate_with(table: &toml::Table, environment: &dyn Environment) -> Result<()> {
    let table = super::file_normalize::normalize(table, environment)?;
    let defaults = serde_json::to_value(ProxyConfig::default())?;
    anyhow::ensure!(
        table.keys().all(|key| defaults.get(key).is_some()),
        "selected configuration contains an unsupported boot setting"
    );
    Figment::from(Serialized::defaults(ProxyConfig::default()))
        .merge(Serialized::defaults(table))
        .extract::<ProxyConfig>()
        .map_err(|_| {
            anyhow::anyhow!("invalid boot configuration field type; values omitted for privacy")
        })?;
    Ok(())
}
