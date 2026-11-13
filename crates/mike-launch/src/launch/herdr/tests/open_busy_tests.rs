#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::collections::BTreeMap;

use serde_json::json;

use super::{FakeHerdr, agent_entry, envelope, pane_info, process_info};
use crate::launch::backend::{OpenRequest, Placement, TerminalBackend};
use crate::launch::herdr::HerdrBackend;

const FOLDER: &str = "/tmp/project";
const SESSION: &str = "11111111-2222-4333-8444-555555555555";

fn request(placement: Placement) -> OpenRequest {
    OpenRequest {
        folder: FOLDER.into(),
        placement,
        env: BTreeMap::new(),
        title: "cc·opus".into(),
        run_tab: None,
        run: None,
        program: "claude".into(),
        args: vec!["--session-id".into(), SESSION.into()],
        name: "cc-11111111".into(),
        prompt: None,
        prompt_wait_ms: 30_000,
    }
}

fn pane_list() -> String {
    envelope(json!({"panes": [pane_info("w1", "w1:t1", "w1:p1", FOLDER)], "type": "pane_list"}))
}

fn agent_started(pane: &str) -> String {
    envelope(
        json!({"agent": agent_entry(pane, "claude", Some(SESSION), "idle"), "argv": ["claude"], "type": "agent_started"}),
    )
}

fn agent_start_calls(fake: &FakeHerdr) -> usize {
    fake.calls()
        .iter()
        .filter(|c| {
            c.first().map(String::as_str) == Some("agent")
                && c.get(1).map(String::as_str) == Some("start")
        })
        .count()
}

#[tokio::test]
async fn open_retries_agent_start_while_a_new_pane_is_still_starting_its_shell() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list())
        .answer("tab create", &envelope(json!({"root_pane": pane_info("w1", "w1:t2", "w1:p2", FOLDER), "type": "tab_created"})))
        .error("agent start", "agent_pane_busy", "agent target pane w1:p2 is not an available shell")
        .error("agent start", "agent_pane_busy", "agent target pane w1:p2 is not an available shell")
        .answer("agent start", &agent_started("w1:p2"))
        .answer("pane process-info", &envelope(process_info("w1:p2", 100, 200, Some(200))));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Tab))
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w1:p2");
    assert_eq!(surface.agent_state.as_deref(), Some("idle"));
    assert_eq!(agent_start_calls(&fake), 3);
    assert!(
        !fake
            .calls()
            .iter()
            .any(|c| c.first().map(String::as_str) == Some("pane")
                && c.get(1).map(String::as_str) == Some("close")),
        "a pane that came up after retrying must not be closed"
    );
}

#[tokio::test]
async fn open_does_not_retry_a_busy_reused_pane() {
    let fake = FakeHerdr::new();
    fake.answer("pane list", &pane_list())
        .answer(
            "pane process-info",
            &envelope(process_info("w1:p1", 100, 100, None)),
        )
        .error(
            "agent start",
            "agent_pane_busy",
            "agent target pane w1:p1 is not an available shell",
        )
        .answer("pane read", "user output\n");
    let err = HerdrBackend::with_binary(fake.bin())
        .open(&request(Placement::Panel))
        .await
        .unwrap_err();
    assert_eq!(err.backend_code(), Some("agent_pane_busy"));
    assert_eq!(agent_start_calls(&fake), 1);
}
