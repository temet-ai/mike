use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::proxy_tools::declare::{self, DeclareOutcome};
use crate::wire::manifest::AppliedStage;

pub struct ProxyToolsStage;

#[async_trait::async_trait]
impl RequestStage for ProxyToolsStage {
    fn name(&self) -> &'static str {
        "proxy_tools"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.proxy_tools
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if !ctx.turn_origin.resends_the_session_tool_list() {
            body.decline(
                "proxy_tools",
                "proxy_tools_sidecar_call",
                serde_json::json!({ "turn_origin": ctx.turn_origin.to_string() }),
            );
            return;
        }
        if crate::proxy_tools::client::served_by_client(ctx.session_id) {
            body.decline(
                "proxy_tools",
                "proxy_tools_served_by_client",
                serde_json::json!({}),
            );
            return;
        }
        if crate::proxy_tools::session::session_brought_its_own(
            &ctx.state.sessions_with_mike_tools,
            ctx.session_id,
            body.body(),
        ) {
            body.decline(
                "proxy_tools",
                "proxy_tools_session_has_them",
                serde_json::json!({}),
            );
            return;
        }
        let DeclareOutcome::Declared { new_body, declared } = declare::declare_tools(body.body())
        else {
            body.decline(
                "proxy_tools",
                "proxy_tools_nothing_declared",
                serde_json::json!({}),
            );
            return;
        };

        tracing::info!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            declared = ?declared,
            "proxy declared its own tools into the request"
        );
        body.rewrite(
            new_body.clone(),
            AppliedStage {
                stage: "proxy_tools",
                policy_id: "proxy_tools",
                policy_version: 1,
                resolved_parameters: serde_json::json!({ "declared": declared }),
                new_bytes: new_body.as_ref(),
                targets: Vec::new(),
                reason_code: "proxy_tool_declaration",
                detail: Some(format!("declared {}", declared.join(", "))),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde_json::Value;

    use super::*;
    use crate::extract::request::TurnOrigin;
    use crate::filter::FilterConfig;
    use crate::lesson_cache::LessonSnapshot;
    use crate::store::models::RequestId;

    const BODY: &[u8] =
        br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":"S","tools":[{"name":"Read"}]}"#;

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
            session_id: "s",
            project_path: "/repo",
            turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: false,
        }
    }

    #[tokio::test]
    async fn declares_the_catalogue_and_records_a_manifest_entry() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles =
            (*state.resolve_session_toggles("session-proxy-tools-stage-test")).clone();
        session_toggles.proxy_tools = true;
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

        let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_string());
        assert!(ProxyToolsStage.enabled(&ctx));
        ProxyToolsStage.apply(&ctx, &mut body).await;

        assert!(body.filter_applied(), "the stage must record its rewrite");
        let v: Value = serde_json::from_slice(body.body())?;
        let names: Vec<&str> = v["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("tools must be an array"))?
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec!["Read", crate::proxy_tools::roster::TOOL_NAME],
            "the client's tools must come first and keep their bytes"
        );

        let parts = body.into_parts();
        let manifest = parts.manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(
            manifest.stages[0].stage, "proxy_tools",
            "capture must be able to see that the proxy declared a tool"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_body_with_nothing_to_declare_records_a_decline() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles =
            (*state.resolve_session_toggles("session-proxy-tools-stage-test")).clone();
        session_toggles.proxy_tools = true;
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

        let mut body = StageBody::new(Bytes::from_static(b"not json"), "orig".to_string());
        ProxyToolsStage.apply(&ctx, &mut body).await;

        assert!(!body.filter_applied());
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].stage, "proxy_tools");
        assert_eq!(
            manifest.stages[0].reason_code,
            "proxy_tools_nothing_declared"
        );
        assert_eq!(manifest.verify_chain(), Ok(()));
        Ok(())
    }

    async fn decline_reason(
        raw: &'static [u8],
        turn_origin: TurnOrigin,
    ) -> anyhow::Result<(bool, String)> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles =
            (*state.resolve_session_toggles("session-proxy-tools-stage-test")).clone();
        session_toggles.proxy_tools = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let mut body = StageBody::new(Bytes::from_static(raw), "orig".to_string());
        ProxyToolsStage.apply(&ctx, &mut body).await;

        let changed = body.filter_applied();
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        Ok((changed, manifest.stages[0].reason_code.clone()))
    }

    #[tokio::test]
    async fn a_session_that_brought_mikes_tools_itself_is_left_alone() -> anyhow::Result<()> {
        let raw = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"tools":[{"name":"Read"},{"name":"mcp__mike__get_workspace_roster"}]}"#;
        let (changed, reason) = decline_reason(raw, TurnOrigin::Primary).await?;
        assert!(
            !changed,
            "the session already has the roster as its own tool"
        );
        assert_eq!(reason, "proxy_tools_session_has_them");
        Ok(())
    }

    #[tokio::test]
    async fn a_sidecar_call_that_shares_nothing_with_the_session_is_left_alone()
    -> anyhow::Result<()> {
        for origin in [
            TurnOrigin::TitleSidecar,
            TurnOrigin::WebFetchSidecar,
            TurnOrigin::MonitorSidecar,
            TurnOrigin::QuotaProbe,
            TurnOrigin::AdvisorSidecar,
        ] {
            let (changed, reason) = decline_reason(BODY, origin).await?;
            assert!(!changed);
            assert_eq!(reason, "proxy_tools_sidecar_call");
        }
        Ok(())
    }

    #[tokio::test]
    async fn the_next_prompt_suggestion_keeps_the_tool_list_its_session_sends() -> anyhow::Result<()>
    {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles =
            (*state.resolve_session_toggles("session-proxy-tools-stage-test")).clone();
        session_toggles.proxy_tools = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::NextInputSuggestionSidecar;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_string());
        ProxyToolsStage.apply(&ctx, &mut body).await;

        assert!(
            body.filter_applied(),
            "a different tool list from the main request would miss its prompt cache"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_session_whose_client_listed_the_tools_is_left_alone_and_another_is_not()
    -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles =
            (*state.resolve_session_toggles("session-proxy-tools-stage-test")).clone();
        session_toggles.proxy_tools = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let mut ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
        );

        ctx.session_id = "11111111-1111-4111-8111-111111111111";
        crate::proxy_tools::client::mark_served(ctx.session_id);
        let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_string());
        ProxyToolsStage.apply(&ctx, &mut body).await;
        assert!(!body.filter_applied());
        let manifest = body.into_parts().manifest.finish("orig".to_string());
        assert_eq!(
            manifest.stages[0].reason_code,
            "proxy_tools_served_by_client"
        );

        ctx.session_id = "22222222-2222-4222-8222-222222222222";
        let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_string());
        ProxyToolsStage.apply(&ctx, &mut body).await;
        assert!(
            body.filter_applied(),
            "an unmarked session keeps today's behaviour"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_disabled_toggle_leaves_the_body_alone() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut session_toggles =
            (*state.resolve_session_toggles("session-proxy-tools-stage-test")).clone();
        session_toggles.proxy_tools = false;
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
        assert!(
            !ProxyToolsStage.enabled(&ctx),
            "with the toggle off the stage must not run at all"
        );
        Ok(())
    }
}
