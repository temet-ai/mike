use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{ChangedPathRow, TreeResultRow};

const WINDOW_HOURS: i64 = 6;
const ROWS_READ: i64 = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ReportProof {
    pub since: Option<DateTime<Utc>>,
    pub row_limit: i64,
    pub limit_reached: bool,
    pub unavailable: Option<String>,
    pub receipts: Vec<ProofReceipt>,
    pub rulings: Vec<super::ruling_proof::RulingProof>,
    pub rulings_unavailable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProofReceipt {
    pub use_id: i64,
    pub session: Option<Uuid>,
    pub seat: Option<String>,
    pub command: String,
    pub kinds: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub tool_verdict: String,
    pub still_valid: Option<bool>,
    pub paths_changed_since_started: usize,
    pub changed_paths: Value,
    pub update_mode: bool,
    pub full_output: Value,
}

fn updates(command: &str) -> bool {
    command.split_whitespace().any(|word| {
        word == "--update"
            || word == "-u"
            || word == "--update-snapshots"
            || word.split_once('=').is_some_and(|(name, value)| {
                name.contains("UPDATE") && !matches!(value, "0" | "false" | "never" | "no")
            })
    })
}

pub fn captured_result_validity(
    row: &TreeResultRow,
    changed: Option<&[ChangedPathRow]>,
) -> Option<bool> {
    let update_mode = row.command.as_deref().is_some_and(updates);
    let invalidated = changed.is_some_and(|changes| {
        changes.iter().any(|change| {
            row.started_at
                .is_some_and(|started| change.seen_at >= started)
        })
    });
    (update_mode || invalidated).then_some(false)
}

fn receipt(row: &TreeResultRow, changed: Option<&[ChangedPathRow]>) -> Option<ProofReceipt> {
    let command = row.command.as_deref()?;
    let paths_changed_since_started = changed
        .unwrap_or_default()
        .iter()
        .filter(|c| row.started_at.is_some_and(|at| c.seen_at >= at))
        .map(|c| c.path.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let update_mode = updates(command);
    let still_valid = captured_result_validity(row, changed);
    let tool_verdict = match (row.finished_at, row.is_error) {
        (None, _) => "running or completion unknown",
        (Some(_), Some(true)) => "failed",
        (Some(_), Some(false)) => "tool succeeded; inspect output for test or lint verdict",
        _ => "unknown",
    };
    Some(ProofReceipt {
        use_id: row.use_id,
        session: row.session_uuid,
        seat: row.seat.clone(),
        command: command.to_string(),
        kinds: Vec::new(),
        started_at: row.started_at,
        finished_at: row.finished_at,
        tool_verdict: tool_verdict.to_string(),
        still_valid,
        paths_changed_since_started,
        changed_paths: json!({
            "tool": "list_fleet_changes",
            "since": row.started_at,
        }),
        update_mode,
        full_output: output_reference(row),
    })
}

fn output_reference(row: &TreeResultRow) -> Value {
    match row.session_uuid {
        Some(session) => retrieval_reference(session, row.use_id),
        None => json!({"captured_output": row.content}),
    }
}

fn retrieval_reference(session: Uuid, use_id: i64) -> Value {
    json!({
        "tool": "list_session_tool_uses",
        "arguments": {"id": session, "include_results": true, "offset": 0},
        "find_use_id": use_id,
    })
}

pub async fn starting_prompt(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    seat: &super::plan::SeatPlan,
) -> String {
    let Some(task) = seat.task.as_deref() else {
        return seat.prompt.clone();
    };
    let mut proof = collect(persist, def, run).await;
    proof.retain_task(task);
    let rulings = super::ruling_notice::prompt(persist, def, run, task).await;
    if proof.receipts.is_empty() && proof.unavailable.is_none() {
        return if rulings.is_empty() {
            seat.prompt.clone()
        } else {
            format!("{}\n\n{rulings}", seat.prompt)
        };
    }
    let body = proof.handoff_body();
    format!(
        "{}\n\nPrevious seats of task {task}: use these captured receipts before repeating a command. They are observations at launch, not a promise that the tree will stay unchanged. Recheck validity with list_fleet_results before reuse.\n{}",
        seat.prompt,
        format_args!("{body}\n\n{rulings}")
    )
}

pub fn from_rows(rows: &[TreeResultRow], changed: Option<&[ChangedPathRow]>) -> ReportProof {
    ReportProof {
        row_limit: ROWS_READ,
        limit_reached: rows.len() >= ROWS_READ as usize,
        receipts: rows
            .iter()
            .filter_map(|row| receipt(row, changed))
            .collect(),
        ..ReportProof::default()
    }
}

pub async fn collect(persist: &PersistHandle, def: &FleetDefinition, run: &str) -> ReportProof {
    let since = Utc::now() - Duration::hours(WINDOW_HOURS);
    let rows = match persist
        .fleet()
        .tree_results(run, None, since, ROWS_READ)
        .await
    {
        Ok(rows) => rows,
        Err(err) => {
            return ReportProof {
                since: Some(since),
                row_limit: ROWS_READ,
                unavailable: Some(format!("captured commands could not be read: {err}")),
                ..ReportProof::default()
            };
        }
    };
    let earliest = rows.iter().filter_map(|row| row.started_at).min();
    let changed = match earliest {
        Some(at) => {
            persist
                .fleet()
                .tree_changed_paths_since(run, at - Duration::microseconds(1))
                .await
        }
        None => Ok(Vec::new()),
    };
    let mut proof = from_rows(&rows, changed.as_deref().ok());
    for receipt in &mut proof.receipts {
        receipt.kinds = match def.execution.named(&receipt.command) {
            Some((name, op)) => vec![format!("declared operation {name}: {}", op.purpose)],
            None => vec!["unclassified command; no declared proof purpose".into()],
        };
    }
    proof.since = Some(since);
    proof.unavailable = changed
        .err()
        .map(|err| format!("tree validity unknown: {err}"));
    proof
}

impl ReportProof {
    pub fn retain_task(&mut self, task: &str) {
        let suffix = format!("-{task}");
        self.receipts.retain(|receipt| {
            receipt
                .seat
                .as_deref()
                .is_some_and(|seat| seat.ends_with(&suffix))
        });
    }

    pub fn handoff_body(&self) -> String {
        let receipts = self.receipts.iter().take(6).map(|receipt| {
            json!({
                "use_id": receipt.use_id,
                "session": receipt.session,
                "seat": receipt.seat.as_deref().map(|seat| seat.chars().take(64).collect::<String>()),
                "command_excerpt": receipt.session.map(|_| receipt.command.chars().take(192).collect::<String>()),
                "full_command_and_output": receipt.session.map(|session| retrieval_reference(session, receipt.use_id)),
                "command_truncated": receipt.session.is_some() && receipt.command.chars().count() > 192,
                "tool_verdict": receipt.tool_verdict,
                "still_valid": receipt.still_valid,
                "update_mode": receipt.update_mode,
            })
        }).collect::<Vec<_>>();
        format!(
            "Captured receipts: {} available, {} shown. Commit binding is unknown; tool success is not proof that tests passed. Command excerpts may be truncated: their full_command_and_output field gives the exact retrieval call: advance offset through the session until find_use_id; input.command and result.content are the complete captured command and output. Retrieve the original before running it. Use list_fleet_results for current validity and list_session_tool_uses for each session/use_id's command and full output. Missing session means the command excerpt and retrieval reference are unavailable; ask for the recorded result instead. No captured output is copied into this brief.\n{}",
            self.receipts.len(),
            receipts.len(),
            json!({
                "since": self.since,
                "row_limit": self.row_limit,
                "limit_reached": self.limit_reached,
                "unavailable": self.unavailable.as_deref().map(|why| why.chars().take(256).collect::<String>()),
                "receipts": receipts,
            }),
        )
    }

    pub fn body(&self) -> String {
        let mut body = String::from(
            "Captured Bash verification receipts (not a criterion verdict). Commit binding: unknown for these captured commands; captured commands have timestamps, not commit SHAs. Tool success alone does not prove tests passed, especially through pipelines or wrappers. Validity is unproven without commit, dependency and event-coverage proof: no observed changes yields null, and any observed tree change since command start invalidates reuse. Coverage of this task must be checked by acceptance.",
        );
        if !self.rulings.is_empty() || self.rulings_unavailable.is_some() {
            body.push_str("\nExecutable rulings are separate: only the exact acceptance-owned version run in mike's verified clean checkout can pass. Missing, failed, running or unverifiable required ruling proof blocks acceptance.");
        }
        body.push_str("\nNo currently valid tests run found in this captured window; proof at the reported commit is unknown.");
        if let Ok(json) = serde_json::to_string(self) {
            body.push_str(
                "\nEach receipt's full_output gives the retrieval call: advance offset through the session until find_use_id; input.command and result.content are the complete captured command and output. Each receipt's changed_paths gives the list_fleet_changes call that returns every path changed in the checkout from the command's start, with its seat and task.\nCaptured receipts, full-output retrieval references and query bounds:\n",
            );
            body.push_str(&json);
        }
        body
    }
}

#[cfg(test)]
#[path = "report_proof_tests.rs"]
mod tests;
