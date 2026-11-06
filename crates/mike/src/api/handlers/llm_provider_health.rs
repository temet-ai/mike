use super::llm_job_models::{enabled_job_models, jobs_on};
use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::llm::codex::{self, CodexLogin};
use crate::llm::model_ref::Provider;
use crate::llm::provider::LlmProvider;

const KEY_FIX: &str = "Set MIKE_STEERER_OPENROUTER_API_KEY (or OPENROUTER_API_KEY) for the proxy, or add the key at /settings/integrations. To run without OpenRouter, set llm.provider to claude or codex at /settings, or give the job a claude/ or codex/ model.";
const CLAUDE_MISSING_FIX: &str = "Install Claude Code on the machine that runs the proxy, or set MIKE_CLAUDE_CLI_BIN to its path, then run `claude auth status` there. No other provider is used in its place.";
const CLAUDE_SIGNED_OUT_FIX: &str = "Run `claude auth login` on the machine that runs the proxy, then check that `claude auth status` reports loggedIn true. No other provider is used in its place.";
const CODEX_MISSING_FIX: &str = "Install the Codex CLI on the machine that runs the proxy (the proxy's PATH must reach `codex`), then run `codex login status` there. No other provider is used in its place.";
const CODEX_SIGNED_OUT_FIX: &str = "Run `codex login` on the machine that runs the proxy, then check that `codex login status` reports a login. No other provider is used in its place.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderProblem {
    pub(crate) detail: String,
    pub(crate) fix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderStatus {
    pub(crate) provider: LlmProvider,
    pub(crate) problems: Vec<ProviderProblem>,
}

impl ProviderStatus {
    #[cfg(test)]
    pub(crate) fn per_role() -> Self {
        Self {
            provider: LlmProvider::PerRole,
            problems: Vec::new(),
        }
    }

    pub(crate) fn heading(&self) -> String {
        format!("LLM provider: {}.", self.provider.label())
    }
}

fn problem(detail: impl Into<String>, fix: &str) -> Option<ProviderProblem> {
    Some(ProviderProblem {
        detail: detail.into(),
        fix: fix.to_owned(),
    })
}

fn openrouter_problem(state: &AppState) -> Option<ProviderProblem> {
    use secrecy::ExposeSecret;
    let configured = state
        .openrouter_key()
        .is_some_and(|key| !key.expose_secret().is_empty());
    if configured {
        return None;
    }
    problem("no OpenRouter API key is configured", KEY_FIX)
}

async fn claude_problem(state: &AppState) -> Option<ProviderProblem> {
    let status = state
        .anthropic_cli
        .status_cache
        .get_or_refresh(&state.anthropic_cli.cli)
        .await;
    if !status.installed {
        return problem(
            "the claude CLI is not installed or not on the proxy's PATH",
            CLAUDE_MISSING_FIX,
        );
    }
    if status.logged_in {
        return None;
    }
    let reason = status
        .error
        .unwrap_or_else(|| "`claude auth status` reports loggedIn false".to_owned());
    problem(
        format!("the machine's Claude Code login is missing: {reason}"),
        CLAUDE_SIGNED_OUT_FIX,
    )
}

async fn codex_problem() -> Option<ProviderProblem> {
    match codex::login_status(codex::DEFAULT_BIN).await {
        CodexLogin::SignedIn(_) => None,
        CodexLogin::NotInstalled => problem(
            "the codex CLI is not installed or not on the proxy's PATH",
            CODEX_MISSING_FIX,
        ),
        CodexLogin::SignedOut(said) => problem(
            format!("the machine's Codex login is missing: {said}"),
            CODEX_SIGNED_OUT_FIX,
        ),
    }
}

pub(crate) async fn provider_problem(
    state: &AppState,
    provider: Provider,
) -> Option<ProviderProblem> {
    match provider {
        Provider::OpenRouter => openrouter_problem(state),
        Provider::Claude => claude_problem(state).await,
        Provider::Codex => codex_problem().await,
    }
}

pub(crate) async fn provider_status(state: &AppState, toggles: &FeatureToggles) -> ProviderStatus {
    let provider = state.settings_overlay.load().llm_provider.provider();
    let models = enabled_job_models(state, toggles).await;
    let mut problems = Vec::new();
    for used in Provider::ALL {
        let jobs = jobs_on(&models, used);
        if jobs.is_empty() {
            continue;
        }
        if let Some(found) = provider_problem(state, used).await {
            problems.push(ProviderProblem {
                detail: format!(
                    "LLM jobs on {} cannot run ({}): {}",
                    used.display_name(),
                    jobs.join(", "),
                    found.detail
                ),
                fix: found.fix,
            });
        }
    }
    ProviderStatus { provider, problems }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_role_status_names_the_per_role_mode_and_has_no_problem() {
        let status = ProviderStatus::per_role();
        assert!(
            status.heading().contains("per role"),
            "{}",
            status.heading()
        );
        assert!(status.problems.is_empty());
    }

    #[test]
    fn heading_names_the_provider() {
        let status = ProviderStatus {
            provider: LlmProvider::Claude,
            problems: Vec::new(),
        };
        assert!(status.heading().contains("Claude"), "{}", status.heading());
    }

    #[test]
    fn fix_lines_name_the_status_command_for_each_cli_provider() {
        assert!(CLAUDE_SIGNED_OUT_FIX.contains("claude auth status"));
        assert!(CLAUDE_MISSING_FIX.contains("claude auth status"));
        assert!(CODEX_SIGNED_OUT_FIX.contains("codex login status"));
        assert!(CODEX_MISSING_FIX.contains("codex login status"));
    }

    #[test]
    fn every_fix_line_says_no_other_provider_takes_over_for_cli_providers() {
        for fix in [
            CLAUDE_MISSING_FIX,
            CLAUDE_SIGNED_OUT_FIX,
            CODEX_MISSING_FIX,
            CODEX_SIGNED_OUT_FIX,
        ] {
            assert!(
                fix.contains("No other provider is used in its place"),
                "{fix}"
            );
        }
    }
}
