use std::collections::{BTreeMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::git::{Git, GitError, split_nul};

const REF_FORMAT: &str = "--format=%(refname)%00%(objectname)";
const COMMITS_PER_MOVE: &str = "50";
const SEEN_COMMITS: usize = 4096;
const HEADS: &str = "refs/heads/";
const REMOTES: &str = "refs/remotes/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefMove {
    Commit {
        sha: String,
        branch: String,
    },
    Push {
        remote_ref: String,
        old: Option<String>,
        new: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFacts {
    pub subject: String,
    pub committed_at: Option<DateTime<Utc>>,
    pub seat_session: Option<Uuid>,
    pub paths: Vec<String>,
}

#[derive(Debug, Default)]
pub struct RefState {
    refs: BTreeMap<String, String>,
    seen: HashSet<String>,
    seen_order: VecDeque<String>,
}

impl RefState {
    pub async fn baseline(git: &Git) -> Result<Self, GitError> {
        let mut state = Self {
            refs: read_refs(git).await?,
            ..Self::default()
        };
        let tips: Vec<String> = state.refs.values().cloned().collect();
        for sha in tips {
            state.remember(sha);
        }
        Ok(state)
    }

    pub async fn observe(&mut self, git: &Git) -> Result<Vec<RefMove>, GitError> {
        let now = read_refs(git).await?;
        let pushed = self.remote_tips();
        let mut moves = Vec::new();
        for (name, sha) in &now {
            let old = self.refs.get(name);
            if old == Some(sha) {
                continue;
            }
            if let Some(branch) = name.strip_prefix(HEADS) {
                for commit in new_commits(git, old, sha, &pushed).await? {
                    if self.remember(commit.clone()) {
                        moves.push(RefMove::Commit {
                            sha: commit,
                            branch: branch.to_string(),
                        });
                    }
                }
            } else if name.starts_with(REMOTES) {
                moves.push(RefMove::Push {
                    remote_ref: name.clone(),
                    old: old.cloned(),
                    new: sha.clone(),
                });
            }
        }
        self.refs = now;
        Ok(moves)
    }

    fn remote_tips(&self) -> Vec<String> {
        let tips: HashSet<&String> = self
            .refs
            .iter()
            .filter(|(name, _)| name.starts_with(REMOTES))
            .map(|(_, sha)| sha)
            .collect();
        tips.into_iter().cloned().collect()
    }

    fn remember(&mut self, sha: String) -> bool {
        if !self.seen.insert(sha.clone()) {
            return false;
        }
        self.seen_order.push_back(sha);
        while self.seen_order.len() > SEEN_COMMITS {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }
}

async fn read_refs(git: &Git) -> Result<BTreeMap<String, String>, GitError> {
    let raw = git
        .run(&["for-each-ref", REF_FORMAT, HEADS, REMOTES])
        .await?;
    let mut refs = BTreeMap::new();
    for line in String::from_utf8_lossy(&raw).lines() {
        let Some((name, sha)) = line.split_once('\0') else {
            continue;
        };
        if name.starts_with(REMOTES) && name.ends_with("/HEAD") {
            continue;
        }
        refs.insert(name.to_string(), sha.to_string());
    }
    Ok(refs)
}

async fn new_commits(
    git: &Git,
    old: Option<&String>,
    new: &str,
    pushed: &[String],
) -> Result<Vec<String>, GitError> {
    let mut args: Vec<String> = match old {
        Some(old) => vec![
            "rev-list".into(),
            "--reverse".into(),
            "-n".into(),
            COMMITS_PER_MOVE.into(),
            format!("{old}..{new}"),
        ],
        None => vec!["rev-list".into(), "-n".into(), "1".into(), new.to_string()],
    };
    args.extend(pushed.iter().map(|sha| format!("^{sha}")));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw = git.run(&args).await?;
    Ok(String::from_utf8_lossy(&raw)
        .lines()
        .map(str::to_string)
        .collect())
}

pub async fn commit_facts(git: &Git, sha: &str) -> Result<CommitFacts, GitError> {
    let raw = git
        .run(&[
            "show",
            "-s",
            "--format=%s%x00%cI%x00%(trailers:key=Fleet-Seat,valueonly)",
            sha,
        ])
        .await?;
    let text = String::from_utf8_lossy(&raw);
    let mut parts = text.split('\0');
    let subject = parts.next().unwrap_or_default().trim().to_string();
    let committed_at = parts
        .next()
        .and_then(|t| DateTime::parse_from_rfc3339(t.trim()).ok())
        .map(|t| t.with_timezone(&Utc));
    let seat_session = parts.next().and_then(seat_uuid);
    let paths = split_nul(
        &git.run(&[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            sha,
        ])
        .await?,
    );
    Ok(CommitFacts {
        subject,
        committed_at,
        seat_session,
        paths,
    })
}

pub async fn pushed_paths(
    git: &Git,
    old: Option<&str>,
    new: &str,
) -> Result<Vec<String>, GitError> {
    match old {
        Some(old) => Ok(split_nul(
            &git.run(&["diff", "--name-only", "-z", old, new]).await?,
        )),
        None => Ok(commit_facts(git, new).await?.paths),
    }
}

pub async fn pushed_seat(
    git: &Git,
    old: Option<&str>,
    new: &str,
) -> Result<Option<Uuid>, GitError> {
    let Some(old) = old else {
        return Ok(commit_facts(git, new).await?.seat_session);
    };
    let range = format!("{old}..{new}");
    let raw = git
        .run(&[
            "log",
            "-n",
            COMMITS_PER_MOVE,
            "--format=%x00%(trailers:key=Fleet-Seat,valueonly)",
            &range,
        ])
        .await?;
    let text = String::from_utf8_lossy(&raw);
    let seats: Vec<Option<Uuid>> = text.split('\0').skip(1).map(seat_uuid).collect();
    Ok(one_seat(&seats))
}

pub fn one_seat(seats: &[Option<Uuid>]) -> Option<Uuid> {
    let first = (*seats.first()?)?;
    seats.iter().all(|s| *s == Some(first)).then_some(first)
}

pub fn seat_uuid(trailer: &str) -> Option<Uuid> {
    let line = trailer.lines().find(|l| !l.trim().is_empty())?;
    let open = line.rfind('(')?;
    let close = line[open..].find(')')? + open;
    Uuid::parse_str(line[open + 1..close].trim()).ok()
}
