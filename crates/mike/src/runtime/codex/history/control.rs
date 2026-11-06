use super::{HistoryEntry, HistoryPage, HistoryQuery, HistoryScope};
use crate::runtime::codex::{Field, ItemCategory};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryCursorVisit {
    NotVisited,
    Committed,
    Unknown,
}

pub trait HistoryCursorLedger {
    fn lookup(&self, query: &HistoryQuery, cursor: &str) -> HistoryCursorVisit;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryBlocker {
    InvalidNextCursor,
    CursorCycle,
    CursorHistoryUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAction {
    ReadTurns(HistoryQuery),
    ReadPage(HistoryQuery),
    ScopeEndObserved,
    EndUndetermined,
    Blocked(HistoryBlocker),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryCoverage {
    pub undiscovered_entries: usize,
    pub uncertain_entry_semantics: usize,
    pub uncertain_metadata: bool,
    pub ephemeral: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStep {
    pub action: HistoryAction,
    pub coverage: HistoryCoverage,
}

pub fn next_history_step(page: &HistoryPage<'_>, ledger: &impl HistoryCursorLedger) -> HistoryStep {
    HistoryStep {
        action: action(page, ledger),
        coverage: coverage(page),
    }
}

fn action(page: &HistoryPage<'_>, ledger: &impl HistoryCursorLedger) -> HistoryAction {
    let query = page.request().query();
    if query.scope() == &HistoryScope::Metadata {
        return HistoryAction::ReadTurns(query.turn_query());
    }
    match page.next_cursor() {
        Field::Missing => HistoryAction::EndUndetermined,
        Field::Null => HistoryAction::ScopeEndObserved,
        Field::Invalid(_) => HistoryAction::Blocked(HistoryBlocker::InvalidNextCursor),
        Field::Value(cursor) => continue_page(query, cursor, ledger),
    }
}

fn continue_page(
    query: &HistoryQuery,
    cursor: &str,
    ledger: &impl HistoryCursorLedger,
) -> HistoryAction {
    if query.cursor() == Some(cursor) {
        return HistoryAction::Blocked(HistoryBlocker::CursorCycle);
    }
    match ledger.lookup(query, cursor) {
        HistoryCursorVisit::NotVisited => {
            HistoryAction::ReadPage(query.continuation(cursor.into()))
        }
        HistoryCursorVisit::Committed => HistoryAction::Blocked(HistoryBlocker::CursorCycle),
        HistoryCursorVisit::Unknown => {
            HistoryAction::Blocked(HistoryBlocker::CursorHistoryUnavailable)
        }
    }
}

fn coverage(page: &HistoryPage<'_>) -> HistoryCoverage {
    let mut coverage = HistoryCoverage::default();
    if let Some(metadata) = page.metadata() {
        coverage.ephemeral = metadata.ephemeral == Field::Value(true);
        coverage.uncertain_metadata = !matches!(&metadata.history_mode, Field::Value(mode) if matches!(mode.as_str(), "legacy" | "paginated"))
            || !matches!(metadata.ephemeral, Field::Value(_));
    }
    for entry in page.entries() {
        match entry {
            Err(_) => coverage.undiscovered_entries += 1,
            Ok(entry) if uncertain_entry(&entry) => coverage.uncertain_entry_semantics += 1,
            Ok(_) => {}
        }
    }
    coverage
}

fn uncertain_entry(entry: &HistoryEntry<'_>) -> bool {
    match entry {
        HistoryEntry::Item(item) => {
            item.category == ItemCategory::Unknown
                || matches!(item.started_at_ms, Field::Invalid(_))
                || matches!(item.completed_at_ms, Field::Invalid(_))
        }
        HistoryEntry::Turn(turn) => {
            !matches!(&turn.items_view, Field::Value(view) if matches!(view.as_str(), "notLoaded" | "summary" | "full"))
                || !matches!(&turn.status, Field::Value(status) if matches!(status.as_str(), "completed" | "interrupted" | "failed" | "inProgress"))
                || matches!(turn.started_at_seconds, Field::Invalid(_))
                || matches!(turn.completed_at_seconds, Field::Invalid(_))
        }
    }
}
