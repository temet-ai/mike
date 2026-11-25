use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mike_base::code_notes::{CodeNote, CodeNotes};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::code_scan::ReadyFinding;
use crate::store::persistence::fleet_repo::FleetPersist as _;

const DELIVERED_SEVERITIES: [&str; 2] = ["warning", "error"];
const REPLY_WAIT: Duration = Duration::from_secs(61);
const REPLY_BATCH: u64 = 100;

pub struct Poster {
    notes: Arc<CodeNotes>,
    cursor: i64,
    posted: HashSet<i64>,
    stale: bool,
    last_delivery: Option<Instant>,
}

impl Poster {
    pub fn new(notes: Arc<CodeNotes>) -> Self {
        Self {
            notes,
            cursor: 0,
            posted: HashSet::new(),
            stale: true,
            last_delivery: Instant::now().checked_sub(REPLY_WAIT),
        }
    }

    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    pub fn stale(&self) -> bool {
        self.stale
    }

    pub fn awaiting_replies(&self) -> bool {
        self.last_delivery.is_some()
    }

    pub fn withdraw(&self, ids: &[i64]) {
        self.notes.withdraw(ids);
    }

    pub async fn post(&mut self, persist: &PersistHandle, roots: &[String]) {
        if roots.is_empty() || !self.stale {
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
        self.stale = unnamed_floor.is_some();
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

    pub async fn remind(&self, persist: &PersistHandle, roots: &[String]) {
        if roots.is_empty() {
            return;
        }
        match persist.fleet().code_findings_to_remind(roots).await {
            Ok(rows) => {
                for row in rows {
                    let session = row.session;
                    let mut reminder = note(row);
                    reminder.message =
                        format!("{} Committed with no answer to mike.", reminder.message);
                    self.notes.post(session, vec![reminder]);
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "code findings: committed findings could not be read");
            }
        }
    }

    pub async fn record_delivered(&mut self, persist: &PersistHandle) {
        let delivered = self.notes.drain_delivered();
        if delivered.is_empty() {
            return;
        }
        match persist
            .fleet()
            .mark_code_findings_delivered(&delivered)
            .await
        {
            Ok(_) if delivered.iter().any(|d| d.request_id.is_some()) => {
                self.last_delivery = Some(Instant::now());
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(error = %err, count = delivered.len(), "code findings: delivery could not be recorded");
            }
        }
    }

    pub async fn record_replies(&mut self, persist: &PersistHandle) {
        let Some(last) = self.last_delivery else {
            return;
        };
        match persist.fleet().record_code_finding_replies().await {
            Ok(recorded) if recorded < REPLY_BATCH && last.elapsed() > REPLY_WAIT => {
                self.last_delivery = None;
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(error = %err, "code findings: replies could not be recorded");
            }
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
