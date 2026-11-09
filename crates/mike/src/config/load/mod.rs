use figment::{Figment, providers::Serialized};

use crate::analysis::EmbeddingConfig;
use crate::config::ProxyConfig;
use crate::environment::{Environment, ProcessEnvironment};

mod cli_overrides;
pub(crate) mod dotenv;
mod environment_layer;
pub(crate) mod file;
mod file_normalize;
pub(crate) mod import;
mod redact;
mod shell_export;

pub use cli_overrides::CliOverrides;
pub(crate) use redact::redact_database_url;
pub(crate) use shell_export::{to_shell_exports, to_toml_string};

pub fn load(cli: CliOverrides) -> anyhow::Result<ProxyConfig> {
    load_with(cli, &ProcessEnvironment)
}

pub(crate) fn load_with(
    cli: CliOverrides,
    environment: &dyn Environment,
) -> anyhow::Result<ProxyConfig> {
    load_layered(cli, environment)
}

fn load_layered(cli: CliOverrides, environment: &dyn Environment) -> anyhow::Result<ProxyConfig> {
    let paths = crate::config::paths::InstallationPaths::resolve_in(
        cli.config.as_deref(),
        &crate::environment::lookup(environment),
    )?;
    let snapshot =
        crate::config::child::read_snapshot(cli.boot_config_snapshot.as_deref(), &paths.state_dir)?;
    let table = if snapshot.is_some() {
        toml::Table::new()
    } else {
        file::read_with(&paths.config_file, environment)?
    };
    file::validate_with(&table, environment)?;
    let embedding_enabled = cli.embedding_enabled.or_else(|| {
        snapshot
            .as_ref()
            .and_then(|value| value.overrides.embedding_enabled)
    });
    let mut figment = Figment::from(Serialized::defaults(ProxyConfig::default()))
        .merge(Serialized::defaults(table));

    figment = environment_layer::merge_prefixed(figment, environment);
    if let Some(raw) = environment.var("MIKE_SPA_ORIGINS") {
        let parsed: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        figment = figment.merge(Serialized::default("spa_origins", parsed));
    }
    if let Some(raw) = environment.var("MIKE_INSPECT_ALLOWED_HOSTS") {
        let parsed: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        figment = figment.merge(Serialized::default("inspect_allowed_hosts", parsed));
    }
    if let Some(raw) = environment.var("MIKE_LAUNCH_CLAUDE_ARGS") {
        let parsed = crate::config::launch::parse_claude_args_env(&raw);
        figment = figment.merge(Serialized::default("launch_claude_args", parsed));
    }
    if let Some(snapshot) = &snapshot {
        figment = figment.merge(Serialized::defaults(&snapshot.config));
    }
    figment = figment.merge(Serialized::defaults(&cli));

    let mut config: ProxyConfig = figment.extract().map_err(|_| {
        anyhow::anyhow!(
            "failed to load boot configuration; check field types (values omitted for privacy)"
        )
    })?;

    for directory in [&mut config.data_dir, &mut config.log_dir]
        .into_iter()
        .flatten()
    {
        if directory.is_relative() {
            *directory = paths.config_dir.join(&*directory);
        }
    }
    config.config_path = paths.config_file;
    config.boot_overrides = snapshot.map_or(cli, |value| value.overrides);
    config.boot_overrides.config = Some(config.config_path.clone());
    config.data_dir.get_or_insert(paths.data_dir);
    config.log_dir.get_or_insert(paths.log_dir);
    apply_openrouter_env(&mut config, environment);
    config.embedding = EmbeddingConfig::default();

    if let Some(v) = embedding_enabled {
        config.embedding.enabled = v;
    }

    Ok(config)
}

pub fn reload_selected(config: &ProxyConfig) -> anyhow::Result<ProxyConfig> {
    reload_selected_with(config, &ProcessEnvironment)
}

pub(crate) fn reload_selected_with(
    config: &ProxyConfig,
    environment: &dyn Environment,
) -> anyhow::Result<ProxyConfig> {
    let mut cli = config.boot_overrides.clone();
    cli.config = Some(config.config_path.clone());
    cli.boot_config_snapshot = None;
    load_with(cli, environment)
}

fn apply_openrouter_env(config: &mut ProxyConfig, environment: &dyn Environment) {
    if let Some(key) = environment
        .var("MIKE_STEERER_OPENROUTER_API_KEY")
        .or_else(|| environment.var("OPENROUTER_API_KEY"))
    {
        config.steerer.openrouter_api_key = Some(secrecy::SecretString::new(key.into()));
    }
}

#[cfg(test)]
mod load_layered_tests;
#[cfg(test)]
mod selected_tests;
