use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::recall_injector::{self, RecallInjectionRequest, RecallInjectorConfig, RecallOutcome};
use crate::wire::manifest::AppliedStage;

pub struct RecallInjectorStage;

#[async_trait::async_trait]
impl RequestStage for RecallInjectorStage {
    fn name(&self) -> &'static str {
        "recall_injector"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.recall_injector
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if recall_injector::is_served(ctx.session_id) {
            body.decline(
                "recall_injector",
                "recall_served_by_client",
                serde_json::json!({}),
            );
            return;
        }
        let cfg = RecallInjectorConfig {
            enabled: ctx.session_toggles.recall_injector,
            max_lessons_per_topic: ctx
                .session_toggles
                .features
                .get::<crate::lessons_settings::RecallOverlay>()
                .max_lessons_per_topic,
        };
        let push_deps = ctx.state.push_deps();
        let decisions = ctx.state.decision_env();
        let outcome = recall_injector::apply_recall_injection_with(
            RecallInjectionRequest {
                raw: body.body(),
                project_path: ctx
                    .lesson_snapshot
                    .project_for_session(ctx.session_id, ctx.project_path),
                persist: &ctx.state.persist,
                cfg: &cfg,
                request_id: ctx.request_id,
                session_id: ctx.session_id,
                push_deps: &push_deps,
            },
            decisions.as_ref(),
        )
        .await;
        let (new_body, topics, lesson_count) = match &outcome {
            RecallOutcome::Injected {
                new_body,
                topics,
                lesson_count,
            } => (new_body, topics, lesson_count),
            RecallOutcome::Declined(reason) => {
                body.decline("recall_injector", reason, serde_json::json!({}));
                return;
            }
        };

        tracing::info!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            ?topics,
            lesson_count,
            "recall injector appended <system-reminder>"
        );
        body.rewrite(
            new_body.clone(),
            AppliedStage {
                stage: "recall_injector",
                policy_id: "recall_injector",
                policy_version: 1,
                resolved_parameters: serde_json::json!({
                    "topics": topics,
                    "lesson_count": lesson_count,
                }),
                new_bytes: new_body.as_ref(),
                targets: Vec::new(),
                reason_code: "recall_lessons_injected",
                detail: Some(format!("topics={topics:?} lesson_count={lesson_count}")),
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

    fn ctx_with_state<'a>(
        state: &'a crate::state::AppState,
        session_toggles: &'a ResolvedToggles,
        filter_cfg: &'a FilterConfig,
        lesson_snapshot: &'a LessonSnapshot,
        turn_origin: &'a TurnOrigin,
        project_path: &'a str,
    ) -> StageContext<'a> {
        StageContext {
            state,
            session_toggles,
            filter_cfg,
            lesson_snapshot,
            session_id: "session-recall-injector-stage-test",
            project_path,
            turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: false,
        }
    }

    #[test]
    fn enabled_mirrors_session_frozen_toggle() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let state = crate::state::AppState::new(config).await?;
            let resolved = state.resolve_session_toggles("session-recall-injector-stage-test");
            let mut session_toggles: ResolvedToggles = (*resolved).clone();
            session_toggles.recall_injector = true;
            let filter_cfg = FilterConfig::default();
            let lesson_snapshot = LessonSnapshot::default();
            let turn_origin = TurnOrigin::Primary;
            let ctx = ctx_with_state(
                &state,
                &session_toggles,
                &filter_cfg,
                &lesson_snapshot,
                &turn_origin,
                "",
            );
            assert!(RecallInjectorStage.enabled(&ctx));
            anyhow::Ok(())
        })
    }

    #[tokio::test]
    async fn apply_leaves_body_untouched_when_no_recall_tags() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles: ResolvedToggles =
            (*state.resolve_session_toggles("session-recall-injector-stage-test")).clone();
        session_toggles.recall_injector = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = ctx_with_state(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
            "/test/project",
        );

        let raw = serde_json::to_vec(&serde_json::json!({
            "messages": [
                {"role": "assistant", "content": "no tags here"},
                {"role": "user", "content": [{"type": "text", "text": "hi"}]}
            ]
        }))?;
        let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
        RecallInjectorStage.apply(&ctx, &mut body).await;

        assert!(!body.filter_applied());
        assert_eq!(body.body().as_ref(), raw.as_slice());
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].stage, "recall_injector");
        assert_eq!(manifest.stages[0].reason_code, "recall_no_tags");
        Ok(())
    }

    #[tokio::test]
    async fn apply_leaves_body_untouched_when_tags_present_but_persist_unwired()
    -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles: ResolvedToggles =
            (*state.resolve_session_toggles("session-recall-injector-stage-test")).clone();
        session_toggles.recall_injector = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = ctx_with_state(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
            "/test/project",
        );

        let raw = serde_json::to_vec(&serde_json::json!({
            "messages": [
                {"role": "assistant", "content": "<mem:recall topic=\"x\"/>"},
                {"role": "user", "content": [{"type": "text", "text": "hi"}]}
            ]
        }))?;
        let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
        RecallInjectorStage.apply(&ctx, &mut body).await;

        assert!(!body.filter_applied());
        assert_eq!(body.body().as_ref(), raw.as_slice());
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].reason_code, "recall_no_persist");
        Ok(())
    }

    async fn recall_reason(state: &crate::state::AppState, session_id: &str, raw: &[u8]) -> String {
        let mut session_toggles: ResolvedToggles =
            (*state.resolve_session_toggles(session_id)).clone();
        session_toggles.recall_injector = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = StageContext {
            state,
            session_toggles: &session_toggles,
            filter_cfg: &filter_cfg,
            lesson_snapshot: &lesson_snapshot,
            session_id,
            project_path: "/test/project",
            turn_origin: &turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: false,
        };
        let mut body = StageBody::new(Bytes::from(raw.to_vec()), "orig".to_string());
        RecallInjectorStage.apply(&ctx, &mut body).await;
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        manifest.stages[0].reason_code.to_string()
    }

    #[tokio::test]
    async fn apply_stands_aside_only_for_the_session_the_client_took() -> anyhow::Result<()> {
        let state =
            crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        let served = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let raw = serde_json::to_vec(&serde_json::json!({
            "messages": [
                {"role": "assistant", "content": "<mem:recall topic=\"x\"/>"}
            ]
        }))?;
        recall_injector::mark_served(&served);
        assert_eq!(
            recall_reason(&state, &served, &raw).await,
            "recall_served_by_client"
        );
        assert_eq!(
            recall_reason(&state, &other, &raw).await,
            "recall_no_persist"
        );
        Ok(())
    }
}
