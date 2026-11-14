use super::*;

fn warning(observed: &Observed, id: &str) -> anyhow::Result<Check> {
    let checks = evaluate(observed);
    let found = check(&checks, id)?.clone();
    assert_eq!(found.status, Status::Warn, "{}", found.detail);
    assert!(found.fix.is_some(), "{id} warns without a fix");
    Ok(found)
}

#[test]
fn scheduled_jobs_pass_and_name_the_checkout_only_jobs_that_stand_aside() -> anyhow::Result<()> {
    let checks = evaluate(&healthy());
    let jobs = check(&checks, "scheduled_jobs")?;
    assert_eq!(jobs.status, Status::Ok);
    assert!(jobs.detail.contains("4 of 5"), "{}", jobs.detail);
    assert!(jobs.detail.contains("lessons_commit"), "{}", jobs.detail);
    Ok(())
}

#[test]
fn scheduled_jobs_warn_with_a_fix_when_a_shipped_script_is_missing() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.features.jobs.runnable = 0;
    observed.features.jobs.missing = vec!["learning_promote", "compressor_loop"];
    let jobs = warning(&observed, "scheduled_jobs")?;
    assert!(jobs.detail.contains("learning_promote"), "{}", jobs.detail);
    assert!(
        jobs.fix
            .is_some_and(|fix| fix.contains("share/mike/scripts")),
        "the fix names the package folder"
    );
    Ok(())
}

#[test]
fn worker_provider_warns_when_neither_claude_nor_a_key_is_available() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.features.worker_provider.claude_on_service_path = None;
    warning(&observed, "worker_provider")?;
    observed.features.worker_provider.openrouter_key_variable =
        Some("OPENROUTER_API_KEY".to_string());
    let checks = evaluate(&observed);
    assert_eq!(check(&checks, "worker_provider")?.status, Status::Ok);
    Ok(())
}

#[test]
fn codex_hooks_warn_only_when_codex_is_on_path_without_them() -> anyhow::Result<()> {
    let mut observed = healthy();
    let checks = evaluate(&observed);
    assert_eq!(check(&checks, "codex_hooks")?.status, Status::Ok);
    observed.features.codex.on_path = true;
    let hooks = warning(&observed, "codex_hooks")?;
    assert!(
        hooks.detail.contains("/home/ada/.codex"),
        "{}",
        hooks.detail
    );
    observed.features.codex.hooks_installed = true;
    let checks = evaluate(&observed);
    assert_eq!(check(&checks, "codex_hooks")?.status, Status::Ok);
    Ok(())
}

#[test]
fn prompt_assistant_warns_when_the_toggle_is_on_and_the_service_is_silent() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.features.prompt_assistant.answering = false;
    let assistant = warning(&observed, "prompt_assistant")?;
    assert!(
        assistant.detail.contains("127.0.0.1:9985"),
        "{}",
        assistant.detail
    );
    observed.features.prompt_assistant.enabled = false;
    let checks = evaluate(&observed);
    assert_eq!(check(&checks, "prompt_assistant")?.status, Status::Ok);
    Ok(())
}
