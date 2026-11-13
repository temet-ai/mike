#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::json;
use uuid::Uuid;

use super::{NO_DB, scratch_def, scratch_repo, scratch_seated};
use crate::launch::fleet::check::{state_for, state_of};
use crate::launch::fleet::commit::{CommitRequest, commit};
use crate::launch::fleet::commit_verify::Verifier;
use crate::launch::fleet::definition::{FleetDefinition, merge};
use crate::launch::fleet::plan_tests::seats;
use crate::launch::fleet::start::control_patch;
use crate::launch::fleet::watch::git;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::session_notes::NewSessionNote;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn ask(session: Uuid, paths: &[&str]) -> CommitRequest {
    CommitRequest {
        session,
        message: "feat: slice".into(),
        paths: paths.iter().map(|p| (*p).to_string()).collect(),
        allow_empty: false,
    }
}

#[tokio::test]
async fn a_commit_of_two_files_is_checked_on_those_two_and_not_on_ten_unformatted_peers()
-> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(21);
    let mut value = serde_json::to_value(scratch_def(&repo))?;
    value["commit_check"] = json!([
        {"command": "! grep -l '  $'", "files": [".txt"]},
        {"command": "sh -c 'test $# -le 2' _", "files": [".txt"]}
    ]);
    let def = crate::launch::fleet::definition::FleetDefinition::from_value(&value)
        .map_err(anyhow::Error::msg)?;
    for n in 0..10 {
        std::fs::write(dir.path().join(format!("theirs/peer{n}.txt")), "untidy  \n")?;
    }
    std::fs::write(dir.path().join("mine/a.txt"), "tidy\n")?;
    std::fs::write(dir.path().join("mine/b.txt"), "tidy\n")?;
    let report = commit(&def, &scratch_seated(session), &ask(session, &[]), NO_DB).await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths, vec!["mine/a.txt", "mine/b.txt"]);
    assert!(
        report.warnings.is_empty(),
        "other seats' unformatted files are not this commit's warning: {:?}",
        report.warnings
    );
    assert!(
        report
            .verified
            .contains("commit checks passed on the committed files")
            && report.verified.contains("! grep -l")
            && report.verified.contains("-le 2"),
        "{}",
        report.verified
    );
    let trailer = String::from_utf8(
        git(&repo, &["log", "-1", "--format=%B"], None)
            .await
            .unwrap(),
    )?;
    assert!(
        trailer.contains("Fleet-Verified:") && trailer.contains("-le 2"),
        "{trailer}"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_commit_check_that_fails_names_itself_and_only_its_own_files() -> anyhow::Result<()>
{
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(22);
    let mut value = serde_json::to_value(scratch_def(&repo))?;
    value["commit_check"] = json!([
        {"command": "! grep -l '  $'", "files": [".txt"]},
        {"command": "sh -c 'test $# -le 1' _", "files": [".txt"]}
    ]);
    let def = crate::launch::fleet::definition::FleetDefinition::from_value(&value)
        .map_err(anyhow::Error::msg)?;
    std::fs::write(dir.path().join("mine/a.txt"), "tidy\n")?;
    std::fs::write(dir.path().join("mine/b.txt"), "tidy\n")?;
    let report = commit(&def, &scratch_seated(session), &ask(session, &[]), NO_DB).await;
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].starts_with("`sh -c 'test $# -le 1' _` does not pass"));
    assert!(report.verified.contains("! grep -l") && !report.verified.contains("-le 1"));
    Ok(())
}

#[tokio::test]
async fn a_named_untracked_directory_of_eight_files_is_committed() -> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(23);
    let def = scratch_def(&repo);
    std::fs::create_dir_all(dir.path().join("mine/shot/test"))?;
    for name in ["README.md", "pixels.py", "shot", "c4", "steps"] {
        std::fs::write(dir.path().join("mine/shot").join(name), "x\n")?;
    }
    for name in ["one", "two", "three"] {
        std::fs::write(dir.path().join("mine/shot/test").join(name), "x\n")?;
    }
    let report = commit(
        &def,
        &scratch_seated(session),
        &ask(session, &["mine/shot/"]),
        NO_DB,
    )
    .await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths.len(), 8, "{:?}", report.paths);
    assert!(report.sha.is_some());
    Ok(())
}

async fn mails_to_acceptance(persist: &PersistHandle) -> anyhow::Result<usize> {
    Ok(persist
        .launch()
        .fetch_for_session_with_activity(Uuid::from_u128(2))
        .await?
        .inbox
        .len())
}

#[tokio::test]
async fn a_commit_after_a_verdict_on_the_task_mails_acceptance_once_and_one_without_does_not()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let launch_db = persist.launch();
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(31);
    launch_db
        .ensure_session_exists(&session.to_string())
        .await?;
    launch_db
        .ensure_session_exists(&Uuid::from_u128(2).to_string())
        .await?;
    let mut value = serde_json::to_value(scratch_def(&repo))?;
    merge(&mut value, &control_patch(&seats()));
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    let verifier = Verifier {
        persist: Some(&persist),
        run: "fleet-test",
    };

    std::fs::write(dir.path().join("mine/a.txt"), "two\n")?;
    let before_verdict = commit(&def, &scratch_seated(session), &ask(session, &[]), verifier).await;
    assert_eq!(before_verdict.refused, None, "{before_verdict:?}");
    assert_eq!(mails_to_acceptance(&persist).await?, 0, "no verdict yet");

    let data = json!({"verdict": "PASS", "tasks": ["M"]});
    launch_db
        .create_session_note(
            Uuid::from_u128(2),
            NewSessionNote {
                kind: Some("decision"),
                title: "PASS M slice 1",
                body: None,
                status: None,
                tags: None,
                data: Some(&data),
            },
        )
        .await?;
    std::fs::write(dir.path().join("mine/a.txt"), "three\n")?;
    let after_verdict = commit(&def, &scratch_seated(session), &ask(session, &[]), verifier).await;
    assert_eq!(after_verdict.refused, None, "{after_verdict:?}");
    assert_eq!(mails_to_acceptance(&persist).await?, 1);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_commit_asked_while_the_shared_check_runs_lands_at_once_and_says_no_check_finished()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let fleet_db = persist.fleet();
    let launch_db = persist.launch();
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(41);
    let mut value = serde_json::to_value(scratch_def(&repo))?;
    value["check"] = json!({"command": "sleep 600", "paths": ["mine"]});
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    launch_db
        .upsert_run(&NewRun {
            run_id: "fleet-running-check".to_owned(),
            project_path: Some("/tmp/running-check".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    std::fs::write(dir.path().join("mine/a.txt"), "two\n")?;
    let state = state_for(&def, "HEAD").await.map_err(anyhow::Error::msg)?;
    fleet_db
        .begin_tree_check("fleet-running-check", &state, "sleep 600", "db-dev-M")
        .await?;
    let verifier = Verifier {
        persist: Some(&persist),
        run: "fleet-running-check",
    };
    let landed = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        commit(&def, &scratch_seated(session), &ask(session, &[]), verifier),
    )
    .await
    .map_err(|_| anyhow::anyhow!("a commit waited on the running check"))?;
    assert_eq!(landed.refused, None, "{landed:?}");
    assert!(landed.sha.is_some());
    assert!(
        landed.verified.starts_with("not yet proven;"),
        "{}",
        landed.verified
    );
    db.cleanup().await?;
    Ok(())
}

async fn state_with(repo: &str, def: &FleetDefinition, path: &str) -> anyhow::Result<String> {
    let listed = git(repo, &["ls-tree", "-r", "-z", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("ls-tree"))?;
    let mut files = crate::launch::fleet::check_state::state_tree(&listed);
    let blob = git(repo, &["hash-object", path], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("hash-object"))?;
    let prefix = files
        .get(path)
        .and_then(|entry| entry.rsplit_once(' ').map(|(meta, _)| meta))
        .unwrap_or("100644 blob");
    let updated = format!("{prefix} {}", String::from_utf8_lossy(&blob).trim());
    files.insert(path.to_string(), updated);
    let policy = serde_json::to_string(&(
        def.execution.environment_policy(),
        def.execution.operation(&def.check.command),
    ))?;
    state_of(&files, &def.check.covered())
        .map(|state| crate::launch::fleet::check_state::command_state(&state, &def.check.command))
        .map(|state| crate::launch::fleet::check_state::command_state(&state, &policy))
        .ok_or_else(|| anyhow::anyhow!("no state"))
}

#[tokio::test]
async fn a_commit_whose_content_already_passed_the_check_is_proven_whatever_a_peer_left_uncommitted()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let fleet_db = persist.fleet();
    let launch_db = persist.launch();
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(42);
    let mut value = serde_json::to_value(scratch_def(&repo))?;
    value["check"] = json!({"command": "true", "paths": ["mine", "theirs"]});
    value["execution"] = json!({"operations": {"check": {"command": "true", "purpose": "fixture", "inputs": ["mine", "theirs"], "cache_key": "fixture-v1"}}});
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    launch_db
        .upsert_run(&NewRun {
            run_id: "fleet-proven".to_owned(),
            project_path: Some("/tmp/proven".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    std::fs::write(dir.path().join("mine/a.txt"), "two\n")?;
    std::fs::write(
        dir.path().join("theirs/b.txt"),
        "half a line of a peer's edit",
    )?;
    let state = state_with(&repo, &def, "mine/a.txt").await?;
    let id = fleet_db
        .begin_tree_check("fleet-proven", &state, "true", "mike")
        .await?;
    fleet_db
        .finish_tree_check(id, Some(0), "", Some(&state))
        .await?;
    let verifier = Verifier {
        persist: Some(&persist),
        run: "fleet-proven",
    };
    let landed = commit(&def, &scratch_seated(session), &ask(session, &[]), verifier).await;
    assert_eq!(landed.refused, None, "{landed:?}");
    assert!(
        landed.verified.starts_with("proven:"),
        "{}",
        landed.verified
    );
    let committed = state_for(&def, "HEAD").await.map_err(anyhow::Error::msg)?;
    assert_eq!(
        committed, state,
        "the evidence is the state of the commit itself"
    );
    let message = git(&repo, &["log", "-1", "--format=%B"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("log"))?;
    assert!(String::from_utf8_lossy(&message).contains("Fleet-Verified: proven:"));
    db.cleanup().await?;
    Ok(())
}

async fn recorded_edits(
    persist: &PersistHandle,
    session: Uuid,
    repo: &str,
    paths: &[&str],
) -> anyhow::Result<()> {
    persist
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    let identity = persist
        .session_identity_by_uuid(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session has no identity"))?;
    let event = crate::change_record::NewChangeEvent {
        project_id: identity.project_id,
        checkout: repo.to_owned(),
        at: chrono::Utc::now(),
        kind: crate::change_record::ChangeKind::Edit,
        session_id: Some(identity.id),
        run_id: None,
        task: None,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        subject: "edit".to_owned(),
        detail: json!({}),
        before_sha: None,
        after_sha: None,
    };
    persist.fleet().insert_change_events(&[event]).await?;
    Ok(())
}

async fn commit_with_the_record(
    record_on: bool,
) -> anyhow::Result<Option<crate::launch::fleet::commit::CommitReport>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    persist
        .upsert_feature_toggle("change_record", record_on)
        .await?;
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(51);
    let def = scratch_def(&repo);
    recorded_edits(&persist, session, &repo, &["mine/a.txt", "theirs/b.txt"]).await?;
    std::fs::write(dir.path().join("mine/a.txt"), "two\n")?;
    std::fs::write(dir.path().join("mine/unrecorded.txt"), "x\n")?;
    std::fs::write(dir.path().join("theirs/b.txt"), "peer\n")?;
    let verifier = Verifier {
        persist: Some(&persist),
        run: "fleet-record",
    };
    let report = commit(&def, &scratch_seated(session), &ask(session, &[]), verifier).await;
    if record_on {
        let foreign = git(&repo, &["show", "HEAD:theirs/b.txt"], None)
            .await
            .ok_or_else(|| anyhow::anyhow!("read committed foreign file"))?;
        assert_ne!(String::from_utf8_lossy(&foreign), "peer\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("theirs/b.txt"))?,
            "peer\n"
        );
    }
    db.cleanup().await?;
    Ok(Some(report))
}

#[tokio::test]
async fn a_commit_with_no_paths_excludes_foreign_recorded_paths_when_the_record_is_on()
-> anyhow::Result<()> {
    let Some(report) = commit_with_the_record(true).await? else {
        return Ok(());
    };
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths_from, "record");
    assert_eq!(report.paths, vec!["mine/a.txt"]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("`theirs/b.txt` belongs to another task and is left uncommitted")),
        "{:?}",
        report.warnings
    );
    assert!(
        !report.pushed && report.push_error.as_deref().is_some_and(|w| !w.is_empty()),
        "a failed push always carries git's words: {report:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_commit_with_no_paths_takes_the_tasks_files_when_the_record_is_off() -> anyhow::Result<()>
{
    let Some(report) = commit_with_the_record(false).await? else {
        return Ok(());
    };
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths_from, "task");
    assert_eq!(report.paths, vec!["mine/a.txt", "mine/unrecorded.txt"]);
    Ok(())
}

#[path = "commit_cases_tests_more.rs"]
pub mod more;
