use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use super::assemble::assemble;
use super::dto::{SessionDeliveryDto, SettingBasis, SkillState, ToggleDto, TopicDto, TopicState};
use super::profile::{DELIVERY_TOGGLES, ProfileInputs};
use crate::feature_toggles::{FeatureToggles, ResolvedToggles, TOGGLE_META, get_toggle_enabled};
use crate::settings::SettingsOverlay;
use crate::store::persistence::session_delivery::{
    AvailableTopicRow, DeliveredLessonRow, DeliveryAgentRow, DeliveryModelRow, DeliveryRecallRow,
    DeliverySessionRow, DeliveryStageRow, DeliveryToggleChangeRow, DeliveryToolUseRow,
    SessionDeliveryRows,
};

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, 0, 0)
        .single()
        .unwrap_or_default()
}

fn empty_rows() -> SessionDeliveryRows {
    SessionDeliveryRows {
        session: DeliverySessionRow {
            id: 7,
            session_uuid: Uuid::nil(),
            first_seen: at(8),
            last_seen: at(12),
            project_resolution: "git".to_owned(),
            project_id: 3,
            project_path: "/work/mike".to_owned(),
            project_name: "mike".to_owned(),
        },
        models: vec![DeliveryModelRow {
            model: Some("claude-opus".to_owned()),
            request_count: 10,
            manifest_count: 10,
        }],
        agents: vec![DeliveryAgentRow {
            persona: Some("Rust Development Expert".to_owned()),
            agent_name: Some("rust-developer".to_owned()),
            first_request_id: Some(101),
            started_at: Some(at(9)),
        }],
        toggle_changes: Vec::new(),
        lessons: Vec::new(),
        attributed_lesson_requests: 0,
        available_topics: Vec::new(),
        stages: Vec::new(),
        recalls: Vec::new(),
        tool_uses: Vec::new(),
    }
}

fn lesson(id: i64, topic: Option<&str>) -> DeliveredLessonRow {
    DeliveredLessonRow {
        lesson_id: id,
        text: format!("lesson {id} full text"),
        kind: "correction".to_owned(),
        topic: topic.map(str::to_owned),
        stage: "daily".to_owned(),
        archived_at: None,
        archived_reason: None,
        injection_count: 2,
        first_injected_at: at(9),
        last_injected_at: at(10),
        first_request_id: 101,
        last_request_id: 105,
        lanes: vec!["primary".to_owned()],
        personas: Vec::new(),
        origins: vec!["proxy".to_owned()],
    }
}

fn stage(name: &str, outcome: &str, reason: &str, count: i64) -> DeliveryStageRow {
    DeliveryStageRow {
        stage: name.to_owned(),
        outcome: outcome.to_owned(),
        reason_code: reason.to_owned(),
        request_count: count,
        first_request_id: 100,
        last_request_id: 110,
    }
}

fn run(
    rows: &SessionDeliveryRows,
    toggles: &FeatureToggles,
    frozen: Option<&ResolvedToggles>,
) -> SessionDeliveryDto {
    let settings = SettingsOverlay::default();
    let inputs = ProfileInputs {
        current_toggles: toggles,
        current_settings: &settings,
        frozen,
    };
    assemble(rows, &inputs, at(13))
}

fn gap_codes(dto: &SessionDeliveryDto) -> Vec<&'static str> {
    dto.evidence_gaps.iter().map(|gap| gap.code).collect()
}

fn toggle_of<'a>(dto: &'a SessionDeliveryDto, key: &str) -> anyhow::Result<&'a ToggleDto> {
    dto.profile
        .toggles
        .iter()
        .find(|toggle| toggle.key == key)
        .ok_or_else(|| anyhow::anyhow!("toggle {key} missing"))
}

fn gap_detail<'a>(dto: &'a SessionDeliveryDto, code: &str) -> anyhow::Result<&'a str> {
    dto.evidence_gaps
        .iter()
        .find(|gap| gap.code == code)
        .map(|gap| gap.detail.as_str())
        .ok_or_else(|| anyhow::anyhow!("gap {code} missing"))
}

fn topic_of<'a>(dto: &'a SessionDeliveryDto, name: &str) -> anyhow::Result<&'a TopicDto> {
    dto.topics
        .iter()
        .find(|topic| topic.topic == name)
        .ok_or_else(|| anyhow::anyhow!("topic {name} missing"))
}

#[test]
fn delivery_toggles_all_exist_in_meta_and_toggle_lookup() {
    let toggles = FeatureToggles::default();
    for key in DELIVERY_TOGGLES {
        assert!(
            TOGGLE_META.iter().any(|meta| meta.key == key),
            "{key} missing from TOGGLE_META"
        );
        assert!(
            get_toggle_enabled(&toggles, key).is_some(),
            "{key} missing from get_toggle_enabled"
        );
    }
}

#[test]
fn profile_reports_every_delivery_toggle_with_project_and_agents() {
    let dto = run(&empty_rows(), &FeatureToggles::default(), None);
    assert_eq!(dto.profile.toggles.len(), DELIVERY_TOGGLES.len());
    assert_eq!(dto.profile.project.name, "mike");
    assert_eq!(dto.profile.project.resolution, "git");
    assert_eq!(dto.profile.models[0].request_count, 10);
    assert_eq!(
        dto.profile.agents[0].persona.as_deref(),
        Some("Rust Development Expert")
    );
}

#[test]
fn toggle_value_at_session_start_comes_from_first_later_change() -> anyhow::Result<()> {
    let mut rows = empty_rows();
    rows.toggle_changes = vec![DeliveryToggleChangeRow {
        toggle_key: "recall_injector".to_owned(),
        changed_at: at(10),
        enabled_before: Some(false),
        enabled_after: Some(true),
        via: "spa".to_owned(),
    }];
    let toggles = FeatureToggles {
        recall_injector: true,
        ..FeatureToggles::default()
    };
    let dto = run(&rows, &toggles, None);
    let toggle = toggle_of(&dto, "recall_injector")?;
    assert_eq!(toggle.enabled, Some(false));
    assert!(toggle.enabled_now);
    assert!(toggle.changed_since_session_start);
    assert_eq!(toggle.basis, SettingBasis::ChangeLogAtSessionStart);
    assert!(dto.profile.toggle_changes[0].during_session);
    Ok(())
}

#[test]
fn toggle_inserted_after_session_start_has_unknown_value() -> anyhow::Result<()> {
    let mut rows = empty_rows();
    rows.toggle_changes = vec![DeliveryToggleChangeRow {
        toggle_key: "launch_brief".to_owned(),
        changed_at: at(20),
        enabled_before: None,
        enabled_after: Some(true),
        via: "migration".to_owned(),
    }];
    let dto = run(&rows, &FeatureToggles::default(), None);
    let toggle = toggle_of(&dto, "launch_brief")?;
    assert_eq!(toggle.enabled, None);
    assert!(!dto.profile.toggle_changes[0].during_session);
    Ok(())
}

#[test]
fn frozen_session_toggles_win_over_current_values() -> anyhow::Result<()> {
    let overlay = FeatureToggles {
        reminders: false,
        ..FeatureToggles::default()
    };
    let frozen = ResolvedToggles::from_overlay(
        &FeatureToggles {
            reminders: true,
            ..FeatureToggles::default()
        },
        &SettingsOverlay::default(),
    );
    let dto = run(&empty_rows(), &overlay, Some(&frozen));
    let toggle = toggle_of(&dto, "reminders")?;
    assert_eq!(toggle.enabled, Some(true));
    assert!(!toggle.enabled_now);
    assert_eq!(toggle.basis, SettingBasis::SessionFrozen);
    assert_eq!(dto.profile.limits.basis, SettingBasis::SessionFrozen);
    assert!(!gap_codes(&dto).contains(&"session_settings_not_in_memory"));
    Ok(())
}

#[test]
fn missing_frozen_settings_are_reported_as_a_gap() {
    let dto = run(&empty_rows(), &FeatureToggles::default(), None);
    assert!(gap_codes(&dto).contains(&"session_settings_not_in_memory"));
    assert_eq!(dto.profile.limits.basis, SettingBasis::CurrentSettings);
}

#[test]
fn delivered_lessons_keep_full_text_and_topic() {
    let mut rows = empty_rows();
    rows.lessons = vec![lesson(11, Some("sqlx")), lesson(12, None)];
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert_eq!(dto.lessons.len(), 2);
    assert_eq!(dto.lessons[0].text, "lesson 11 full text");
    assert_eq!(dto.lessons[0].topic.as_deref(), Some("sqlx"));
    assert!(gap_codes(&dto).contains(&"lesson_text_not_versioned"));
}

#[test]
fn delivered_lessons_say_whether_the_proxy_or_the_mod_put_them_in() {
    let mut rows = empty_rows();
    let mut from_mod = lesson(31, None);
    from_mod.origins = vec!["mod".to_owned()];
    let mut from_both = lesson(32, None);
    from_both.origins = vec!["mod".to_owned(), "proxy".to_owned()];
    rows.lessons = vec![lesson(30, None), from_mod, from_both];
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert_eq!(dto.lessons[0].origins, vec!["proxy"]);
    assert_eq!(dto.lessons[1].origins, vec!["mod"]);
    assert_eq!(dto.lessons[2].origins, vec!["mod", "proxy"]);
}

#[test]
fn topic_states_distinguish_recalled_delivered_and_available() -> anyhow::Result<()> {
    let mut rows = empty_rows();
    rows.available_topics = vec![
        AvailableTopicRow {
            topic: "sqlx".to_owned(),
            shared: false,
            lesson_count: 3,
        },
        AvailableTopicRow {
            topic: "sqlx".to_owned(),
            shared: true,
            lesson_count: 1,
        },
        AvailableTopicRow {
            topic: "axum".to_owned(),
            shared: false,
            lesson_count: 2,
        },
        AvailableTopicRow {
            topic: "tokio".to_owned(),
            shared: false,
            lesson_count: 4,
        },
    ];
    rows.recalls = vec![DeliveryRecallRow {
        request_id: 104,
        recalled_at: at(10),
        topics: serde_json::json!(["sqlx"]),
        lesson_count: Some(3),
    }];
    rows.lessons = vec![lesson(21, Some("axum"))];
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert_eq!(topic_of(&dto, "sqlx")?.state, TopicState::Recalled);
    assert_eq!(topic_of(&dto, "axum")?.state, TopicState::DeliveredInFull);
    assert_eq!(topic_of(&dto, "tokio")?.state, TopicState::Available);
    let sqlx = topic_of(&dto, "sqlx")?;
    assert_eq!(sqlx.recall_request_ids, vec![104]);
    assert_eq!(sqlx.available_project_lessons, 3);
    assert_eq!(sqlx.available_shared_lessons, 1);
    assert_eq!(dto.recall_injections[0].topics, vec!["sqlx".to_owned()]);
    Ok(())
}

#[test]
fn recalled_topic_without_a_live_lesson_is_still_listed() {
    let mut rows = empty_rows();
    rows.recalls = vec![DeliveryRecallRow {
        request_id: 9,
        recalled_at: at(9),
        topics: serde_json::json!(["gone-topic"]),
        lesson_count: None,
    }];
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert_eq!(dto.topics.len(), 1);
    assert_eq!(dto.topics[0].state, TopicState::Recalled);
    assert_eq!(dto.topics[0].available_project_lessons, 0);
}

#[test]
fn skill_uses_split_from_recall_calls_and_failed_loads_are_marked() {
    let mut rows = empty_rows();
    rows.tool_uses = vec![
        DeliveryToolUseRow {
            tool_name: "Skill".to_owned(),
            skill: Some("rust-lsp".to_owned()),
            use_count: 2,
            error_count: 0,
            first_request_id: 102,
            last_request_id: 106,
            first_used_at: at(9),
            last_used_at: at(11),
        },
        DeliveryToolUseRow {
            tool_name: "Skill".to_owned(),
            skill: Some("missing-skill".to_owned()),
            use_count: 1,
            error_count: 1,
            first_request_id: 103,
            last_request_id: 103,
            first_used_at: at(9),
            last_used_at: at(9),
        },
        DeliveryToolUseRow {
            tool_name: "mcp__mike__recall_project_context".to_owned(),
            skill: None,
            use_count: 4,
            error_count: 0,
            first_request_id: 104,
            last_request_id: 109,
            first_used_at: at(9),
            last_used_at: at(12),
        },
    ];
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert_eq!(dto.skills.len(), 2);
    assert_eq!(dto.skills[0].state, SkillState::Loaded);
    assert_eq!(dto.skills[1].state, SkillState::LoadFailed);
    assert_eq!(dto.recall_tool_calls.len(), 1);
    assert_eq!(dto.recall_tool_calls[0].call_count, 4);
    assert!(gap_codes(&dto).contains(&"skill_selection_not_recorded"));
}

#[test]
fn lesson_reminders_without_attribution_are_reported_with_both_counts() -> anyhow::Result<()> {
    let mut rows = empty_rows();
    rows.stages = vec![
        stage("session_lessons", "applied", "session_lessons_injected", 6),
        stage("agent_lessons", "applied", "agent_lessons_injected", 2),
    ];
    rows.attributed_lesson_requests = 5;
    let dto = run(&rows, &FeatureToggles::default(), None);
    let detail = gap_detail(&dto, "lesson_reminders_unattributed")?;
    assert!(detail.contains("8 times"));
    assert!(detail.contains("only 5 requests"));
    assert!(gap_codes(&dto).contains(&"topic_catalog_not_recorded"));
    Ok(())
}

#[test]
fn fully_attributed_lesson_reminders_raise_no_unattributed_gap() {
    let mut rows = empty_rows();
    rows.stages = vec![stage(
        "session_lessons",
        "applied",
        "session_lessons_injected",
        3,
    )];
    rows.attributed_lesson_requests = 3;
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert!(!gap_codes(&dto).contains(&"lesson_reminders_unattributed"));
}

#[test]
fn requests_without_manifest_are_reported() -> anyhow::Result<()> {
    let mut rows = empty_rows();
    rows.models[0].manifest_count = 4;
    let dto = run(&rows, &FeatureToggles::default(), None);
    assert!(gap_detail(&dto, "requests_without_manifest")?.starts_with("6 of 10 requests"));
    Ok(())
}

#[test]
fn empty_session_reports_only_standing_gaps() {
    let dto = run(&empty_rows(), &FeatureToggles::default(), None);
    assert!(dto.lessons.is_empty());
    assert!(dto.topics.is_empty());
    assert!(dto.skills.is_empty());
    assert_eq!(
        gap_codes(&dto),
        vec![
            "skill_selection_not_recorded",
            "session_settings_not_in_memory"
        ]
    );
}
