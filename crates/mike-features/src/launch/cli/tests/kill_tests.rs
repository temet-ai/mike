use uuid::Uuid;

use super::list_tests::row;
use super::{fake, json, runner};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::{FakeBackend, FakeCall};
use crate::launch::cli::kill;
use crate::launch::cli::render::print_kill;
use crate::launch::{CloseOutcome, LaunchError};

#[tokio::test]
async fn kill_no_surface_is_a_no_op_exit_zero() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let session = Uuid::new_v4();
    let report = kill::execute(&runner, session).await.unwrap();
    assert!(!report.closed);
    assert_eq!(report.surface, None);
    assert!(report.note.as_deref().unwrap().contains("nothing to do"));
    assert_eq!(print_kill(&report, true), 0);
    let value = json(&report.to_json_line());
    assert_eq!(value["session"], session.to_string());
    assert_eq!(value["closed"], false);
    assert_eq!(value["process_exited"], serde_json::Value::Null);
    assert!(value.get("note").is_none());
    let calls = backend.calls.lock().await.clone();
    assert_eq!(calls, vec![FakeCall::FindBySession(session)]);
}

#[tokio::test]
async fn kill_surface_with_another_session_never_calls_close() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let session = Uuid::new_v4();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(FakeBackend::surface(Some(Uuid::new_v4())))));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = kill::execute(&runner, session).await.unwrap();
    assert!(!report.closed);
    assert_eq!(report.surface.as_deref(), Some("w1:p1"));
    assert!(
        report.note.as_deref().unwrap().contains("not"),
        "{report:?}"
    );
    let calls = backend.calls.lock().await.clone();
    assert!(
        !calls.iter().any(|c| matches!(c, FakeCall::Close(_))),
        "{calls:?}"
    );
    assert_eq!(print_kill(&report, false), 0);
}

#[tokio::test]
async fn kill_surface_without_a_session_id_never_calls_close() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(FakeBackend::surface(None))));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = kill::execute(&runner, Uuid::new_v4()).await.unwrap();
    assert!(!report.closed);
    let calls = backend.calls.lock().await.clone();
    assert!(
        !calls.iter().any(|c| matches!(c, FakeCall::Close(_))),
        "{calls:?}"
    );
}

#[tokio::test]
async fn kill_closes_the_matching_surface_and_reports_the_process() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let session = Uuid::new_v4();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(FakeBackend::surface(Some(session)))));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::Closed {
            process_exited: true,
        }));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = kill::execute(&runner, session).await.unwrap();
    assert!(report.closed);
    assert_eq!(report.surface.as_deref(), Some("w1:p1"));
    assert_eq!(report.process_exited, Some(true));
    assert_eq!(report.note, None);
    let value = json(&report.to_json_line());
    assert_eq!(value["surface"], "w1:p1");
    assert_eq!(value["closed"], true);
    assert_eq!(value["process_exited"], true);
    let calls = backend.calls.lock().await.clone();
    assert_eq!(
        calls,
        vec![
            FakeCall::FindBySession(session),
            FakeCall::Close("w1:p1".into())
        ]
    );
}

#[tokio::test]
async fn kill_unknown_pid_reports_process_exited_unknown() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let session = Uuid::new_v4();
    let mut surface = FakeBackend::surface(Some(session));
    surface.pid = None;
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(surface)));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::Closed {
            process_exited: false,
        }));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = kill::execute(&runner, session).await.unwrap();
    assert!(report.closed);
    assert_eq!(report.process_exited, None);
    assert_eq!(
        json(&report.to_json_line())["process_exited"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn kill_backend_mismatch_and_not_found_are_no_ops() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let session = Uuid::new_v4();
    for outcome in [
        CloseOutcome::Mismatch {
            detail: "pane w1:p1 hosts another session".into(),
        },
        CloseOutcome::NotFound,
    ] {
        backend
            .find_answers
            .lock()
            .await
            .push_back(Ok(Some(FakeBackend::surface(Some(session)))));
        backend.close_answers.lock().await.push_back(Ok(outcome));
        let config = LaunchConfig::default();
        let runner = runner(&backend, &config, home.path());
        let report = kill::execute(&runner, session).await.unwrap();
        assert!(!report.closed);
        assert!(report.note.is_some());
        assert_eq!(print_kill(&report, true), 0);
    }
}

#[tokio::test]
async fn kill_backend_error_propagates() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Err(LaunchError::Failed("herdr down".into())));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let err = kill::execute(&runner, Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err.exit_code(), 1);
}

#[test]
fn mismatch_ledger_surface_differing_from_the_backends_is_reported() {
    let session = Uuid::new_v4();
    let surface = FakeBackend::surface(Some(session));
    assert_eq!(kill::mismatch(&surface, session, None), None);
    assert_eq!(kill::mismatch(&surface, session, Some("w1:p1")), None);
    let detail = kill::mismatch(&surface, session, Some("w1:p7")).unwrap();
    assert!(
        detail.contains("w1:p7") && detail.contains("w1:p1"),
        "{detail}"
    );
    let other = kill::mismatch(&surface, Uuid::new_v4(), None).unwrap();
    assert!(other.contains("not"), "{other}");
}

#[test]
fn current_backend_and_root_must_match_the_recorded_launch() {
    let session = Uuid::new_v4();
    let mut recorded = row(1, session, "/tmp/project", None, None);
    recorded.pid = Some(671_124);
    let mut unknown = FakeBackend::surface(Some(session));
    unknown.pid = None;
    assert_eq!(kill::launch_mismatch(&unknown, Some(&recorded)), None);
    assert_eq!(unknown.pid, None);
    assert!(kill::launch_mismatch(&FakeBackend::surface(Some(session)), Some(&recorded)).is_some());
    recorded.pid = Some(4242);
    assert_eq!(
        kill::launch_mismatch(&FakeBackend::surface(Some(session)), Some(&recorded)),
        None
    );
    recorded.backend = "tmux".into();
    assert!(kill::launch_mismatch(&unknown, Some(&recorded)).is_some());
}

#[test]
fn a_seat_resumed_after_its_recorded_root_exited_can_be_closed() {
    let session = Uuid::new_v4();
    let mut recorded = row(1, session, "/tmp/project", None, None);
    recorded.pid = Some(671_124);
    recorded.closed_at = Some(chrono::Utc::now());
    let resumed = FakeBackend::surface(Some(session));
    recorded.close_process_exited = None;
    assert!(kill::launch_mismatch(&resumed, Some(&recorded)).is_some());
    recorded.close_process_exited = Some(true);
    assert_eq!(kill::launch_mismatch(&resumed, Some(&recorded)), None);
}

#[tokio::test]
async fn kill_hands_close_the_surface_with_the_pid_it_resolved() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let session = Uuid::new_v4();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(FakeBackend::surface(Some(session)))));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::Closed {
            process_exited: true,
        }));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = kill::execute(&runner, session).await.unwrap();
    assert_eq!(report.process_exited, Some(true));
    let closed = backend.closed.lock().await.clone();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].pid, Some(4242));
}

#[tokio::test]
async fn kill_unconfirmed_outcome_never_queues_a_closure() -> anyhow::Result<()> {
    kill::closure_cases::unconfirmed_kill_does_not_record_closure().await
}

#[tokio::test]
async fn kill_confirmed_pane_preserves_live_or_unknown_root_evidence() -> anyhow::Result<()> {
    kill::closure_cases::confirmed_pane_closure_preserves_exit_evidence().await
}

#[tokio::test]
async fn kill_not_found_never_posts_a_proxy_ledger_close() -> anyhow::Result<()> {
    use super::proxy_ledger_tests::{fake_proxy, hello_route, proxy_ledger};

    let home = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let mut launch = row(
        31,
        session,
        "/proj",
        Some("fleet-kill-12345678"),
        Some("dev"),
    );
    launch.surface_ref = Some("w1:p1".into());
    let proxy = fake_proxy(vec![
        hello_route(),
        (
            "GET",
            format!("/api/launches/{session}"),
            200,
            serde_json::to_string(&launch)?,
        ),
    ])
    .await?;
    let backend = fake();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(FakeBackend::surface(Some(session)))));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::NotFound));
    let config = LaunchConfig::default();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = proxy_ledger(&proxy.addr).await?;
    let report = kill::execute(&runner, session).await?;
    assert!(!report.closed);
    assert_eq!(report.process_exited, None);
    assert!(
        proxy
            .seen()
            .await
            .iter()
            .all(|request| request.method == "GET")
    );
    Ok(())
}

#[tokio::test]
async fn kill_never_backfills_a_foreign_persisted_pid_into_current_surface() -> anyhow::Result<()> {
    use super::proxy_ledger_tests::{fake_proxy, hello_route, proxy_ledger};

    let home = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let mut launch = row(
        31,
        session,
        "/foreign",
        Some("fleet-foreign"),
        Some("worker"),
    );
    launch.surface_ref = Some("w1:p1".into());
    launch.pid = Some(i32::MAX);
    let proxy = fake_proxy(vec![
        hello_route(),
        (
            "GET",
            format!("/api/launches/{session}"),
            200,
            serde_json::to_string(&launch)?,
        ),
    ])
    .await?;
    let backend = fake();
    let mut surface = FakeBackend::surface(Some(session));
    surface.pid = None;
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(surface)));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::Closed {
            process_exited: true,
        }));
    let config = LaunchConfig::default();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = proxy_ledger(&proxy.addr).await?;
    let report = kill::execute(&runner, session).await?;
    assert!(report.closed);
    assert_eq!(report.process_exited, None);
    assert_eq!(backend.closed.lock().await[0].pid, None);
    let closed = proxy
        .must_hit("POST", "/api/launch-ledger/launches/31/closed")
        .await?;
    let body: serde_json::Value = serde_json::from_str(&closed.body)?;
    assert_eq!(body["process_exited"], serde_json::Value::Null);
    Ok(())
}
