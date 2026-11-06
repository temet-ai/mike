use figment::{Figment, Provider, providers::Serialized};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

pub(crate) fn validate(
    key: &str,
    overrides: &impl Serialize,
    environment: impl Provider,
    explicit: Option<&str>,
) -> anyhow::Result<()> {
    let wanted = serde_json::json!({"steerer":{"openrouter_api_key":key}});
    let effective: KeyLayer = Figment::from(Serialized::defaults(wanted))
        .merge(environment)
        .merge(Serialized::defaults(overrides))
        .extract()?;
    let actual = explicit.or_else(|| {
        effective
            .steerer
            .openrouter_api_key
            .as_ref()
            .map(ExposeSecret::expose_secret)
    });
    anyhow::ensure!(actual == Some(key), "provider key override mismatch");
    Ok(())
}

#[derive(Deserialize)]
struct KeyLayer {
    steerer: ProviderKey,
}
#[derive(Deserialize)]
struct ProviderKey {
    openrouter_api_key: Option<SecretString>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conflicting_nested_environment_or_explicit_alias_is_rejected() {
        let environment = Serialized::defaults(json!({"steerer":{"openrouter_api_key":"other"}}));
        assert!(validate("wanted", &json!({}), environment, None).is_err());
        let empty = Serialized::defaults(json!({}));
        assert!(validate("wanted", &json!({}), empty, Some("other")).is_err());
    }

    #[test]
    fn literal_key_and_equal_override_preserve_the_requested_effective_value() -> anyhow::Result<()>
    {
        let key = "quote' $HOME `command` λ";
        validate(key, &json!({}), Serialized::defaults(json!({})), None)?;
        validate(key, &json!({}), Serialized::defaults(json!({})), Some(key))?;
        Ok(())
    }

    #[test]
    fn explicit_key_alias_matches_the_boot_loader_precedence() -> anyhow::Result<()> {
        let environment = Serialized::defaults(json!({"steerer":{"openrouter_api_key":"nested"}}));
        let cli = json!({"steerer":{"openrouter_api_key":"cli"}});
        validate("explicit", &cli, environment, Some("explicit"))?;
        Ok(())
    }
}
