#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::time::Duration;

use super::FakeTmux;
use crate::launch::backend::{BackendId, CloseOutcome, Surface, TerminalBackend};
use crate::launch::tmux::TmuxBackend;

fn surface(pid: Option<u32>) -> Surface {
    Surface {
        backend: BackendId::Tmux,
        workspace_ref: Some("mike-abcd1234".into()),
        tab_ref: Some("mike-abcd1234:0".into()),
        surface_ref: "%3".into(),
        pid,
        session_uuid: None,
        agent_state: None,
        cwd: Some("/tmp/project".into()),
        actual_placement: "window".into(),
        reused: false,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    }
}

#[tokio::test]
async fn close_missing_pane_is_not_found_without_calling_kill_pane() {
    let fake = FakeTmux::new();
    fake.error("list-panes", "can't find pane: %3");
    let backend = TmuxBackend::with_binary(fake.bin());
    let outcome = backend
        .close(&surface(None), Duration::from_millis(50))
        .await
        .unwrap();
    assert_eq!(outcome, CloseOutcome::NotFound);
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn close_existing_pane_kills_it_and_reports_unknown_process_exit_without_a_pid() {
    let fake = FakeTmux::new();
    fake.answer("list-panes", "%3\n");
    fake.answer("kill-pane", "");
    let backend = TmuxBackend::with_binary(fake.bin());
    let outcome = backend
        .close(&surface(None), Duration::from_millis(50))
        .await
        .unwrap();
    assert_eq!(
        outcome,
        CloseOutcome::Closed {
            process_exited: false
        }
    );
}

#[tokio::test]
async fn close_kill_pane_reporting_already_gone_is_not_found() {
    let fake = FakeTmux::new();
    fake.answer("list-panes", "%3\n");
    fake.error("kill-pane", "can't find pane: %3");
    let backend = TmuxBackend::with_binary(fake.bin());
    let outcome = backend
        .close(&surface(None), Duration::from_millis(50))
        .await
        .unwrap();
    assert_eq!(outcome, CloseOutcome::NotFound);
}

#[tokio::test]
async fn close_with_a_live_pid_waits_and_reports_it_did_not_exit() {
    let fake = FakeTmux::new();
    fake.answer("list-panes", "%3\n");
    fake.answer("kill-pane", "");
    let backend = TmuxBackend::with_binary(fake.bin());
    let outcome = backend
        .close(
            &surface(Some(std::process::id())),
            Duration::from_millis(50),
        )
        .await
        .unwrap();
    assert_eq!(
        outcome,
        CloseOutcome::Closed {
            process_exited: false
        }
    );
}
