use bytes::Bytes;

use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::reminder_on_token::Outcome;
use crate::wire::manifest::AppliedStage;

pub(crate) struct ReminderOnTokenStage;

#[async_trait::async_trait]
impl RequestStage for ReminderOnTokenStage {
    fn name(&self) -> &'static str {
        "reminder_on_token"
    }

    fn enabled(&self, _ctx: &StageContext<'_>) -> bool {
        true
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let reminder_on_token_enabled = ctx.state.feature_toggle_overlay.load().reminder_on_token;
        let outcome = crate::reminder_on_token::apply(body.body(), reminder_on_token_enabled).await;
        let (new_bytes, reason_code, detail) = match outcome {
            Outcome::Unchanged => {
                body.decline(
                    "reminder_on_token",
                    "reminder_on_token_no_token",
                    serde_json::json!({ "resolve_enabled": reminder_on_token_enabled }),
                );
                return;
            }
            Outcome::Resolved { new_body } => (
                Bytes::from(new_body),
                "reminder_on_token_resolved",
                "hook token resolved to <system-reminder>",
            ),
            Outcome::Stripped { new_body } => (
                Bytes::from(new_body),
                "reminder_on_token_stripped",
                "hook token stripped (miss/timeout/error/toggle off)",
            ),
        };

        tracing::info!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            injected = reminder_on_token_enabled,
            "reminder_on_token stripped hook token from request body"
        );
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "reminder_on_token",
                policy_id: "reminder_on_token",
                policy_version: 1,
                resolved_parameters: serde_json::json!({
                    "resolve_enabled": reminder_on_token_enabled
                }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code,
                detail: Some(detail.to_string()),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::extract::request::TurnOrigin;
    use crate::feature_toggles::ResolvedToggles;
    use crate::filter::FilterConfig;
    use crate::lesson_worker::LessonSnapshot;
    use crate::store::models::RequestId;

    fn hook_block_text(inner_token: &str) -> String {
        format!(
            "<system-reminder>\nPostToolUse:Bash hook additional context: {}{inner_token}{}\n</system-reminder>",
            '\u{E000}', '\u{E001}'
        )
    }

    fn ctx_with_state<'a>(
        state: &'a crate::config::AppState,
        session_toggles: &'a ResolvedToggles,
        filter_cfg: &'a FilterConfig,
        lesson_snapshot: &'a LessonSnapshot,
        turn_origin: &'a TurnOrigin,
    ) -> StageContext<'a> {
        StageContext {
            state,
            session_toggles,
            filter_cfg,
            lesson_snapshot,
            session_id: "session-reminder-on-token-stage-test",
            project_path: "",
            turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: false,
        }
    }

    #[test]
    fn reminder_on_token_stage_is_always_enabled() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let state = crate::config::AppState::new(config).await?;
            let session_toggles =
                state.resolve_session_toggles("session-reminder-on-token-stage-test");
            let filter_cfg = FilterConfig::default();
            let lesson_snapshot = LessonSnapshot::default();
            let turn_origin = TurnOrigin::Primary;
            let ctx = ctx_with_state(
                &state,
                &session_toggles,
                &filter_cfg,
                &lesson_snapshot,
                &turn_origin,
            );
            assert!(ReminderOnTokenStage.enabled(&ctx));
            anyhow::Ok(())
        })
    }

    #[tokio::test]
    async fn apply_leaves_body_untouched_when_no_token_present() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::config::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-reminder-on-token-stage-test");
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = ctx_with_state(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let raw = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"plain"}]}]}"#;
        let mut body = StageBody::new(Bytes::from_static(raw), "orig".to_string());
        ReminderOnTokenStage.apply(&ctx, &mut body).await;

        assert!(!body.filter_applied());
        assert_eq!(body.body().as_ref(), raw);
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].stage, "reminder_on_token");
        assert_eq!(
            manifest.stages[0].outcome,
            crate::wire::manifest::StageOutcome::Unchanged
        );
        assert_eq!(manifest.stages[0].reason_code, "reminder_on_token_no_token");
        Ok(())
    }

    #[tokio::test]
    async fn apply_strips_malformed_token_and_records_manifest() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::config::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-reminder-on-token-stage-test");
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = ctx_with_state(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let raw = serde_json::to_vec(&serde_json::json!({
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": hook_block_text("!!!not-base64!!!")}
            ]}]
        }))?;
        let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
        ReminderOnTokenStage.apply(&ctx, &mut body).await;

        assert!(body.filter_applied());
        assert!(
            !String::from_utf8_lossy(body.body()).contains('\u{E000}'),
            "PUA token must be stripped from the forwarded body"
        );
        let parts = body.into_parts();
        let manifest = parts.manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].stage, "reminder_on_token");
        Ok(())
    }
}
