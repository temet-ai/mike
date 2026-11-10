use std::collections::BTreeSet;

use anyhow::{Context as _, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use super::*;

const FIXTURE: &str = include_str!("fixtures/continue_prompts.jsonl");

#[derive(Deserialize)]
struct Row {
    turn_id: i64,
    expected_slug: Option<String>,
    prompt: String,
    now: DateTime<Utc>,
    session: RowSession,
    threads: Vec<RowThread>,
}

#[derive(Deserialize)]
struct RowSession {
    session_uuid: Uuid,
    cwd: Option<String>,
    recent_turns: Vec<String>,
}

#[derive(Deserialize)]
struct RowThread {
    slug: String,
    title: String,
    goal: String,
    branch: Option<String>,
    worktree: Option<String>,
    last_session_uuid: Option<Uuid>,
    last_checkpoint_at: DateTime<Utc>,
    linked_sessions: BTreeSet<Uuid>,
    checkout_dirs: BTreeSet<String>,
}

impl RowThread {
    fn facts(&self) -> ThreadFacts {
        ThreadFacts {
            slug: self.slug.clone(),
            title: self.title.clone(),
            goal: self.goal.clone(),
            status: "active".to_owned(),
            branch: self.branch.clone(),
            worktree: self.worktree.clone(),
            last_session_uuid: self.last_session_uuid,
            last_checkpoint_at: self.last_checkpoint_at,
            linked_sessions: self.linked_sessions.clone(),
            checkout_dirs: self.checkout_dirs.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct Tally {
    right: usize,
    wrong: usize,
    silent: usize,
    abstained: usize,
    spoke_unasked: usize,
}

fn picked(resolution: &Resolution) -> Vec<&str> {
    match resolution {
        Resolution::One(pick) => vec![pick.slug.as_str()],
        Resolution::Several(picks) => picks.iter().map(|pick| pick.slug.as_str()).collect(),
        Resolution::None => Vec::new(),
    }
}

fn grade(row: &Row, tally: &mut Tally) {
    let threads: Vec<ThreadFacts> = row.threads.iter().map(RowThread::facts).collect();
    let resolution = resolve(&ResolveInput {
        prompt: Some(&row.prompt),
        session: SessionFacts {
            session_uuid: row.session.session_uuid,
            cwd: row.session.cwd.as_deref(),
            branch: None,
            recent_turn_text: row
                .session
                .recent_turns
                .iter()
                .map(String::as_str)
                .collect(),
        },
        threads: &threads,
        now: row.now,
    });
    let slugs = picked(&resolution);
    match (&row.expected_slug, slugs.as_slice()) {
        (None, []) => tally.abstained += 1,
        (None, _) => tally.spoke_unasked += 1,
        (Some(_), []) => tally.silent += 1,
        (Some(want), [one]) if one == want => tally.right += 1,
        (Some(_), _) => tally.wrong += 1,
    }
}

#[test]
fn resolve_continue_prompt_fixture_names_the_thread_worked_on_and_rarely_a_wrong_one() -> Result<()>
{
    let mut tally = Tally::default();
    for (line, text) in FIXTURE.lines().enumerate() {
        let row: Row = serde_json::from_str(text).with_context(|| format!("row {}", line + 1))?;
        ensure!(row.turn_id > 0, "row {} has no turn id", line + 1);
        grade(&row, &mut tally);
    }
    ensure!(
        tally.right >= 8 && tally.wrong <= 2 && tally.spoke_unasked == 0,
        "resolver regressed on the continue-prompt fixture: {tally:?}"
    );
    Ok(())
}
