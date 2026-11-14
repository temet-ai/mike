use std::collections::HashSet;
use std::sync::Arc;

use mike_base::code_notes::{CodeNote, CodeNotes};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::code_scan::ReadyFinding;
use crate::store::persistence::fleet_repo::FleetPersist as _;

const DELIVERED_SEVERITIES: [&str; 2] = ["warning", "error"];

pub struct Poster {
    notes: Arc<CodeNotes>,
    cursor: i64,
    posted: HashSet<i64>,
}

impl Poster {
    pub fn new(notes: Arc<CodeNotes>) -> Self {
        Self {
            notes,
            cursor: 0,
            posted: HashSet::new(),
        }
    }

    pub async fn post(&mut self, persist: &PersistHandle, roots: &[String]) {
        if roots.is_empty() {
            return;
        }
        let severities: Vec<String> = DELIVERED_SEVERITIES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let ready = persist
            .fleet()
            .code_findings_ready_to_post(roots, self.cursor, &severities)
            .await;
        let floor = persist
            .fleet()
            .code_findings_unnamed_floor(roots, self.cursor)
            .await;
        let (rows, floor) = match (ready, floor) {
            (Ok(rows), Ok(floor)) => (rows, floor),
            (Err(err), _) | (_, Err(err)) => {
                tracing::warn!(error = %err, "code findings: findings to tell sessions about could not be read");
                return;
            }
        };
        self.hand_over(rows, floor);
    }

    pub fn hand_over(&mut self, rows: Vec<ReadyFinding>, unnamed_floor: Option<i64>) {
        let newest = rows.iter().map(|r| r.id).max().unwrap_or(self.cursor);
        for row in rows {
            if self.posted.insert(row.id) {
                self.notes.post(row.session, vec![note(row)]);
            }
        }
        self.cursor = match unnamed_floor {
            Some(floor) => newest.min(floor - 1).max(self.cursor),
            None => newest,
        };
        let cursor = self.cursor;
        self.posted.retain(|id| *id > cursor);
    }

    pub async fn record_delivered(&self, persist: &PersistHandle) {
        let delivered = self.notes.drain_delivered();
        if delivered.is_empty() {
            return;
        }
        if let Err(err) = persist
            .fleet()
            .mark_code_findings_delivered(&delivered)
            .await
        {
            tracing::warn!(error = %err, count = delivered.len(), "code findings: delivery could not be recorded");
        }
    }
}

fn note(row: ReadyFinding) -> CodeNote {
    CodeNote {
        id: row.id,
        path: row.path,
        line: row.line,
        rule_id: row.rule_id,
        message: row.message,
        fix: row.fix,
    }
}
