use std::sync::Arc;

use arc_swap::ArcSwap;
use secrecy::{ExposeSecret, SecretString};

use super::model_ref::ModelRef;
use super::openrouter::missing_key_error;
use super::provider::{ModelTier, ProviderPolicy};
use super::{LlmError, OpenRouterClient};
use crate::environment::{Environment, ProcessEnvironment};

pub type OpenRouterKeySlot = Arc<ArcSwap<Option<SecretString>>>;

pub type AnthropicKeySlot = Arc<ArcSwap<Option<SecretString>>>;

pub fn seed_anthropic_from_env() -> Option<SecretString> {
    seed_anthropic_from(&ProcessEnvironment)
}

pub fn seed_anthropic_from(environment: &dyn Environment) -> Option<SecretString> {
    secret_from(environment, "MIKE_ASSISTANT_ANTHROPIC_API_KEY")
}

pub fn seed_from_env(config_key: Option<&SecretString>) -> Option<SecretString> {
    seed_from(config_key, &ProcessEnvironment)
}

pub fn seed_from(
    config_key: Option<&SecretString>,
    environment: &dyn Environment,
) -> Option<SecretString> {
    config_key
        .map(|s| SecretString::new(s.expose_secret().to_owned().into()))
        .or_else(|| secret_from(environment, "MIKE_STEERER_OPENROUTER_API_KEY"))
        .or_else(|| secret_from(environment, "OPENROUTER_API_KEY"))
}

fn secret_from(environment: &dyn Environment, name: &str) -> Option<SecretString> {
    environment
        .var(name)
        .filter(|value| !value.is_empty())
        .map(|value| SecretString::new(value.into()))
}

pub fn resolve(slot: &OpenRouterKeySlot) -> Option<SecretString> {
    let guard = slot.load();
    let current: &Option<SecretString> = &guard;
    current.clone()
}

fn api_key_text(slot: &OpenRouterKeySlot) -> String {
    resolve(slot)
        .map(|key| key.expose_secret().to_owned())
        .unwrap_or_default()
}

fn with_key(base: &OpenRouterClient, api_key: String) -> Option<OpenRouterClient> {
    match base.with_api_key(api_key) {
        Ok(client) => Some(client),
        Err(e) => {
            tracing::warn!(error = %e, "openrouter: failed to apply current API key to client");
            None
        }
    }
}

fn with_model_or_base(base: &OpenRouterClient, model: &str) -> OpenRouterClient {
    if model.is_empty() || base.model() == model {
        return base.clone();
    }
    base.with_model(model.to_owned()).unwrap_or_else(|e| {
        tracing::warn!(error = %e, model, "llm: could not switch the client to the requested model");
        base.clone()
    })
}

pub fn model_needs_openrouter_key(
    policy: &ProviderPolicy,
    tier: Option<ModelTier>,
    model: &str,
) -> bool {
    ModelRef::parse(&policy.resolve(tier, model))
        .provider
        .needs_openrouter_key()
}

pub fn cli_api_key(
    configured: Option<&SecretString>,
    policy: &ProviderPolicy,
    tier: Option<ModelTier>,
    model: &str,
) -> Result<String, LlmError> {
    cli_api_key_in(configured, policy, tier, model, &ProcessEnvironment)
}

pub fn cli_api_key_in(
    configured: Option<&SecretString>,
    policy: &ProviderPolicy,
    tier: Option<ModelTier>,
    model: &str,
    environment: &dyn Environment,
) -> Result<String, LlmError> {
    if let Some(key) = seed_from(configured, environment) {
        return Ok(key.expose_secret().to_owned());
    }
    if !model_needs_openrouter_key(policy, tier, model) {
        return Ok(String::new());
    }
    Err(missing_key_error(model, tier.is_some(), policy.provider()))
}

pub fn keyed_client(
    base: &OpenRouterClient,
    slot: &OpenRouterKeySlot,
) -> Option<OpenRouterClient> {
    with_key(base, api_key_text(slot))
}

pub fn effective_client(
    base: &OpenRouterClient,
    slot: &OpenRouterKeySlot,
) -> Option<OpenRouterClient> {
    let api_key = api_key_text(slot);
    if api_key.is_empty() && base.target().provider.needs_openrouter_key() {
        return None;
    }
    with_key(base, api_key)
}

pub fn effective_client_for(
    base: &OpenRouterClient,
    slot: &OpenRouterKeySlot,
    model: &str,
) -> Option<OpenRouterClient> {
    effective_client(&with_model_or_base(base, model), slot)
}

pub fn effective_client_for_any<'a>(
    base: &OpenRouterClient,
    slot: &OpenRouterKeySlot,
    models: impl IntoIterator<Item = &'a str>,
) -> Option<OpenRouterClient> {
    let keyed = keyed_client(base, slot)?;
    let usable = keyed.has_api_key()
        || std::iter::once("").chain(models).any(|model| {
            !with_model_or_base(base, model)
                .target()
                .provider
                .needs_openrouter_key()
        });
    usable.then_some(keyed)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::environment::MapEnvironment;

    fn slot_with(key: Option<&str>) -> OpenRouterKeySlot {
        Arc::new(ArcSwap::from_pointee(
            key.map(|k| SecretString::new(k.to_owned().into())),
        ))
    }

    fn no_keys() -> MapEnvironment {
        MapEnvironment::default()
    }

    #[test]
    fn seed_from_config_field_beats_env_vars() {
        let environment = no_keys().with("MIKE_STEERER_OPENROUTER_API_KEY", "from-env");

        let config_key = SecretString::new("from-config".to_owned().into());
        let seeded = seed_from(Some(&config_key), &environment);
        assert_eq!(
            seeded.map(|s| s.expose_secret().to_owned()),
            Some("from-config".to_owned())
        );
    }

    #[test]
    fn seed_from_primary_env_var_beats_the_secondary() {
        let environment = no_keys()
            .with("MIKE_STEERER_OPENROUTER_API_KEY", "from-primary")
            .with("OPENROUTER_API_KEY", "from-fallback-env");

        let seeded = seed_from(None, &environment);
        assert_eq!(
            seeded.map(|s| s.expose_secret().to_owned()),
            Some("from-primary".to_owned())
        );
    }

    #[test]
    fn seed_from_secondary_env_var_when_primary_is_unset() {
        let environment = no_keys().with("OPENROUTER_API_KEY", "from-fallback-env");

        let seeded = seed_from(None, &environment);
        assert_eq!(
            seeded.map(|s| s.expose_secret().to_owned()),
            Some("from-fallback-env".to_owned())
        );
    }

    #[test]
    fn seed_from_none_when_nothing_configured() {
        assert!(seed_from(None, &no_keys()).is_none());
    }

    #[test]
    fn seed_from_skips_an_empty_primary_and_uses_the_secondary() {
        let environment = no_keys()
            .with("MIKE_STEERER_OPENROUTER_API_KEY", "")
            .with("OPENROUTER_API_KEY", "from-fallback-env");

        let seeded = seed_from(None, &environment);
        assert_eq!(
            seeded.map(|s| s.expose_secret().to_owned()),
            Some("from-fallback-env".to_owned())
        );
    }

    #[test]
    fn seed_anthropic_from_reads_configured_var() {
        let environment = no_keys().with("MIKE_ASSISTANT_ANTHROPIC_API_KEY", "sk-ant-from-env");

        let seeded = seed_anthropic_from(&environment);
        assert_eq!(
            seeded.map(|s| s.expose_secret().to_owned()),
            Some("sk-ant-from-env".to_owned())
        );
    }

    #[test]
    fn seed_anthropic_from_none_when_unset() {
        assert!(seed_anthropic_from(&no_keys()).is_none());
    }

    #[test]
    fn seed_anthropic_from_none_when_empty() {
        let environment = no_keys().with("MIKE_ASSISTANT_ANTHROPIC_API_KEY", "");
        assert!(seed_anthropic_from(&environment).is_none());
    }

    #[test]
    fn resolve_reflects_a_swap() {
        let slot = slot_with(Some("old-key"));
        assert_eq!(
            resolve(&slot).map(|s| s.expose_secret().to_owned()),
            Some("old-key".to_owned())
        );

        slot.store(Arc::new(Some(SecretString::new(
            "new-key".to_owned().into(),
        ))));

        assert_eq!(
            resolve(&slot).map(|s| s.expose_secret().to_owned()),
            Some("new-key".to_owned())
        );
    }

    #[test]
    fn resolve_none_when_slot_empty() {
        let slot = slot_with(None);
        assert!(resolve(&slot).is_none());
    }

    #[test]
    fn effective_client_none_when_slot_empty() -> anyhow::Result<()> {
        let base =
            OpenRouterClient::new("boot-key".to_owned(), "test-model".to_owned(), None, None)?;
        let slot = slot_with(None);
        assert!(effective_client(&base, &slot).is_none());
        Ok(())
    }

    fn client_on(model: &str) -> anyhow::Result<OpenRouterClient> {
        Ok(OpenRouterClient::new(
            "boot-key".to_owned(),
            model.to_owned(),
            None,
            None,
        )?)
    }

    #[test]
    fn effective_client_without_a_key_serves_codex_and_claude_models() -> anyhow::Result<()> {
        let slot = slot_with(None);
        for model in ["codex/gpt-6-sol", "claude/sonnet"] {
            assert!(
                effective_client(&client_on(model)?, &slot).is_some(),
                "{model}"
            );
        }
        Ok(())
    }

    #[test]
    fn effective_client_without_a_key_refuses_every_openrouter_name() -> anyhow::Result<()> {
        let slot = slot_with(None);
        for model in [
            "google/gemini-3.8-flash",
            "openrouter/anthropic/claude-sonnet-5-5",
            "openrouter/openai/gpt-6-sol",
        ] {
            assert!(
                effective_client(&client_on(model)?, &slot).is_none(),
                "{model}"
            );
        }
        Ok(())
    }

    #[test]
    fn effective_client_with_an_empty_key_counts_as_no_key() -> anyhow::Result<()> {
        let slot = slot_with(Some(""));
        assert!(effective_client(&client_on("google/gemini-3.8-flash")?, &slot).is_none());
        assert!(effective_client(&client_on("codex/gpt-6-sol")?, &slot).is_some());
        Ok(())
    }

    #[test]
    fn effective_client_for_gates_on_the_model_switched_to() -> anyhow::Result<()> {
        let slot = slot_with(None);
        let base = client_on("google/gemini-3.8-flash")?;
        let switched = effective_client_for(&base, &slot, "codex/gpt-6-sol")
            .ok_or_else(|| anyhow::anyhow!("expected a client for the codex model"))?;
        assert_eq!(switched.model(), "codex/gpt-6-sol");
        assert!(effective_client_for(&base, &slot, "").is_none());
        assert!(
            effective_client_for(&client_on("codex/gpt-6-sol")?, &slot, "other/model").is_none()
        );
        Ok(())
    }

    #[test]
    fn effective_client_for_any_passes_when_one_candidate_needs_no_key() -> anyhow::Result<()> {
        let slot = slot_with(None);
        let base = client_on("google/gemini-3.8-flash")?;
        assert!(effective_client_for_any(&base, &slot, ["a/b", "claude/haiku"]).is_some());
        assert!(effective_client_for_any(&base, &slot, ["a/b", "c/d"]).is_none());
        assert!(effective_client_for_any(&base, &slot_with(Some("k")), ["a/b"]).is_some());
        Ok(())
    }

    #[test]
    fn cli_api_key_needs_no_key_for_codex_and_claude_models() {
        let policy = ProviderPolicy::default();
        for model in ["codex/gpt-6-sol", "claude/sonnet"] {
            assert_eq!(
                cli_api_key_in(None, &policy, None, model, &no_keys()).ok(),
                Some(String::new()),
                "{model}"
            );
        }
    }

    #[test]
    fn cli_api_key_refuses_an_openrouter_model_naming_the_model_and_the_provider() {
        let policy = ProviderPolicy::default();
        let message = cli_api_key_in(None, &policy, None, "google/gemini-3.8-flash", &no_keys())
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(message.contains("google/gemini-3.8-flash"), "{message}");
        assert!(message.contains("OpenRouter"), "{message}");
    }

    #[test]
    fn cli_api_key_names_llm_provider_for_a_tierless_job_under_a_cli_provider() {
        let mut policy = ProviderPolicy::default();
        assert!(policy.set_provider("claude"));
        let message = cli_api_key_in(None, &policy, None, "google/gemini-3.8-flash", &no_keys())
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(message.contains("llm.provider is claude"), "{message}");
    }

    #[test]
    fn cli_api_key_follows_the_provider_tier_model_not_the_roles_own_name() {
        let mut policy = ProviderPolicy::default();
        assert!(policy.set_provider("codex"));
        assert_eq!(
            cli_api_key_in(
                None,
                &policy,
                Some(ModelTier::Strong),
                "google/gemini-3.8-flash",
                &no_keys()
            )
            .ok(),
            Some(String::new())
        );
    }

    #[test]
    fn cli_api_key_uses_a_configured_key_whatever_the_model() {
        let configured = SecretString::new("sk-config".to_owned().into());
        assert_eq!(
            cli_api_key_in(
                Some(&configured),
                &ProviderPolicy::default(),
                None,
                "codex/gpt-6-sol",
                &no_keys()
            )
            .ok(),
            Some("sk-config".to_owned())
        );
    }

    #[test]
    fn cli_api_key_uses_an_environment_key_for_an_openrouter_model() {
        let environment = no_keys().with("OPENROUTER_API_KEY", "sk-env");
        assert_eq!(
            cli_api_key_in(
                None,
                &ProviderPolicy::default(),
                None,
                "google/gemini-3.8-flash",
                &environment
            )
            .ok(),
            Some("sk-env".to_owned())
        );
    }

    #[test]
    fn keyed_client_never_refuses_for_a_missing_key() -> anyhow::Result<()> {
        let keyless = keyed_client(&client_on("google/gemini-3.8-flash")?, &slot_with(None))
            .ok_or_else(|| anyhow::anyhow!("expected a client"))?;
        assert!(!keyless.has_api_key());
        Ok(())
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestVerdict {
        ok: bool,
    }

    #[tokio::test]
    async fn effective_client_sends_current_slot_key_not_boot_key() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer new-key",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": r#"{"ok":true}"#}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let base = OpenRouterClient::new(
            "boot-key".to_owned(),
            "test-model".to_owned(),
            Some(server.uri()),
            None,
        )?;
        let slot = slot_with(Some("new-key"));

        let client =
            effective_client(&base, &slot).ok_or_else(|| anyhow::anyhow!("expected Some"))?;
        let schema = json!({
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": false
        });
        let result: TestVerdict = client.verdict("sys", "user", schema).await?;
        assert!(result.ok);

        server.verify().await;
        Ok(())
    }
}
