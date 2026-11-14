use std::path::Path;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::commit_check;
use super::commit_modules;
use super::commit_note;
use super::commit_paths::record_paths;
pub use super::commit_paths::{
    Chosen, Ownership, choose_from_record, choose_paths, covers, owner_of, ownership,
};
use super::commit_verify::{Verification, Verifier, verify};
use super::definition::FleetDefinition;
use super::push::{merge_origin, push};
use super::seated::Seated;
use super::watch::git;

const HOOK: &str = ".git/hooks/pre-commit";
const INDEX_ATTEMPTS: usize = 20;
const INDEX_RETRY: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitRequest {
    pub session: Uuid,
    pub message: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub allow_empty: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitReport {
    pub seat: String,
    pub task: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub paths_from: String,
    pub sha: Option<String>,
    pub pushed: bool,
    #[serde(default)]
    pub push_error: Option<String>,
    pub verified: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub refused: Option<String>,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrateReport {
    pub merged: bool,
    pub problem: Option<String>,
}

fn locks() -> &'static DashMap<String, Arc<Mutex<()>>> {
    static LOCKS: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(DashMap::new)
}

pub async fn commit(
    def: &FleetDefinition,
    seated: &Seated,
    request: &CommitRequest,
    verifier: Verifier<'_>,
) -> CommitReport {
    let Some(owner) = ownership(def, seated, request.session) else {
        return refused(
            "?",
            "?",
            format!("session {} holds no task in this fleet", request.session),
        );
    };
    let repo = def.repo.as_str();
    let lock = locks()
        .entry(repo.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _held = lock.lock().await;
    let previous = match verifier.persist {
        Some(persist) => {
            super::ruling_proof::newest_commit(persist, request.session, &owner.task).await
        }
        None => None,
    };
    match commit_locked(def, &owner, request, verifier).await {
        Ok(mut landed) => {
            if let Some(persist) = verifier.persist {
                landed.warnings.extend(
                    super::release::commit_warning(persist, def, previous.as_deref()).await,
                );
            }
            let issues = landed.issues;
            let report = CommitReport {
                seat: owner.seat,
                task: owner.task,
                paths: landed.paths,
                paths_from: landed.paths_from.to_string(),
                sha: Some(landed.sha),
                pushed: landed.push_error.is_none(),
                push_error: landed.push_error,
                verified: landed.verified,
                warnings: landed.warnings,
                refused: None,
                repo: def.repo.clone(),
                branch: def.branch.clone(),
            };
            if let Some(persist) = verifier.persist {
                commit_note::tell_acceptance(persist, def, &report).await;
                if let Some(sha) = report.sha.as_deref() {
                    super::issue_close::record(
                        persist,
                        request.session,
                        &report.task,
                        sha,
                        &issues,
                    )
                    .await;
                    super::commit_landed::record(
                        persist,
                        verifier.run,
                        (&report.task, request.session),
                        sha,
                    )
                    .await;
                }
            }
            report
        }
        Err(why) => refused(&owner.seat, &owner.task, why),
    }
}

pub async fn integrate(def: &FleetDefinition) -> IntegrateReport {
    let lock = locks()
        .entry(def.repo.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _held = lock.lock().await;
    let merged = match on_the_runs_branch(def).await {
        Ok(()) => merge_origin(&def.repo, &def.branch).await,
        Err(why) => Err(why),
    };
    match merged {
        Ok(merged) => IntegrateReport {
            merged,
            problem: None,
        },
        Err(why) => IntegrateReport {
            merged: false,
            problem: Some(why),
        },
    }
}

async fn on_the_runs_branch(def: &FleetDefinition) -> Result<(), String> {
    let head_ref = text(git(&def.repo, &["symbolic-ref", "--short", "HEAD"], None).await)
        .ok_or("the checkout is not on a branch")?;
    if head_ref == def.branch {
        return Ok(());
    }
    Err(format!(
        "the checkout is on `{head_ref}`, not the run's branch `{}`",
        def.branch
    ))
}

fn refused(seat: &str, task: &str, why: String) -> CommitReport {
    CommitReport {
        seat: seat.to_string(),
        task: task.to_string(),
        verified: "nothing committed".to_string(),
        refused: Some(why),
        ..CommitReport::default()
    }
}

struct Landed {
    paths_from: &'static str,
    paths: Vec<String>,
    sha: String,
    push_error: Option<String>,
    verified: String,
    warnings: Vec<String>,
    issues: Vec<u32>,
}

async fn commit_locked(
    def: &FleetDefinition,
    owner: &Ownership,
    request: &CommitRequest,
    verifier: Verifier<'_>,
) -> Result<Landed, String> {
    let repo = def.repo.as_str();
    on_the_runs_branch(def).await?;
    let changed = changed_paths(repo).await?;
    let (
        Chosen {
            paths,
            mut warnings,
        },
        paths_from,
    ) = select_paths(def, owner, request, verifier, &changed).await?;
    let staged = stage(repo, &paths, request.allow_empty).await?;
    let verified = verify(
        verifier,
        Verification {
            def,
            tree: &staged.tree,
            paths: &paths,
        },
    )
    .await;
    warnings.extend(verified.warning.clone());
    let commit_checks = commit_check::run(def, &paths).await;
    warnings.extend(commit_checks.warnings);
    warnings.extend(commit_modules::run(def, (&staged.tree, &staged.head), &paths).await);
    let verified_text = [
        Some(verified.text.clone()),
        commit_check::describe_passed(&commit_checks.passed),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("; ");
    let (said, issues) = rewrite_closing(&request.message);
    if !issues.is_empty() {
        warnings.push(format!(
            "the message closes issues; mike wrote `Refs` in its place and closes them when acceptance passes {}",
            owner.task
        ));
    }
    let message = message_for(&said, owner, request.session, &verified_text);
    let sha = land(repo, &def.branch, &staged, (&paths, &message)).await?;
    let push_error = push(repo, &def.branch)
        .await
        .err()
        .map(|words| match words.trim() {
            "" => "git push failed and said nothing".to_string(),
            said => said.to_string(),
        });
    Ok(Landed {
        paths_from,
        paths,
        sha,
        push_error,
        verified: verified_text,
        warnings,
        issues,
    })
}

async fn select_paths(
    def: &FleetDefinition,
    owner: &Ownership,
    request: &CommitRequest,
    verifier: Verifier<'_>,
    changed: &[String],
) -> Result<(Chosen, &'static str), String> {
    let named = !request.paths.is_empty();
    let mut warnings = Vec::new();
    if !named
        && let Some(recorded) = record_paths(verifier, def, request.session).await
        && let Some(chosen) = choose_from_record(owner, changed, &recorded)
    {
        if !chosen.paths.is_empty() {
            return Ok((chosen, "record"));
        }
        warnings = chosen.warnings;
    }
    let from = if named { "named" } else { "task" };
    let mut chosen = match choose_paths(owner, changed, &request.paths) {
        Err(why) if request.allow_empty => Chosen {
            paths: Vec::new(),
            warnings: vec![why],
        },
        Err(why) => {
            return Err(std::iter::once(why)
                .chain(warnings)
                .collect::<Vec<_>>()
                .join("; "));
        }
        Ok(chosen) => chosen,
    };
    chosen.warnings.extend(warnings);
    Ok((chosen, from))
}

fn closing_words() -> Option<&'static Regex> {
    static WORDS: OnceLock<Option<Regex>> = OnceLock::new();
    WORDS
        .get_or_init(|| {
            Regex::new(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)(:?\s+)(#\d+(?:(?:\s*,\s*|\s+and\s+|\s+)#\d+)*)").ok()
        })
        .as_ref()
}

pub fn rewrite_closing(message: &str) -> (String, Vec<u32>) {
    let mut issues = Vec::new();
    let Some(words) = closing_words() else {
        return (message.to_string(), issues);
    };
    let rewritten = words.replace_all(message, |found: &regex::Captures<'_>| {
        issues.extend(found[2].split('#').skip(1).filter_map(|digits| {
            digits
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse::<u32>()
                .ok()
        }));
        format!("Refs{}{}", &found[1], &found[2])
    });
    issues.sort_unstable();
    issues.dedup();
    (rewritten.into_owned(), issues)
}

fn message_for(message: &str, owner: &Ownership, session: Uuid, verified: &str) -> String {
    format!(
        "{}\n\nFleet-Seat: {} ({session})\nFleet-Verified: {verified}\n",
        message.trim_end(),
        owner.seat,
    )
}

fn text(out: Option<Vec<u8>>) -> Option<String> {
    out.map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(super) async fn changed_paths(repo: &str) -> Result<Vec<String>, String> {
    let raw = git(
        repo,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        None,
    )
    .await
    .ok_or("git status failed")?;
    let mut changed = Vec::new();
    let mut entries = raw.split(|b| *b == 0).filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let line = String::from_utf8_lossy(entry).into_owned();
        match line.as_bytes().first() {
            Some(b'?') => changed.push(line[2..].to_string()),
            Some(b'1') => changed.extend(line.splitn(9, ' ').nth(8).map(str::to_string)),
            Some(b'2') => {
                changed.extend(line.splitn(10, ' ').nth(9).map(str::to_string));
                entries.next();
            }
            _ => {}
        }
    }
    Ok(changed)
}

struct Staged {
    index: tempfile::NamedTempFile,
    head: String,
    tree: String,
}

impl Staged {
    fn index_path(&self) -> String {
        self.index.path().to_string_lossy().into_owned()
    }
}

async fn stage(repo: &str, paths: &[String], allow_empty: bool) -> Result<Staged, String> {
    let index = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let index_path = index.path().to_string_lossy().into_owned();
    let head = text(git(repo, &["rev-parse", "HEAD"], None).await).ok_or("no HEAD")?;
    git_indexed(repo, &index_path, &["read-tree", "HEAD"])
        .await
        .ok_or("read-tree failed")?;
    if !paths.is_empty() {
        let mut update = vec!["update-index", "--add", "--remove", "--"];
        update.extend(paths.iter().map(String::as_str));
        git_indexed(repo, &index_path, &update)
            .await
            .ok_or("update-index failed")?;
    }
    let tree =
        text(git_indexed(repo, &index_path, &["write-tree"]).await).ok_or("write-tree failed")?;
    let head_tree = text(git(repo, &["rev-parse", "HEAD^{tree}"], None).await).unwrap_or_default();
    if tree == head_tree && !allow_empty {
        return Err(
            "those paths are already as committed; nothing to commit. To land a message with no \
             file change, such as a closing `Closes #n`, ask again with allow_empty"
                .to_string(),
        );
    }
    Ok(Staged { index, head, tree })
}

async fn land(
    repo: &str,
    branch: &str,
    staged: &Staged,
    (paths, message): (&[String], &str),
) -> Result<String, String> {
    run_hook(repo, &staged.index_path()).await?;
    let sha = text(
        git(
            repo,
            &[
                "commit-tree",
                &staged.tree,
                "-p",
                &staged.head,
                "-m",
                message,
            ],
            None,
        )
        .await,
    )
    .ok_or("commit-tree failed")?;
    let reference = format!("refs/heads/{branch}");
    git(repo, &["update-ref", &reference, &sha, &staged.head], None)
        .await
        .ok_or("the branch moved while the commit was being written; try again")?;
    refresh_index(repo, paths).await;
    Ok(sha)
}

async fn refresh_index(repo: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let mut refresh = vec!["update-index", "--add", "--remove", "--"];
    refresh.extend(paths.iter().map(String::as_str));
    for _ in 0..INDEX_ATTEMPTS {
        if git(repo, &refresh, None).await.is_some() {
            return;
        }
        tokio::time::sleep(INDEX_RETRY).await;
    }
    tracing::warn!(%repo, "the shared index could not be brought in step with the new commit; `git reset -q -- <paths>` repairs it");
}

async fn run_hook(repo: &str, index_path: &str) -> Result<(), String> {
    let hook = Path::new(repo).join(HOOK);
    if !hook.is_file() {
        return Ok(());
    }
    let output = tokio::process::Command::new(&hook)
        .current_dir(repo)
        .env("GIT_INDEX_FILE", index_path)
        .output()
        .await
        .map_err(|e| format!("the commit check could not run: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let tail: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap_or_default()
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Err(format!("the commit check refused it:\n{}", tail.join("\n")))
}

async fn git_indexed(repo: &str, index_path: &str, args: &[&str]) -> Option<Vec<u8>> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_INDEX_FILE", index_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let output = command.output().await.ok()?;
    output.status.success().then_some(output.stdout)
}
