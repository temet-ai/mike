use super::*;

#[tokio::test]
async fn a_legacy_source_only_event_does_not_establish_execution_policy() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let fleet_db = persist.fleet();
    let launch_db = persist.launch();
    let (_dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(52);
    let mut value = serde_json::to_value(scratch_def(&repo))?;
    value["check"] = json!({"command": "true", "paths": ["mine"]});
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    launch_db
        .ensure_session_exists(&session.to_string())
        .await?;
    let identity = persist
        .session_identity_by_uuid(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session has no identity"))?;
    let head = git(&repo, &["rev-parse", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("rev-parse"))?;
    let head = String::from_utf8_lossy(&head).trim().to_string();
    fleet_db.insert_change_events(&[crate::change_record::NewChangeEvent {
            project_id: identity.project_id,
            checkout: repo.clone(),
            at: chrono::Utc::now(),
            kind: crate::change_record::ChangeKind::Check,
            session_id: None,
            run_id: None,
            task: None,
            paths: Vec::new(),
            subject: "check passed".to_owned(),
            detail: json!({"passed": true, "source_verified": true, "proof_version": 1, "command": "true"}),
            before_sha: None,
            after_sha: Some(head),
        }])
        .await?;
    launch_db
        .upsert_run(&NewRun {
            run_id: "fleet-no-tree-check".into(),
            project_path: Some(repo.clone()),
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let verifier = Verifier {
        persist: Some(&persist),
        run: "fleet-no-tree-check",
    };
    let mut request = ask(session, &[]);
    request.allow_empty = true;
    let landed = commit(&def, &scratch_seated(session), &request, verifier).await;
    assert_eq!(landed.refused, None, "{landed:?}");
    assert!(
        landed.verified.starts_with("not yet proven;"),
        "{}",
        landed.verified
    );
    let events = fleet_db
        .fleet_events_since("fleet-no-tree-check", 0)
        .await?;
    assert!(
        events.iter().any(|event| {
            event.kind == crate::launch::fleet::events::EventKind::CommitLanded
                && event.task.as_deref() == Some("M")
                && event.session_uuid == Some(session)
                && event.payload.get("sha").and_then(|v| v.as_str()) == landed.sha.as_deref()
        }),
        "{events:?}"
    );
    db.cleanup().await?;
    Ok(())
}
