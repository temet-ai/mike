#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test fixtures build their own JSON bodies; a malformed fixture must fail loudly"
)]

use bytes::Bytes;

use super::*;
use crate::extract::request::TurnOrigin;
use crate::filter::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};
use crate::lesson_worker::LessonSnapshot;
use crate::store::models::RequestId;

fn drop_rule(id: i64, value: &str) -> FilterRule {
    FilterRule {
        id,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::XmlTag,
        value: value.to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}

fn append_rule(id: i64, value: &str, text: &str) -> FilterRule {
    FilterRule {
        id,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::TextSubstring,
        value: value.to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Append,
        text: Some(text.to_string()),
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}

fn claude_code_source(billing: &str, project: &str) -> String {
    format!(
        r#"{{"model":"claude-opus-5","messages":[{{"role":"user","content":"hi"}}],"system":[{{"type":"text","text":"{billing}"}},{{"type":"text","text":"You are Claude Code."}},{{"cache_control":{{"scope":"global","ttl":"1h","type":"ephemeral"}},"type":"text","text":"main prompt"}},{{"cache_control":{{"ttl":"1h","type":"ephemeral"}},"type":"text","text":"{project}"}}]}}"#
    )
}

fn claude_code_body(billing: &str) -> Bytes {
    Bytes::from(claude_code_source(billing, "project guidance here").into_bytes())
}

fn system_tail(body: &Bytes) -> String {
    let value: serde_json::Value = serde_json::from_slice(body).expect("json body");
    value["system"]
        .as_array()
        .expect("system array")
        .iter()
        .skip(3)
        .map(|b| b["text"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn ctx_for<'a>(
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
        session_id: "session-filter-stage-test",
        project_path: "",
        turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    }
}

#[tokio::test]
async fn filter_stage_is_on_for_a_first_turn_and_asks_once_about_a_later_one_without_a_replay()
-> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-filter-stage-test");
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let enabled = |origin: &TurnOrigin| {
        FilterStage.enabled(&ctx_for(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            origin,
        ))
    };

    assert!(enabled(&TurnOrigin::PrimaryFirst));
    assert!(
        enabled(&TurnOrigin::Primary),
        "after a restart a session's later request is looked at once"
    );
    assert!(!enabled(&TurnOrigin::Subagent));
    assert!(!enabled(&TurnOrigin::NextInputSuggestionSidecar));

    let asked = ctx_for(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &TurnOrigin::Primary,
    );
    state.system_replay.mark_unfiltered(asked.session_id);
    assert!(!enabled(&TurnOrigin::Primary));
    Ok(())
}

#[tokio::test]
async fn a_later_turn_replays_the_first_turns_rewrite() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-replay");
    let filter_cfg = FilterConfig {
        rules: vec![append_rule(62, "project guidance", " EXTRA")],
    };
    let lesson_snapshot = LessonSnapshot::default();

    let first_origin = TurnOrigin::PrimaryFirst;
    let ctx = ctx_for(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &first_origin,
    );
    let mut body = StageBody::new(claude_code_body("cch=aaa"), "orig".to_string());
    assert!(FilterStage.enabled(&ctx));
    FilterStage.apply(&ctx, &mut body).await;
    assert!(body.filter_applied(), "turn 1 must be rewritten");
    assert!(
        String::from_utf8_lossy(body.body()).contains("project guidance EXTRA here"),
        "turn 1 must carry the append"
    );

    let later_origin = TurnOrigin::Primary;
    let later_ctx = ctx_for(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &later_origin,
    );
    assert!(
        FilterStage.enabled(&later_ctx),
        "a session with a captured replay must run the stage on later turns"
    );
    let mut later = StageBody::new(claude_code_body("cch=zzz"), "orig".to_string());
    FilterStage.apply(&later_ctx, &mut later).await;

    let sent = String::from_utf8(later.body().to_vec())?;
    assert!(
        sent.contains("project guidance EXTRA here"),
        "turn 2 must carry the same append; got {sent}"
    );
    assert!(
        sent.contains("cch=zzz") && !sent.contains("cch=aaa"),
        "turn 2 must keep its own billing header; got {sent}"
    );
    assert_eq!(
        later.firing_rule_ids(),
        &[62i64],
        "a replayed request must record the rules its body actually carries"
    );
    Ok(())
}

#[tokio::test]
async fn a_rule_edit_mid_session_keeps_the_captured_rewrite_replaying() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-pinned");
    let opening_cfg = FilterConfig {
        rules: vec![append_rule(62, "project guidance", " EXTRA")],
    };
    let lesson_snapshot = LessonSnapshot::default();

    let first_origin = TurnOrigin::PrimaryFirst;
    let first_ctx = ctx_for(
        &state,
        &session_toggles,
        &opening_cfg,
        &lesson_snapshot,
        &first_origin,
    );
    let mut first = StageBody::new(claude_code_body("cch=aaa"), "orig".to_string());
    FilterStage.apply(&first_ctx, &mut first).await;
    assert!(first.filter_applied(), "turn 1 must be rewritten");

    let mut edited_rule = append_rule(62, "project guidance", " EXTRA");
    edited_rule.updated_at += chrono::Duration::seconds(5);
    let edited_cfg = FilterConfig {
        rules: vec![edited_rule],
    };
    let later_origin = TurnOrigin::Primary;
    let later_ctx = ctx_for(
        &state,
        &session_toggles,
        &edited_cfg,
        &lesson_snapshot,
        &later_origin,
    );
    assert!(
        FilterStage.enabled(&later_ctx),
        "an edit must not evict the session's captured replay"
    );
    let mut later = StageBody::new(claude_code_body("cch=zzz"), "orig".to_string());
    FilterStage.apply(&later_ctx, &mut later).await;

    let sent = String::from_utf8(later.body().to_vec())?;
    assert!(
        sent.contains("project guidance EXTRA here"),
        "a session must keep the rewrite it opened with after a rule edit; got {sent}"
    );
    assert!(
        FilterStage.enabled(&later_ctx),
        "the replay must still be registered after a later turn"
    );
    Ok(())
}

#[tokio::test]
async fn turn_one_and_turn_two_send_the_same_system_prompt() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-cache-parity");
    let filter_cfg = FilterConfig {
        rules: vec![append_rule(62, "project guidance", " EXTRA")],
    };
    let lesson_snapshot = LessonSnapshot::default();

    let first_origin = TurnOrigin::PrimaryFirst;
    let mut first = StageBody::new(claude_code_body("cch=aaa"), "orig".to_string());
    FilterStage
        .apply(
            &ctx_for(
                &state,
                &session_toggles,
                &filter_cfg,
                &lesson_snapshot,
                &first_origin,
            ),
            &mut first,
        )
        .await;

    let later_origin = TurnOrigin::Primary;
    let mut later = StageBody::new(claude_code_body("cch=zzz"), "orig".to_string());
    FilterStage
        .apply(
            &ctx_for(
                &state,
                &session_toggles,
                &filter_cfg,
                &lesson_snapshot,
                &later_origin,
            ),
            &mut later,
        )
        .await;

    assert_eq!(
        system_tail(first.body()),
        system_tail(later.body()),
        "the cacheable tail of system[] must be identical across turns"
    );
    Ok(())
}

#[tokio::test]
async fn a_recomposed_prompt_is_forwarded_untouched() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-recomposed");
    let filter_cfg = FilterConfig {
        rules: vec![append_rule(62, "project guidance", " EXTRA")],
    };
    let lesson_snapshot = LessonSnapshot::default();

    let first_origin = TurnOrigin::PrimaryFirst;
    let mut first = StageBody::new(claude_code_body("cch=aaa"), "orig".to_string());
    FilterStage
        .apply(
            &ctx_for(
                &state,
                &session_toggles,
                &filter_cfg,
                &lesson_snapshot,
                &first_origin,
            ),
            &mut first,
        )
        .await;

    let recomposed = Bytes::from(
        claude_code_source("cch=zzz", "a completely different project block").into_bytes(),
    );
    let later_origin = TurnOrigin::Primary;
    let mut later = StageBody::new(recomposed.clone(), "orig".to_string());
    FilterStage
        .apply(
            &ctx_for(
                &state,
                &session_toggles,
                &filter_cfg,
                &lesson_snapshot,
                &later_origin,
            ),
            &mut later,
        )
        .await;

    assert!(!later.filter_applied());
    assert_eq!(later.body().as_ref(), recomposed.as_ref());
    let manifest = later.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(
        manifest.stages[0].reason_code,
        "filter_replay_system_mismatch"
    );
    Ok(())
}

#[tokio::test]
async fn apply_rewrites_body_and_records_manifest_on_match() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-filter-stage-test");
    let filter_cfg = FilterConfig {
        rules: vec![drop_rule(11, "system-reminder")],
    };
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = StageContext {
        state: &state,
        session_toggles: &session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id: "session-filter-stage-test",
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };

    let raw = br#"{"system":[{"type":"text","text":"<system-reminder>x</system-reminder>"}]}"#;
    let mut body = StageBody::new(Bytes::from_static(raw), "orig".to_string());
    FilterStage.apply(&ctx, &mut body).await;

    assert!(body.filter_applied());
    assert_eq!(body.firing_rule_ids(), &[11i64]);
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "filter");
    Ok(())
}

#[tokio::test]
async fn apply_leaves_body_untouched_when_no_rule_matches() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-filter-stage-test");
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = StageContext {
        state: &state,
        session_toggles: &session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id: "session-filter-stage-test",
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };

    let raw = br#"{"system":[{"type":"text","text":"hello"}]}"#;
    let mut body = StageBody::new(Bytes::from_static(raw), "orig".to_string());
    FilterStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert!(body.firing_rule_ids().is_empty());
    assert_eq!(body.body().as_ref(), raw);
    Ok(())
}

#[tokio::test]
async fn filter_rule_anchored_in_the_mutable_tail_fires_on_primary_first() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-rule-71-shape");
    let filter_cfg = FilterConfig {
        rules: vec![append_rule(
            71,
            "project guidance",
            "\n\n# Communicating with the user\n...",
        )],
    };
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = ctx_for(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );

    let mut body = StageBody::new(claude_code_body("cch=aaa"), "orig".to_string());
    assert!(FilterStage.enabled(&ctx), "PrimaryFirst must run the stage");
    FilterStage.apply(&ctx, &mut body).await;

    assert!(body.filter_applied(), "the anchor is present and reachable");
    assert_eq!(
        body.firing_rule_ids(),
        &[71i64],
        "rule 71's id must land in firing_rule_ids"
    );
    let tail = system_tail(body.body());
    assert!(
        tail.contains("project guidance\n\n# Communicating with the user\n..."),
        "the appended text must reach the outgoing body; got {tail}"
    );
    Ok(())
}
