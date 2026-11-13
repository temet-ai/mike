use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::change_record::{ChangeEvent, ChangeKind};
use crate::store::persistence::{PersistError, PersistHandle};

const SHORT_SHA: usize = 8;
const PATHS_IN_SHORT_LINE: usize = 2;
pub const NO_AUTHOR: &str = "no author known";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangeLine {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub who: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<uuid::Uuid>,
    pub how: &'static str,
    pub paths: Vec<String>,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pushed: bool,
}

pub type Names = HashMap<i64, (String, uuid::Uuid)>;

pub fn how(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Edit => "edited",
        ChangeKind::ShellChange => "changed from the shell",
        ChangeKind::Commit => "committed",
        ChangeKind::Push => "pushed",
        ChangeKind::Check => "check finished",
        ChangeKind::Verdict => "verdict",
        ChangeKind::Note => "note",
        ChangeKind::Mail => "mail",
        ChangeKind::Brief => "brief",
        ChangeKind::TaskState => "task state",
    }
}

pub async fn names_for(
    persist: &PersistHandle,
    events: &[ChangeEvent],
) -> Result<Names, PersistError> {
    let mut ids: Vec<i64> = events.iter().filter_map(|e| e.session_id).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(Names::new());
    }
    let identities = persist.session_identities(&ids).await?;
    Ok(identities
        .into_iter()
        .map(|i| {
            let name = i.name.unwrap_or_else(|| i.uuid.to_string());
            (i.id, (name, i.uuid))
        })
        .collect())
}

pub fn line_of(event: &ChangeEvent, names: &Names) -> ChangeLine {
    let known = event.session_id.and_then(|id| names.get(&id));
    let who = match (event.session_id, &event.task) {
        (Some(id), _) => known
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| format!("session {id}")),
        (None, Some(task)) => format!("task {task}"),
        (None, None) => NO_AUTHOR.to_owned(),
    };
    let commit = (event.kind == ChangeKind::Commit)
        .then_some(event.after_sha.as_deref())
        .flatten()
        .map(|sha| sha.chars().take(SHORT_SHA).collect());
    ChangeLine {
        id: event.id,
        at: event.at,
        who,
        session: known.map(|(_, uuid)| *uuid),
        how: how(event.kind),
        paths: event.paths.clone(),
        subject: event.subject.clone(),
        commit,
        pushed: false,
    }
}

pub fn short(line: &ChangeLine) -> String {
    let shown: Vec<&str> = line
        .paths
        .iter()
        .take(PATHS_IN_SHORT_LINE)
        .map(String::as_str)
        .collect();
    let rest = line.paths.len().saturating_sub(PATHS_IN_SHORT_LINE);
    let mut what = shown.join(", ");
    if rest > 0 {
        what.push_str(&format!(" (+{rest})"));
    }
    if what.is_empty() {
        what = line.subject.clone();
    }
    let commit = line
        .commit
        .as_ref()
        .map(|c| format!(" {c}"))
        .unwrap_or_default();
    format!(
        "{} {} {}{} at {}",
        line.who,
        line.how,
        what,
        commit,
        line.at.format("%H:%M:%S")
    )
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn line(paths: &[&str]) -> ChangeLine {
        ChangeLine {
            id: 1,
            at: DateTime::UNIX_EPOCH,
            who: "seat".to_owned(),
            session: None,
            how: "edited",
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
            subject: "subject".to_owned(),
            commit: None,
            pushed: false,
        }
    }

    #[test]
    fn short_line_names_two_paths_and_counts_the_rest() {
        let text = short(&line(&["a.rs", "b.rs", "c.rs", "d.rs"]));
        assert_eq!(text, "seat edited a.rs, b.rs (+2) at 00:00:00");
    }

    #[test]
    fn short_line_without_paths_falls_back_to_the_subject() {
        assert_eq!(short(&line(&[])), "seat edited subject at 00:00:00");
    }

    #[test]
    fn a_commit_line_carries_the_short_sha() {
        let mut commit = line(&["a.rs"]);
        commit.how = how(ChangeKind::Commit);
        commit.commit = Some("abcdef12".to_owned());
        assert_eq!(short(&commit), "seat committed a.rs abcdef12 at 00:00:00");
    }
}
