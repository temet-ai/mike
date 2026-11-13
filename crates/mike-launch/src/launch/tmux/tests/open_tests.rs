#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::collections::BTreeMap;

use uuid::Uuid;

use super::FakeTmux;
use crate::launch::backend::{BackendId, OpenRequest, Placement, TerminalBackend};
use crate::launch::request::agent_name;
use crate::launch::tmux::TmuxBackend;

fn request(session: Uuid, run_tab: Option<&str>, prompt: Option<&str>) -> OpenRequest {
    OpenRequest {
        folder: "/tmp/project".into(),
        placement: Placement::Window,
        env: BTreeMap::new(),
        title: "t".into(),
        run_tab: run_tab.map(str::to_string),
        run: None,
        program: "claude".into(),
        args: vec![
            "--session-id".into(),
            session.to_string(),
            "--model".into(),
            "opus".into(),
        ],
        name: agent_name(session),
        prompt: prompt.map(str::to_string),
        prompt_wait_ms: 1000,
    }
}

#[tokio::test]
async fn open_new_session_no_prompt_records_session_uuid_and_starts_claude() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "%1\u{1f}0\n");
    fake.answer("set-option", "");
    fake.answer("send-keys", "");
    fake.answer("display-message", "claude\n");
    fake.answer("display-message", "4242\n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let session = Uuid::new_v4();
    let surface = backend.open(&request(session, None, None)).await.unwrap();

    assert_eq!(surface.backend, BackendId::Tmux);
    assert_eq!(surface.surface_ref, "%1");
    let expected_session = format!("mike-{}", &session.simple().to_string()[..8]);
    assert_eq!(
        surface.workspace_ref.as_deref(),
        Some(expected_session.as_str())
    );
    assert_eq!(surface.session_uuid, Some(session));
    assert_eq!(surface.delivered, None);
    assert!(!surface.confirmed);
    assert!(
        surface.pid.is_none(),
        "no real /proc child exists under the fake"
    );

    let calls = fake.calls();
    let heads: Vec<&str> = calls.iter().map(|c| c[0].as_str()).collect();
    assert_eq!(
        heads,
        vec![
            "new-session",
            "set-option",
            "send-keys",
            "display-message",
            "display-message"
        ]
    );
}

#[tokio::test]
async fn open_with_a_prompt_delivers_via_set_buffer_paste_buffer_then_enter() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "%1\u{1f}0\n");
    fake.answer("set-option", "");
    fake.answer("send-keys", "");
    fake.answer("display-message", "claude\n");
    fake.answer("display-message", "4242\n");
    fake.answer("capture-pane", "before\n");
    fake.answer("set-buffer", "");
    fake.answer("paste-buffer", "");
    fake.answer("capture-pane", "after: hello world\n");
    fake.answer("send-keys", "");
    fake.answer("capture-pane", "\u{276f} \n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let session = Uuid::new_v4();
    let surface = backend
        .open(&request(session, None, Some("hello world")))
        .await
        .unwrap();

    assert_eq!(surface.delivered, Some(true));
    assert!(
        !surface.confirmed,
        "tmux has no agent_state to confirm a turn"
    );
    assert!(surface.prompt_detail.is_some());

    let calls = fake.calls();
    let heads: Vec<&str> = calls.iter().map(|c| c[0].as_str()).collect();
    assert_eq!(
        heads,
        vec![
            "new-session",
            "set-option",
            "send-keys",
            "display-message",
            "capture-pane",
            "set-buffer",
            "paste-buffer",
            "capture-pane",
            "send-keys",
            "capture-pane",
            "display-message",
        ]
    );
}

#[tokio::test]
async fn open_with_a_prompt_left_in_the_box_presses_enter_again() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "%1\u{1f}0\n");
    fake.answer("set-option", "");
    fake.answer("send-keys", "");
    fake.answer("display-message", "claude\n");
    fake.answer("display-message", "4242\n");
    fake.answer("capture-pane", "before\n");
    fake.answer("set-buffer", "");
    fake.answer("paste-buffer", "");
    fake.answer("capture-pane", "\u{276f}\u{a0}hello world\n");
    fake.answer("send-keys", "");
    fake.answer("capture-pane", "\u{276f}\u{a0}hello world\n");
    fake.answer("send-keys", "");
    fake.answer("capture-pane", "\u{276f} \n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let surface = backend
        .open(&request(Uuid::new_v4(), None, Some("hello world")))
        .await
        .unwrap();

    assert_eq!(surface.delivered, Some(true));
    let calls = fake.calls();
    let enters = calls
        .iter()
        .filter(|c| c[0] == "send-keys" && c.last().is_some_and(|k| k == "Enter"))
        .count();
    assert_eq!(enters, 3, "launch line, first submit, and the retry");
}

#[tokio::test]
async fn open_with_a_pane_that_never_shows_a_box_gives_up_soon() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "%1\u{1f}0\n");
    fake.answer("set-option", "");
    fake.answer("send-keys", "");
    fake.answer("display-message", "claude\n");
    fake.answer("display-message", "4242\n");
    fake.answer("capture-pane", "before\n");
    fake.answer("set-buffer", "");
    fake.answer("paste-buffer", "");
    fake.answer("capture-pane", "no input box here\n");
    fake.answer("send-keys", "");
    let backend = TmuxBackend::with_binary(fake.bin());
    let started = std::time::Instant::now();
    let surface = backend
        .open(&request(Uuid::new_v4(), None, Some("hello world")))
        .await
        .unwrap();

    assert_eq!(surface.delivered, Some(true));
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
}

#[tokio::test]
async fn open_with_a_prompt_waits_for_the_pane_to_change_before_sending_enter() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "%1\u{1f}0\n");
    fake.answer("set-option", "");
    fake.answer("send-keys", "");
    fake.answer("display-message", "claude\n");
    fake.answer("display-message", "4242\n");
    fake.answer("capture-pane", "before\n");
    fake.answer("set-buffer", "");
    fake.answer("paste-buffer", "");
    fake.answer("capture-pane", "before\n");
    fake.answer("capture-pane", "before\n");
    fake.answer("capture-pane", "after: hello world\n");
    fake.answer("send-keys", "");
    fake.answer("capture-pane", "\u{276f} \n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let session = Uuid::new_v4();
    let surface = backend
        .open(&request(session, None, Some("hello world")))
        .await
        .unwrap();

    assert_eq!(surface.delivered, Some(true));
    let calls = fake.calls();
    let capture_count = calls.iter().filter(|c| c[0] == "capture-pane").count();
    assert_eq!(
        capture_count, 5,
        "polled past the two unchanged answers before proceeding, then read the box once"
    );
    let submit_check = calls.iter().rposition(|c| c[0] == "capture-pane").unwrap();
    assert_eq!(calls[submit_check - 1][0], "send-keys");
}

#[tokio::test]
async fn open_with_a_run_tab_splits_into_the_existing_window_and_tiles() {
    let fake = FakeTmux::new();
    fake.answer("split-window", "mike-abcd1234\u{1f}%5\u{1f}1\n");
    fake.answer("select-layout", "");
    fake.answer("set-option", "");
    fake.answer("send-keys", "");
    fake.answer("display-message", "claude\n");
    fake.answer("display-message", "4242\n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let session = Uuid::new_v4();
    let surface = backend
        .open(&request(session, Some("mike-abcd1234:0"), None))
        .await
        .unwrap();

    assert_eq!(surface.surface_ref, "%5");
    assert_eq!(surface.workspace_ref.as_deref(), Some("mike-abcd1234"));
    assert_eq!(surface.tab_ref.as_deref(), Some("mike-abcd1234:1"));
    assert!(surface.reused);
    assert_eq!(surface.actual_placement, "split (run window, tiled)");

    let calls = fake.calls();
    assert_eq!(calls[0][0], "split-window");
    assert_eq!(calls[1][0], "select-layout");
}

#[tokio::test]
async fn open_malformed_new_session_answer_is_an_error() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "\n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let err = backend
        .open(&request(Uuid::new_v4(), None, None))
        .await
        .unwrap_err();
    assert_eq!(err.exit_code(), 1);
}
