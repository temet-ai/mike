use super::answer::Validated;
use super::apply::{FileOutcome, edit_file_statement, rewrite_lesson};
use super::decide::{EvidenceItem, Stmt};
use super::execute::{Executed, attach_edits};
use super::items::Env;
use super::record::{Entry, Recorder, text_before};
use super::subjects::Run;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteKind {
    Corrected,
    Tightened,
}

impl RewriteKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Corrected => "corrected",
            Self::Tightened => "tightened",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub id: i64,
    pub kind: RewriteKind,
    pub text: String,
    pub evidence: Option<i64>,
    pub reason: String,
}

pub const TIGHTEN_MIN_CHARS: usize = 200;
pub const TIGHTEN_MAX_PERCENT: usize = 75;

fn rewritable(stmt: &Stmt) -> bool {
    stmt.current && (stmt.is_active_lesson() || (stmt.is_file() && stmt.passage.is_some()))
}

fn original_len(stmt: &Stmt) -> usize {
    if stmt.is_file() {
        stmt.passage.as_deref().unwrap_or(&stmt.text).chars().count()
    } else {
        stmt.text.chars().count()
    }
}

pub fn plan_rewrites(
    validated: &Validated,
    stmts: &HashMap<i64, Stmt>,
    evidence: &[EvidenceItem],
    retired: &HashSet<i64>,
) -> Vec<Rewrite> {
    let mut out = Vec::new();
    for item in &validated.outdated {
        let (Some(stmt), Some(proof)) = (
            stmts.get(&item.id),
            evidence.iter().find(|e| e.id == item.evidence_id),
        ) else {
            continue;
        };
        let operator = matches!(proof.kind.as_str(), "correction" | "unmet_ask" | "question_answer");
        if rewritable(stmt) && !retired.contains(&stmt.id) && operator && proof.dated_at > stmt.stated_at {
            out.push(Rewrite {
                id: item.id,
                kind: RewriteKind::Corrected,
                text: item.text.clone(),
                evidence: Some(item.evidence_id),
                reason: format!("Corrected by {} ({})", proof.reference, proof.kind),
            });
        }
    }
    for item in &validated.tighten {
        let Some(stmt) = stmts.get(&item.id) else {
            continue;
        };
        let (before, after) = (original_len(stmt), item.text.chars().count());
        let taken = out.iter().any(|r| r.id == item.id);
        if rewritable(stmt)
            && !taken
            && !retired.contains(&stmt.id)
            && before >= TIGHTEN_MIN_CHARS
            && after * 100 <= before * TIGHTEN_MAX_PERCENT
        {
            out.push(Rewrite {
                id: item.id,
                kind: RewriteKind::Tightened,
                text: item.text.clone(),
                evidence: None,
                reason: format!("Rewritten shorter: {before} to {after} characters"),
            });
        }
    }
    out
}

pub async fn execute_rewrite<L, E>(
    env: &Env<'_, L, E>,
    run: &Run,
    recorder: &Recorder,
    rewrite: &Rewrite,
    stmts: &HashMap<i64, Stmt>,
    out: &mut Executed,
) {
    let Some(stmt) = stmts.get(&rewrite.id) else {
        return;
    };
    let outcome = if stmt.is_lesson() {
        FileOutcome::from(rewrite_lesson(env.persist, stmt, &rewrite.text).await)
    } else {
        edit_file_statement(
            env.persist,
            run.machine,
            env.settings.edit_files,
            stmt,
            Some(&rewrite.text),
        )
        .await
    };
    out.rejudge |= outcome.mismatch;
    let mut decision = recorder.decision(
        stmt,
        Entry {
            verdict: rewrite.kind.code(),
            counterpart: None,
            before: text_before(stmt),
            after: Some(rewrite.text.clone()),
            reason: rewrite.reason.clone(),
            applied: outcome.applied.clone(),
        },
    );
    attach_edits(&mut decision, &outcome.edits);
    out.decisions.push(decision);
}
