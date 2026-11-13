use super::refocus::resolve_composition_block2_text;

pub async fn resolve_steer_author_system_prompt(
    persist: Option<&crate::store::persistence::PersistHandle>,
) -> String {
    let Some(ph) = persist else {
        return build_steer_author_system_prompt_default();
    };

    let Some(agent_id) = resolve_steer_author_agent_id(ph).await else {
        return build_steer_author_system_prompt_default();
    };

    resolve_composition_block2_text(
        ph,
        agent_id,
        "steer-author",
        build_steer_author_system_prompt_default,
    )
    .await
}

async fn resolve_steer_author_agent_id(
    ph: &crate::store::persistence::PersistHandle,
) -> Option<i64> {
    match ph.fetch_agent_by_name("steer-author").await {
        Ok(Some(agent)) => Some(agent.id),
        Ok(None) => {
            log_steer_author_agent_missing();
            None
        }
        Err(e) => {
            log_steer_author_agent_lookup_failed(&e);
            None
        }
    }
}

fn log_steer_author_agent_missing() {
    tracing::debug!("steer-author: no agent row found — using static default");
}

fn log_steer_author_agent_lookup_failed(e: &crate::store::persistence::PersistError) {
    tracing::warn!(
        error = %e,
        "steer-author: agent lookup failed — using static default"
    );
}

pub fn build_steer_author_system_prompt_default() -> String {
    "You are a steer-author: a prompt-smith that writes steering anchors \
        for a runtime session steerer. A runtime steerer watches a live Claude Code session \
        and injects a correction when a turn violates its anchor. \
        You do NOT talk to the session and you do NOT continue its conversation. \
        \n\nYour ONLY output is an effective steering anchor: a directive, in the imperative, \
        stating what the session must be held to and what behaviors it must not exhibit. \
        Write it as a rule the steerer evaluates a turn AGAINST — never as a reply TO a turn, \
        and never as a description of what the session is doing. \
        \n\nThe anchor must forbid the failure behaviors this steer targets: \
        do not dismiss work as pre-existing or out-of-scope; do not shift ownership of a failure; \
        do not claim success without a working, verified deliverable; finish what was actually requested. \
        \n\nGood anchor examples: \
        'Hold the session to the originally requested deliverable. The session MUST NOT dismiss \
        unresolved errors as pre-existing or out of scope; every error raised must be addressed \
        before any completion claim.' \
        'The session MUST NOT claim a deliverable works unless tests were run and their output \
        appears in the same turn. Premature success claims are a violation.' \
        \n\nBad output examples (NEVER produce these — they are conversation, not anchors): \
        'The session is working on implementing the steerer fleet.' \
        'You are right that a RIGHT JOIN would be better here.' \
        \n\nWhen diagnostic (weasel) findings are provided, the anchor MUST name each detected \
        behavior explicitly so the steerer recognizes the recurrence. \
        \n\nReturn a JSON object with fields: suggestion (prompt, authority, mode, config) and \
        rationale (one sentence per field explaining the recommendation). \
        suggestion.prompt is the steering anchor the runtime steerer will enforce. \
        authority must be one of: supply_context, redirect, command. \
        mode must be one of: augment, drive. \
        config.confidence_threshold should be 0.7-0.95 for behavioral violations (they are specific). \
        config.debounce_turns should be 1-3 (re-check frequently when behavior violations are detected)."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steer_author_default_is_non_empty() {
        let prompt = build_steer_author_system_prompt_default();
        assert!(!prompt.is_empty());
    }

    #[test]
    fn steer_author_default_contains_key_directives() {
        let prompt = build_steer_author_system_prompt_default();
        assert!(
            prompt.contains("steer-author"),
            "prompt should identify the steer-author role"
        );
        assert!(
            prompt.contains("imperative"),
            "prompt should mention imperative form"
        );
        assert!(
            prompt.contains("suggestion"),
            "prompt should describe the suggestion output field"
        );
        assert!(
            prompt.contains("authority"),
            "prompt should describe the authority field"
        );
    }

    #[tokio::test]
    async fn resolve_steer_author_falls_back_when_persist_none() {
        let result = resolve_steer_author_system_prompt(None).await;
        assert_eq!(result, build_steer_author_system_prompt_default());
    }

    #[tokio::test]
    async fn resolve_steer_author_fallback_is_non_empty() {
        let result = resolve_steer_author_system_prompt(None).await;
        assert!(!result.is_empty());
    }
}
