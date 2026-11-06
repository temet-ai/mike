#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use super::{FakeHerdr, agent_entry, envelope, pane_info};
use crate::launch::backend::{BackendId, CloseOutcome, Surface, TerminalBackend};
use crate::launch::error::LaunchError;
use crate::launch::herdr::HerdrBackend;

const SESSION: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

fn surface(session: Option<Uuid>, pid: Option<u32>) -> Surface {
    Surface {
        backend: BackendId::Herdr,
        workspace_ref: Some("w1".into()),
        tab_ref: Some("w1:t1".into()),
        surface_ref: "w1:p1".into(),
        pid,
        session_uuid: session,
        agent_state: Some("idle".into()),
        cwd: Some("/tmp/project".into()),
        actual_placement: "tab".into(),
        reused: true,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    }
}

fn pane_get_ok() -> String {
    envelope(
        json!({"pane": pane_info("w1", "w1:t1", "w1:p1", "/tmp/project"), "type": "pane_info"}),
    )
}

fn agents(entries: Vec<serde_json::Value>) -> String {
    envelope(json!({"agents": entries, "type": "agent_list"}))
}

#[tokio::test]
async fn close_matching_session_closes_and_reports_process_exited() {
    let fake = FakeHerdr::new();
    fake.answer("pane get", &pane_get_ok())
        .answer(
            "agent list",
            &agents(vec![agent_entry("w1:p1", "claude", Some(SESSION), "idle")]),
        )
        .answer("pane close", &envelope(json!({"type": "ok"})));
    let backend = HerdrBackend::with_binary(fake.bin());
    let outcome = backend
        .close(
            &surface(Some(SESSION.parse().unwrap()), Some(i32::MAX as u32)),
            Duration::from_millis(20),
        )
        .await
        .unwrap();
    assert_eq!(
        outcome,
        CloseOutcome::Closed {
            process_exited: true
        }
    );
    assert_eq!(fake.calls()[2], vec!["pane", "close", "w1:p1"]);
}

#[tokio::test]
async fn close_session_mismatch_never_closes_the_pane() {
    let fake = FakeHerdr::new();
    let other = "ffffffff-1111-4222-8333-444444444444";
    fake.answer("pane get", &pane_get_ok()).answer(
        "agent list",
        &agents(vec![agent_entry("w1:p1", "claude", Some(other), "idle")]),
    );
    let outcome = HerdrBackend::with_binary(fake.bin())
        .close(
            &surface(Some(SESSION.parse().unwrap()), None),
            Duration::ZERO,
        )
        .await
        .unwrap();
    match outcome {
        CloseOutcome::Mismatch { detail } => assert!(
            detail.contains(other) && detail.contains(SESSION),
            "{detail}"
        ),
        other => panic!("expected Mismatch, got {other:?}"),
    }
    assert!(fake.calls().iter().all(|c| c[..2] != ["pane", "close"]));
}

#[tokio::test]
async fn close_pane_without_claude_agent_is_mismatch() {
    let fake = FakeHerdr::new();
    fake.answer("pane get", &pane_get_ok())
        .answer("agent list", &agents(vec![]));
    let outcome = HerdrBackend::with_binary(fake.bin())
        .close(
            &surface(Some(SESSION.parse().unwrap()), None),
            Duration::ZERO,
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, CloseOutcome::Mismatch { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn close_missing_pane_is_not_found_no_op() {
    let fake = FakeHerdr::new();
    fake.error("pane get", "pane_not_found", "pane w1:p1 not found");
    let outcome = HerdrBackend::with_binary(fake.bin())
        .close(
            &surface(Some(SESSION.parse().unwrap()), None),
            Duration::ZERO,
        )
        .await
        .unwrap();
    assert_eq!(outcome, CloseOutcome::NotFound);
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn close_without_session_skips_the_match_and_reports_unknown_pid_as_not_exited() {
    let fake = FakeHerdr::new();
    fake.answer("pane get", &pane_get_ok())
        .error("pane close", "pane_not_found", "raced away");
    let backend = HerdrBackend::with_binary(fake.bin());
    assert_eq!(
        backend
            .close(&surface(None, None), Duration::ZERO)
            .await
            .unwrap(),
        CloseOutcome::NotFound
    );
    let fake2 = FakeHerdr::new();
    fake2
        .answer("pane get", &pane_get_ok())
        .answer("pane close", &envelope(json!({"type": "ok"})));
    let outcome = HerdrBackend::with_binary(fake2.bin())
        .close(&surface(None, None), Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        CloseOutcome::Closed {
            process_exited: false
        }
    );
    assert!(fake2.calls().iter().all(|c| c[0] != "agent"));
}

#[tokio::test]
async fn close_backend_error_other_than_not_found_propagates() {
    let fake = FakeHerdr::new();
    fake.error("pane get", "connect_failed", "no server");
    let err = HerdrBackend::with_binary(fake.bin())
        .close(&surface(None, None), Duration::ZERO)
        .await
        .unwrap_err();
    assert_eq!(err.backend_code(), Some("connect_failed"));
}

#[tokio::test]
async fn find_by_session_matches_case_insensitively_and_ignores_other_agents() {
    let fake = FakeHerdr::new();
    fake.answer(
        "agent list",
        &agents(vec![
            agent_entry("w1:p1", "codex", Some(SESSION), "idle"),
            agent_entry("w1:p2", "claude", Some(&SESSION.to_uppercase()), "working"),
            agent_entry("w1:p3", "claude", None, "unknown"),
        ]),
    );
    let backend = HerdrBackend::with_binary(fake.bin());
    let found = backend
        .find_by_session(SESSION.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.surface_ref, "w1:p2");
    assert_eq!(found.agent_state.as_deref(), Some("working"));
    assert_eq!(found.session_uuid, Some(SESSION.parse().unwrap()));
}

#[tokio::test]
async fn find_by_session_unknown_uuid_is_none_and_list_filters_to_claude() {
    let fake = FakeHerdr::new();
    let listing = agents(vec![
        agent_entry("w1:p1", "codex", Some(SESSION), "idle"),
        agent_entry("w1:p2", "claude", None, "idle"),
    ]);
    fake.answer("agent list", &listing)
        .answer("agent list", &listing);
    let backend = HerdrBackend::with_binary(fake.bin());
    assert_eq!(backend.find_by_session(Uuid::new_v4()).await.unwrap(), None);
    let all = backend.list().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].surface_ref, "w1:p2");
    assert_eq!(all[0].backend, BackendId::Herdr);
}

#[tokio::test]
async fn list_without_agents_array_is_malformed_and_server_error_propagates() {
    let fake = FakeHerdr::new();
    fake.answer("agent list", &envelope(json!({"type": "agent_list"})))
        .error("agent list", "connect_failed", "no server");
    let backend = HerdrBackend::with_binary(fake.bin());
    assert!(matches!(
        backend.list().await.unwrap_err(),
        LaunchError::Malformed { .. }
    ));
    assert_eq!(
        backend.list().await.unwrap_err().backend_code(),
        Some("connect_failed")
    );
}

#[tokio::test]
async fn find_by_session_reads_fresh_root_pid_from_the_matching_pane() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    fake.answer(
        "agent list",
        &agents(vec![agent_entry(
            "w1:p1",
            "claude",
            Some(SESSION),
            "working",
        )]),
    )
    .answer(
        "pane process-info",
        &envelope(super::process_info("w1:p1", 100, 321, Some(321))),
    );
    let found = HerdrBackend::with_binary(fake.bin())
        .find_by_session(SESSION.parse()?)
        .await?
        .ok_or_else(|| anyhow::anyhow!("matching session disappeared"))?;
    assert_eq!(found.pid, Some(321));
    assert_eq!(found.session_uuid, Some(SESSION.parse()?));
    assert!(
        fake.calls()
            .iter()
            .any(|call| call[..2] == ["pane", "process-info"])
    );
    Ok(())
}
