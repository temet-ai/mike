use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::checks::Missing;
use super::commit::covers;
use super::definition::{CommitWait, FleetDefinition, TaskDefinition};
use super::status::FleetStatus;
use super::tell::Notice;
use super::watch::git;

#[derive(Debug, PartialEq, Eq)]
pub struct Committed {
    pub sha: String,
    pub seat: String,
    pub session: Uuid,
    pub paths: Vec<String>,
    pub after: Vec<String>,
}

pub fn validate(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    for (id, task) in &def.tasks {
        if task.stub_by == Some(0) {
            found.push(Missing::new(
                format!("tasks.{id}.stub_by"),
                "must be positive minutes",
            ));
        }
        for (index, wait) in task.wait_for_commits.iter().enumerate() {
            let item = format!("tasks.{id}.wait_for_commits.{index}");
            if !def.tasks.contains_key(&wait.task) || wait.task == *id {
                found.push(Missing::new(&item, "task must name another existing task"));
            }
            if wait.paths.iter().any(|p| !valid_path(p)) {
                found.push(Missing::new(&item, "paths must be nonempty repository-relative files or directories without traversal or globs"));
            }
            if wait.after.as_deref().is_some_and(|s| !full_sha(s)) {
                found.push(Missing::new(&item, "after must be a full commit SHA"));
            }
        }
    }
    found
}

fn full_sha(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|c| c.is_ascii_hexdigit())
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['*', '?', '[', '\\'])
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn belongs(event: &Committed, task: &TaskDefinition) -> bool {
    task.seat.as_deref() == Some(event.seat.as_str())
        && task.session.as_deref().and_then(|s| s.parse().ok()) == Some(event.session)
        && event.paths.iter().any(|p| covers(&task.files, p))
}

fn matching<'a>(
    events: &'a [Committed],
    owner: &TaskDefinition,
    wait: &CommitWait,
) -> Option<&'a Committed> {
    events.iter().find(|event| {
        wait.after
            .as_ref()
            .is_none_or(|sha| event.after.contains(sha))
            && belongs(event, owner)
            && (wait.paths.is_empty() || event.paths.iter().any(|p| covers(&wait.paths, p)))
    })
}

pub fn notices(def: &FleetDefinition, status: &FleetStatus, events: &[Committed]) -> Vec<Notice> {
    let mut notices = deadlines(def, status, events);
    for (id, task) in &def.tasks {
        let Some(to) = task.session.as_deref().and_then(|s| s.parse().ok()) else {
            continue;
        };
        for wait in &task.wait_for_commits {
            let Some(owner) = def.tasks.get(&wait.task) else {
                continue;
            };
            let Some(event) = matching(events, owner, wait) else {
                continue;
            };
            notices.push(Notice {
                to,
                key: format!("commit-wait:{id}:{to}:{}", serde_json::to_string(wait).unwrap_or_default()),
                covers: Vec::new(),
                body: format!("[mike commit] Task {} ({}) committed {} touching {}. Your commit subscription is satisfied. This means committed, not that compilation or tests passed. Ask fleet_check for the committed-tree verdict; do not sleep-poll builds.", wait.task, event.seat, event.sha, event.paths.join(", ")),
            });
        }
    }
    notices
}

fn deadlines(def: &FleetDefinition, status: &FleetStatus, events: &[Committed]) -> Vec<Notice> {
    let mut notices = Vec::new();
    for (id, task) in &def.tasks {
        let (Some(minutes), Some(seat), Some(to)) = (
            task.stub_by,
            task.seat.as_deref(),
            task.session.as_deref().and_then(|s| s.parse::<Uuid>().ok()),
        ) else {
            continue;
        };
        let opened = status
            .seats
            .iter()
            .find(|s| s.seat == seat && !s.closed)
            .and_then(|s| s.opened_at);
        if opened.is_none_or(|at| (status.now - at).num_minutes() < i64::from(minutes))
            || events.iter().any(|e| belongs(e, task))
        {
            continue;
        }
        let body = format!(
            "[mike commit] Task {id}'s explicit stub deadline ({minutes} minutes after its seat opened) is due, and no commit in its owned files from this session has been observed. Commit the helper signatures as a small stub now, or explain the blocker to the coordinator. A first commit signals availability, not completed implementation or passing tests."
        );
        notices.push(Notice {
            to,
            key: format!("stub-due:{id}:{to}"),
            covers: Vec::new(),
            body,
        });
    }
    notices
}

fn identity(raw: &str) -> Option<(String, Uuid)> {
    let mut parts = raw.split_whitespace();
    let seat = parts.next()?.to_string();
    let session = parts
        .next()?
        .strip_prefix('(')?
        .strip_suffix(')')?
        .parse()
        .ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((seat, session))
}

async fn event(repo: &str, sha: &str) -> Option<Committed> {
    let trailer = git(
        repo,
        &[
            "show",
            "-s",
            "--format=%(trailers:key=Fleet-Seat,valueonly)",
            sha,
        ],
        None,
    )
    .await?;
    let (seat, session) = identity(String::from_utf8_lossy(&trailer).trim())
        .unwrap_or_else(|| (String::new(), Uuid::nil()));
    let paths = git(
        repo,
        &[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            sha,
        ],
        None,
    )
    .await?;
    Some(Committed {
        sha: sha.to_string(),
        seat,
        session,
        after: Vec::new(),
        paths: paths
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect(),
    })
}

pub async fn read(def: &FleetDefinition, since: Option<DateTime<Utc>>) -> Option<Vec<Committed>> {
    let since = since?;
    let range = format!("--since={}", since.to_rfc3339());
    let anchors: Vec<&str> = def
        .tasks
        .values()
        .flat_map(|t| &t.wait_for_commits)
        .filter_map(|w| w.after.as_deref())
        .collect();
    validate_anchors(&def.repo, &anchors).await?;
    let mut args = vec!["log", "--reverse", "--topo-order", "--format=%H"];
    args.push(&range);
    args.push("HEAD");
    let raw = git(&def.repo, &args, None).await?;
    let mut events = Vec::new();
    for sha in String::from_utf8_lossy(&raw)
        .lines()
        .filter(|s| full_sha(s))
    {
        let mut event = event(&def.repo, sha).await?;
        for anchor in &anchors {
            if sha != *anchor
                && git(
                    &def.repo,
                    &["merge-base", "--is-ancestor", anchor, sha],
                    None,
                )
                .await
                .is_some()
            {
                event.after.push((*anchor).to_string());
            }
        }
        events.push(event);
    }
    Some(events)
}

async fn validate_anchors(repo: &str, anchors: &[&str]) -> Option<()> {
    for sha in anchors {
        if !full_sha(sha)
            || git(repo, &["merge-base", "--is-ancestor", sha, "HEAD"], None)
                .await
                .is_none()
        {
            tracing::warn!(%sha, "fleet: commit subscription anchor is not an ancestor of HEAD");
            return None;
        }
    }
    Some(())
}

#[cfg(test)]
#[path = "tell_commits_tests.rs"]
pub mod tests;

pub fn brief(task: &TaskDefinition) -> String {
    let mut lines = Vec::new();
    if let Some(minutes) = task.stub_by {
        lines.push(format!("Stub deadline: commit the helper signatures in your owned files within {minutes} minutes of your seat opening. Publish a small stub before filling in the implementation."));
    }
    for wait in &task.wait_for_commits {
        let paths = if wait.paths.is_empty() {
            "its owned files".to_string()
        } else {
            wait.paths.join(", ")
        };
        let after = wait
            .after
            .as_deref()
            .map(|sha| format!(" after {sha}"))
            .unwrap_or_default();
        lines.push(format!("Commit subscription: mike will mail you once when task {} commits {paths}{after}. Carry on with independent work rather than sleep-polling files or builds. The signal means committed, not compilation or test success; ask fleet_check for the verdict.", wait.task));
    }
    lines.join("\n")
}
