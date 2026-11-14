use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Args, ValueEnum};
use serde_json::{Map, Value, json};

use crate::config::CliOverrides;
use crate::llm::decisions::record::{
    Applied, CallResult, DecisionContext, DecisionMode, RecordInsert,
};
use crate::store::persistence::PersistHandle;

const PRECHECK_QUESTIONS: &str = r#"{
 "worth_sending": {"type": "noul", "instructions": "A supervisor proposes sending proposed_note to the assistant before the operator reads final_message. Would the operator want this note sent?", "criteria": {"true": "The note points at a real gap the operator would otherwise have to raise themselves.", "false": "The operator would accept final_message as it is, or the note's concern does not matter for this ask."}},
 "gap_matters": {"type": "noul", "instructions": "Does the operator's ask depend on the result being checked by running, rendering or testing it?", "criteria": {"true": "The ask is about behaviour or appearance that only running or looking at it confirms.", "false": "The ask is about text, a plan, an answer or a change whose correctness is visible in the change itself."}},
 "already_disclosed": {"type": "noul", "instructions": "Does final_message itself tell the operator plainly what was not checked?", "criteria": {"true": "The message names the unchecked part in words the operator cannot miss.", "false": "The message presents the work as finished without naming what was not checked."}}
}"#;

const SEAT_QUESTION: &str = r#"{
 "spent_on": {"type": "choice", "instructions": "This is one tool call by a coding-agent seat in a fleet, working on the task named in task_name. What is this call spent on?", "criteria": {"project_work": "Reading, writing or testing the code, docs or issues that task_name is about.", "fleet_machinery": "Fleet process rather than the task itself: fleet tools, mail, notes, roster, reports, check receipts, commit bookkeeping, answering hooks, reading fleet rules or briefs.", "waiting": "Waiting or polling: sleeping, checking whether a build, check, permit or reply has arrived."}}
}"#;

const NOTES: [(&str, &str); 3] = [
    (
        "unobserved_done",
        "The assistant reports work as done, but nothing observed the result after its last change.",
    ),
    (
        "relapse",
        "The assistant has slipped back into a behaviour it was corrected for earlier.",
    ),
    (
        "unbacked_wait",
        "The assistant says it is waiting on something, but nothing it started is running.",
    ),
];

#[cfg(test)]
#[path = "decisions_import_tests.rs"]
mod decisions_import_tests;

const REPLAY_DAY: &str = "2026-11-13T18:00:00Z";
const JEV_MODEL: &str = "typesafe/jev-1.14";

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReplayKind {
    Precheck,
    Seat,
    Replay,
}

#[derive(Debug, Clone, Args)]
pub struct DecisionsImportArgs {
    #[arg(long, value_enum)]
    pub kind: ReplayKind,
    pub file: PathBuf,
}

struct Imported {
    point: &'static str,
    question_set: &'static str,
    created_at: DateTime<Utc>,
    state: Value,
    questions: Value,
    answers: Option<Value>,
    cost_usd: Option<f64>,
    label: Option<(String, &'static str)>,
    source_ref: String,
    model: String,
    latency_ms: Option<i32>,
    composed: Option<Value>,
}

fn precheck_row(row: &Value) -> Imported {
    let question = row["question"].as_str().unwrap_or("");
    let concern = NOTES
        .iter()
        .find(|(q, _)| *q == question)
        .map_or(question, |(_, n)| n);
    let observation = &row["facts"]["observation"];
    let state = json!({
        "operator_ask": row["ask"],
        "final_message": row["final"],
        "facts": {"observed_after_last_change": observation["observed_after_change"],
                  "wait_backing": row["facts"]["wait_backing"]},
        "proposed_note": {"concern": concern, "why": row["steerer_reason"]},
    });
    let answers = row["jev"].as_object().map(|a| {
        Value::Object(
            a.iter()
                .map(|(k, v)| (k.clone(), json!({"type": "noul", "noul": v})))
                .collect::<Map<_, _>>(),
        )
    });
    Imported {
        point: "turn_guard_precheck",
        question_set: "turn_guard_precheck_replay_2026_11_13",
        created_at: REPLAY_DAY.parse().unwrap_or_else(|_| Utc::now()),
        state,
        questions: serde_json::from_str(PRECHECK_QUESTIONS).unwrap_or(Value::Null),
        answers,
        cost_usd: None,
        label: row["label"]
            .as_str()
            .map(|l| (l.to_owned(), "turn_guard_label")),
        source_ref: format!("jev_precheck_replay:{}", row["id"]),
        model: JEV_MODEL.to_owned(),
        latency_ms: None,
        composed: None,
    }
}

fn seat_row(index: usize, row: &Value) -> Imported {
    let answers = row["jev"].as_str().map(|choice| {
        json!({"spent_on": {"type": "choice", "choice": choice, "confidence": row["confidence"]}})
    });
    Imported {
        point: "effort_split",
        question_set: "fleet_seat_step_replay_2026_11_13",
        created_at: row["at"]
            .as_str()
            .and_then(|t| t.parse().ok())
            .unwrap_or_else(Utc::now),
        state: json!({"task_name": row["task_name"], "tool": row["tool"],
                      "input": row["input"], "result": row["result"]}),
        questions: serde_json::from_str(SEAT_QUESTION).unwrap_or(Value::Null),
        answers,
        cost_usd: None,
        label: row["rule"]
            .as_str()
            .map(|l| (l.to_owned(), "tool_name_rule")),
        source_ref: format!(
            "jev_seat_replay:{}:{}",
            row["task"].as_str().unwrap_or("?"),
            index
        ),
        model: JEV_MODEL.to_owned(),
        latency_ms: None,
        composed: None,
    }
}

async fn store(persist: &PersistHandle, row: &Imported) -> Result<i64> {
    let request = json!({"model": row.model, "state": row.state, "questions": row.questions});
    let result = CallResult {
        outcome: if row.answers.is_some() {
            "ok"
        } else {
            "transport_error"
        },
        error: row
            .answers
            .is_none()
            .then(|| "the replay recorded no answer".to_owned()),
        answers: row.answers.clone(),
        served_model: Some(row.model.clone()),
        cost_usd: row.cost_usd,
        latency_ms: row.latency_ms,
        ..CallResult::default()
    };
    let mut context = DecisionContext::new(row.point, DecisionMode::Replay, row.question_set);
    context.source_ref = Some(row.source_ref.clone());
    let id = crate::llm::decisions::store(persist)
        .insert_decision_record(RecordInsert {
            context: &context,
            model_requested: &row.model,
            state: &row.state,
            questions: &row.questions,
            request_body: &request,
            result: &result,
            created_at: Some(row.created_at),
        })
        .await?;
    if let Some(composed) = &row.composed {
        let applied = Applied {
            composed: composed.clone(),
            applied: "none",
            fail_open: row.answers.is_none(),
        };
        crate::llm::decisions::store(persist)
            .mark_decision_applied(id, &applied, None)
            .await?;
    }
    if let Some((label, source)) = &row.label {
        crate::llm::decisions::store(persist)
            .insert_decision_outcome(id, label, source, &json!({"source_ref": row.source_ref}))
            .await?;
    }
    Ok(id)
}

fn replay_row(line: usize, row: &Value) -> Result<Imported> {
    let point = row["point"].as_str().unwrap_or("");
    let Some(known) = super::decisions_replay::known_point(point) else {
        bail!("line {line}: `{point}` is not a replay point");
    };
    if row["schema"] != super::decisions_replay::SCHEMA {
        bail!(
            "line {line}: schema is not {}",
            super::decisions_replay::SCHEMA
        );
    }
    let outcome = &row["outcome"];
    Ok(Imported {
        point: known.point,
        question_set: known.question_set,
        created_at: row["asked_at"]
            .as_str()
            .and_then(|t| t.parse().ok())
            .unwrap_or_else(Utc::now),
        state: row["request"]["state"].clone(),
        questions: row["request"]["questions"].clone(),
        answers: (!row["answers"].is_null()).then(|| row["answers"].clone()),
        cost_usd: row["cost_usd"].as_f64(),
        label: outcome["label"]
            .as_str()
            .map(|label| (label.to_owned(), known.outcome_source)),
        source_ref: row["source_ref"].as_str().unwrap_or("").to_owned(),
        model: row["request"]["model"]
            .as_str()
            .unwrap_or(JEV_MODEL)
            .to_owned(),
        latency_ms: row["latency_ms"]
            .as_i64()
            .and_then(|l| i32::try_from(l).ok()),
        composed: (!row["composed"].is_null()).then(|| row["composed"].clone()),
    })
}

fn import_rows(kind: ReplayKind, rows: &[Value]) -> Result<Vec<Imported>> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| match kind {
            ReplayKind::Precheck => Ok(precheck_row(r)),
            ReplayKind::Seat => Ok(seat_row(i, r)),
            ReplayKind::Replay => replay_row(i + 1, r),
        })
        .collect()
}

pub async fn run(args: DecisionsImportArgs, cli: CliOverrides) -> Result<()> {
    let text = std::fs::read_to_string(&args.file)
        .with_context(|| format!("reading {}", args.file.display()))?;
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .context("each line must be one JSON object")?;
    if rows.is_empty() {
        bail!("{} has no rows", args.file.display());
    }
    let imported = import_rows(args.kind, &rows)?;
    let persist = super::eval::connect(cli, true).await?;
    let prefix = match args.kind {
        ReplayKind::Precheck => "jev_precheck_replay:",
        ReplayKind::Seat => "jev_seat_replay:",
        ReplayKind::Replay => "decisions_replay:",
    };
    let existing: HashSet<String> = crate::llm::decisions::store(&persist)
        .decision_source_refs(prefix)
        .await?
        .into_iter()
        .collect();
    let mut added = 0;
    for row in imported
        .iter()
        .filter(|r| !existing.contains(&r.source_ref))
    {
        store(&persist, row).await?;
        added += 1;
    }
    println!(
        "imported {added} of {} rows ({} were already recorded)",
        imported.len(),
        imported.len() - added
    );
    Ok(())
}
