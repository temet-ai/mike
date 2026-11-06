#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

mod channels_dialog_tests;
mod close_tests;
mod fake;
mod live;
mod open_busy_tests;
mod open_tests;
mod probe_tests;
mod prompt_tests;
mod run_layout_tests;
mod validate_tests;

use serde_json::{Value, json};

pub(super) use fake::FakeHerdr;

pub(super) fn envelope(result: Value) -> String {
    json!({"id": "cli:fake", "result": result}).to_string()
}

pub(super) fn pane_info(workspace: &str, tab: &str, pane: &str, cwd: &str) -> Value {
    json!({
        "agent_status": "unknown", "cwd": cwd, "focused": false, "foreground_cwd": cwd,
        "pane_id": pane, "revision": 0, "tab_id": tab, "terminal_id": "term_x", "workspace_id": workspace
    })
}

pub(super) fn agent_entry(pane: &str, agent: &str, session: Option<&str>, status: &str) -> Value {
    let (workspace, tab) = pane
        .split_once(':')
        .map(|(w, _)| (w, format!("{w}:t1")))
        .unwrap_or(("w1", "w1:t1".into()));
    let mut entry = json!({
        "agent": agent, "agent_status": status, "cwd": "/tmp/project", "focused": false,
        "name": "cc-x", "pane_id": pane, "revision": 3, "tab_id": tab, "terminal_id": "term_y",
        "workspace_id": workspace, "interactive_ready": true, "launch_pending": false
    });
    if let Some(session) = session {
        entry["agent_session"] =
            json!({"agent": agent, "kind": "id", "source": "hook", "value": session});
    }
    entry
}

pub(super) fn process_info(
    pane: &str,
    shell_pid: u64,
    leader: u64,
    claude_pid: Option<u64>,
) -> Value {
    let mut processes = vec![
        json!({"argv": ["/usr/bin/zsh"], "cmdline": "/usr/bin/zsh", "cwd": "/tmp/project", "name": "zsh", "pid": shell_pid}),
    ];
    if let Some(pid) = claude_pid {
        processes.push(json!({"argv": ["claude"], "cmdline": "claude", "cwd": "/tmp/project", "name": "claude", "pid": pid}));
    }
    json!({
        "process_info": {
            "foreground_process_group_id": leader, "foreground_processes": processes,
            "pane_id": pane, "shell_pid": shell_pid
        },
        "type": "pane_process_info"
    })
}
