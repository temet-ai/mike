use super::*;
use crate::store::persistence::launch::LaunchRow;
use serde_json::json;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn relaunch() -> Relaunch {
    Relaunch {
        folder: "/repo".into(),
        claude_args: args(&["--resume", "tmpl", "--fork-session", "--session-id", "x"]),
        prompt: Some("the seat's brief".into()),
        template_id: Some(12),
        run: Some("r1".into()),
        seat: Some("dev-T1".into()),
        role: Some("dev".into()),
        instructions: None,
        goal: None,
        worktree_ok: false,
        proxy_env: true,
        remote_control: true,
        launch_id: Some(4),
        backend: None,
    }
}

#[test]
fn corrected_args_resume_the_seat_and_pass_mike_explicitly() {
    let session = Uuid::from_u128(7);
    let fork = args(&[
        "--resume",
        "tmpl",
        "--fork-session",
        "--session-id",
        "abc",
        "--model",
        "sonnet",
        "--mcp-config={\"old\":1}",
    ]);
    let got = corrected_args(&fork, session, "{\"mcpServers\":{}}");
    assert_eq!(
        got,
        args(&[
            "--resume",
            &session.to_string(),
            "--model",
            "sonnet",
            "--mcp-config={\"mcpServers\":{}}",
        ])
    );
}

#[test]
fn fresh_args_start_the_same_session_id_from_the_original_launch() {
    let session = Uuid::from_u128(7);
    let fork = args(&[
        "--resume",
        "tmpl",
        "--fork-session",
        "--session-id",
        "abc",
        "--model",
        "sonnet",
        "--mcp-config={\"old\":1}",
    ]);
    let got = start::fresh_args(&fork, session, "{}");
    assert_eq!(
        got,
        args(&[
            "--resume",
            "tmpl",
            "--fork-session",
            "--model",
            "sonnet",
            "--session-id",
            &session.to_string(),
            "--mcp-config={}",
        ])
    );
}

#[tokio::test]
async fn wait_for_tools_sees_a_report_and_gives_up_after_the_wait() {
    let (seen, unseen) = (Uuid::new_v4(), Uuid::new_v4());
    mcp_seen::record(seen, chrono::Utc::now());
    let (wait, poll) = (Duration::from_millis(60), Duration::from_millis(10));
    assert!(wait_for_tools(seen, wait, poll).await);
    assert!(!wait_for_tools(unseen, wait, poll).await);
}

#[test]
fn a_relaunch_keeps_the_seat_and_turns_channel_push_off() {
    let req = relaunch_request(&relaunch(), Uuid::from_u128(9), "{}", Start::Resume);
    assert_eq!(req.seat.as_deref(), Some("dev-T1"));
    assert!(!req.channels);
    assert_eq!(req.prompt.as_deref(), Some(RELAUNCH_PROMPT));
    assert_eq!(req.claude_args[0], "--resume");
    assert_eq!(req.template_id, None);
}

#[test]
fn a_seat_that_never_started_is_started_fresh_with_its_own_brief() {
    let session = Uuid::from_u128(9);
    let req = relaunch_request(&relaunch(), session, "{}", Start::Fresh);
    assert_eq!(req.prompt.as_deref(), Some("the seat's brief"));
    assert_eq!(req.template_id, Some(12));
    let at = req
        .claude_args
        .iter()
        .position(|a| a == "--session-id")
        .map(|i| req.claude_args[i + 1].clone());
    assert_eq!(at, Some(session.to_string()));
    assert_eq!(
        &req.claude_args[..3],
        &args(&["--resume", "tmpl", "--fork-session"])[..]
    );
}

#[test]
fn a_failed_fresh_start_is_recorded_as_a_start_failure_not_missing_tools() {
    let resume = failed_outcome(FAILED_REASON, "no tools");
    let fresh = failed_outcome(START_FAILED, "no tools");
    assert_eq!(resume.reason.as_deref(), Some("mike_tools_missing"));
    assert!(resume.opened);
    assert_eq!(fresh.reason.as_deref(), Some("start_failed"));
    assert!(!fresh.opened);
}

fn launch(id: i64, seat: &str, session: Uuid, minutes: i64) -> LaunchRow {
    let at = chrono::Utc::now() - chrono::Duration::minutes(60 - minutes);
    let mut row = json!({
        "id": id, "session_uuid": session, "backend": "herdr", "folder": "/repo",
        "worktree": false, "run_id": "r1", "seat": seat, "claude_args": [],
        "prompt_present": true, "remote_control": true, "proxy_env": true,
        "opened": false, "confirmed": false, "result": "pending",
        "stderr_truncated": false, "created_at": at,
    });
    let absent = [
        "repo_root",
        "workspace_ref",
        "tab_ref",
        "surface_ref",
        "pid",
        "template_id",
        "model",
        "agent",
        "brief_mode",
        "brief_path",
        "brief_text",
        "brief_note_uuid",
        "brief_read_at",
        "requested_placement",
        "actual_placement",
        "split_direction",
        "anchor_ref",
        "channel_check",
        "channel_checked_at",
        "delivered",
        "reason",
        "stderr_text",
        "opened_at",
        "closed_at",
        "close_reason",
        "close_process_exited",
    ];
    for field in absent {
        row[field] = serde_json::Value::Null;
    }
    serde_json::from_value(row).unwrap_or_else(|e| panic!("launch fixture: {e}"))
}

#[test]
fn a_later_launch_of_the_same_seat_with_another_session_means_it_was_replaced() {
    let (dead, fresh) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let first = launch(1110, "backend-hard-T2", dead, 0);
    let other_seat = launch(1112, "web-T6", fresh, 1);
    assert!(!start::later_seat_launch(
        &[first.clone(), other_seat],
        "backend-hard-T2",
        dead
    ));
    let replacement = launch(1116, "backend-hard-T2", fresh, 6);
    assert!(start::later_seat_launch(
        &[first, replacement],
        "backend-hard-T2",
        dead
    ));
}

#[test]
fn a_relaunch_of_the_same_session_is_not_a_replacement() {
    let dead = Uuid::from_u128(1);
    let rows = [
        launch(1110, "backend-hard-T2", dead, 0),
        launch(1113, "backend-hard-T2", dead, 3),
    ];
    assert!(!start::later_seat_launch(&rows, "backend-hard-T2", dead));
}
