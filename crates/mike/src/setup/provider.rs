use figment::providers::Env;
use secrecy::ExposeSecret;

use crate::config::{CliOverrides, ProxyConfig};

pub use super::provider_flow::ApplyError;

pub async fn apply(config: &mut ProxyConfig, key: &str) -> Result<(), ApplyError> {
    super::provider_flow::save_and_reconcile(
        config,
        |current| save_and_reload(current, key, false),
        super::plan::reconcile_selected,
    )
    .await
}

pub async fn apply_reference(config: &mut ProxyConfig, variable: &str) -> Result<(), ApplyError> {
    super::provider_flow::save_and_reconcile(
        config,
        |current| save_and_reload(current, variable, true),
        super::plan::reconcile_selected,
    )
    .await
}

fn save_and_reload(
    config: &ProxyConfig,
    input: &str,
    from_env: bool,
) -> Result<ProxyConfig, ApplyError> {
    let resolved = if from_env {
        std::env::var(input).ok()
    } else {
        Some(input.into())
    };
    let key = resolved.as_deref().unwrap_or_default();
    if input.is_empty() || !config.config_path.is_absolute() {
        return Err(failure(
            false,
            "a nonempty provider key and an absolute selected configuration are required; no configuration was changed",
        ));
    }
    validate_overrides(key, &config.boot_overrides).map_err(|_| failure(false,
        "an environment or CLI override replaces the requested provider key; remove the conflicting override before saving; no configuration was changed"))?;
    let saved = if from_env {
        super::persistence::save_provider_setting_at(&config.config_path, input, true)
    } else {
        super::persistence::save_provider_key_at(&config.config_path, input)
    };
    saved.map_err(|_| failure(true,
        "provider key write could not be verified; inspect the selected private configuration and its backup before retrying"))?;
    let reloaded = crate::config::reload_selected(config).map_err(|_| failure(true,
        "provider key was saved but configuration reload failed; setup remains incomplete; correct the overrides or restore the private backup"))?;
    if reloaded
        .steerer
        .openrouter_api_key
        .as_ref()
        .map(ExposeSecret::expose_secret)
        != resolved.as_deref()
    {
        return Err(failure(
            true,
            "provider key was saved but an override replaced its effective value; setup remains incomplete; remove the override or restore the private backup",
        ));
    }
    Ok(reloaded)
}

fn validate_overrides(key: &str, overrides: &CliOverrides) -> anyhow::Result<()> {
    let explicit = std::env::var("MIKE_STEERER_OPENROUTER_API_KEY")
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
        .ok();
    super::provider_override::validate(
        key,
        overrides,
        Env::prefixed("MIKE_").split("__"),
        explicit.as_deref(),
    )
}

fn failure(configuration_may_have_changed: bool, message: &'static str) -> ApplyError {
    ApplyError {
        configuration_may_have_changed,
        message,
    }
}
