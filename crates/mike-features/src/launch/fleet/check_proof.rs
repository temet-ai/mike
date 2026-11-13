use std::sync::OnceLock;

use dashmap::DashMap;
use serde_json::json;
use uuid::Uuid;

use super::check::{Asker, Verdict, short, short_commit, state_for};
use super::check_report::answer;
use super::commit::ownership;
use super::commit_note::{Draft, store};
use super::definition::FleetDefinition;
use super::diagnostics::Diagnostic;
use super::seated::Seated;
use super::watch::git;
use crate::store::persistence::PersistHandle;

const LOOK_BACK: &str = "200";
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const TRAILERS: char = '\u{1d}';
const FORMAT: &str = "--format=%H%x1f%(trailers:key=Fleet-Seat,valueonly,separator=%x1d)%x1e";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    pub sha: String,
    pub session: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    pub sha: String,
    pub proven: bool,
}

fn checked() -> &'static DashMap<String, String> {
    static CHECKED: OnceLock<DashMap<String, String>> = OnceLock::new();
    CHECKED.get_or_init(DashMap::new)
}

pub fn parse_landed(raw: &str) -> Vec<Landed> {
    raw.split(RECORD)
        .filter_map(|record| {
            let (sha, trailers) = record.trim().split_once(FIELD)?;
            let session = trailers.split(TRAILERS).find_map(session_of)?;
            Some(Landed {
                sha: sha.trim().to_string(),
                session,
            })
        })
        .collect()
}

fn session_of(trailer: &str) -> Option<Uuid> {
    let (_, rest) = trailer.trim().rsplit_once('(')?;
    rest.trim_end_matches(')').trim().parse().ok()
}

async fn landed_since(repo: &str, last: Option<&str>, commit: &str) -> Vec<Landed> {
    let range = match last {
        Some(last) if last != commit => format!("{last}..{commit}"),
        _ => commit.to_string(),
    };
    let raw = git(
        repo,
        &["log", "--reverse", "-n", LOOK_BACK, FORMAT, &range],
        None,
    )
    .await;
    raw.map(|bytes| parse_landed(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

async fn landed_at(repo: &str, commit: &str) -> Option<Landed> {
    let raw = git(repo, &["log", "-n", "1", FORMAT, commit], None).await?;
    parse_landed(&String::from_utf8_lossy(&raw))
        .into_iter()
        .find(|landed| landed.sha == commit)
}

pub async fn next_commit(
    def: &FleetDefinition,
    seated: &Seated,
    run: &str,
    head: &str,
) -> Option<String> {
    let last = checked().get(run).map(|commit| commit.clone());
    if last.as_deref() == Some(head) {
        return None;
    }
    landed_since(&def.repo, last.as_deref(), head)
        .await
        .into_iter()
        .find(|commit| ownership(def, seated, commit.session).is_some())
        .map(|commit| commit.sha)
}

pub async fn record(persist: &PersistHandle, def: &FleetDefinition, run: &str, verdict: &Verdict) {
    if !super::check_report::verified(verdict) {
        return;
    }
    checked().insert(run.to_string(), verdict.commit.clone());
    let Some(commit) = landed_at(&def.repo, &verdict.commit).await else {
        return;
    };
    let seated = Seated::of_run(persist, run, def).await;
    let Some(owner) = ownership(def, &seated, commit.session) else {
        return;
    };
    let proven = verdict.row.passed()
        && state_for(def, &commit.sha)
            .await
            .is_ok_and(|state| state == verdict.row.state);
    let proofs = [Proof {
        sha: commit.sha,
        proven,
    }];
    let who = Asker {
        seat: owner.seat,
        owned: owner.owned,
        everything: false,
    };
    let mine = answer(def, &who, verdict).mine;
    let note = draft((&owner.task, &who.seat), verdict, &proofs, &mine);
    store(persist, commit.session, &note).await;
}

fn headline(task: &str, verdict: &Verdict, proofs: &[Proof], mine: &[Diagnostic]) -> String {
    let command = verdict.row.command.trim();
    let at = short_commit(&verdict.commit);
    let proven = proofs.iter().filter(|p| p.proven).count();
    if !verdict.row.passed() {
        let place = if mine.is_empty() {
            "not in this task's files"
        } else {
            "in this task's files"
        };
        return format!("{task}: `{command}` failed at commit {at}, {place}");
    }
    format!(
        "{task}: `{command}` passed at commit {at}; {proven} of {} commit(s) proven",
        proofs.len()
    )
}

fn proof_line(proof: &Proof) -> String {
    let why = if proof.proven {
        "proven: the check passed at its content"
    } else {
        "not proven: the check did not pass at its content"
    };
    format!("- {} {why}", proof.sha)
}

pub fn draft(
    (task, seat): (&str, &str),
    verdict: &Verdict,
    proofs: &[Proof],
    mine: &[Diagnostic],
) -> Draft {
    let mut parts = vec![format!(
        "`{}` {} at commit {} (state {}), run by mike in its clean checkout of that commit.",
        verdict.row.command.trim(),
        if verdict.row.passed() {
            "passed"
        } else {
            "failed"
        },
        verdict.commit,
        short(&verdict.row.state)
    )];
    let lines: Vec<String> = proofs.iter().map(proof_line).collect();
    parts.push(format!(
        "This task's commits it covers:\n{}",
        lines.join("\n")
    ));
    if !mine.is_empty() {
        let errors: Vec<String> = mine
            .iter()
            .map(|d| format!("- {}:{} {}", d.path, d.line, d.text))
            .collect();
        parts.push(format!(
            "Errors in this task's files:\n{}",
            errors.join("\n")
        ));
    }
    Draft {
        title: headline(task, verdict, proofs, mine),
        body: parts.join("\n\n"),
        tags: vec!["check".to_string(), task.to_string()],
        data: json!({
            "task": task,
            "seat": seat,
            "checked_commit": verdict.commit,
            "state": verdict.row.state,
            "passed": verdict.row.passed(),
            "commits": proofs
                .iter()
                .map(|p| json!({"sha": p.sha, "proven": p.proven}))
                .collect::<Vec<_>>(),
            "mine": mine,
        }),
    }
}
