use super::*;

#[test]
fn settings_meta_keys_are_unique() {
    let mut keys: Vec<&str> = SETTINGS_META.iter().map(|m| m.key).collect();
    let original_len = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        original_len,
        "SETTINGS_META must have unique keys"
    );
}

#[test]
fn settings_meta_enum_variants_non_empty() {
    for meta in SETTINGS_META.iter() {
        if meta.kind == SettingKind::Enum {
            let variants = meta.enum_variants.unwrap_or(&[]);
            assert!(
                !variants.is_empty(),
                "setting '{}' of Enum kind must have non-empty enum_variants",
                meta.key
            );
        }
    }
}

#[test]
fn settings_overlay_defaults_match_toggle_off_by_default_contract() {
    let overlay = SettingsOverlay::default();
    assert!(
        !overlay.reminders_inject_lessons,
        "reminders.inject_lessons must default false"
    );
    assert!(
        !overlay.compression_dedup_enabled,
        "compression_dedup.enabled must default false"
    );
    assert!(
        !overlay.file_read_compression_protected_paths.is_empty(),
        "file_read_compression.protected_paths must not default empty"
    );
}

#[test]
fn weasel_correction_prompt_default_matches_compiled_and_catalogue() {
    let overlay_default = SettingsOverlay::default().weasel_intercept_correction_prompt;
    let compiled_default =
        crate::config::weasel_intercept::WeaselInterceptConfig::default().correction_prompt;
    assert_eq!(
        overlay_default, compiled_default,
        "SettingsOverlay::default must mirror WeaselInterceptConfig::default"
    );

    let Some(meta) = SETTINGS_META
        .iter()
        .find(|m| m.key == "weasel_intercept.correction_prompt")
    else {
        panic!("weasel_intercept.correction_prompt must be registered in SETTINGS_META");
    };
    let Some(meta_default) = meta.default.as_str() else {
        panic!("weasel_intercept.correction_prompt default must be a string");
    };
    assert_eq!(
        overlay_default, meta_default,
        "SETTINGS_META's declared default must mirror the compiled default"
    );
}

#[test]
fn settings_meta_text_kind_defaults_are_never_blank() {
    for meta in SETTINGS_META.iter() {
        if meta.kind != SettingKind::Text {
            continue;
        }
        let default = meta.default.as_str().unwrap_or_else(|| {
            panic!(
                "setting '{}' of Text kind must have a string default",
                meta.key
            )
        });
        assert!(
            !default.trim().is_empty(),
            "setting '{}' of Text kind must not default to a blank prompt",
            meta.key
        );
    }
}

#[test]
fn settings_overlay_default_matches_file_read_compression_compiled_defaults() {
    let overlay = SettingsOverlay::default();
    assert_eq!(overlay.file_read_compression_min_bytes, 1500);
    assert_eq!(overlay.file_read_compression_head_lines, 40);
    assert_eq!(overlay.file_read_compression_tail_lines, 20);
    assert_eq!(
        overlay.file_read_compression_enabled_producers,
        vec!["Read".to_owned()]
    );
    assert_eq!(
        overlay.file_read_compression_protected_paths,
        vec![
            "**/CLAUDE.md".to_owned(),
            "**/BRIEF*.md".to_owned(),
            "**/.claude/**".to_owned(),
            "**/docs/plans/**".to_owned(),
            "**/artifacts/threads/**".to_owned(),
            "**/*.rules.md".to_owned(),
        ]
    );
}

#[test]
fn settings_meta_declares_no_secret_shaped_key() {
    for meta in SETTINGS_META.iter() {
        assert!(
            !service::is_secret_setting_key(meta.key),
            "setting '{}' looks like a secret; secrets live in the environment and only a `_env` variable name may be a setting",
            meta.key
        );
    }
}

fn meta_default_string(key: &str) -> String {
    let Some(meta) = SETTINGS_META.iter().find(|m| m.key == key) else {
        panic!("{key} must be registered in SETTINGS_META");
    };
    let Some(value) = meta.default.as_str() else {
        panic!("{key} default must be a string");
    };
    value.to_owned()
}

#[test]
fn model_settings_registry_defaults_match_compiled_defaults() {
    let steerer = crate::config::steerer::SteererConfig::default();
    let lesson_relations = crate::config::lesson_relations::LessonRelationsConfig::default();
    let session_judge = crate::config::session_judge::SessionJudgeConfig::default();

    assert_eq!(meta_default_string("steerer.model"), steerer.model);
    assert_eq!(
        Some(meta_default_string("judge_worker.model")),
        steerer.judge.model
    );
    assert_eq!(
        meta_default_string("session_judge.model"),
        session_judge.model
    );
    assert_eq!(
        meta_default_string("lesson_relations.proposer_model"),
        lesson_relations.proposer_model
    );
    assert_eq!(
        meta_default_string("lesson_relations.adjudicator_model"),
        lesson_relations.adjudicator_model
    );
}

#[test]
fn judge_worker_reasoning_effort_registry_default_matches_overlay_default() {
    assert_eq!(
        meta_default_string("judge_worker.reasoning_effort"),
        SettingsOverlay::default().judge_worker_reasoning_effort
    );
}

async fn machine_settings_service(
    url: &str,
) -> anyhow::Result<std::sync::Arc<service::SettingsService>> {
    let handle = crate::store::persistence::PersistHandle::connect(url).await?;
    let overlay = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(SettingsOverlay::default()));
    let write_lock =
        std::sync::Arc::new(tokio::sync::Mutex::new(crate::config::SettingsWriteGuard));
    service::SettingsService::reconcile_on_startup(overlay, write_lock, handle).await
}

#[tokio::test]
async fn machine_scoped_launch_backend_each_machine_reads_its_own_and_the_rest_the_shared_default()
-> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let svc = machine_settings_service(&db.database_url).await?;
    let source = crate::change_log::ChangeSource::internal();
    let (machine_a, machine_b, machine_c) = (
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    svc.persist
        .upsert_setting("launch_backend", &serde_json::json!("herdr"))
        .await?;
    svc.set_for_machine_with_source(
        "launch_backend",
        serde_json::json!("tmux"),
        Some(machine_a),
        &source,
    )
    .await?;
    svc.set_for_machine_with_source(
        "launch_backend",
        serde_json::json!("herdr-core"),
        Some(machine_b),
        &source,
    )
    .await?;

    let read = |rows: Vec<(String, serde_json::Value)>| {
        rows.into_iter()
            .find(|(k, _)| k == "launch_backend")
            .map(|(_, v)| v)
    };
    for (machine, expected, expected_source) in [
        (machine_a, "tmux", ValueSource::Own),
        (machine_b, "herdr-core", ValueSource::Own),
        (machine_c, "herdr", ValueSource::SharedDefault),
    ] {
        let resolved = read(
            svc.persist
                .fetch_settings_for_machine(Some(machine))
                .await?,
        );
        assert_eq!(resolved, Some(serde_json::json!(expected)), "{machine}");
        let view = svc.machine_view(Some(machine)).await?;
        assert_eq!(
            view.value("launch_backend"),
            Some(&serde_json::json!(expected))
        );
        assert_eq!(view.source("launch_backend"), expected_source, "{machine}");
    }
    assert_eq!(
        read(svc.persist.fetch_shared_settings().await?),
        Some(serde_json::json!("herdr")),
        "a machine's own write never changes the shared row"
    );

    let (cleared_for, cleared) = svc
        .clear_machine_value("launch_backend", Some(machine_a), &source)
        .await?;
    assert_eq!((cleared_for, cleared), (machine_a, true));
    let view = svc.machine_view(Some(machine_a)).await?;
    assert_eq!(view.source("launch_backend"), ValueSource::SharedDefault);
    assert_eq!(
        view.value("launch_backend"),
        Some(&serde_json::json!("herdr"))
    );

    db.cleanup().await
}

#[tokio::test]
async fn shared_setting_refuses_a_named_machine_and_clear_refuses_a_shared_key()
-> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let svc = machine_settings_service(&db.database_url).await?;
    let source = crate::change_log::ChangeSource::internal();
    let named = svc
        .set_for_machine_with_source(
            "launch_max_surfaces",
            serde_json::json!(4),
            Some(uuid::Uuid::new_v4()),
            &source,
        )
        .await;
    assert!(
        matches!(&named, Err(service::SettingsServiceError::InvalidValue { reason, .. }) if reason.contains("shared by every machine")),
        "{named:?}"
    );
    let cleared = svc
        .clear_machine_value("launch_max_surfaces", Some(uuid::Uuid::new_v4()), &source)
        .await;
    assert!(
        matches!(&cleared, Err(service::SettingsServiceError::InvalidValue { reason, .. }) if reason.contains("machine-scoped")),
        "{cleared:?}"
    );
    db.cleanup().await
}

#[tokio::test]
async fn existing_shared_rows_stay_the_value_every_machine_reads_after_the_upgrade()
-> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let svc = machine_settings_service(&db.database_url).await?;
    svc.persist
        .upsert_setting("launch_claude_args", &serde_json::json!(["--chrome"]))
        .await?;
    for machine in [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()] {
        let view = svc.machine_view(Some(machine)).await?;
        assert_eq!(
            view.value("launch_claude_args"),
            Some(&serde_json::json!(["--chrome"]))
        );
        assert_eq!(
            view.source("launch_claude_args"),
            ValueSource::SharedDefault
        );
        assert_eq!(view.source("launch_ca_cert"), ValueSource::CompiledDefault);
    }
    db.cleanup().await
}

#[tokio::test]
async fn repo_location_on_machine_finds_own_root_another_machines_path_and_no_location()
-> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let svc = machine_settings_service(&db.database_url).await?;
    let pool = svc.persist.pool();
    let linux_root = format!("/mnt/data/projects/repo-{}", uuid::Uuid::new_v4());
    let mac_root = format!("/Users/dawid/projects/repo-{}", uuid::Uuid::new_v4());
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1) RETURNING id",
    )
    .bind(&linux_root)
    .fetch_one(pool)
    .await?;
    let (linux, mac, bare) = (
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    for (machine, root) in [(linux, &linux_root), (mac, &mac_root)] {
        sqlx::query("INSERT INTO projects.machines (machine_uuid, name) VALUES ($1, 'm')")
            .bind(machine)
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO projects.locations (project_id, machine_uuid, root_path) VALUES ($1, $2, $3)",
        )
        .bind(project_id)
        .bind(machine)
        .bind(root)
        .execute(pool)
        .await?;
    }
    let found = |machine| svc.persist.repo_location_on_machine(&linux_root, machine);
    let own = found(linux).await?;
    assert_eq!((own.known, own.own_root), (true, Some(linux_root.clone())));
    let other = found(mac).await?;
    assert_eq!(
        (other.known, other.own_root),
        (true, Some(mac_root.clone()))
    );
    let none = found(bare).await?;
    assert_eq!((none.known, none.own_root), (true, None));
    let unknown = svc
        .persist
        .repo_location_on_machine("/nowhere/at/all", linux)
        .await?;
    assert_eq!((unknown.known, unknown.own_root), (false, None));
    db.cleanup().await
}
