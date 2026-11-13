use mike::launch::fleet::{check::state_for, gate::ledger};
use mike::store::persistence::fleet::NewTreeChange;
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::session_notes::{NoteOrder, SessionNoteFilter};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

#[path = "fleet_admission/support.rs"]
mod support;
use support::{Harness, git};

fn writes(h: &Harness) -> Vec<(&'static str, Value)> {
    vec![
        (
            "/check",
            json!({"session": h.worker, "command": "printf forbidden > admission-spawned"}),
        ),
        ("/gate", json!({"session": h.acceptance})),
        (
            "/commit",
            json!({"session": h.worker, "message": "must not commit", "allow_empty": true}),
        ),
        ("/integrate", json!({})),
        ("/report", report(h)),
        ("/verdict", verdict(h)),
        (
            "/propose",
            json!({"session": h.worker, "name": "new task", "why": "coverage",
            "new_files": ["src/new.rs"], "criteria": ["C1"], "discipline": "rust"}),
        ),
        (
            "/split",
            json!({"session": h.worker, "task": "T1", "name": "extra",
            "why": "separate slice", "files": ["src/extra.rs"]}),
        ),
    ]
}

fn report(h: &Harness) -> Value {
    json!({"session": h.worker, "task": "T1", "slice": "owned", "criteria": ["C1"],
        "look_at": "src/owned.rs", "evidence": ["src/owned.rs"],
        "commit": "abc123abc123",
        "exercised": {"command": "true", "exit": 0, "output_ref": "out.txt"},
        "tests_changed": ["none"], "unestablished": []})
}

fn verdict(h: &Harness) -> Value {
    json!({"session": h.acceptance, "task": "T1", "verdict": "pass", "scope": "final",
        "criteria": ["C1"], "reason": "owned function exists"})
}

async fn no_new_work(h: &Harness) -> anyhow::Result<()> {
    assert!(h.persist.fleet().latest_tree_check(&h.run).await?.is_none());
    assert!(
        h.persist
            .fleet()
            .latest_tree_check(&ledger(&h.run))
            .await?
            .is_none()
    );
    assert!(!h.repo.path().join("admission-spawned").exists());
    for session in [h.worker, h.other, h.acceptance] {
        assert_eq!(h.inbox_len(session).await?, 0);
        let notes = h
            .persist
            .launch()
            .fetch_session_notes(
                session,
                SessionNoteFilter {
                    kind: None,
                    status: None,
                    tag: None,
                    q: None,
                    order: NoteOrder::NewestFirst,
                    limit: 20,
                    offset: 0,
                },
            )
            .await?;
        assert!(notes.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn mutation_routes_non_running_fleet_with_run_refuse_without_side_effects()
-> anyhow::Result<()> {
    for state in ["stopping", "done", "starting", "failed", "ready", "draft"] {
        let Some(h) = Harness::boot_in_state(state).await? else {
            return Ok(());
        };
        let head = git(h.repo.path(), &["rev-parse", "HEAD"]).await?;
        let before = h.row().await?;
        assert_eq!(before.state, state);
        assert_eq!(before.run_id.as_deref(), Some(h.run.as_str()));
        for (route, body) in writes(&h) {
            let (code, answer) = h.request(Method::POST, route, body).await?;
            assert_eq!(code, StatusCode::CONFLICT, "{state} {route}: {answer}");
            assert_eq!(
                answer["error"], "fleet_not_running",
                "{state} {route}: {answer}"
            );
            assert!(
                answer["message"]
                    .as_str()
                    .is_some_and(|m| m.contains(state))
            );
        }
        assert_eq!(h.row().await?, before);
        no_new_work(&h).await?;
        assert_eq!(git(h.repo.path(), &["rev-parse", "HEAD"]).await?, head);
        h.cleanup().await?;
    }
    Ok(())
}

#[tokio::test]
async fn definition_patch_stopping_fleet_refuses_patch_and_ready_without_changes()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let before = h.state("stopping").await?;
    for body in [
        json!({"patch": {"goal": "must not change"}}),
        json!({"ready": true}),
        json!({"patch": {"tasks": "invalid even before validation"}}),
    ] {
        let (code, answer) = h.request(Method::PATCH, "", body).await?;
        assert_eq!(code, StatusCode::CONFLICT, "{answer}");
        assert_eq!(answer["error"], "fleet_stopping", "{answer}");
        assert_eq!(h.row().await?, before);
    }
    h.cleanup().await
}

#[tokio::test]
async fn history_routes_stopping_and_done_fleets_remain_readable_without_new_work()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    h.persist
        .fleet()
        .insert_tree_changes(
            &h.run,
            &[NewTreeChange {
                path: "src/owned.rs".into(),
                change: "modified".into(),
                content_sha: Some("historical-content".into()),
            }],
        )
        .await?;
    for state in ["stopping", "done"] {
        let before = h.state(state).await?;
        for route in [
            "/results",
            "/changes",
            "/reuse?command=cargo%20check",
            "/where?path=src/owned.rs",
        ] {
            let (code, answer) = h.request(Method::GET, route, Value::Null).await?;
            assert_eq!(code, StatusCode::OK, "{state} {route}: {answer}");
            if route == "/changes" {
                assert_eq!(answer["run_id"], h.run);
                assert_eq!(answer["changes"][0]["path"], "src/owned.rs");
            }
            if route == "/results" {
                assert_eq!(answer["run_id"], h.run);
                assert_eq!(answer["results"], json!([]));
            }
            if route.starts_with("/where") {
                assert_eq!(answer["definitions"][0]["path"], "src/owned.rs");
            }
        }
        for route in ["/check", "/gate"] {
            let (code, answer) = h
                .request(
                    Method::POST,
                    route,
                    json!({"session": h.worker, "status": true}),
                )
                .await?;
            assert_eq!(code, StatusCode::OK, "{state} {route}: {answer}");
            assert_eq!(answer["how"], "not run");
            assert_eq!(answer["passed"], false);
            assert_eq!(answer["refused"], Value::Null);
        }
        assert_eq!(h.row().await?, before);
        no_new_work(&h).await?;
    }
    h.cleanup().await
}

#[tokio::test]
async fn check_and_gate_status_finished_fleets_read_receipts_without_askers_or_mail()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let commit = git(h.repo.path(), &["rev-parse", "HEAD"]).await?;
    let state = state_for(&h.def, &commit)
        .await
        .map_err(anyhow::Error::msg)?;
    let receipt = h
        .persist
        .fleet()
        .begin_tree_check(&h.run, &state, &h.def.check.command, "original-asker")
        .await?;
    h.persist
        .fleet()
        .finish_tree_check(
            receipt,
            Some(1),
            "src/other.rs:1:1: recorded failure",
            Some(&state),
        )
        .await?;
    let gate_run = ledger(&h.run);
    let gate_receipt = h
        .persist
        .fleet()
        .begin_tree_check(&gate_run, &commit, &h.def.gate.command, "acceptance")
        .await?;
    h.persist
        .fleet()
        .finish_tree_check(gate_receipt, Some(0), "recorded gate", Some(&commit))
        .await?;
    let check_before = h.persist.fleet().latest_tree_check(&h.run).await?;
    let gate_before = h.persist.fleet().latest_tree_check(&gate_run).await?;
    for state in ["running", "stopping", "done"] {
        let fleet_before = h.state(state).await?;
        let request = json!({"session": h.worker, "status": true});
        let (code, check) = h.request(Method::POST, "/check", request.clone()).await?;
        assert_eq!(code, StatusCode::OK, "{state}: {check}");
        assert_eq!(check["receipt_id"], receipt);
        assert_eq!(check["how"], "reused");
        assert_eq!(check["passed"], false);
        assert_eq!(check["others"][0]["task"], "T2");
        let (code, gate) = h.request(Method::POST, "/gate", request).await?;
        assert_eq!(code, StatusCode::OK, "{state}: {gate}");
        assert_eq!(gate["how"], "reused");
        assert_eq!(gate["passed"], true);
        assert_eq!(gate["source_verified"], true);
        assert_eq!(
            h.persist.fleet().latest_tree_check(&h.run).await?,
            check_before
        );
        assert_eq!(
            h.persist.fleet().latest_tree_check(&gate_run).await?,
            gate_before
        );
        assert_eq!(h.row().await?, fleet_before);
        assert_eq!(h.inbox_len(h.other).await?, 0);
        assert!(!h.repo.path().join("admission-spawned").exists());
    }
    h.cleanup().await
}

#[tokio::test]
async fn report_and_verdict_running_fleet_preserve_closed_seat_delivery_results()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let mut body = report(&h);
    body["commit"] = json!(git(h.repo.path(), &["rev-parse", "HEAD"]).await?);
    let (code, reported) = h.request(Method::POST, "/report", body).await?;
    assert_eq!(code, StatusCode::OK, "{reported}");
    assert_eq!(reported["told_acceptance"], true);
    assert_eq!(h.inbox_len(h.acceptance).await?, 1);
    h.persist
        .launch()
        .record_launch_closed(h.worker_launch, "finished", Some(true))
        .await?;
    let mut judged = verdict(&h);
    judged["report_note"] = reported["note"].clone();
    let (code, answer) = h.request(Method::POST, "/verdict", judged).await?;
    assert_eq!(code, StatusCode::OK, "{answer}");
    assert_eq!(answer["told_seat"], false);
    assert_eq!(answer["seat_closed"], h.worker.to_string());
    assert_eq!(h.inbox_len(h.worker).await?, 0);
    h.cleanup().await
}
