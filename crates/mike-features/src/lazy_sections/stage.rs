use crate::lazy_sections::{self, LazySectionsOutcome};
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::wire::manifest::AppliedStage;

pub struct LazySectionsStage;

#[async_trait::async_trait]
impl RequestStage for LazySectionsStage {
    fn name(&self) -> &'static str {
        "lazy_sections"
    }

    fn enabled(&self, _ctx: &StageContext<'_>) -> bool {
        true
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let cfg = ctx.filter_cfg;
        let outcome = lazy_sections::apply_lazy_sections(body.body(), cfg, ctx.project_path);
        let LazySectionsOutcome::Rewrote {
            new_body,
            original_len,
            rewritten_len,
            stripped_bytes,
            injected,
            firing_rule_ids: lazy_rule_ids,
        } = outcome
        else {
            let reason = if lazy_sections::has_active_rules(cfg, ctx.project_path) {
                "lazy_sections_no_match"
            } else {
                "lazy_sections_no_rules"
            };
            body.decline("lazy_sections", reason, serde_json::json!({}));
            return;
        };

        tracing::info!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            original_bytes = original_len,
            rewritten_bytes = rewritten_len,
            stripped_bytes,
            injected,
            rule_ids = ?lazy_rule_ids,
            "lazy sections rewrote body"
        );
        body.merge_rule_ids(lazy_rule_ids.iter().copied());
        body.rewrite(
            new_body.clone(),
            AppliedStage {
                stage: "lazy_sections",
                policy_id: "lazy_sections",
                policy_version: 1,
                resolved_parameters: serde_json::json!({
                    "rule_ids": lazy_rule_ids,
                    "injected": injected,
                }),
                new_bytes: new_body.as_ref(),
                targets: Vec::new(),
                reason_code: "lazy_section_rewrite",
                detail: Some(format!(
                    "stripped {stripped_bytes} bytes; re-injected={injected}"
                )),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde_json::json;

    use super::*;
    use crate::extract::request::TurnOrigin;
    use crate::filter::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};
    use crate::lesson_cache::LessonSnapshot;
    use crate::store::models::RequestId;

    const HEADING: &str = "# Claude in Chrome browser automation";
    const STUB: &str = "# Claude in Chrome browser automation\n\nOn-demand: output !!CHROME!! then call tabs_context_mcp.";
    const TOKEN: &str = "!!CHROME!!";

    fn lazy_rule(id: i64) -> FilterRule {
        FilterRule {
            id,
            uuid: uuid::Uuid::new_v4(),
            kind: FilterKind::LazySection,
            value: HEADING.to_string(),
            enabled: true,
            source: RuleSource::Ui,
            updated_at: chrono::Utc::now(),
            sort_order: 0,
            action: FilterAction::Replace,
            text: Some(STUB.to_string()),
            trigger_token: Some(TOKEN.to_string()),
            inject_text: None,
            project_path: None,
            style_override: false,
            title: None,
        }
    }

    fn make_ctx<'a>(
        state: &'a crate::state::AppState,
        session_toggles: &'a crate::feature_toggles::ResolvedToggles,
        filter_cfg: &'a FilterConfig,
        lesson_snapshot: &'a LessonSnapshot,
        turn_origin: &'a TurnOrigin,
    ) -> StageContext<'a> {
        StageContext {
            state,
            session_toggles,
            filter_cfg,
            lesson_snapshot,
            session_id: "session-lazy-sections-stage-test",
            project_path: "",
            turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: false,
        }
    }

    #[tokio::test]
    async fn lazy_sections_stage_is_always_enabled() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-lazy-sections-stage-test");
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );
        assert!(LazySectionsStage.enabled(&ctx));
        Ok(())
    }

    #[tokio::test]
    async fn apply_strips_section_and_records_manifest() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-lazy-sections-stage-test");
        let filter_cfg = FilterConfig {
            rules: vec![lazy_rule(7)],
        };
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let raw = serde_json::to_vec(&json!({
            "system": [{
                "type": "text",
                "text": format!("You are Claude Code.\n\n{HEADING}\nUse tabs_context_mcp first.\n# Tone\nBe concise."),
            }],
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}],
        }))?;
        let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
        LazySectionsStage.apply(&ctx, &mut body).await;

        assert!(body.filter_applied());
        assert_eq!(body.firing_rule_ids(), &[7i64]);
        let value: serde_json::Value = serde_json::from_slice(body.body())?;
        let text = value["system"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("text must be string"))?;
        assert!(text.contains(STUB), "stub replaces the stripped section");
        assert!(!text.contains("Use tabs_context_mcp first."));
        let parts = body.into_parts();
        let manifest = parts.manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].stage, "lazy_sections");
        Ok(())
    }

    #[tokio::test]
    async fn apply_leaves_body_untouched_when_heading_absent() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-lazy-sections-stage-test");
        let filter_cfg = FilterConfig {
            rules: vec![lazy_rule(7)],
        };
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let raw = serde_json::to_vec(&json!({
            "system": [{"type": "text", "text": "no chrome section here"}],
            "messages": [],
        }))?;
        let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
        LazySectionsStage.apply(&ctx, &mut body).await;

        assert!(!body.filter_applied());
        assert!(body.firing_rule_ids().is_empty());
        assert_eq!(body.body().as_ref(), raw.as_slice());
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].stage, "lazy_sections");
        assert_eq!(
            manifest.stages[0].outcome,
            crate::wire::manifest::StageOutcome::Unchanged
        );
        assert_eq!(manifest.stages[0].reason_code, "lazy_sections_no_match");
        Ok(())
    }

    #[tokio::test]
    async fn apply_without_any_lazy_rules_records_no_rules() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-lazy-sections-stage-test");
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let raw = serde_json::to_vec(&json!({
            "system": [{"type": "text", "text": "anything"}],
            "messages": [],
        }))?;
        let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
        LazySectionsStage.apply(&ctx, &mut body).await;

        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].reason_code, "lazy_sections_no_rules");
        assert_eq!(manifest.verify_chain(), Ok(()));
        Ok(())
    }
}
