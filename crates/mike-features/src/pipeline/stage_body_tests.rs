use super::*;

fn applied_stage<'a>(stage: &'a str, new_bytes: &'a [u8]) -> AppliedStage<'a> {
    AppliedStage {
        stage,
        policy_id: stage,
        policy_version: 1,
        resolved_parameters: serde_json::json!({}),
        new_bytes,
        targets: Vec::new(),
        reason_code: "test",
        detail: None,
    }
}

#[test]
fn stage_order_matches_documented_pipeline_sequence() {
    assert_eq!(
        STAGE_ORDER,
        [
            "filter",
            "reminder_on_token",
            "lazy_sections",
            "recall_injector",
            "lessons",
            "memory_keeper",
            "self_identity",
            "workspace_briefing",
            "agent_lessons",
            "reminders",
            "change_record_tail",
            "git_read_hint",
            "steerer",
            "repeated_notices",
            "read_dedup",
            "file_read_compression",
            "shell_output_compression",
            "proxy_tools",
            "context_focus_question",
        ]
    );
}

#[test]
fn stage_order_has_no_duplicate_entries() {
    let mut seen = std::collections::HashSet::new();
    for name in STAGE_ORDER {
        assert!(seen.insert(name), "duplicate stage name: {name}");
    }
}

#[test]
fn rewrite_sets_body_filtered_stored_and_applied_flag() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    let new_bytes = Bytes::from_static(br#"{"a":1}"#);
    body.rewrite(new_bytes.clone(), applied_stage("test", new_bytes.as_ref()));
    assert_eq!(body.body(), &new_bytes);
    assert_eq!(body.filtered_stored(), Some(&new_bytes));
    assert!(body.filter_applied());
}

#[test]
fn rewrite_forwarded_only_leaves_filtered_stored_untouched() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    let new_bytes = Bytes::from_static(br#"{"a":1}"#);
    body.rewrite_forwarded_only(new_bytes.clone(), applied_stage("test", new_bytes.as_ref()));
    assert_eq!(body.body(), &new_bytes);
    assert_eq!(body.filtered_stored(), None);
    assert!(body.filter_applied());
}

#[test]
fn absorb_updates_body_without_touching_the_manifest() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    let new_bytes = Bytes::from_static(br#"{"a":1}"#);
    body.absorb(new_bytes.clone());
    assert_eq!(body.body(), &new_bytes);
    assert_eq!(body.filtered_stored(), Some(&new_bytes));
    assert!(body.filter_applied());
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert!(manifest.stages.is_empty());
}

#[test]
fn merge_rule_ids_deduplicates_across_calls() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    body.merge_rule_ids([1i64, 2]);
    body.merge_rule_ids([2i64, 3]);
    assert_eq!(body.firing_rule_ids().to_vec(), vec![1i64, 2, 3]);
}

#[test]
fn merge_lesson_ids_deduplicates_across_calls() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    body.merge_lesson_ids([1i64, 2]);
    body.merge_lesson_ids([2i64, 3]);
    assert_eq!(body.injected_lesson_ids().to_vec(), vec![1i64, 2, 3]);
}

#[test]
fn into_parts_carries_every_field_forward() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    let new_bytes = Bytes::from_static(br#"{"a":1}"#);
    body.rewrite(new_bytes.clone(), applied_stage("test", new_bytes.as_ref()));
    body.merge_rule_ids([7i64]);
    body.merge_lesson_ids([9i64]);
    let parts = body.into_parts();
    assert_eq!(parts.body, new_bytes);
    assert_eq!(parts.filtered_stored, Some(new_bytes));
    assert!(parts.filter_applied);
    assert_eq!(parts.firing_rule_ids, vec![7i64]);
    assert_eq!(parts.injected_lesson_ids, vec![9i64]);
}

#[test]
fn from_parts_round_trips_into_parts_state() {
    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    let new_bytes = Bytes::from_static(br#"{"a":1}"#);
    body.rewrite(new_bytes.clone(), applied_stage("test", new_bytes.as_ref()));
    body.merge_rule_ids([7i64]);
    body.merge_lesson_ids([9i64]);

    let resumed = StageBody::from_parts(body.into_parts());
    assert_eq!(resumed.body(), &new_bytes);
    assert_eq!(resumed.filtered_stored(), Some(&new_bytes));
    assert!(resumed.filter_applied());
    assert_eq!(resumed.firing_rule_ids(), &[7i64]);
    assert_eq!(resumed.injected_lesson_ids(), &[9i64]);
    let parts = resumed.into_parts();
    let manifest = parts.manifest.finish("fin".to_string());
    assert_eq!(manifest.stages.len(), 1, "manifest survives the round trip");
}

struct FakeStage {
    stage_name: &'static str,
    should_run: bool,
}

#[async_trait::async_trait]
impl RequestStage for FakeStage {
    fn name(&self) -> &'static str {
        self.stage_name
    }

    fn enabled(&self, _ctx: &StageContext<'_>) -> bool {
        self.should_run
    }

    async fn apply(&self, _ctx: &StageContext<'_>, body: &mut StageBody) {
        if !self.should_run {
            return;
        }
        let new_bytes = Bytes::from_static(br#"{"stage":"fake"}"#);
        body.rewrite(
            new_bytes.clone(),
            applied_stage(self.stage_name, new_bytes.as_ref()),
        );
    }
}

#[tokio::test]
async fn request_stage_runs_through_a_dyn_registry_in_order() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-pipeline-test");
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = StageContext {
        state: &state,
        session_toggles: &session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id: "session-pipeline-test",
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };

    let stages: Vec<Box<dyn RequestStage>> = vec![
        Box::new(FakeStage {
            stage_name: "one",
            should_run: true,
        }),
        Box::new(FakeStage {
            stage_name: "two",
            should_run: false,
        }),
    ];

    let mut body = StageBody::new(Bytes::from_static(b"{}"), "orig".to_string());
    for stage in &stages {
        if stage.enabled(&ctx) {
            stage.apply(&ctx, &mut body).await;
        }
    }

    assert!(
        body.filter_applied(),
        "the enabled fake stage must rewrite the body"
    );
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("fin".to_string());
    assert_eq!(
        manifest.stages.len(),
        1,
        "only the enabled stage records a manifest entry"
    );
    assert_eq!(manifest.stages[0].stage, "one");
    Ok(())
}
