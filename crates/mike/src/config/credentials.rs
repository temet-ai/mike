use anyhow::Result;
use secrecy::{ExposeSecret, SecretString};

use super::ProxyConfig;

pub(crate) fn reload_openrouter_key(
    config: &ProxyConfig,
    env_var_name: &str,
) -> Result<Option<SecretString>> {
    if let Ok(value) = std::env::var(env_var_name) {
        return Ok((!value.is_empty()).then(|| SecretString::new(value.into())));
    }
    let loaded = super::reload_selected(config)?;
    Ok(loaded
        .steerer
        .openrouter_api_key
        .filter(|key| !key.expose_secret().is_empty()))
}
