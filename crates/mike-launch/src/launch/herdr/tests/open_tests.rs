#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::collections::BTreeMap;

use serde_json::json;
use uuid::Uuid;

use super::{FakeHerdr, agent_entry, envelope, pane_info, process_info};
use crate::launch::backend::{OpenRequest, Placement, SplitDirection, TerminalBackend};
use crate::launch::error::LaunchError;
use crate::launch::herdr::HerdrBackend;

const FOLDER: &str = "/tmp/project";

fn request(placement: Placement) -> OpenRequest {
    OpenRequest {
        folder: FOLDER.into(),
        placement,
        env: BTreeMap::from([
            (
                "HTTPS_PROXY".to_string(),
                "http://127.0.0.1:9987".to_string(),
            ),
            ("NO_PROXY".to_string(), "localhost,127.0.0.1".to_string()),
        ]),
        title: "cc·opus — fix it".into(),
        run_tab: None,
        run: None,
        program: "claude".into(),
        args: vec![
            "--session-id".into(),
            "11111111-2222-4333-8444-555555555555".into(),
            "--model".into(),
            "opus".into(),
            "fix it".into(),
        ],
        name: "cc-11111111".into(),
        prompt: None,
        prompt_wait_ms: 30_000,
    }
}

fn prompted(placement: Placement) -> OpenRequest {
    let mut req = request(placement);
    req.args.pop();
    req.prompt = Some("fix it".into());
    req
}

fn agent_prompted(pane: &str, status: &str) -> String {
    envelope(json!({"agent": agent_entry(pane, "claude", None, status), "type": "agent_prompted"}))
}

fn tab_in_existing(fake: &FakeHerdr, session: Option<&str>) {
    fake.answer("pane list", &pane_list(FOLDER))
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "tab": {"tab_id": "w1:t2"}, "type": "tab_created"})))
        .answer("agent start", &agent_started("w1:p2", session, "idle"));
}

fn pane_list(cwd_of_p1: &str) -> String {
    envelope(json!({"panes": [pane_info("w1", "w1:t1", "w1:p1", cwd_of_p1)], "type": "pane_list"}))
}

fn agent_started(pane: &str, session: Option<&str>, status: &str) -> String {
    envelope(
        json!({"agent": agent_entry(pane, "claude", session, status), "argv": ["claude"], "type": "agent_started"}),
    )
}

#[tokio::test]
async fn open_tab_placement_in_existing_workspace_issues_exact_argv_sequence() {
    let fake = FakeHerdr::new();
    let session = "11111111-2222-4333-8444-555555555555";
    fake.answer("pane list", &pane_list(FOLDER))
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "tab": {"tab_id": "w1:t2"}, "type": "tab_created"})))
        .answer("agent start", &agent_started("w1:p2", Some(session), "idle"))
        .answer("pane process-info", &envelope(process_info("w1:p2", 100, 200, Some(200))));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w1:p2");
    assert_eq!(surface.workspace_ref.as_deref(), Some("w1"));
    assert_eq!(surface.tab_ref.as_deref(), Some("w1:t2"));
    assert_eq!(surface.session_uuid, Some(session.parse::<Uuid>().unwrap()));
    assert_eq!(surface.agent_state.as_deref(), Some("idle"));
    assert_eq!(surface.actual_placement, "tab");
    assert!(surface.reused);
    assert_eq!(surface.cwd.as_deref(), Some(FOLDER));
    assert_eq!(surface.pid, Some(200));
    let calls = fake.calls();
    assert_eq!(calls[0], vec!["pane", "list"]);
    assert_eq!(
        calls[1],
        vec![
            "tab",
            "create",
            "--workspace",
            "w1",
            "--cwd",
            FOLDER,
            "--env",
            "HTTPS_PROXY=http://127.0.0.1:9987",
            "--env",
            "NO_PROXY=localhost,127.0.0.1",
            "--no-focus"
        ]
    );
    assert_eq!(
        calls[2],
        vec![
            "agent",
            "start",
            "cc-11111111",
            "--kind",
            "claude",
            "--pane",
            "w1:p2",
            "--timeout",
            "60000",
            "--",
            "--session-id",
            session,
            "--model",
            "opus",
            "fix it"
        ]
    );
    assert_eq!(calls[3], vec!["pane", "process-info", "--pane", "w1:p2"]);
}

#[tokio::test]
async fn open_no_workspace_for_folder_creates_one_with_env() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list("/elsewhere"))
        .answer("workspace create", &envelope(json!({"root_pane": pane_info("w2", "w2:t1", "w2:p1", FOLDER), "type": "workspace_created"})))
        .answer("agent start", &agent_started("w2:p1", None, "working"));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w2:p1");
    assert!(!surface.reused);
    assert_eq!(surface.actual_placement, "workspace");
    assert_eq!(surface.session_uuid, None);
    assert_eq!(surface.pid, None);
    let calls = fake.calls();
    assert_eq!(calls[1][..4], ["workspace", "create", "--cwd", FOLDER]);
    assert!(calls[1].contains(&"--env".to_string()));
    assert_eq!(calls[1].last().unwrap(), "--no-focus");
}

#[tokio::test]
async fn open_split_left_maps_to_right_and_says_so() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer(
            "pane split",
            &envelope(
                json!({"pane": pane_info("w1", "w1:t1", "w1:p3", FOLDER), "type": "pane_info"}),
            ),
        )
        .answer("agent start", &agent_started("w1:p3", None, "idle"));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Split(SplitDirection::Left)))
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w1:p3");
    assert_eq!(surface.actual_placement, "split:right (requested left)");
    let calls = fake.calls();
    assert_eq!(
        calls[1][..6],
        ["pane", "split", "--pane", "w1:p1", "--direction", "right"]
    );
}

#[tokio::test]
async fn open_panel_with_env_uses_a_fresh_tab_not_the_root_pane() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "type": "tab_created"})))
        .answer("agent start", &agent_started("w1:p2", None, "idle"));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Panel))
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w1:p2");
    assert!(surface.actual_placement.starts_with("tab (requested panel"));
}

#[tokio::test]
async fn open_panel_without_env_reuses_root_pane_at_shell_prompt() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer(
            "pane process-info",
            &envelope(process_info("w1:p1", 100, 100, None)),
        )
        .answer("agent start", &agent_started("w1:p1", None, "idle"));
    let mut req = request(Placement::Panel);
    req.env.clear();
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&req)
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w1:p1");
    assert!(
        surface.actual_placement.contains("shell prompt"),
        "{}",
        surface.actual_placement
    );
    assert_eq!(
        fake.calls()[1],
        vec!["pane", "process-info", "--pane", "w1:p1"]
    );
}

#[tokio::test]
async fn open_agent_not_ready_returns_surface_with_reported_state() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "type": "tab_created"})))
        .error("agent start", "agent_not_ready", "agent blocked during startup")
        .answer("agent get", &envelope(json!({"agent": agent_entry("w1:p2", "claude", None, "blocked"), "type": "agent_info"})));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.agent_state.as_deref(), Some("blocked"));
    assert_eq!(surface.surface_ref, "w1:p2");
}

#[tokio::test]
async fn open_agent_start_timeout_is_backend_response_error() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "type": "tab_created"})))
        .error("agent start", "timeout", "timed out waiting for agent startup");
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap_err();
    assert_eq!(err.backend_code(), Some("timeout"));
    assert_eq!(err.exit_code(), 1);
}

#[tokio::test]
async fn open_agent_start_failure_enriches_error_with_pane_tail_and_closes_the_new_pane() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "type": "tab_created"})))
        .error("agent start", "timeout", "timed out waiting for agent startup")
        .answer(
            "pane read",
            "claude --session-id abc --instructions 'do it'\nerror: unknown option '--instructions'\n",
        )
        .answer("pane close", &envelope(json!({"type": "pane_closed"})));
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap_err();
    assert_eq!(err.backend_code(), Some("timeout"));
    assert_eq!(err.exit_code(), 1);
    assert!(
        err.to_string()
            .contains("error: unknown option '--instructions'"),
        "{err}"
    );
    let calls = fake.calls();
    assert_eq!(
        calls[3],
        vec![
            "pane", "read", "w1:p2", "--source", "recent", "--lines", "20", "--format", "text"
        ]
    );
    assert_eq!(calls[4], vec!["pane", "close", "w1:p2"]);
}

#[tokio::test]
async fn open_agent_start_failure_on_reused_shell_prompt_pane_does_not_close_it() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list(FOLDER))
        .answer(
            "pane process-info",
            &envelope(process_info("w1:p1", 100, 100, None)),
        )
        .error(
            "agent start",
            "timeout",
            "timed out waiting for agent startup",
        )
        .answer("pane read", "some earlier shell output\n");
    let mut req = request(Placement::Panel);
    req.env.clear();
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&req)
        .await
        .unwrap_err();
    assert_eq!(err.backend_code(), Some("timeout"));
    assert!(
        !fake.calls().iter().any(|c| c[..2] == ["pane", "close"]),
        "a reused pre-existing pane must never be closed on agent_start failure: {:?}",
        fake.calls()
    );
}

#[tokio::test]
async fn open_program_override_is_unsupported() {
    let fake = FakeHerdr::new();
    let mut req = request(Placement::Tab);
    req.program = "/opt/claude-nightly".into();
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&req)
        .await
        .unwrap_err();
    assert!(matches!(err, LaunchError::Unsupported(_)), "{err}");
    assert_eq!(err.exit_code(), 2);
    assert!(fake.calls().is_empty());
}

#[tokio::test]
async fn open_pane_list_without_panes_is_malformed() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &envelope(json!({"type": "pane_list"})));
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap_err();
    assert!(matches!(err, LaunchError::Malformed { .. }), "{err}");
}

#[tokio::test]
async fn open_with_prompt_starts_without_the_token_then_prompts_with_it() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    fake.answer("agent prompt", &agent_prompted("w1:p2", "working"));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&prompted(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.delivered, Some(true));
    assert!(surface.confirmed);
    assert_eq!(surface.agent_state.as_deref(), Some("working"));
    assert_eq!(surface.prompt_detail, None);
    let calls = fake.calls();
    assert_eq!(calls[2][..2], ["agent", "start"]);
    assert!(
        !calls[2].contains(&"fix it".to_string()),
        "the prompt must not be a claude argument: {:?}",
        calls[2]
    );
    assert_eq!(
        calls[3],
        vec![
            "agent",
            "prompt",
            "cc-11111111",
            "fix it",
            "--wait",
            "--until",
            "working",
            "--until",
            "blocked",
            "--timeout",
            "30000"
        ]
    );
}

#[tokio::test]
async fn open_with_prompt_blocked_after_submission_is_confirmed_with_blocked_state() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    fake.answer("agent prompt", &agent_prompted("w1:p2", "blocked"));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&prompted(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.delivered, Some(true));
    assert!(surface.confirmed);
    assert_eq!(surface.agent_state.as_deref(), Some("blocked"));
}

#[tokio::test]
async fn open_with_prompt_stalled_is_delivered_but_unconfirmed() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    fake.error(
        "agent prompt",
        "agent_prompt_stalled",
        "no working or blocked state within 5000ms",
    );
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&prompted(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.delivered, Some(true));
    assert!(!surface.confirmed);
    assert_eq!(surface.agent_state.as_deref(), Some("idle"));
    assert!(
        surface
            .prompt_detail
            .as_deref()
            .unwrap()
            .contains("agent_prompt_stalled"),
        "{:?}",
        surface.prompt_detail
    );
}

#[tokio::test]
async fn open_with_prompt_blocked_before_send_is_not_delivered() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    fake.error(
        "agent prompt",
        "agent_blocked",
        "agent is blocked on a dialog",
    );
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&prompted(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.delivered, Some(false));
    assert!(!surface.confirmed);
    assert_eq!(surface.agent_state.as_deref(), Some("blocked"));
    assert!(
        surface
            .prompt_detail
            .as_deref()
            .unwrap()
            .contains("blocked on a dialog"),
        "{:?}",
        surface.prompt_detail
    );
}

#[tokio::test]
async fn open_with_prompt_unknown_state_after_submission_is_unconfirmed() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    fake.answer("agent prompt", &agent_prompted("w1:p2", "unknown"));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&prompted(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.delivered, Some(true));
    assert!(!surface.confirmed);
    assert!(surface.prompt_detail.unwrap().contains("unknown"));
}

#[tokio::test]
async fn open_with_prompt_other_prompt_error_fails_the_open() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    fake.error("agent prompt", "agent_not_found", "no agent cc-11111111");
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&prompted(Placement::Tab))
        .await
        .unwrap_err();
    assert_eq!(err.backend_code(), Some("agent_not_found"));
}

#[tokio::test]
async fn open_without_prompt_never_calls_agent_prompt() {
    let fake = FakeHerdr::new();
    tab_in_existing(&fake, None);
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.delivered, None);
    assert!(!surface.confirmed);
    assert!(
        !fake.calls().iter().any(|c| c[..2] == ["agent", "prompt"]),
        "{:?}",
        fake.calls()
    );
}
