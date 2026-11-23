use super::decide::Stmt;
use crate::store::persistence::consolidation::EvidenceChunk;

pub const EVIDENCE_BASE: i64 = 1_000_000;
const EVIDENCE_CHARS: usize = 700;
const OPERATOR_KINDS: [&str; 3] = ["correction", "unmet_ask", "question_answer"];

pub fn statement_line(stmt: &Stmt) -> String {
    let label = stmt.source_name.as_deref().unwrap_or(&stmt.kind);
    format!(
        "[{}] ({}, {}, {}) {}",
        stmt.id,
        label,
        stmt.carrier,
        stmt.stated_at.format("%Y-%m-%d"),
        stmt.text.replace('\n', " ")
    )
}

pub fn evidence_id(index: usize) -> i64 {
    EVIDENCE_BASE + index as i64
}

pub fn is_operator_kind(kind: &str) -> bool {
    OPERATOR_KINDS.contains(&kind)
}

fn evidence_line(index: usize, chunk: &EvidenceChunk) -> String {
    let who = if is_operator_kind(&chunk.kind) {
        "typed by the operator"
    } else {
        "session text"
    };
    let body: String = chunk.body.chars().take(EVIDENCE_CHARS).collect();
    format!(
        "[{}] ({}, {}, {}) {}",
        evidence_id(index),
        chunk.kind,
        chunk.dated_at.format("%Y-%m-%d"),
        who,
        body.replace('\n', " ")
    )
}

pub fn judge_user(members: &[Stmt], nearby: &[Stmt], evidence: &[EvidenceChunk]) -> String {
    let list = |items: &[Stmt]| {
        items
            .iter()
            .map(statement_line)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let evidence_text = if evidence.is_empty() {
        "(none)".to_owned()
    } else {
        evidence
            .iter()
            .enumerate()
            .map(|(i, chunk)| evidence_line(i, chunk))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "STATEMENTS\n{}\n\nNEARBY\n{}\n\nEVIDENCE\n{evidence_text}",
        list(members),
        if nearby.is_empty() {
            "(none)".to_owned()
        } else {
            list(nearby)
        }
    )
}
