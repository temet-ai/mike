use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::diff_git::{relative_path, resolve_commit, run, valid_object};
use super::lines::{ChangeLine, line_of, names_for};
use super::{WhoseAnswer, whose};
use crate::change_record::writers::{author_of, change_record_enabled};
use crate::change_record::{ChangeEvent, ChangeKind};
use crate::store::persistence::PersistHandle;

const MAX_REFS: usize = 6;
const MAX_FILES: usize = 20;
const MAX_TEXT: usize = 16_384;

#[derive(Debug, Serialize)]
pub struct MentionFacts {
    source: &'static str,
    checkout: String,
    observed_at: DateTime<Utc>,
    commits: Vec<CommitFact>,
    paths: Vec<WhoseAnswer>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct CommitFact {
    commit: String,
    on_current_branch: Option<bool>,
    in_upstream_tracking_ref: Option<bool>,
    files: Vec<String>,
    files_truncated: bool,
    recorded_author: Option<ChangeLine>,
    latest_recorded_check: Option<RecordedCheck>,
}

#[derive(Debug, Serialize)]
struct RecordedCheck {
    commit: String,
    at: DateTime<Utc>,
    passed: Option<bool>,
    command: Option<String>,
}

fn tokens(text: &str) -> (Vec<String>, bool) {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut values = Vec::new();
    for word in text[..end].split_whitespace() {
        let word = word.trim_matches(|c: char| {
            matches!(
                c,
                '`' | '\''
                    | '"'
                    | ','
                    | ';'
                    | ':'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '<'
                    | '>'
                    | '*'
                    | '!'
                    | '?'
                    | '.'
            )
        });
        if !word.is_empty() && !values.iter().any(|old| old == word) {
            values.push(word.to_string());
        }
    }
    (values, end < text.len())
}

pub async fn mentioned_facts(
    persist: &PersistHandle,
    session: Uuid,
    text: &str,
) -> Option<MentionFacts> {
    tokio::time::timeout(Duration::from_secs(2), for_session(persist, session, text))
        .await
        .ok()
        .flatten()
}

async fn for_session(persist: &PersistHandle, session: Uuid, text: &str) -> Option<MentionFacts> {
    if !change_record_enabled(persist).await {
        return None;
    }
    let author = author_of(persist, session).await.ok()??;
    facts(persist, &author.checkout, text).await.map(bounded)
}

async fn facts(persist: &PersistHandle, repo: &str, text: &str) -> Option<MentionFacts> {
    let (words, truncated) = tokens(text);
    let (tracked, capped) = run(repo, &["ls-files", "-z"], false).await.ok()?;
    let tracked: HashSet<String> = tracked
        .split(|c| *c == 0)
        .filter_map(|b| std::str::from_utf8(b).ok())
        .map(str::to_string)
        .collect();
    let mut result = MentionFacts {
        source: "mike_change_record_and_local_git",
        checkout: repo.to_string(),
        observed_at: Utc::now(),
        commits: Vec::new(),
        paths: Vec::new(),
        truncated: truncated || capped,
    };
    let events = persist
        .fleet()
        .latest_change_events(repo, &[], &[ChangeKind::Commit, ChangeKind::Check], 100)
        .await
        .ok()?;
    let names = names_for(persist, &events).await.ok()?;
    for word in words {
        if result.commits.len() + result.paths.len() == MAX_REFS {
            result.truncated = true;
            break;
        }
        if valid_object(&word)
            && let Ok(commit) = resolve_commit(repo, &word).await
        {
            if !result.commits.iter().any(|c| c.commit == commit) {
                result
                    .commits
                    .push(commit_fact(repo, &commit, &events, &names).await);
            }
        } else {
            let path = word.strip_prefix(&format!("{repo}/")).unwrap_or(&word);
            if relative_path(path).is_ok()
                && tracked.contains(path)
                && let Ok(answer) = whose(persist, repo, path).await
            {
                result.paths.push(answer);
            }
        }
    }
    (!result.commits.is_empty() || !result.paths.is_empty()).then_some(result)
}

async fn ancestry(repo: &str, commit: &str, reference: &str) -> Option<bool> {
    let (base, capped) = run(repo, &["merge-base", commit, reference], false)
        .await
        .ok()?;
    if capped {
        return None;
    }
    Some(std::str::from_utf8(&base).ok()?.trim() == commit)
}

async fn commit_fact(
    repo: &str,
    commit: &str,
    events: &[ChangeEvent],
    names: &super::lines::Names,
) -> CommitFact {
    let listed = run(
        repo,
        &[
            "show",
            "--format=",
            "--first-parent",
            "--name-only",
            "-z",
            commit,
            "--",
        ],
        false,
    )
    .await;
    let (files, files_truncated) = match listed {
        Ok((bytes, capped)) => {
            let all: Vec<String> = bytes
                .split(|c| *c == 0)
                .filter_map(|b| std::str::from_utf8(b).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            let truncated = capped || all.len() > MAX_FILES;
            (all.into_iter().take(MAX_FILES).collect(), truncated)
        }
        Err(_) => (Vec::new(), true),
    };
    let event = events
        .iter()
        .find(|e| e.kind == ChangeKind::Commit && e.after_sha.as_deref() == Some(commit));
    CommitFact {
        commit: commit.to_string(),
        on_current_branch: ancestry(repo, commit, "HEAD").await,
        in_upstream_tracking_ref: ancestry(repo, commit, "@{upstream}").await,
        files,
        files_truncated,
        recorded_author: event.map(|e| {
            let mut line = line_of(e, names);
            line.paths.truncate(MAX_FILES);
            line.subject.clear();
            line
        }),
        latest_recorded_check: check(repo, commit, events).await,
    }
}

async fn check(repo: &str, commit: &str, events: &[ChangeEvent]) -> Option<RecordedCheck> {
    let event = events.iter().find(|e| e.kind == ChangeKind::Check)?;
    let checked = event
        .detail
        .get("commit")
        .and_then(|v| v.as_str())
        .or(event.after_sha.as_deref())?;
    if !valid_object(checked)
        || (checked != commit && ancestry(repo, commit, checked).await != Some(true))
    {
        return None;
    }
    Some(RecordedCheck {
        commit: checked.to_string(),
        at: event.at,
        passed: event.detail.get("passed").and_then(|v| v.as_bool()),
        command: event
            .detail
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

fn bounded(mut facts: MentionFacts) -> MentionFacts {
    while serde_json::to_vec(&facts).is_ok_and(|bytes| bytes.len() > 8192) {
        facts.truncated = true;
        if facts.paths.pop().is_none() && facts.commits.pop().is_none() {
            break;
        }
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_tokens_keep_short_commit_ids_and_root_paths_without_instructions() {
        let (words, truncated) = tokens("Committed `AB12`, see README.md and src/a.rs.");
        assert!(words.contains(&"AB12".to_string()));
        assert!(words.contains(&"README.md".to_string()));
        assert!(words.contains(&"src/a.rs".to_string()));
        assert!(!truncated);
    }

    #[test]
    fn long_utf8_input_is_bounded_on_a_character_boundary() {
        let (words, truncated) = tokens(&"界".repeat(MAX_TEXT));
        assert!(truncated);
        assert!(words[0].len() <= MAX_TEXT);
    }

    #[tokio::test]
    async fn commit_facts_read_real_git_and_keep_unknown_record_values_unknown()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().to_string_lossy();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "test"],
            vec!["config", "user.email", "test@test"],
        ] {
            run(&repo, &args, false).await.map_err(anyhow::Error::msg)?;
        }
        std::fs::write(dir.path().join("README.md"), "first\n")?;
        run(&repo, &["add", "README.md"], false)
            .await
            .map_err(anyhow::Error::msg)?;
        run(&repo, &["commit", "-m", "first"], false)
            .await
            .map_err(anyhow::Error::msg)?;
        let (sha, _) = run(&repo, &["rev-parse", "HEAD"], false)
            .await
            .map_err(anyhow::Error::msg)?;
        let sha = String::from_utf8(sha)?;
        let answer = commit_fact(&repo, sha.trim(), &[], &super::super::lines::Names::new()).await;
        assert_eq!(answer.on_current_branch, Some(true));
        assert_eq!(answer.in_upstream_tracking_ref, None);
        assert_eq!(answer.files, vec!["README.md"]);
        assert!(answer.recorded_author.is_none());
        assert!(answer.latest_recorded_check.is_none());
        Ok(())
    }

    #[test]
    fn facts_over_eight_kib_are_explicitly_truncated() -> anyhow::Result<()> {
        let facts = MentionFacts {
            source: "mike_change_record_and_local_git",
            checkout: "/test".into(),
            observed_at: Utc::now(),
            commits: Vec::new(),
            paths: vec![WhoseAnswer {
                path: "x".repeat(9000),
                last_change: None,
                committed: false,
            }],
            truncated: false,
        };
        let facts = bounded(facts);
        assert!(facts.truncated);
        assert!(serde_json::to_vec(&facts)?.len() <= 8192);
        Ok(())
    }
}
