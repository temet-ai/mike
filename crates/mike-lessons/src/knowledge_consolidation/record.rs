use super::apply::Applied;
use super::decide::Stmt;
use crate::store::persistence::consolidation::NewDecision;
use chrono::Utc;
use serde_json::Value;

pub struct Recorder {
    pub run_id: i64,
    pub project_id: Option<i64>,
    pub subject_id: i64,
    pub model: String,
    pub evidence: Value,
}

pub struct Entry<'a> {
    pub verdict: &'a str,
    pub counterpart: Option<&'a Stmt>,
    pub before: String,
    pub after: Option<String>,
    pub reason: String,
    pub applied: Applied,
}

impl Recorder {
    pub fn decision(&self, stmt: &Stmt, entry: Entry<'_>) -> NewDecision {
        NewDecision {
            run_id: self.run_id,
            project_id: self.project_id,
            subject_id: Some(self.subject_id),
            statement_id: Some(stmt.id),
            item_id: Some(stmt.item_id),
            carrier_reference: stmt.carrier.clone(),
            verdict: entry.verdict.to_owned(),
            counterpart_statement_id: entry.counterpart.map(|c| c.id),
            counterpart_reference: entry.counterpart.map(|c| c.carrier.clone()),
            text_before: entry.before,
            text_after: entry.after,
            reason: entry.reason,
            evidence: self.evidence.clone(),
            model: self.model.clone(),
            application: entry.applied.application().code().to_owned(),
            applied_at: matches!(entry.applied, Applied::Done).then(Utc::now),
            error: entry.applied.error(),
        }
    }
}

pub fn text_before(stmt: &Stmt) -> String {
    if stmt.is_file() {
        stmt.passage.clone().unwrap_or_else(|| stmt.text.clone())
    } else {
        stmt.text.clone()
    }
}
