use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct WorkItemRow {
    pub kind: String,
    pub task_id: String,
    pub description: Option<String>,
    pub owner_agent_id: Option<String>,
    pub state: String,
    pub launched_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub summary: Option<String>,
    pub last_request_at: Option<DateTime<Utc>>,
    pub request_count: i64,
    pub last_edit_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct GitOutputRow {
    pub agent_id: Option<String>,
    pub at: DateTime<Utc>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Push {
    pub range: String,
    pub target: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GitActivity {
    pub pushes: Vec<Push>,
    pub commits: Vec<Commit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentEntry {
    pub agent_id: String,
    pub description: Option<String>,
    pub dispatched_by: Option<String>,
    pub state: String,
    pub launched_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub last_request_at: Option<DateTime<Utc>>,
    pub request_count: i64,
    pub summary: Option<String>,
    pub git: GitActivity,
    pub open_background: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundEntry {
    pub task_id: String,
    pub owner_agent_id: Option<String>,
    pub description: Option<String>,
    pub state: String,
    pub launched_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ledger {
    pub session_id: String,
    pub agents: Vec<AgentEntry>,
    pub background: Vec<BackgroundEntry>,
    pub main_thread_git: GitActivity,
    pub running_agents: usize,
    pub running_background: usize,
    pub orphaned_agents: usize,
    pub orphaned_background: usize,
    pub attention: Vec<super::attention::Attention>,
}

pub fn parse_pushes(text: &str, at: DateTime<Utc>) -> Vec<Push> {
    text.lines()
        .filter_map(|line| parse_push_line(line, at))
        .collect()
}

fn parse_push_line(line: &str, at: DateTime<Utc>) -> Option<Push> {
    let (left, right) = line.trim().split_once(" -> ")?;
    let range = left
        .split_whitespace()
        .find(|token| *token != "+")?
        .trim_start_matches('+');
    let (from, to) = range.split_once("..")?;
    if !is_sha(from) || !is_sha(to.trim_start_matches('.')) {
        return None;
    }
    let target = right.split_whitespace().next()?.to_string();
    Some(Push {
        range: range.to_string(),
        target,
        at,
    })
}

pub fn parse_commits(text: &str, at: DateTime<Utc>) -> Vec<Commit> {
    text.lines()
        .filter_map(|line| parse_commit_line(line, at))
        .collect()
}

fn parse_commit_line(line: &str, at: DateTime<Utc>) -> Option<Commit> {
    let rest = line.trim().strip_prefix('[')?;
    let (head, subject) = rest.split_once("] ")?;
    let sha = head.split_whitespace().last()?;
    if !is_sha(sha) || head.split_whitespace().count() < 2 {
        return None;
    }
    Some(Commit {
        sha: sha.to_string(),
        subject: subject.trim().to_string(),
        at,
    })
}

fn is_sha(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn resumed_since_finish(row: &WorkItemRow) -> bool {
    match (row.finished_at, row.last_request_at) {
        (Some(finished), Some(last)) => last > finished,
        _ => false,
    }
}

pub fn derived_state(row: &WorkItemRow, git: &GitActivity, parent_finished: bool) -> String {
    if row.state != "running" && !resumed_since_finish(row) {
        return row.state.clone();
    }
    if parent_finished {
        return "orphaned".to_string();
    }
    let last_push = git.pushes.iter().map(|p| p.at).max();
    match (last_push, row.last_edit_at) {
        (Some(push), Some(edit)) if push >= edit => "landed_polling".to_string(),
        (Some(_), None) => "landed_polling".to_string(),
        _ => "running".to_string(),
    }
}

fn git_by_agent(rows: &[GitOutputRow]) -> BTreeMap<Option<String>, GitActivity> {
    let mut map: BTreeMap<Option<String>, GitActivity> = BTreeMap::new();
    for row in rows {
        let entry = map.entry(row.agent_id.clone()).or_default();
        entry.pushes.extend(parse_pushes(&row.text, row.at));
        entry.commits.extend(parse_commits(&row.text, row.at));
    }
    map
}

fn is_finished(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "killed")
}

fn is_open(row: &WorkItemRow) -> bool {
    row.state == "running" || resumed_since_finish(row)
}

fn finished_agents(rows: &[WorkItemRow]) -> HashSet<String> {
    let mut gone: HashSet<String> = rows
        .iter()
        .filter(|r| r.kind == "agent" && is_finished(&r.state) && !resumed_since_finish(r))
        .map(|r| r.task_id.clone())
        .collect();
    loop {
        let orphans: Vec<String> = rows
            .iter()
            .filter(|r| r.kind == "agent" && is_open(r) && !gone.contains(&r.task_id))
            .filter(|r| owner_finished(r, &gone))
            .map(|r| r.task_id.clone())
            .collect();
        if orphans.is_empty() {
            return gone;
        }
        gone.extend(orphans);
    }
}

fn owner_finished(row: &WorkItemRow, finished: &HashSet<String>) -> bool {
    row.owner_agent_id
        .as_ref()
        .is_some_and(|owner| finished.contains(owner))
}

fn background_entry(row: &WorkItemRow, finished: &HashSet<String>) -> BackgroundEntry {
    let state = if row.state == "running" && owner_finished(row, finished) {
        "orphaned".to_string()
    } else {
        row.state.clone()
    };
    BackgroundEntry {
        task_id: row.task_id.clone(),
        owner_agent_id: row.owner_agent_id.clone(),
        description: row.description.clone(),
        state,
        launched_at: row.launched_at,
        finished_at: row.finished_at,
        exit_code: row.exit_code,
    }
}

fn agent_entry(
    row: &WorkItemRow,
    git: GitActivity,
    open_background: Vec<String>,
    parent_finished: bool,
) -> AgentEntry {
    AgentEntry {
        agent_id: row.task_id.clone(),
        description: row.description.clone(),
        dispatched_by: row.owner_agent_id.clone(),
        state: derived_state(row, &git, parent_finished),
        launched_at: row.launched_at,
        finished_at: row.finished_at,
        last_request_at: row.last_request_at,
        request_count: row.request_count,
        summary: row.summary.clone(),
        git,
        open_background,
    }
}

pub fn build_ledger(
    session_id: &str,
    rows: &[WorkItemRow],
    git_rows: &[GitOutputRow],
) -> Ledger {
    let mut git = git_by_agent(git_rows);
    let finished = finished_agents(rows);
    let background: Vec<BackgroundEntry> = rows
        .iter()
        .filter(|r| r.kind == "bash")
        .map(|r| background_entry(r, &finished))
        .collect();
    let agents: Vec<AgentEntry> = rows
        .iter()
        .filter(|r| r.kind == "agent")
        .map(|r| {
            let open = open_background_of(&background, &r.task_id);
            let own_git = git.remove(&Some(r.task_id.clone())).unwrap_or_default();
            agent_entry(r, own_git, open, owner_finished(r, &finished))
        })
        .collect();
    Ledger {
        session_id: session_id.to_string(),
        running_agents: count_state(agents.iter().map(|a| a.state.as_str()), "running"),
        running_background: count_state(background.iter().map(|b| b.state.as_str()), "running"),
        orphaned_agents: count_state(agents.iter().map(|a| a.state.as_str()), "orphaned"),
        orphaned_background: count_state(background.iter().map(|b| b.state.as_str()), "orphaned"),
        main_thread_git: git.remove(&None).unwrap_or_default(),
        attention: Vec::new(),
        agents,
        background,
    }
}

fn count_state<'a>(states: impl Iterator<Item = &'a str>, wanted: &str) -> usize {
    states.filter(|s| *s == wanted).count()
}

fn open_background_of(background: &[BackgroundEntry], agent_id: &str) -> Vec<String> {
    background
        .iter()
        .filter(|b| b.state == "running" && b.owner_agent_id.as_deref() == Some(agent_id))
        .map(|b| b.task_id.clone())
        .collect()
}
