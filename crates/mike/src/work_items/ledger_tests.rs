use chrono::{DateTime, TimeZone, Utc};

use super::ledger::{
    GitActivity, GitOutputRow, WorkItemRow, build_ledger, derived_state, parse_commits,
    parse_pushes,
};

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 23, 10, minute, 0)
        .single()
        .unwrap_or_default()
}

fn agent_row(task_id: &str, state: &str, last_edit: Option<u32>) -> WorkItemRow {
    WorkItemRow {
        kind: "agent".into(),
        task_id: task_id.into(),
        description: Some("Workbench: project picker".into()),
        owner_agent_id: None,
        state: state.into(),
        launched_at: at(0),
        finished_at: None,
        exit_code: None,
        summary: None,
        last_request_at: Some(at(50)),
        request_count: 40,
        last_edit_at: last_edit.map(at),
    }
}

fn bash_row(task_id: &str, owner: &str, state: &str) -> WorkItemRow {
    WorkItemRow {
        kind: "bash".into(),
        task_id: task_id.into(),
        description: Some("deploy".into()),
        owner_agent_id: Some(owner.into()),
        state: state.into(),
        launched_at: at(45),
        finished_at: None,
        exit_code: None,
        summary: None,
        last_request_at: None,
        request_count: 0,
        last_edit_at: None,
    }
}

const PUSH_OUTPUT: &str = "To github.com:temet-ai/mike.git\n   6aa5d52c..af93617c  main -> main\n";

#[test]
fn push_range_and_target_are_parsed() {
    let pushes = parse_pushes(PUSH_OUTPUT, at(1));
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].range, "6aa5d52c..af93617c");
    assert_eq!(pushes[0].target, "main");
}

#[test]
fn forced_push_range_is_parsed_and_noise_is_not() {
    let text =
        " + 1111111...2222222 feat -> feat (forced update)\nEverything up-to-date\nremote: done";
    let pushes = parse_pushes(text, at(1));
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].range, "1111111...2222222");
}

#[test]
fn commit_line_yields_sha_and_subject() {
    let commits = parse_commits(
        "[main 5533e358] feat(assistant): render replies as markdown\n 3 files changed",
        at(2),
    );
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].sha, "5533e358");
    assert_eq!(
        commits[0].subject,
        "feat(assistant): render replies as markdown"
    );
}

#[test]
fn bracketed_text_that_is_not_a_commit_is_ignored() {
    assert!(parse_commits("[info] nothing here", at(2)).is_empty());
}

#[test]
fn running_agent_that_pushed_after_its_last_edit_is_landed_polling() {
    let git = GitActivity {
        pushes: parse_pushes(PUSH_OUTPUT, at(40)),
        commits: vec![],
    };
    assert_eq!(
        derived_state(&agent_row("a1", "running", Some(30)), &git, false),
        "landed_polling"
    );
    assert_eq!(
        derived_state(&agent_row("a1", "running", Some(45)), &git, false),
        "running"
    );
    assert_eq!(
        derived_state(&agent_row("a1", "killed", Some(30)), &git, false),
        "killed"
    );
}

#[test]
fn running_agent_without_pushes_stays_running() {
    assert_eq!(
        derived_state(
            &agent_row("a1", "running", Some(30)),
            &GitActivity::default(),
            false
        ),
        "running"
    );
}

#[test]
fn ledger_groups_git_by_agent_and_lists_open_background_work() {
    let rows = vec![
        agent_row("a6fecf0fcf0e7b063", "running", Some(30)),
        bash_row("bdeploy", "a6fecf0fcf0e7b063", "running"),
        bash_row("bdone", "a6fecf0fcf0e7b063", "completed"),
    ];
    let git = vec![
        GitOutputRow {
            agent_id: Some("a6fecf0fcf0e7b063".into()),
            at: at(40),
            text: PUSH_OUTPUT.into(),
        },
        GitOutputRow {
            agent_id: None,
            at: at(41),
            text: "[main abcdef1] docs: handoff".into(),
        },
    ];
    let ledger = build_ledger("6f544a61", &rows, &git);
    assert_eq!(ledger.agents.len(), 1);
    let agent = &ledger.agents[0];
    assert_eq!(agent.state, "landed_polling");
    assert_eq!(agent.git.pushes.len(), 1);
    assert_eq!(agent.open_background, vec!["bdeploy".to_string()]);
    assert_eq!(ledger.main_thread_git.commits[0].sha, "abcdef1");
    assert_eq!(ledger.running_agents, 0);
    assert_eq!(ledger.running_background, 1);
}

#[test]
fn open_child_of_a_finished_parent_is_orphaned() {
    let mut child = agent_row("child", "running", None);
    child.owner_agent_id = Some("parent".into());
    let mut job = bash_row("bjob", "parent", "running");
    job.owner_agent_id = Some("parent".into());
    let rows = vec![agent_row("parent", "completed", None), child, job];
    let ledger = build_ledger("s", &rows, &[]);
    let child_state = ledger
        .agents
        .iter()
        .find(|a| a.agent_id == "child")
        .map(|a| a.state.clone());
    assert_eq!(child_state.as_deref(), Some("orphaned"));
    assert_eq!(ledger.background[0].state, "orphaned");
    assert_eq!((ledger.orphaned_agents, ledger.orphaned_background), (1, 1));
    assert_eq!(ledger.running_agents, 0);
}

#[test]
fn open_child_of_a_running_parent_stays_running() {
    let mut child = agent_row("child", "running", None);
    child.owner_agent_id = Some("parent".into());
    let rows = vec![agent_row("parent", "running", None), child];
    let ledger = build_ledger("s", &rows, &[]);
    assert_eq!(ledger.orphaned_agents, 0);
    assert_eq!(ledger.running_agents, 2);
}

#[test]
fn grandchildren_of_a_finished_agent_are_orphaned_too() {
    let mut child = agent_row("child", "running", None);
    child.owner_agent_id = Some("parent".into());
    let mut grandchild = agent_row("grandchild", "running", None);
    grandchild.owner_agent_id = Some("child".into());
    let rows = vec![agent_row("parent", "completed", None), child, grandchild];
    let ledger = build_ledger("s", &rows, &[]);
    assert_eq!(ledger.orphaned_agents, 2);
    assert_eq!(ledger.running_agents, 0);
}

#[test]
fn finished_agent_that_keeps_making_requests_is_running_again() {
    let mut row = agent_row("a1", "completed", None);
    row.finished_at = Some(at(20));
    row.last_request_at = Some(at(35));
    assert_eq!(
        derived_state(&row, &GitActivity::default(), false),
        "running"
    );
    row.last_request_at = Some(at(19));
    assert_eq!(
        derived_state(&row, &GitActivity::default(), false),
        "completed"
    );
}
