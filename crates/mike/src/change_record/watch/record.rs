use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use super::author::{self, FileChange, Window};
use super::git::Git;
use super::refs::{self, RefMove};
use super::tree::FileDelta;
use crate::change_record::{ChangeKind, NewChangeEvent};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone)]
pub struct Checkout {
    pub root: String,
    pub project_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Notification,
    Backstop,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Notification => "notification",
            Source::Backstop => "backstop",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Unnamed {
    File {
        id: i64,
        path: String,
        before_sha: Option<String>,
        after_sha: Option<String>,
        edit: Window,
        shell: Window,
        retried: bool,
    },
    Commit {
        id: i64,
        seat: Option<uuid::Uuid>,
        shell: Window,
    },
    Push {
        id: i64,
        seat: Option<uuid::Uuid>,
        shell: Window,
    },
}

impl Unnamed {
    pub fn is_retry(&self) -> bool {
        matches!(self, Unnamed::File { retried: true, .. })
    }
}

pub fn file_events(
    checkout: &Checkout,
    deltas: &[FileDelta],
    at: DateTime<Utc>,
    source: Source,
) -> Vec<NewChangeEvent> {
    deltas
        .iter()
        .map(|d| NewChangeEvent {
            project_id: checkout.project_id,
            checkout: checkout.root.clone(),
            at,
            kind: ChangeKind::ShellChange,
            session_id: None,
            run_id: None,
            task: None,
            paths: vec![d.path.clone()],
            subject: format!("{} {}", d.change.as_str(), d.path),
            detail: json!({"change": d.change.as_str(), "seen_by": source.as_str(), "author": "pending"}),
            before_sha: d.before_sha.clone(),
            after_sha: d.after_sha.clone(),
        })
        .collect()
}

pub fn file_unnamed(
    ids: &[i64],
    deltas: &[FileDelta],
    at: DateTime<Utc>,
    looked_back: Duration,
    edit_window: Duration,
) -> Vec<Unnamed> {
    ids.iter()
        .zip(deltas)
        .map(|(id, d)| Unnamed::File {
            id: *id,
            path: d.path.clone(),
            before_sha: d.before_sha.clone(),
            after_sha: d.after_sha.clone(),
            edit: Window {
                from: at - edit_window - looked_back,
                to: at,
            },
            shell: Window {
                from: at - looked_back,
                to: at,
            },
            retried: false,
        })
        .collect()
}

pub async fn ref_events(
    git: &Git,
    checkout: &Checkout,
    moves: &[RefMove],
    at: DateTime<Utc>,
) -> (Vec<NewChangeEvent>, Vec<Option<uuid::Uuid>>) {
    let mut events = Vec::new();
    let mut seats = Vec::new();
    for movement in moves {
        let built = match movement {
            RefMove::Commit { sha, branch } => commit_event(git, checkout, sha, branch, at).await,
            RefMove::Push {
                remote_ref,
                old,
                new,
            } => push_event(git, checkout, remote_ref, old.as_deref(), new, at).await,
        };
        match built {
            Ok((event, seat)) => {
                events.push(event);
                seats.push(seat);
            }
            Err(err) => {
                tracing::warn!(root = %checkout.root, error = %err, "change record: a ref move could not be read")
            }
        }
    }
    (events, seats)
}

async fn commit_event(
    git: &Git,
    checkout: &Checkout,
    sha: &str,
    branch: &str,
    at: DateTime<Utc>,
) -> Result<(NewChangeEvent, Option<uuid::Uuid>), super::git::GitError> {
    let facts = refs::commit_facts(git, sha).await?;
    let event = NewChangeEvent {
        project_id: checkout.project_id,
        checkout: checkout.root.clone(),
        at,
        kind: ChangeKind::Commit,
        session_id: None,
        run_id: None,
        task: None,
        paths: facts.paths,
        subject: facts.subject,
        detail: json!({
            "sha": sha,
            "branch": branch,
            "committed_at": facts.committed_at,
            "author": "pending",
        }),
        before_sha: None,
        after_sha: None,
    };
    Ok((event, facts.seat_session))
}

async fn push_event(
    git: &Git,
    checkout: &Checkout,
    remote_ref: &str,
    old: Option<&str>,
    new: &str,
    at: DateTime<Utc>,
) -> Result<(NewChangeEvent, Option<uuid::Uuid>), super::git::GitError> {
    let paths = refs::pushed_paths(git, old, new).await?;
    let seat = refs::pushed_seat(git, old, new).await?;
    let short = new.get(..12).unwrap_or(new);
    let event = NewChangeEvent {
        project_id: checkout.project_id,
        checkout: checkout.root.clone(),
        at,
        kind: ChangeKind::Push,
        session_id: None,
        run_id: None,
        task: None,
        paths,
        subject: format!("{remote_ref} moved to {short}"),
        detail: json!({"ref": remote_ref, "old": old, "new": new, "author": "pending"}),
        before_sha: None,
        after_sha: None,
    };
    Ok((event, seat))
}

pub fn ref_unnamed(
    ids: &[i64],
    events: &[NewChangeEvent],
    seats: &[Option<uuid::Uuid>],
    at: DateTime<Utc>,
    looked_back: Duration,
) -> Vec<Unnamed> {
    let shell = Window {
        from: at - looked_back,
        to: at,
    };
    ids.iter()
        .zip(events)
        .zip(seats)
        .map(|((id, event), seat)| match event.kind {
            ChangeKind::Commit => Unnamed::Commit {
                id: *id,
                seat: *seat,
                shell,
            },
            _ => Unnamed::Push {
                id: *id,
                seat: *seat,
                shell,
            },
        })
        .collect()
}

pub async fn name(
    persist: &PersistHandle,
    git: &Git,
    item: &Unnamed,
) -> Result<Option<Unnamed>, PersistError> {
    let root = git.root();
    let mut again = None;
    let (id, kind, who) = match item {
        Unnamed::File {
            id,
            path,
            before_sha,
            after_sha,
            edit,
            shell,
            retried,
        } => {
            let absolute = git.absolute(path).to_string_lossy().into_owned();
            let change = FileChange {
                absolute_path: &absolute,
                relative_path: path,
                before_sha: before_sha.as_deref(),
                after_sha: after_sha.as_deref(),
                edit: *edit,
                shell: *shell,
            };
            let who = author::of_file_change(persist, git, &change).await?;
            let settled = matches!(
                who,
                author::Authorship::Edit { .. }
                    | author::Authorship::Shell {
                        named_by: Some(_),
                        ..
                    }
            );
            if !*retried && !settled {
                again = Some(Unnamed::File {
                    id: *id,
                    path: path.clone(),
                    before_sha: before_sha.clone(),
                    after_sha: after_sha.clone(),
                    edit: *edit,
                    shell: *shell,
                    retried: true,
                });
            }
            (*id, who.file_kind(), who)
        }
        Unnamed::Commit { id, seat, shell } => (
            *id,
            ChangeKind::Commit,
            seat_or_shell(persist, root, *seat, *shell).await?,
        ),
        Unnamed::Push { id, seat, shell } => (
            *id,
            ChangeKind::Push,
            seat_or_shell(persist, root, *seat, *shell).await?,
        ),
    };
    persist
        .update_change_author(id, who.session(), kind, &who.detail())
        .await?;
    Ok(again)
}

async fn seat_or_shell(
    persist: &PersistHandle,
    root: &str,
    seat: Option<uuid::Uuid>,
    shell: Window,
) -> Result<author::Authorship, PersistError> {
    match author::of_seat(persist, seat).await? {
        Some(seat) => Ok(seat),
        None => author::of_shell(persist, root, shell).await,
    }
}
