use super::Check;
use super::observed::{
    CodexFacts, FeatureFacts, JobFacts, PromptAssistantFacts, WorkerProviderFacts,
};

fn warn_with_fix(id: &'static str, name: &'static str, detail: String, fix: &str) -> Check {
    let mut check = Check::warn(id, name, detail, None);
    check.fix = Some(fix.to_owned());
    check
}

pub(super) fn features(facts: &FeatureFacts) -> Vec<Check> {
    vec![
        scheduled_jobs(&facts.jobs),
        worker_provider(&facts.worker_provider),
        codex_hooks(&facts.codex),
        prompt_assistant(&facts.prompt_assistant),
    ]
}

fn scheduled_jobs(facts: &JobFacts) -> Check {
    let total = facts.runnable + facts.missing.len() + facts.needs_checkout.len();
    let checkout = if facts.needs_checkout.is_empty() {
        String::new()
    } else {
        format!(
            "; {} need a mike source checkout and are not scheduled here ({})",
            facts.needs_checkout.len(),
            facts.needs_checkout.join(", ")
        )
    };
    if facts.missing.is_empty() {
        return Check::ok(
            "scheduled_jobs",
            "scheduled jobs",
            format!("{} of {total} can run here{checkout}", facts.runnable),
        );
    }
    warn_with_fix(
        "scheduled_jobs",
        "scheduled jobs",
        format!(
            "{} of {total} can run here; their scripts are missing for {}{checkout}",
            facts.runnable,
            facts.missing.join(", ")
        ),
        "install mike from a release archive that has share/mike/scripts beside bin, or start mike from a source checkout",
    )
}

fn worker_provider(facts: &WorkerProviderFacts) -> Check {
    match (&facts.claude_on_service_path, &facts.openrouter_key_variable) {
        (Some(path), _) => Check::ok(
            "worker_provider",
            "background worker model",
            format!("claude is at {} on the PATH the service uses", path.display()),
        ),
        (None, Some(variable)) => Check::ok(
            "worker_provider",
            "background worker model",
            format!("OpenRouter key in {variable}; claude is not on the PATH the service uses"),
        ),
        (None, None) => warn_with_fix(
            "worker_provider",
            "background worker model",
            "background workers have no model: claude is not on the PATH the service uses and no OpenRouter key is set"
                .to_owned(),
            "put claude in ~/.local/bin and run `mike service install` again, or set MIKE_STEERER_OPENROUTER_API_KEY",
        ),
    }
}

fn codex_hooks(facts: &CodexFacts) -> Check {
    if !facts.on_path {
        return Check::ok(
            "codex_hooks",
            "codex hooks",
            "codex is not on PATH, so there is nothing to hook",
        );
    }
    let home = facts
        .home
        .as_ref()
        .map_or_else(|| "~/.codex".to_owned(), |home| home.display().to_string());
    if facts.hooks_installed {
        return Check::ok(
            "codex_hooks",
            "codex hooks",
            format!("mike's hooks are installed in {home}"),
        );
    }
    warn_with_fix(
        "codex_hooks",
        "codex hooks",
        format!(
            "codex is on PATH but mike's hooks are not installed in {home}, so ordinary Codex sessions are not observed"
        ),
        "share/mike/scripts/codex-hooks/install --codex-home ~/.codex (under the release install's prefix; scripts/codex-hooks/install in a checkout), then review the hooks in Codex with /hooks",
    )
}

fn prompt_assistant(facts: &PromptAssistantFacts) -> Check {
    if !facts.enabled {
        return Check::ok(
            "prompt_assistant",
            "prompt assistant",
            "switched off by the prompt_assistant toggle",
        );
    }
    if facts.answering {
        return Check::ok(
            "prompt_assistant",
            "prompt assistant",
            format!("the service answers on {}", facts.addr),
        );
    }
    warn_with_fix(
        "prompt_assistant",
        "prompt assistant",
        format!(
            "the prompt_assistant toggle is on but nothing answers on {}",
            facts.addr
        ),
        "start the prompt assistant service (scripts/start-proxy.sh in a mike checkout), or turn the prompt_assistant toggle off in /settings",
    )
}
