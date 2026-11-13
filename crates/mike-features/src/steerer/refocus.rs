use crate::store::persistence::proxy_repo::ProxyPersist as _;
pub async fn resolve_verdict_system_prompt(
    persist: Option<&crate::store::persistence::PersistHandle>,
    agent_id: i64,
    steerer_prompt: Option<&str>,
) -> String {
    if let Some(prompt) = tier1_steerer_prompt(steerer_prompt) {
        return prompt;
    }

    let Some(ph) = persist else {
        return build_verdict_system_prompt();
    };

    resolve_composition_block2_text(ph, agent_id, "steerer fleet", build_verdict_system_prompt)
        .await
}

fn tier1_steerer_prompt(steerer_prompt: Option<&str>) -> Option<String> {
    let trimmed = steerer_prompt?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

pub async fn resolve_composition_block2_text(
    ph: &crate::store::persistence::PersistHandle,
    agent_id: i64,
    subsystem: &str,
    default: impl Fn() -> String,
) -> String {
    match ph.proxy().list_compositions_for_agent(agent_id).await {
        Ok(compositions) => {
            if compositions.is_empty() {
                log_no_compositions(agent_id, subsystem);
            } else if block2_text(&compositions[0]).is_none() {
                log_block2_text_absent(agent_id, subsystem);
            }
            block2_or_default(&compositions, default).0
        }
        Err(e) => {
            log_composition_fetch_error(agent_id, subsystem, &e);
            default()
        }
    }
}

pub fn block2_or_default(
    compositions: &[crate::wire::composition::Composition],
    default: impl Fn() -> String,
) -> (String, Option<(i64, i64)>) {
    match compositions.first() {
        Some(first) => match block2_text(first) {
            Some(text) => (text, Some((first.id, first.version))),
            None => (default(), None),
        },
        None => (default(), None),
    }
}

fn block2_text(composition: &crate::wire::composition::Composition) -> Option<String> {
    let text = composition
        .blocks
        .get(2)
        .and_then(|b| b.get("text"))
        .and_then(|v| v.as_str())?;
    (!text.is_empty()).then(|| text.to_owned())
}

fn log_block2_text_absent(agent_id: i64, subsystem: &str) {
    tracing::debug!(
        agent_id,
        "{subsystem}: composition exists but blocks[2].text is absent — using static default"
    );
}

fn log_no_compositions(agent_id: i64, subsystem: &str) {
    tracing::debug!(
        agent_id,
        "{subsystem}: no compositions for agent — using static default"
    );
}

fn log_composition_fetch_error(
    agent_id: i64,
    subsystem: &str,
    e: &crate::store::persistence::PersistError,
) {
    tracing::warn!(
        agent_id,
        error = %e,
        "{subsystem}: composition fetch error — using static default"
    );
}

pub fn build_verdict_system_prompt() -> String {
    "You are a session grounding assistant. Your job is to assess whether a Claude Code session \
     has drifted from its current work thread, anchored on the thread's latest checkpoint \
     (where it stands and what is next up). \
     \n\nAssess the session's last turn against the anchor. Return a JSON verdict:\n\
     - drifting: true if the session is working on something not in Next steps\n\
     - confidence: 0.0–1.0 confidence in your drift assessment\n\
     - off_thread_topic: brief description of what the session drifted into (empty if not drifting)\n\
     - command: one of 'restate-thread' / 'redirect-attention' / 'resume-Next' (empty if not drifting)\n\
     \nBe conservative: only flag genuine off-thread work, not natural exploration within scope."
        .to_string()
}

pub fn build_dry_run_user_message(constraint: &str, last_user_message: &str) -> String {
    format!(
        "Behavioral constraint being tested:\n{constraint}\n\n\
         Turn to evaluate:\n{last_user_message}"
    )
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn verdict_system_prompt_is_non_empty() {
        let prompt = build_verdict_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("drift"));
    }

    #[test]
    fn verdict_system_prompt_names_the_work_thread_checkpoint_not_a_worklog() {
        let prompt = build_verdict_system_prompt();
        assert!(prompt.contains("work thread"));
        assert!(prompt.contains("checkpoint"));
        assert!(!prompt.to_lowercase().contains("worklog"));
    }

    #[tokio::test]
    async fn resolve_prefers_steerer_prompt_over_static_default() {
        let result = resolve_verdict_system_prompt(None, 99, Some("Custom authored prompt.")).await;
        assert_eq!(result, "Custom authored prompt.");
    }

    #[tokio::test]
    async fn resolve_falls_back_when_steerer_prompt_empty() {
        let result = resolve_verdict_system_prompt(None, 99, Some("   ")).await;
        assert_eq!(result, build_verdict_system_prompt());
    }

    #[tokio::test]
    async fn resolve_falls_back_when_steerer_prompt_none() {
        let result = resolve_verdict_system_prompt(None, 99, None).await;
        assert_eq!(result, build_verdict_system_prompt());
    }

    #[test]
    fn dry_run_user_message_contains_constraint_and_turn() {
        let msg = build_dry_run_user_message(
            "The session MUST NOT claim success without test evidence.",
            "ok ship it, tests pass",
        );
        assert!(msg.contains("Behavioral constraint being tested:"));
        assert!(msg.contains("The session MUST NOT claim success"));
        assert!(msg.contains("ok ship it, tests pass"));
    }
}
