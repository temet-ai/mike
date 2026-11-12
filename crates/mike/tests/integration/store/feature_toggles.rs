use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

#[tokio::test]
async fn fetch_feature_toggles_returns_all_operator_set_rows() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let toggles = handle.fetch_feature_toggles().await?;

    assert_eq!(toggles.len(), 25);
    let not_operator_set_but_enabled = ["prompt_assistant", "semantic_context"];
    let fresh_install_off = [
        "agent_lessons",
        "directive_proposer",
        "dossier_builder",
        "file_read_compression",
        "judge_lessons",
        "judge_worker",
        "mailbox_redelivery",
        "recall_injector",
        "reminder_on_token",
        "reminders",
        "self_identity_reminder",
        "session_judge",
        "steerer",
        "weasel_intercept",
        "workspace_briefing",
        "wright",
    ];
    for (key, enabled, operator_set) in &toggles {
        if not_operator_set_but_enabled.contains(&key.as_str()) {
            assert!(
                !operator_set && *enabled,
                "toggle '{key}' must be enabled by its own migration but not \
                 operator_set, so the compiled default keeps governing it"
            );
            continue;
        }
        if fresh_install_off.contains(&key.as_str()) {
            assert!(
                !operator_set && !enabled,
                "toggle '{key}' must be off and not operator_set on a fresh install"
            );
            continue;
        }
        assert!(
            operator_set,
            "toggle '{key}' must be operator_set after migration backfill"
        );
        assert_eq!(*enabled, key != "proxy_tools", "toggle '{key}'");
    }

    let mut keys: Vec<&str> = toggles.iter().map(|(k, _, _)| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "agent_lessons",
            "channel_link_sweep",
            "db_resync",
            "directive_proposer",
            "dossier_builder",
            "file_read_compression",
            "harvester",
            "judge_lessons",
            "judge_worker",
            "lesson_worker",
            "mailbox_redelivery",
            "prompt_assistant",
            "proxy_tools",
            "recall_injector",
            "reminder_on_token",
            "reminders",
            "retention_sweep",
            "self_identity_reminder",
            "semantic_context",
            "session_judge",
            "steerer",
            "weasel_intercept",
            "workspace_briefing",
            "workspace_capture_registration",
            "wright",
        ]
    );

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn upsert_feature_toggle_round_trips_sets_operator_set_and_rejects_unknown_key()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    handle.upsert_feature_toggle("steerer", false).await?;
    let toggles = handle.fetch_feature_toggles().await?;
    let (steerer_enabled, steerer_operator_set) = toggles
        .iter()
        .find(|(k, _, _)| k == "steerer")
        .map(|(_, e, o)| (*e, *o))
        .ok_or_else(|| anyhow::anyhow!("steerer toggle must exist in DB after upsert"))?;
    assert!(!steerer_enabled);
    assert!(steerer_operator_set);

    handle.upsert_feature_toggle("steerer", true).await?;
    let toggles = handle.fetch_feature_toggles().await?;
    let steerer_enabled = toggles
        .iter()
        .find(|(k, _, _)| k == "steerer")
        .map(|(_, e, _)| *e)
        .ok_or_else(|| anyhow::anyhow!("steerer toggle must exist in DB after re-enable"))?;
    assert!(steerer_enabled);

    let result = handle.upsert_feature_toggle("no_such_feature", true).await;
    assert!(result.is_err());

    handle_db.cleanup().await?;
    Ok(())
}
