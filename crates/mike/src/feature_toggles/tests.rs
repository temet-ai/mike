use super::*;

#[test]
fn feature_toggles_mutation_and_egress_default_off() {
    let t = FeatureToggles::default();
    assert!(!t.reminders);
    assert!(!t.recall_injector);
    assert!(!t.weasel_intercept);
    assert!(!t.reminder_on_token);
    assert!(!t.mailbox_redelivery);
    assert!(!t.steerer);
    assert!(!t.judge_worker);
    assert!(!t.session_judge);
    assert!(!t.agent_lessons);
    assert!(!t.file_read_compression);
    assert!(!t.self_identity_reminder);
    assert!(!t.proxy_tools);
    assert!(!t.workspace_briefing);
}

#[test]
fn feature_toggles_capture_only_default_on() {
    let t = FeatureToggles::default();
    assert!(t.lesson_worker);
    assert!(t.harvester);
    assert!(t.db_resync);
    assert!(t.retention_sweep);
    assert!(t.channel_link_sweep);
    assert!(t.workspace_capture_registration);
}

#[test]
fn feature_toggles_prompt_assistant_default_on() {
    let t = FeatureToggles::default();
    assert!(
        t.prompt_assistant,
        "prompt_assistant is operator-invoked, not an autonomous mutator or egress path, \
         so it defaults on"
    );
    let flipped = FeatureToggles {
        prompt_assistant: false,
        ..t
    };
    assert_eq!(
        get_toggle_enabled(&flipped, "prompt_assistant"),
        Some(false)
    );
}

#[test]
fn feature_toggles_dossier_builder_default_off_and_immediate() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert!(
        !t.dossier_builder,
        "dossier_builder ships off so a deploy changes nothing in live judging"
    );
    let on = set_toggle_enabled(&t, "dossier_builder", true).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for dossier_builder")
    })?;
    assert_eq!(get_toggle_enabled(&on, "dossier_builder"), Some(true));
    let meta = TOGGLE_META
        .iter()
        .find(|m| m.key == "dossier_builder")
        .ok_or_else(|| anyhow::anyhow!("dossier_builder missing from TOGGLE_META"))?;
    assert_eq!(meta.applies, ToggleApplies::Immediate);
    assert_eq!(meta.scope, ToggleScope::Global);
    Ok(())
}

#[test]
fn get_toggle_enabled_roundtrips() {
    let t = FeatureToggles {
        reminders: false,
        recall_injector: true,
        weasel_intercept: false,
        reminder_on_token: true,
        mailbox_redelivery: false,
        steerer: true,
        lesson_worker: false,
        harvester: true,
        db_resync: false,
        judge_worker: true,
        session_judge: true,
        agent_lessons: false,
        file_read_compression: true,
        retention_sweep: true,
        channel_link_sweep: true,
        proxy_tools: false,
        workspace_capture_registration: true,
        self_identity_reminder: false,
        workspace_briefing: true,
        repeated_notices: true,
        launch_brief: true,
        native_peer_mail: true,
        mod_lesson_delivery: true,
        fleet_auto_start: false,
        prompt_assistant: false,
        dossier_builder: true,
        semantic_context: true,
        turn_guard: false,
        directive_proposer: false,
        session_end_shadow: false,
        judge_lessons: false,
        change_record: true,
        change_record_tail: false,
        ..FeatureToggles::default()
    };
    let expected = [
        ("change_record", true),
        ("change_record_tail", false),
        ("reminders", false),
        ("recall_injector", true),
        ("reminder_on_token", true),
        ("mailbox_redelivery", false),
        ("steerer", true),
        ("db_resync", false),
        ("agent_lessons", false),
        ("file_read_compression", true),
        ("retention_sweep", true),
        ("channel_link_sweep", true),
        ("workspace_capture_registration", true),
        ("self_identity_reminder", false),
        ("workspace_briefing", true),
    ];
    for (key, on) in expected {
        assert_eq!(get_toggle_enabled(&t, key), Some(on), "{key}");
    }
    assert_eq!(get_toggle_enabled(&t, "unknown"), None);
}

#[test]
fn set_toggle_enabled_agent_lessons_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "agent_lessons"), Some(false));

    let on = set_toggle_enabled(&t, "agent_lessons", true)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some for agent_lessons"))?;
    assert_eq!(get_toggle_enabled(&on, "agent_lessons"), Some(true));
    assert_eq!(get_toggle_enabled(&t, "agent_lessons"), Some(false));

    let off = set_toggle_enabled(&on, "agent_lessons", false)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some for agent_lessons"))?;
    assert_eq!(get_toggle_enabled(&off, "agent_lessons"), Some(false));
    assert!(!off.reminders);
    assert!(!off.steerer);
    Ok(())
}

#[test]
fn set_toggle_enabled_mailbox_redelivery_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "mailbox_redelivery"), Some(false));

    let on = set_toggle_enabled(&t, "mailbox_redelivery", true).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for mailbox_redelivery")
    })?;
    assert_eq!(get_toggle_enabled(&on, "mailbox_redelivery"), Some(true));
    assert_eq!(get_toggle_enabled(&t, "mailbox_redelivery"), Some(false));

    let off = set_toggle_enabled(&on, "mailbox_redelivery", false).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for mailbox_redelivery")
    })?;
    assert_eq!(get_toggle_enabled(&off, "mailbox_redelivery"), Some(false));
    assert!(!off.reminders);
    assert!(!off.steerer);
    Ok(())
}

#[test]
fn set_toggle_enabled_reminder_on_token_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "reminder_on_token"), Some(false));

    let on = set_toggle_enabled(&t, "reminder_on_token", true).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for reminder_on_token")
    })?;
    assert_eq!(get_toggle_enabled(&on, "reminder_on_token"), Some(true));
    assert_eq!(get_toggle_enabled(&t, "reminder_on_token"), Some(false));

    let off = set_toggle_enabled(&on, "reminder_on_token", false).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for reminder_on_token")
    })?;
    assert_eq!(get_toggle_enabled(&off, "reminder_on_token"), Some(false));
    assert!(!off.reminders);
    assert!(!off.steerer);
    Ok(())
}

#[test]
fn set_toggle_enabled_db_resync_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "db_resync"), Some(true));

    let off = set_toggle_enabled(&t, "db_resync", false)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some for db_resync"))?;
    assert_eq!(get_toggle_enabled(&off, "db_resync"), Some(false));
    assert_eq!(get_toggle_enabled(&t, "db_resync"), Some(true));

    let on = set_toggle_enabled(&off, "db_resync", true)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some for db_resync"))?;
    assert_eq!(get_toggle_enabled(&on, "db_resync"), Some(true));
    assert!(!on.steerer);
    assert!(on.lesson_worker);
    Ok(())
}

#[test]
fn set_toggle_enabled_file_read_compression_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "file_read_compression"), Some(false));

    let on = set_toggle_enabled(&t, "file_read_compression", true).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for file_read_compression")
    })?;
    assert_eq!(get_toggle_enabled(&on, "file_read_compression"), Some(true));
    assert_eq!(get_toggle_enabled(&t, "file_read_compression"), Some(false));

    let off = set_toggle_enabled(&on, "file_read_compression", false).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for file_read_compression")
    })?;
    assert_eq!(
        get_toggle_enabled(&off, "file_read_compression"),
        Some(false)
    );
    assert!(!off.reminders);
    assert!(!off.steerer);
    Ok(())
}

#[test]
fn set_toggle_enabled_produces_new_value() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    let updated = set_toggle_enabled(&t, "reminders", true)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some for a known key"))?;
    assert!(updated.reminders);
    assert!(!updated.recall_injector);
    assert!(!updated.steerer);
    assert!(!t.reminders);
    Ok(())
}

#[test]
fn set_toggle_enabled_unknown_key_returns_none() {
    let t = FeatureToggles::default();
    assert!(set_toggle_enabled(&t, "unknown_flag", true).is_none());
}

#[test]
fn toggle_meta_covers_all_keys() {
    let keys: Vec<&str> = TOGGLE_META.iter().map(|m| m.key).collect();
    assert_eq!(
        keys,
        vec![
            "agent_lessons",
            "change_record",
            "change_record_tail",
            "channel_link_sweep",
            "client_serves_agent_lessons",
            "client_serves_change_record_tail",
            "client_serves_git_read_hint",
            "client_serves_mail",
            "client_serves_memory_keeper",
            "client_serves_prompt_shaping",
            "client_serves_proxy_tools",
            "client_serves_recall",
            "client_serves_reminders",
            "client_serves_self_identity",
            "client_serves_weasel_guard",
            "client_serves_workspace_briefing",
            "context_focus_question",
            "db_resync",
            "directive_proposer",
            "dossier_builder",
            "file_read_compression",
            "fleet_auto_start",
            "harvester",
            "judge_lessons",
            "judge_worker",
            "launch_brief",
            "lesson_worker",
            "mailbox_redelivery",
            "memory_keeper",
            "mod_lesson_delivery",
            "native_peer_mail",
            "prompt_assistant",
            "proxy_tools",
            "recall_injector",
            "reminder_on_token",
            "reminders",
            "repeated_notices",
            "retention_sweep",
            "seat_memory_index",
            "semantic_context",
            "self_identity_reminder",
            "session_end_shadow",
            "session_judge",
            "shell_output_compression",
            "steerer",
            "turn_guard",
            "weasel_intercept",
            "workspace_briefing",
            "workspace_capture_registration"
        ]
    );
}

#[test]
fn live_disable_workers_re_read_their_own_toggle_every_iteration() {
    for key in [
        "judge_worker",
        "session_judge",
        "steerer",
        "lesson_worker",
        "channel_link_sweep",
        "memory_keeper",
        "seat_memory_index",
    ] {
        let meta = TOGGLE_META
            .iter()
            .find(|m| m.key == key)
            .unwrap_or_else(|| panic!("{key} missing from TOGGLE_META"));
        assert_eq!(
            meta.applies,
            ToggleApplies::LiveDisable,
            "{key} must report LiveDisable now that its loop re-reads the toggle"
        );
    }
}

#[test]
fn harvester_applies_immediately_in_both_directions() {
    let meta = TOGGLE_META
        .iter()
        .find(|m| m.key == "harvester")
        .unwrap_or_else(|| panic!("harvester missing from TOGGLE_META"));
    assert_eq!(meta.applies, ToggleApplies::Immediate);
}

fn overlay_with_mixed_toggles() -> FeatureToggles {
    FeatureToggles {
        reminders: false,
        recall_injector: true,
        weasel_intercept: false,
        reminder_on_token: true,
        mailbox_redelivery: true,
        steerer: true,
        lesson_worker: true,
        harvester: true,
        db_resync: true,
        judge_worker: true,
        session_judge: true,
        agent_lessons: true,
        file_read_compression: false,
        retention_sweep: true,
        channel_link_sweep: true,
        proxy_tools: true,
        workspace_capture_registration: false,
        self_identity_reminder: false,
        workspace_briefing: false,
        repeated_notices: false,
        launch_brief: false,
        native_peer_mail: false,
        mod_lesson_delivery: false,
        fleet_auto_start: true,
        prompt_assistant: true,
        dossier_builder: true,
        semantic_context: false,
        turn_guard: false,
        directive_proposer: false,
        session_end_shadow: false,
        judge_lessons: false,
        change_record: false,
        change_record_tail: true,
        ..FeatureToggles::default()
    }
}

fn settings_with_mixed_values() -> crate::settings::SettingsOverlay {
    crate::settings::SettingsOverlay {
        reminders_inject_lessons: false,
        reminders_max_lessons_to_inject: 7,
        weasel_intercept_forbidden_words: vec!["bad".to_owned()],
        weasel_intercept_correction_prompt: "correct!".to_owned(),
        recall_injector_max_lessons_per_topic: 4,
        file_read_compression_min_bytes: 4000,
        file_read_compression_head_lines: 10,
        file_read_compression_tail_lines: 5,
        file_read_compression_enabled_producers: vec!["Read".to_owned(), "Grep".to_owned()],
        file_read_compression_protected_paths: vec!["**/*.env".to_owned()],
        compression_dedup_enabled: false,
        ..crate::settings::SettingsOverlay::default()
    }
}

fn assert_resolved_flags(resolved: &ResolvedToggles) {
    assert!(!resolved.reminders);
    assert!(resolved.recall_injector);
    assert!(!resolved.weasel_intercept);
    assert!(resolved.agent_lessons);
    assert!(!resolved.file_read_compression);
    assert!(!resolved.workspace_capture_registration);
    assert!(!resolved.self_identity_reminder);
    assert!(!resolved.workspace_briefing);
    assert!(!resolved.reminders_inject_lessons);
    assert!(!resolved.compression_dedup_enabled);
}

fn assert_resolved_settings(resolved: &ResolvedToggles) {
    assert_eq!(resolved.reminders_max_lessons_to_inject, 7);
    assert_eq!(resolved.weasel_intercept_forbidden_words, vec!["bad"]);
    assert_eq!(resolved.weasel_intercept_correction_prompt, "correct!");
    assert_eq!(resolved.recall_injector_max_lessons_per_topic, 4);
    assert_eq!(resolved.file_read_compression_min_bytes, 4000);
    assert_eq!(resolved.file_read_compression_head_lines, 10);
    assert_eq!(resolved.file_read_compression_tail_lines, 5);
    assert_eq!(
        resolved.file_read_compression_enabled_producers,
        vec!["Read".to_owned(), "Grep".to_owned()]
    );
    assert_eq!(
        resolved.file_read_compression_protected_paths,
        vec!["**/*.env".to_owned()]
    );
}

#[test]
fn resolved_toggles_from_overlay() {
    let overlay = overlay_with_mixed_toggles();
    let settings = settings_with_mixed_values();
    let resolved = ResolvedToggles::from_overlay(&overlay, &settings);
    assert_resolved_flags(&resolved);
    assert_resolved_settings(&resolved);
}
