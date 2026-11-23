use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Roles {
    pub members: u32,
    pub elsewhere: u32,
    pub covered: u32,
    pub covered_outside: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    pub items_done: u32,
    pub items_deferred: u32,
    pub statements_created: u32,
    pub statements_kept: u32,
    pub statements_gone: u32,
    pub subjects_created: u32,
    pub subjects_changed: u32,
    pub subjects_judged: u32,
    pub subjects_skipped: u32,
    pub decisions: u32,
    pub applied: u32,
    pub proposed: u32,
    pub failed: u32,
    pub projects: BTreeSet<Option<i64>>,
    pub roles: BTreeMap<Option<i64>, Roles>,
    pub last_error: Option<String>,
}

impl Tally {
    pub fn worked(&self) -> bool {
        self.items_done > 0
            || self.items_deferred > 0
            || self.subjects_judged > 0
            || self.subjects_skipped > 0
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
    }

    pub fn summary(&self) -> String {
        format!(
            "items {} done {} deferred, statements {}, subjects {} judged {} skipped, decisions {} ({} applied, {} proposed, {} failed)",
            self.items_done,
            self.items_deferred,
            self.statements_created,
            self.subjects_judged,
            self.subjects_skipped,
            self.decisions,
            self.applied,
            self.proposed,
            self.failed
        )
    }
}
