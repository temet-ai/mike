use super::*;
use crate::launch::fleet::tests::changed;
use serde_json::json;

fn def() -> FleetDefinition {
    changed(json!({"tasks": {
        "T1": {"seat": "core-T1", "session": Uuid::from_u128(1).to_string(), "files": ["crates/core"], "stub_by": 3},
        "T2": {"seat": "ui-T2", "session": Uuid::from_u128(2).to_string(), "wait_for_commits": [{"task": "T1", "paths": ["crates/core/api.rs"]}]}
    }}))
}

fn event(path: &str) -> Committed {
    Committed {
        sha: "a".repeat(40),
        seat: "core-T1".into(),
        session: Uuid::from_u128(1),
        paths: vec![path.into()],
        after: Vec::new(),
    }
}

fn status() -> FleetStatus {
    serde_json::from_value(json!({
        "fleet": "f", "slug": "s", "state": "running", "run_id": "r", "goal": "g",
        "started_at": "2026-10-01T00:00:00Z", "now": "2026-10-01T00:03:00Z",
        "missing": [], "criteria": [], "tasks": [], "verdicts": [], "blocked": [],
        "seats": [{"seat": "core-T1", "role": "worker", "model": "opus", "state": "working",
            "context_tokens": null, "requests": 0, "last_request_at": null,
            "opened_at": "2026-10-01T00:00:00Z", "closed": false}]
    }))
    .unwrap_or_else(|err| panic!("invalid fixture: {err}"))
}

#[test]
fn explicit_deadline_is_due_at_the_minute_and_suppressed_by_owned_commit() {
    let def = def();
    let mut state = status();
    state.now -= chrono::Duration::seconds(1);
    assert!(notices(&def, &state, &[]).is_empty());
    state.now += chrono::Duration::seconds(1);
    assert_eq!(notices(&def, &state, &[]).len(), 1);
    let sent = notices(&def, &state, &[event("crates/core/api.rs")]);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, Uuid::from_u128(2));
    assert!(
        sent[0]
            .body
            .contains("not that compilation or tests passed")
    );
}

#[test]
fn session_and_owned_path_are_required_not_a_command_or_other_runs_seat_name() {
    let def = def();
    let mut commit = event("crates/core/api.rs");
    commit.session = Uuid::from_u128(999);
    assert!(!belongs(&commit, &def.tasks["T1"]));
    assert!(!belongs(&event("crates/core-old/api.rs"), &def.tasks["T1"]));
    assert!(
        matching(
            &[event("crates/core/api.rs.bak")],
            &def.tasks["T1"],
            &def.tasks["T2"].wait_for_commits[0]
        )
        .is_none()
    );
}

#[test]
fn explicit_anchor_requires_descendant_evidence() {
    let def = def();
    let owner = &def.tasks["T1"];
    let mut wait = def.tasks["T2"].wait_for_commits[0].clone();
    wait.after = Some("b".repeat(40));
    let mut commit = event("crates/core/api.rs");
    assert!(matching(&[event("crates/core/api.rs")], owner, &wait).is_none());
    commit.after.push("b".repeat(40));
    assert!(matching(&[commit], owner, &wait).is_some());
}

#[test]
fn subscription_survives_serialization_and_notice_key_is_stable() {
    let original = def();
    let restored = FleetDefinition::from_value(
        &serde_json::to_value(&original).unwrap_or_else(|err| panic!("invalid fixture: {err}")),
    )
    .unwrap_or_else(|err| panic!("invalid fixture: {err}"));
    let events = [event("crates/core/api.rs")];
    assert_eq!(
        notices(&original, &status(), &events),
        notices(&restored, &status(), &events)
    );
    let mut newer = event("crates/core/api.rs");
    newer.sha = "c".repeat(40);
    assert_eq!(
        notices(&restored, &status(), &events)[0].key,
        notices(&restored, &status(), &[newer])[0].key
    );
}

#[test]
fn invalid_definitions_are_rejected_and_empty_paths_mean_first_owned_commit() {
    let mut def = def();
    assert!(validate(&def).is_empty());
    let task = def
        .tasks
        .get_mut("T2")
        .unwrap_or_else(|| panic!("missing T2"));
    task.stub_by = Some(0);
    task.wait_for_commits[0] = CommitWait {
        task: "unknown".into(),
        paths: vec!["../secret".into()],
        after: Some("--all".into()),
    };
    assert_eq!(validate(&def).len(), 4);
    let wait = CommitWait {
        task: "T1".into(),
        ..CommitWait::default()
    };
    assert!(matching(&[event("crates/core/other.rs")], &def.tasks["T1"], &wait).is_some());
}

#[test]
fn trailer_requires_exact_session_identity() {
    assert_eq!(
        identity(&format!("core-T1 ({})", Uuid::from_u128(1))),
        Some(("core-T1".into(), Uuid::from_u128(1)))
    );
    assert!(identity("core-T1").is_none());
    assert!(
        identity(&format!(
            "core-T1 ({})\nother ({})",
            Uuid::from_u128(1),
            Uuid::from_u128(2)
        ))
        .is_none()
    );
}

#[tokio::test]
async fn committed_paths_not_uncommitted_files_and_nonancestor_is_refused() -> anyhow::Result<()> {
    let (dir, repo) = crate::launch::fleet::commit_tests::scratch_repo().await?;
    let mut definition = def();
    definition.repo = repo.clone();
    let since = Some(Utc::now() - chrono::Duration::minutes(1));
    std::fs::create_dir_all(dir.path().join("crates/core"))?;
    std::fs::write(dir.path().join("crates/core/api.rs"), "stub")?;
    assert!(
        read(&definition, since)
            .await
            .ok_or_else(|| anyhow::anyhow!("read"))?
            .iter()
            .all(|e| !belongs(e, &definition.tasks["T1"]))
    );
    let base = git(&repo, &["rev-parse", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("base"))?;
    let base = String::from_utf8(base)?.trim().to_string();
    git(&repo, &["add", "crates/core/api.rs"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("add"))?;
    let message = format!("stub\n\nFleet-Seat: core-T1 ({})", Uuid::from_u128(1));
    git(&repo, &["commit", "-qm", &message], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("commit"))?;
    definition
        .tasks
        .get_mut("T2")
        .ok_or_else(|| anyhow::anyhow!("T2"))?
        .wait_for_commits[0]
        .after = Some(base);
    let events = read(&definition, since)
        .await
        .ok_or_else(|| anyhow::anyhow!("events"))?;
    assert!(
        matching(
            &events,
            &definition.tasks["T1"],
            &definition.tasks["T2"].wait_for_commits[0]
        )
        .is_some()
    );
    definition
        .tasks
        .get_mut("T2")
        .ok_or_else(|| anyhow::anyhow!("T2"))?
        .wait_for_commits[0]
        .after = Some("f".repeat(40));
    assert!(read(&definition, since).await.is_none());
    Ok(())
}

#[test]
fn briefing_explains_deadline_and_commit_only_signal() {
    let definition = def();
    assert!(brief(&definition.tasks["T1"]).contains("within 3 minutes"));
    assert!(brief(&definition.tasks["T2"]).contains("not compilation or test success"));
}
