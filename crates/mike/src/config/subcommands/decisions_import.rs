use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Args, ValueEnum};
use serde_json::{Map, Value, json};

use crate::config::CliOverrides;
use crate::llm::decisions::record::{CallResult, DecisionContext, DecisionMode, RecordInsert};
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

const REPLAY_DAY: &str = "2026-11-13T18:00:00Z";

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReplayKind {
    Precheck,
    Seat,
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
    }
}

async fn store(persist: &PersistHandle, row: &Imported) -> Result<i64> {
    let request =
        json!({"model": "typesafe/jev-1.14", "state": row.state, "questions": row.questions});
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
        served_model: Some("typesafe/jev-1.14".to_owned()),
        cost_usd: row.cost_usd,
        ..CallResult::default()
    };
    let mut context = DecisionContext::new(row.point, DecisionMode::Replay, row.question_set);
    context.source_ref = Some(row.source_ref.clone());
    let id = crate::llm::decisions::store(persist)
        .insert_decision_record(RecordInsert {
            context: &context,
            model_requested: "typesafe/jev-1.14",
            state: &row.state,
            questions: &row.questions,
            request_body: &request,
            result: &result,
            created_at: Some(row.created_at),
        })
        .await?;
    if let Some((label, source)) = &row.label {
        crate::llm::decisions::store(persist)
            .insert_decision_outcome(id, label, source, &json!({"source_ref": row.source_ref}))
            .await?;
    }
    Ok(id)
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
    let imported: Vec<Imported> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| match args.kind {
            ReplayKind::Precheck => precheck_row(r),
            ReplayKind::Seat => seat_row(i, r),
        })
        .collect();
    let persist = super::eval::connect(cli, true).await?;
    let prefix = match args.kind {
        ReplayKind::Precheck => "jev_precheck_replay:",
        ReplayKind::Seat => "jev_seat_replay:",
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
