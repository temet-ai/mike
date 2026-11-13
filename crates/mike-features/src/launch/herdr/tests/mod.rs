#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

pub mod channels_dialog_tests;
pub mod close_tests;
pub mod fake;
pub mod live;
pub mod open_busy_tests;
pub mod open_tests;
pub mod probe_tests;
pub mod prompt_tests;
pub mod run_layout_tests;
pub mod validate_tests;

use serde_json::{Value, json};

pub use fake::FakeHerdr;

pub fn envelope(result: Value) -> String {
    json!({"id": "cli:fake", "result": result}).to_string()
}

pub fn pane_info(workspace: &str, tab: &str, pane: &str, cwd: &str) -> Value {
    json!({
        "agent_status": "unknown", "cwd": cwd, "focused": false, "foreground_cwd": cwd,
        "pane_id": pane, "revision": 0, "tab_id": tab, "terminal_id": "term_x", "workspace_id": workspace
    })
}

pub fn agent_entry(pane: &str, agent: &str, session: Option<&str>, status: &str) -> Value {
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

pub fn process_info(
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
