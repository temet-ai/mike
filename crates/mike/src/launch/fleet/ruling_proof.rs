use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::check::{CheckRequest, state_for};
use super::check_run::baseline;
use super::definition::FleetDefinition;
use super::ruling;
use super::status::AcceptanceObligation;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;
use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RulingProof {
    pub note: Uuid,
    pub obligation_id: String,
    pub task: String,
    pub required: bool,
    pub version: Option<u64>,
    pub command: Option<String>,
    pub expected_exit_code: Option<i32>,
    pub commit: Option<String>,
    pub state: Option<String>,
    pub ledger: Option<String>,
    pub check_id: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub outcome: String,
    pub problem: Option<String>,
    pub output: Option<String>,
}

impl RulingProof {
    pub fn passed(&self) -> bool {
        self.outcome == "passed" && self.check_id.is_some() && self.problem.is_none()
    }
}

fn missing(entry: &AcceptanceObligation, task: &str, commit: Option<&str>) -> RulingProof {
    RulingProof {
        note: entry.note_id,
        obligation_id: entry.obligation_id.clone(),
        task: task.into(),
        required: entry.required,
        version: None,
        command: None,
        expected_exit_code: None,
        commit: commit.map(str::to_string),
        state: None,
        ledger: None,
        check_id: None,
        started_at: None,
        finished_at: None,
        outcome: "missing".into(),
        problem: None,
        output: None,
    }
}

fn observed(proof: &mut RulingProof, row: &TreeCheckRow, command: &str) {
    proof.check_id = Some(row.id);
    proof.started_at = Some(row.started_at);
    proof.finished_at = row.finished_at;
    proof.output = Some(row.output.clone());
    let verified = proof.ledger.as_deref() == Some(row.run_id.as_str())
        && proof.state.as_deref() == Some(row.state.as_str())
        && row.state_after.as_deref() == Some(row.state.as_str())
        && row.command == command
        && row.finished_at.is_some()
        && row.exit_code.is_some();
    proof.outcome = match (verified, row.exit_code) {
        (true, Some(0)) => "passed",
        (true, _) => "failed",
        _ => "unverifiable",
    }
    .into();
    if !verified {
        proof.problem =
            Some("receipt does not verify the exact clean commit and ruling contract".into());
    } else if row.exit_code != Some(0)
        && super::nextest_filter::matched_no_test(command, row.exit_code, &row.output)
    {
        proof.outcome = "definition_problem".into();
        proof.problem = Some(super::nextest_filter::definition_problem(&format!(
            "the ruling command of obligation `{}`",
            proof.obligation_id
        )));
    }
}

async fn read(
    persist: &PersistHandle,
    (def, run): (&FleetDefinition, &str),
    entry: &AcceptanceObligation,
    proof: &mut RulingProof,
) -> Result<(), String> {
    let rule = ruling::executable(entry)?;
    proof.version = Some(rule.version);
    proof.command = Some(rule.command.clone());
    proof.expected_exit_code = Some(rule.expected_exit_code);
    let commit = proof
        .commit
        .clone()
        .ok_or("no reported commit for this task")?;
    let def = ruling::definition(def, &rule);
    let request = CheckRequest {
        session: Uuid::nil(),
        wait_seconds: None,
        status: true,
        test: None,
        command: None,
        commit: Some(commit),
        ruling: None,
        withdraw: None,
    };
    let run = ruling::ledger(run, entry, &rule);
    let (request, ledger) = baseline::request(&def, &run, &request).await?;
    let commit = request.commit.ok_or("ruling commit was not resolved")?;
    let state = state_for(&def, &commit).await?;
    proof.commit = Some(commit);
    proof.state = Some(state);
    proof.ledger = Some(ledger.clone());
    match persist
        .latest_tree_check(&ledger)
        .await
        .map_err(|err| err.to_string())?
    {
        Some(row) if row.finished_at.is_none() => {
            proof.check_id = Some(row.id);
            proof.started_at = Some(row.started_at);
            proof.outcome = "running".into();
        }
        Some(row) => observed(proof, &row, &def.check.command),
        None => {}
    }
    Ok(())
}

pub(super) async fn collect_entries(
    persist: &PersistHandle,
    (def, run): (&FleetDefinition, &str),
    entries: &[AcceptanceObligation],
    task: &str,
    commit: Option<&str>,
) -> Vec<RulingProof> {
    let mut proofs = Vec::new();
    for entry in ruling::current_rules(entries, task) {
        let mut proof = missing(entry, task, commit);
        if let Err(problem) = read(persist, (def, run), entry, &mut proof).await {
            proof.outcome = "unverifiable".into();
            proof.problem = Some(problem);
        }
        proofs.push(proof);
    }
    proofs
}

pub async fn collect(
    persist: &PersistHandle,
    (def, fleet, run): (&FleetDefinition, &str, &str),
    task: &str,
    commit: Option<&str>,
) -> Result<Vec<RulingProof>, String> {
    let entries = ruling::registered(persist, def, fleet, run).await?;
    Ok(collect_entries(persist, (def, run), &entries, task, commit).await)
}

pub async fn newest_commit(persist: &PersistHandle, session: Uuid, task: &str) -> Option<String> {
    let commits = notes(persist, session, ("result", "commit")).await?;
    let reports = notes(
        persist,
        session,
        (super::report::REPORT_KIND, super::report::REPORT_TAG),
    )
    .await
    .unwrap_or_default();
    super::report_commit::judged_commit_of(&commits, &reports, task)
}

async fn notes(
    persist: &PersistHandle,
    session: Uuid,
    (kind, tag): (&str, &str),
) -> Option<Vec<(serde_json::Value, DateTime<Utc>)>> {
    let rows = persist
        .fetch_session_notes(
            session,
            SessionNoteFilter {
                kind: Some(kind),
                status: None,
                tag: Some(tag),
                q: None,
                order: NoteOrder::NewestFirst,
                limit: 100,
                offset: 0,
            },
        )
        .await
        .ok()?;
    Some(
        rows.into_iter()
            .map(|row| (row.data, row.created_at))
            .collect(),
    )
}

pub(super) async fn hydrate(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    entries: &mut [AcceptanceObligation],
) {
    for entry in entries
        .iter_mut()
        .filter(|entry| entry.data.get("ruling").is_some())
    {
        let mut proofs = Vec::new();
        for task in &entry.tasks {
            let commit = match def.tasks.get(task).and_then(super::report::task_session) {
                Some(session) => newest_commit(persist, session, task).await,
                None => None,
            };
            proofs.extend(
                collect_entries(
                    persist,
                    (def, run),
                    std::slice::from_ref(entry),
                    task,
                    commit.as_deref(),
                )
                .await,
            );
        }
        entry.ruling_proof = proofs;
    }
}

#[cfg(test)]
#[path = "ruling_proof_tests.rs"]
mod tests;
