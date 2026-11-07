use std::collections::BTreeMap;

use serde_json::json;
use uuid::Uuid;

use super::close::session_mismatch;
use super::open::{agent_command, run_argv, session_uuid_from_args, with_caller_path};
use super::place::{Place, choose_space, run_space_id, spaces_matching};
use super::probe::agent_ran;
use super::prompt::turn_started;
use super::terminal::{Terminal, live_addresses};

fn app_ls() -> serde_json::Value {
    json!({"spaces": [
        {"id": "spA", "machine": null, "tabs": [{"id": "tb1", "terminals": [{"terminal": "t2"}, {"terminal": "t8"}]}]},
        {"id": "spB", "machine": null, "tabs": [{"id": "tb2", "terminals": [{"terminal": "t9"}]}]},
        {"id": "spR", "machine": "infra", "tabs": [{"id": "tb3", "terminals": [{"terminal": "t4"}]}]}
    ]})
}

fn hc_ls() -> serde_json::Value {
    json!({"terminals": [
        {"address": "t2", "cwd": "/home/vimes", "state": "live"},
        {"address": "t8", "cwd": "/repo/sub", "state": "live"},
        {"address": "t9", "cwd": "/repo", "state": "live"},
        {"address": "t4", "cwd": "/repo", "state": "live"}
    ]})
}

#[test]
fn spaces_matching_folder_lists_local_spaces_with_a_terminal_inside_it() {
    assert_eq!(
        spaces_matching(&app_ls(), &hc_ls(), "/repo"),
        vec!["spA", "spB"]
    );
    assert_eq!(
        spaces_matching(&app_ls(), &hc_ls(), "/repo/sub/"),
        vec!["spA"]
    );
    assert!(spaces_matching(&app_ls(), &hc_ls(), "/rep").is_empty());
}

#[test]
fn spaces_matching_without_spaces_is_empty() {
    assert!(spaces_matching(&json!({}), &hc_ls(), "/repo").is_empty());
}

#[test]
fn choose_space_prefers_the_callers_space_when_it_is_in_the_folder() {
    let candidates = vec!["spA".to_string(), "spB".to_string()];
    let place = choose_space(Some("spB".into()), &candidates, "tbX".into());
    assert_eq!((place.space.as_str(), place.reused), ("spB", true));
    assert!(place.placement.contains("caller"), "{}", place.placement);
}

#[test]
fn choose_space_caller_elsewhere_takes_the_folders_space() {
    let candidates = vec!["spA".to_string()];
    let place = choose_space(Some("spZ".into()), &candidates, "tbX".into());
    assert_eq!(place.space, "spA");
}

#[test]
fn choose_space_no_match_makes_a_new_space() {
    let Place {
        space, tab, reused, ..
    } = choose_space(None, &[], "tbX".into());
    assert!(space.starts_with("sp") && space.len() > 2);
    assert_eq!(tab, "tbX");
    assert!(!reused);
}

#[test]
fn run_space_id_is_stable_and_label_safe() {
    assert_eq!(run_space_id("run 1/a"), "spmike-run-1-a");
    assert_eq!(run_space_id("r"), run_space_id("r"));
}

#[test]
fn terminal_from_get_reads_labels_status_and_foreground_pid() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let value = json!({
        "address": "t13", "state": "live", "cwd": "/repo", "pid": 10, "turn": 9,
        "foreground": {"pid": 42, "name": "claude"},
        "status": {"app": "claude-code", "state": "working"},
        "labels": {"herdr.space": "spA", "herdr.tab": "tb1", "mike.session": session.to_string()}
    });
    let t = Terminal::from_get(&value).ok_or_else(|| anyhow::anyhow!("does not parse"))?;
    assert_eq!(t.address, "t13");
    assert!(t.live);
    assert_eq!(t.pid, Some(42));
    assert_eq!(t.status.as_deref(), Some("working"));
    assert_eq!(t.space.as_deref(), Some("spA"));
    assert_eq!(t.session, Some(session));
    let surface = t.surface("tab", true);
    assert_eq!(surface.workspace_ref.as_deref(), Some("spA"));
    assert_eq!(surface.surface_ref, "t13");
    Ok(())
}

#[test]
fn terminal_from_get_without_address_is_none() {
    assert!(Terminal::from_get(&json!({"state": "live"})).is_none());
}

#[test]
fn live_addresses_skip_finished_terminals() {
    let listed = json!({"terminals": [
        {"address": "t1", "state": "live"},
        {"address": "t2", "state": "finished"},
        {"address": "t3"}
    ]});
    assert_eq!(live_addresses(&listed), vec!["t1", "t3"]);
}

#[test]
fn a_seat_gets_the_callers_path_so_the_server_can_find_claude() -> anyhow::Result<()> {
    let env = with_caller_path(
        &BTreeMap::new(),
        Some("/home/me/.local/bin:/usr/bin".into()),
    );
    let cmd = agent_command("claude", &[], &env)?;
    assert_eq!(
        cmd,
        vec!["env", "PATH=/home/me/.local/bin:/usr/bin", "claude"]
    );
    Ok(())
}

#[test]
fn a_path_the_launch_set_is_kept_and_no_path_adds_nothing() {
    let mut set = BTreeMap::new();
    set.insert("PATH".to_string(), "/seat".to_string());
    assert_eq!(
        with_caller_path(&set, Some("/caller".into()))["PATH"],
        "/seat"
    );
    assert!(with_caller_path(&BTreeMap::new(), None).is_empty());
    assert!(with_caller_path(&BTreeMap::new(), Some(String::new())).is_empty());
}

#[test]
fn agent_command_without_env_is_the_program_then_its_args() -> anyhow::Result<()> {
    let cmd = agent_command(
        "claude",
        &["--model".into(), "opus".into()],
        &BTreeMap::new(),
    )?;
    assert_eq!(cmd, vec!["claude", "--model", "opus"]);
    Ok(())
}

#[test]
fn agent_command_with_env_runs_through_env() -> anyhow::Result<()> {
    let mut env = BTreeMap::new();
    env.insert("A".to_string(), "1 2".to_string());
    let cmd = agent_command("claude", &[], &env)?;
    assert_eq!(cmd, vec!["env", "A=1 2", "claude"]);
    Ok(())
}

#[test]
fn run_argv_starts_in_the_background_without_a_parent_and_labels_the_tab() {
    let args = run_argv(
        "/repo",
        &[("herdr.space", "spA".into()), ("herdr.tab", "tb1".into())],
        &["claude".into()],
    );
    assert_eq!(
        args,
        vec![
            "run",
            "--bg",
            "--keep",
            "--json",
            "--no-parent",
            "--cwd",
            "/repo",
            "--label",
            "herdr.space=spA",
            "--label",
            "herdr.tab=tb1",
            "--",
            "claude"
        ]
    );
}

#[test]
fn session_uuid_from_args_reads_both_forms() {
    let id = Uuid::new_v4();
    assert_eq!(
        session_uuid_from_args(&["--session-id".into(), id.to_string()]),
        Some(id)
    );
    assert_eq!(
        session_uuid_from_args(&[format!("--session-id={id}")]),
        Some(id)
    );
    assert_eq!(session_uuid_from_args(&[]), None);
}

fn term(status: Option<&str>, turn: Option<u64>) -> Terminal {
    Terminal {
        address: "t1".into(),
        live: true,
        cwd: None,
        pid: None,
        status: status.map(str::to_string),
        turn,
        space: None,
        tab: None,
        session: None,
    }
}

#[test]
fn turn_started_when_working_or_the_turn_advanced() {
    let before = term(Some("idle"), Some(1));
    assert!(turn_started(Some(&before), &term(Some("working"), Some(1))));
    assert!(turn_started(Some(&before), &term(Some("idle"), Some(2))));
    assert!(!turn_started(Some(&before), &term(Some("idle"), Some(1))));
}

#[test]
fn session_mismatch_names_the_other_session_or_a_missing_label() {
    let expected = Uuid::new_v4();
    let mut t = term(None, None);
    assert!(session_mismatch(&t, expected).is_some());
    t.session = Some(expected);
    assert!(session_mismatch(&t, expected).is_none());
    t.session = Some(Uuid::new_v4());
    assert!(session_mismatch(&t, expected).is_some_and(|d| d.contains("not")));
}

#[test]
fn validation_fails_when_herdr_core_cannot_find_claude() {
    let missing =
        json!({"output": "env: 'claude': No such file or directory", "wait": {"exit_code": 127}});
    let check = agent_ran(&missing);
    assert!(!check.passed);
    assert!(check.detail.contains("No such file"));
    let found = json!({"output": "2.1.318 (Claude Code)", "wait": {"exit_code": 0}});
    assert!(agent_ran(&found).passed);
    assert!(!agent_ran(&json!({})).passed);
}
