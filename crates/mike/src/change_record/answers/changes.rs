use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::caller::{AnswerError, Caller};
use super::lines::{ChangeLine, line_of, names_for};
use crate::change_record::{ChangeEvent, ChangeKind, ChangeQuery};
use crate::store::persistence::PersistHandle;

const DEFAULT_LIMIT: i64 = 200;
const FIRST_LOOK_HOURS: i64 = 6;
const HISTORY_READ: i64 = 500;
const OWN_KINDS: [ChangeKind; 3] = [
    ChangeKind::Edit,
    ChangeKind::ShellChange,
    ChangeKind::Commit,
];
const READ_WINDOW_DAYS: i64 = 7;
const MAX_PREFIXES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Since {
    Place,
    Time(DateTime<Utc>),
    Commit(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Mine,
    All,
    Paths(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangesAnswer {
    pub lines: Vec<ChangeLine>,
    pub more: bool,
    pub place: i64,
}

impl ChangesAnswer {
    fn empty(place: i64) -> Self {
        Self {
            lines: Vec::new(),
            more: false,
            place,
        }
    }
}

pub async fn changes(
    persist: &PersistHandle,
    caller: &Caller,
    since: &Since,
    scope: &Scope,
) -> Result<ChangesAnswer, AnswerError> {
    changes_limited(persist, caller, since, scope, DEFAULT_LIMIT).await
}

pub(super) async fn changes_limited(
    persist: &PersistHandle,
    caller: &Caller,
    since: &Since,
    scope: &Scope,
    limit: i64,
) -> Result<ChangesAnswer, AnswerError> {
    let place = persist
        .change_place(caller.session_id, &caller.checkout)
        .await?;
    let Some(prefixes) = prefixes_for(persist, caller, scope).await? else {
        return Ok(ChangesAnswer::empty(place));
    };
    let Some(after_id) = start_after(persist, caller, since, place).await? else {
        return Ok(ChangesAnswer::empty(place));
    };
    let query = ChangeQuery {
        checkout: caller.checkout.clone(),
        after_id,
        path_prefixes: prefixes,
        exclude_session: Some(caller.session_id),
        kinds: Vec::new(),
        limit: limit + 1,
    };
    let mut rows = persist.change_events_after(&query).await?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let names = names_for(persist, &rows).await?;
    let lines: Vec<ChangeLine> = rows.iter().map(|e| line_of(e, &names)).collect();
    let moved_to = rows.last().map_or(place, |last| last.id.max(place));
    if moved_to > place {
        persist
            .move_change_place(caller.session_id, &caller.checkout, moved_to)
            .await?;
    }
    Ok(ChangesAnswer {
        lines,
        more,
        place: moved_to,
    })
}

async fn start_after(
    persist: &PersistHandle,
    caller: &Caller,
    since: &Since,
    place: i64,
) -> Result<Option<i64>, AnswerError> {
    match since {
        Since::Place if place > 0 => Ok(Some(place)),
        Since::Place => {
            let from = Utc::now() - Duration::hours(FIRST_LOOK_HOURS);
            id_before_time(persist, caller, from).await
        }
        Since::Time(from) => id_before_time(persist, caller, *from).await,
        Since::Commit(sha) => {
            let commits = persist
                .latest_change_events(&caller.checkout, &[], &[ChangeKind::Commit], HISTORY_READ)
                .await?;
            commits
                .iter()
                .find(|e| e.after_sha.as_deref().is_some_and(|s| s.starts_with(sha)))
                .map(|e| Some(e.id))
                .ok_or_else(|| AnswerError::UnknownCommit(sha.clone()))
        }
    }
}

async fn id_before_time(
    persist: &PersistHandle,
    caller: &Caller,
    from: DateTime<Utc>,
) -> Result<Option<i64>, AnswerError> {
    let recent = persist
        .latest_change_events(&caller.checkout, &[], &[], HISTORY_READ)
        .await?;
    Ok(recent
        .iter()
        .filter(|e| e.at >= from)
        .map(|e| e.id)
        .min()
        .map(|first| first - 1))
}

async fn prefixes_for(
    persist: &PersistHandle,
    caller: &Caller,
    scope: &Scope,
) -> Result<Option<Vec<String>>, AnswerError> {
    match scope {
        Scope::All => Ok(Some(Vec::new())),
        Scope::Paths(paths) => Ok((!paths.is_empty()).then(|| paths.clone())),
        Scope::Mine => {
            let mut paths = own_paths(persist, caller).await?;
            paths.extend(caller.task_files.iter().cloned());
            paths.extend(read_paths(persist, caller).await?);
            paths.sort();
            paths.dedup();
            paths.truncate(MAX_PREFIXES);
            Ok((!paths.is_empty()).then_some(paths))
        }
    }
}

async fn own_paths(persist: &PersistHandle, caller: &Caller) -> Result<Vec<String>, AnswerError> {
    let recent = persist
        .latest_change_events(&caller.checkout, &[], &OWN_KINDS, HISTORY_READ)
        .await?;
    Ok(paths_of_session(&recent, caller.session_id))
}

fn paths_of_session(events: &[ChangeEvent], session: i64) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.session_id == Some(session))
        .flat_map(|e| e.paths.iter().cloned())
        .collect()
}

async fn read_paths(persist: &PersistHandle, caller: &Caller) -> Result<Vec<String>, AnswerError> {
    let since = Utc::now() - Duration::days(READ_WINDOW_DAYS);
    let seen = persist.paths_seen_by(caller.session_id, since).await?;
    let root = format!("{}/", caller.checkout.trim_end_matches('/'));
    Ok(seen
        .into_iter()
        .filter_map(|s| s.path.strip_prefix(&root).map(str::to_owned))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_record::NewChangeEvent;

    fn event(session: Option<i64>, paths: &[&str]) -> ChangeEvent {
        let new = NewChangeEvent {
            project_id: 1,
            checkout: "/c".to_owned(),
            at: Utc::now(),
            kind: ChangeKind::Edit,
            session_id: session,
            run_id: None,
            task: None,
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
            subject: String::new(),
            detail: serde_json::json!({}),
            before_sha: None,
            after_sha: None,
        };
        ChangeEvent {
            id: 1,
            project_id: new.project_id,
            checkout: new.checkout,
            at: new.at,
            kind: new.kind,
            session_id: new.session_id,
            run_id: new.run_id,
            task: new.task,
            paths: new.paths,
            subject: new.subject,
            detail: new.detail,
            before_sha: new.before_sha,
            after_sha: new.after_sha,
        }
    }

    #[test]
    fn own_paths_are_only_those_the_session_named() {
        let events = vec![event(Some(1), &["a.rs"]), event(Some(2), &["b.rs"])];
        assert_eq!(paths_of_session(&events, 1), vec!["a.rs".to_owned()]);
    }
}
