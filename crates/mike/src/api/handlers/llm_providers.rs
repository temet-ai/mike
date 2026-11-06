use super::*;
use crate::llm::codex::{self, CodexLogin};
use crate::llm::model_ref::{ModelRef, Provider};

const KEY_ENV_SETTING: &str = "steerer.openrouter_api_key_env";
const DEFAULT_KEY_ENV: &str = "MIKE_STEERER_OPENROUTER_API_KEY";
const CLAUDE_ALIASES: [&str; 3] = ["haiku", "sonnet", "opus"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProviderModel {
    pub(crate) id: String,
    #[serde(rename = "ref")]
    pub(crate) model_ref: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) structured_output: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) known_good: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProviderEntry {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) prefix: &'static str,
    pub(crate) available: bool,
    pub(crate) detail: String,
    pub(crate) fix: String,
    pub(crate) key_env_var: Option<String>,
    pub(crate) models_authoritative: bool,
    pub(crate) models: Vec<ProviderModel>,
    pub(crate) models_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProvidersResponse {
    pub(crate) llm_provider: &'static str,
    pub(crate) providers: Vec<ProviderEntry>,
}

fn plain_model(provider: Provider, name: &str) -> ProviderModel {
    ProviderModel {
        id: name.to_owned(),
        model_ref: ModelRef::new(provider, name).format(),
        label: name.to_owned(),
        structured_output: None,
        known_good: None,
    }
}

pub(crate) fn cli_model_options(
    provider: Provider,
    seeds: &[&str],
    configured: &[ModelRef],
) -> Vec<ProviderModel> {
    let mut names: Vec<String> = seeds.iter().map(|s| (*s).to_owned()).collect();
    for model_ref in configured.iter().filter(|m| m.provider == provider) {
        if !names.contains(&model_ref.model) {
            names.push(model_ref.model.clone());
        }
    }
    names
        .iter()
        .map(|name| plain_model(provider, name))
        .collect()
}

fn tier_names(state: &AppState, provider: Provider) -> Vec<String> {
    let overlay = state.settings_overlay.load();
    overlay
        .llm_provider
        .tier_models(provider)
        .into_iter()
        .map(ModelRef::parse)
        .filter(|m| m.provider == provider)
        .map(|m| m.model)
        .collect()
}

async fn key_env_var(state: &AppState) -> String {
    let Some(persist) = state.persist() else {
        return DEFAULT_KEY_ENV.to_owned();
    };
    persist
        .fetch_settings()
        .await
        .ok()
        .and_then(|rows| {
            rows.into_iter()
                .find(|(key, _)| key == KEY_ENV_SETTING)
                .and_then(|(_, value)| value.as_str().map(str::to_owned))
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_KEY_ENV.to_owned())
}

async fn available_detail(state: &AppState, provider: Provider) -> String {
    match provider {
        Provider::OpenRouter => format!(
            "An OpenRouter API key is configured; it is read from the environment variable {}",
            key_env_var(state).await
        ),
        Provider::Claude => {
            let status = state
                .anthropic_cli
                .status_cache
                .get_or_refresh(&state.anthropic_cli.cli)
                .await;
            let version = status
                .version
                .unwrap_or_else(|| "unknown version".to_owned());
            let method = status
                .auth_method
                .map_or_else(String::new, |m| format!(", signed in with {m}"));
            format!(
                "claude ({version}) at {} is logged in{method}",
                status.binary
            )
        }
        Provider::Codex => match codex::login_status(codex::DEFAULT_BIN).await {
            CodexLogin::SignedIn(said) if !said.is_empty() => said,
            _ => "codex is installed and logged in".to_owned(),
        },
    }
}

async fn openrouter_models(state: &AppState) -> (Vec<ProviderModel>, Option<String>) {
    match state.model_catalogue().get().await {
        Ok(entries) => (
            entries
                .into_iter()
                .map(|entry| ProviderModel {
                    model_ref: ModelRef::new(Provider::OpenRouter, entry.id.clone()).format(),
                    id: entry.id,
                    label: entry.name,
                    structured_output: Some(entry.supports_structured_output),
                    known_good: Some(entry.known_good),
                })
                .collect(),
            None,
        ),
        Err(e) => {
            tracing::warn!(error = %e, "llm providers: OpenRouter model catalogue fetch failed");
            (Vec::new(), Some(e.to_string()))
        }
    }
}

async fn models_for(
    state: &AppState,
    provider: Provider,
    configured: &[ModelRef],
) -> (Vec<ProviderModel>, Option<String>) {
    match provider {
        Provider::OpenRouter => openrouter_models(state).await,
        Provider::Claude => {
            let tiers = tier_names(state, provider);
            let seeds: Vec<&str> = CLAUDE_ALIASES
                .into_iter()
                .chain(tiers.iter().map(String::as_str))
                .collect();
            (cli_model_options(provider, &seeds, configured), None)
        }
        Provider::Codex => {
            let tiers = tier_names(state, provider);
            let seeds: Vec<&str> = tiers.iter().map(String::as_str).collect();
            (cli_model_options(provider, &seeds, configured), None)
        }
    }
}

async fn provider_entry(
    state: &AppState,
    provider: Provider,
    configured: &[ModelRef],
) -> ProviderEntry {
    let problem = provider_problem(state, provider).await;
    let (detail, fix) = match &problem {
        Some(found) => (found.detail.clone(), found.fix.clone()),
        None => (available_detail(state, provider).await, String::new()),
    };
    let key_env_var = match provider {
        Provider::OpenRouter => Some(key_env_var(state).await),
        Provider::Claude | Provider::Codex => None,
    };
    let (models, models_error) = models_for(state, provider, configured).await;
    ProviderEntry {
        id: provider.as_str(),
        label: provider.display_name(),
        prefix: provider.prefix(),
        available: problem.is_none(),
        detail,
        fix,
        key_env_var,
        models_authoritative: provider == Provider::OpenRouter,
        models,
        models_error,
    }
}

pub(crate) async fn get_llm_providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    let configured = configured_model_refs(&state);
    let mut providers = Vec::with_capacity(Provider::ALL.len());
    for provider in Provider::ALL {
        providers.push(provider_entry(&state, provider, &configured).await);
    }
    Json(ProvidersResponse {
        llm_provider: state
            .settings_overlay
            .load()
            .llm_provider
            .provider()
            .as_str(),
        providers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_model_options_lists_seeds_then_configured_models_of_that_provider_once() {
        let configured = [
            ModelRef::parse("claude/sonnet"),
            ModelRef::parse("claude/claude-sonnet-5-5"),
            ModelRef::parse("codex/gpt-6-sol"),
            ModelRef::parse("openrouter/anthropic/claude-sonnet-5-5"),
        ];
        let options = cli_model_options(Provider::Claude, &CLAUDE_ALIASES, &configured);
        let refs: Vec<&str> = options.iter().map(|m| m.model_ref.as_str()).collect();
        assert_eq!(
            refs,
            vec![
                "claude/haiku",
                "claude/sonnet",
                "claude/opus",
                "claude/claude-sonnet-5-5"
            ]
        );
    }

    #[test]
    fn cli_model_options_for_codex_never_carries_other_providers_models() {
        let configured = [
            ModelRef::parse("claude/opus"),
            ModelRef::parse("codex/gpt-6-luna"),
        ];
        let options = cli_model_options(Provider::Codex, &["gpt-6-sol"], &configured);
        let ids: Vec<&str> = options.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["gpt-6-sol", "gpt-6-luna"]);
    }

    #[test]
    fn provider_model_serialises_its_ref_under_the_ref_key_and_omits_unknown_flags()
    -> anyhow::Result<()> {
        let value = serde_json::to_value(plain_model(Provider::Codex, "gpt-6-sol"))?;
        assert_eq!(
            value,
            serde_json::json!({"id": "gpt-6-sol", "ref": "codex/gpt-6-sol", "label": "gpt-6-sol"})
        );
        Ok(())
    }
}
