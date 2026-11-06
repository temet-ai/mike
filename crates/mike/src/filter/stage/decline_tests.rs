use bytes::Bytes;

use super::FilterStage;
use crate::compress::manifest::StageOutcome;
use crate::extract::request::TurnOrigin;
use crate::filter::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::store::models::RequestId;

const BODY: &[u8] = br#"{"system":[{"type":"text","text":"hello"}]}"#;

fn drop_rule(id: i64, tag: &str, project_path: Option<&str>) -> FilterRule {
    FilterRule {
        id,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::XmlTag,
        value: tag.to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: project_path.map(str::to_string),
        style_override: false,
        title: None,
    }
}

async fn decline_reason(filter_cfg: FilterConfig) -> anyhow::Result<(String, StageOutcome, i64)> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-filter-decline-test");
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = StageContext {
        state: &state,
        session_toggles: &session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id: "session-filter-decline-test",
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };
    let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_string());
    FilterStage.apply(&ctx, &mut body).await;
    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), BODY);
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    anyhow::ensure!(
        manifest.stages.len() == 1,
        "a declining filter stage must record exactly one entry: {:?}",
        manifest.stages
    );
    let entry = &manifest.stages[0];
    anyhow::ensure!(entry.stage == "filter", "stage was {}", entry.stage);
    anyhow::ensure!(
        manifest.verify_chain().is_ok(),
        "the manifest hash chain must stay closed"
    );
    let rule_count = entry.resolved_parameters["rule_count"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("rule_count missing"))?;
    Ok((entry.reason_code.clone(), entry.outcome, rule_count))
}

#[tokio::test]
async fn an_empty_ruleset_records_filter_no_rules() -> anyhow::Result<()> {
    let (reason, outcome, rule_count) = decline_reason(FilterConfig::default()).await?;
    assert_eq!(reason, "filter_no_rules");
    assert_eq!(outcome, StageOutcome::Unchanged);
    assert_eq!(rule_count, 0);
    Ok(())
}

#[tokio::test]
async fn rules_that_match_nothing_record_filter_no_match() -> anyhow::Result<()> {
    let filter_cfg = FilterConfig {
        rules: vec![drop_rule(11, "absent-tag", None)],
    };
    let (reason, outcome, rule_count) = decline_reason(filter_cfg).await?;
    assert_eq!(reason, "filter_no_match");
    assert_eq!(outcome, StageOutcome::Unchanged);
    assert_eq!(rule_count, 1);
    Ok(())
}

#[tokio::test]
async fn rules_scoped_to_another_project_record_no_applicable_rules() -> anyhow::Result<()> {
    let filter_cfg = FilterConfig {
        rules: vec![drop_rule(12, "hello", Some("/some/other/project"))],
    };
    let (reason, outcome, _rule_count) = decline_reason(filter_cfg).await?;
    assert_eq!(reason, "filter_no_applicable_rules");
    assert_eq!(outcome, StageOutcome::Unchanged);
    Ok(())
}

#[tokio::test]
async fn a_later_request_of_a_session_never_rewritten_is_left_alone_and_asked_once()
-> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-filter-recover-test");
    let lesson_snapshot = LessonSnapshot::default();
    let filter_cfg = FilterConfig {
        rules: vec![drop_rule(1, "nomatch", None)],
    };
    let turn_origin = TurnOrigin::Primary;
    let ctx = StageContext {
        state: &state,
        session_toggles: &session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id: "session-filter-recover-test",
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };
    anyhow::ensure!(
        FilterStage.enabled(&ctx),
        "a session mike has not seen since it started is asked about once"
    );
    let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_string());
    FilterStage.apply(&ctx, &mut body).await;
    assert_eq!(body.body().as_ref(), BODY);
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages[0].reason_code, "filter_not_in_force");
    anyhow::ensure!(
        !FilterStage.enabled(&ctx),
        "once known to be unfiltered the session is not asked about again"
    );
    Ok(())
}
