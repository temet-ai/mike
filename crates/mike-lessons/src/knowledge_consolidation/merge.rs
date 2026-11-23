use super::answer::MergeGroup;
use super::apply::{Applied, merge_lessons};
use super::decide::{Plan, Stmt, merge_check_prompt, oldest};
use super::rewrite::{Rewrite, RewriteKind};
use super::guard::Guard;
use super::items::Env;
use super::llm::{ConsolidationLlm, Embedder};
use super::record::{Entry, Recorder};
use super::execute::Executed;
use crate::store::persistence::consolidation::StatementStateChange;
use std::collections::HashMap;
use std::time::Instant;

pub async fn accept_merges<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    plan: &Plan,
    stmts: &HashMap<i64, Stmt>,
) -> Vec<MergeGroup> {
    let mut accepted = Vec::new();
    for group in &plan.merge_candidates {
        if guard.open(Instant::now()) {
            break;
        }
        let texts: Vec<&str> = group
            .ids
            .iter()
            .filter_map(|id| stmts.get(id))
            .map(|s| s.text.as_str())
            .collect();
        match env
            .llm
            .check_missing(&merge_check_prompt(&texts, &group.text))
            .await
        {
            Ok((answer, _)) => {
                guard.succeeded(Instant::now());
                if answer.missing.iter().all(|m| m.trim().is_empty()) {
                    accepted.push(group.clone());
                }
            }
            Err(failure) => {
                guard.failed(&failure, Instant::now());
                tracing::info!(error = %failure.message, "knowledge consolidation: merge check failed; the lessons stay as they are");
            }
        }
    }
    accepted
}

pub async fn execute_merges<L, E>(
    env: &Env<'_, L, E>,
    recorder: &Recorder,
    merges: &[MergeGroup],
    stmts: &HashMap<i64, Stmt>,
    out: &mut Executed,
) {
    for group in merges {
        let Some(keep) = oldest(stmts, &group.ids) else {
            continue;
        };
        let others: Vec<&Stmt> = group
            .ids
            .iter()
            .filter(|id| **id != keep.id)
            .filter_map(|id| stmts.get(id))
            .collect();
        match merge_lessons(env.persist, keep, &others, &group.text).await {
            Ok(()) => record_merged(recorder, keep, &others, &group.text, out),
            Err(error) => out.decisions.push(recorder.decision(
                keep,
                Entry {
                    verdict: "tightened",
                    counterpart: None,
                    before: keep.text.clone(),
                    after: Some(group.text.clone()),
                    reason: "Merging the lessons into one failed".to_owned(),
                    applied: Applied::Failed(error),
                },
            )),
        }
    }
}

fn record_merged(
    recorder: &Recorder,
    keep: &Stmt,
    others: &[&Stmt],
    text: &str,
    out: &mut Executed,
) {
    let reason = if others.is_empty() {
        "Rewritten shorter; every instruction is still given".to_owned()
    } else {
        format!(
            "Now also gives the instructions of {} other lesson(s)",
            others.len()
        )
    };
    out.decisions.push(recorder.decision(
        keep,
        Entry {
            verdict: "tightened",
            counterpart: None,
            before: keep.text.clone(),
            after: Some(text.to_owned()),
            reason,
            applied: Applied::Done,
        },
    ));
    for other in others {
        out.decisions.push(recorder.decision(
            other,
            Entry {
                verdict: "merged",
                counterpart: Some(keep),
                before: other.text.clone(),
                after: Some(text.to_owned()),
                reason: format!("Merged into {}", keep.carrier),
                applied: Applied::Done,
            },
        ));
        out.states.push(StatementStateChange {
            statement_id: other.id,
            state: "merged".to_owned(),
            resolved_by_statement_id: Some(keep.id),
        });
    }
}

pub async fn accept_rewrites<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    plan: &Plan,
    stmts: &HashMap<i64, Stmt>,
) -> Vec<Rewrite> {
    let mut accepted = Vec::new();
    for rewrite in &plan.rewrites {
        if rewrite.kind == RewriteKind::Corrected {
            accepted.push(rewrite.clone());
            continue;
        }
        let Some(stmt) = stmts.get(&rewrite.id) else {
            continue;
        };
        if guard.open(Instant::now()) {
            break;
        }
        let original = stmt.passage.as_deref().filter(|_| stmt.is_file()).unwrap_or(&stmt.text);
        let prompt = merge_check_prompt(&[original], &rewrite.text);
        match env.llm.check_missing(&prompt).await {
            Ok((answer, _)) => {
                guard.succeeded(Instant::now());
                if answer.missing.iter().all(|m| m.trim().is_empty()) {
                    accepted.push(rewrite.clone());
                }
            }
            Err(failure) => {
                guard.failed(&failure, Instant::now());
                tracing::info!(error = %failure.message, "knowledge consolidation: tighten check failed; the statement stays as it is");
            }
        }
    }
    accepted
}
