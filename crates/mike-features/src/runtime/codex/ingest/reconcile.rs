use std::collections::{BTreeSet, HashSet, VecDeque};

use uuid::Uuid;

use crate::runtime::RuntimeObservation;
use crate::runtime::codex::history::{
    HistoryAction, HistoryBlocker, HistoryCoverage, HistoryCursorLedger, HistoryCursorVisit,
    HistoryEntry, HistoryError, HistoryErrorKind, HistoryIdentity, HistoryPage, HistoryQuery,
    HistoryRequest, HistoryScope, next_history_step,
};
use crate::runtime::codex::{ItemKey, RequestId};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub pages_read: usize,
    pub turns: BTreeSet<(String, String)>,
    pub items: BTreeSet<ItemKey>,
    pub blockers: Vec<(String, HistoryBlocker)>,
    pub failed_pages: Vec<(String, HistoryErrorKind)>,
    pub undetermined_ends: usize,
    pub coverage: HistoryCoverage,
}

#[derive(Debug, Clone)]
pub struct Reconciler {
    namespace_id: Uuid,
    scan_id: Uuid,
    limit: u32,
    queue: VecDeque<HistoryQuery>,
    visited: HashSet<CursorKey>,
    threads: BTreeSet<String>,
    report: ReconcileReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CursorKey {
    thread_id: String,
    turn_id: Option<String>,
    cursor: String,
}

impl CursorKey {
    fn new(query: &HistoryQuery, cursor: &str) -> Self {
        Self {
            thread_id: query.identity().thread_id().to_owned(),
            turn_id: query.turn_id().map(str::to_owned),
            cursor: cursor.to_owned(),
        }
    }
}

struct Ledger<'a>(&'a HashSet<CursorKey>);

impl HistoryCursorLedger for Ledger<'_> {
    fn lookup(&self, query: &HistoryQuery, cursor: &str) -> HistoryCursorVisit {
        if self.0.contains(&CursorKey::new(query, cursor)) {
            HistoryCursorVisit::Committed
        } else {
            HistoryCursorVisit::NotVisited
        }
    }
}

impl Reconciler {
    pub fn new(namespace_id: Uuid, limit: u32) -> Self {
        Self {
            namespace_id,
            scan_id: Uuid::new_v4(),
            limit,
            queue: VecDeque::new(),
            visited: HashSet::new(),
            threads: BTreeSet::new(),
            report: ReconcileReport::default(),
        }
    }

    pub fn add_thread(&mut self, thread_id: &str) -> Result<(), HistoryError> {
        if !self.threads.insert(thread_id.to_owned()) {
            return Ok(());
        }
        let identity = HistoryIdentity::new(self.scan_id, self.namespace_id, thread_id.into())?;
        self.queue
            .push_back(HistoryQuery::metadata(identity, self.limit)?);
        Ok(())
    }

    pub fn next_query(&mut self) -> Option<HistoryQuery> {
        let query = self.queue.pop_front()?;
        if let Some(cursor) = query.cursor() {
            self.visited.insert(CursorKey::new(&query, cursor));
        }
        Some(query)
    }

    pub fn next_request(&mut self, request_id: RequestId) -> Option<HistoryRequest> {
        self.next_query().map(|query| query.request(request_id))
    }

    pub fn finished(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn report(&self) -> &ReconcileReport {
        &self.report
    }

    pub fn on_response(&mut self, observation: &RuntimeObservation, request: &HistoryRequest) {
        let thread = request.query().identity().thread_id().to_owned();
        match HistoryPage::decode(observation, request) {
            Ok(page) => self.on_page(&page),
            Err(error) => self.report.failed_pages.push((thread, error.kind)),
        }
    }

    fn on_page(&mut self, page: &HistoryPage<'_>) {
        self.report.pages_read += 1;
        let query = page.request().query().clone();
        let thread = query.identity().thread_id().to_owned();
        let step = next_history_step(page, &Ledger(&self.visited));
        add_coverage(&mut self.report.coverage, step.coverage);
        for entry in page.entries().flatten() {
            self.on_entry(&query, &thread, entry);
        }
        match step.action {
            HistoryAction::ReadTurns(next) | HistoryAction::ReadPage(next) => {
                self.queue.push_back(next);
            }
            HistoryAction::ScopeEndObserved => {}
            HistoryAction::EndUndetermined => self.report.undetermined_ends += 1,
            HistoryAction::Blocked(blocker) => self.report.blockers.push((thread, blocker)),
        }
    }

    fn on_entry(&mut self, query: &HistoryQuery, thread: &str, entry: HistoryEntry<'_>) {
        match entry {
            HistoryEntry::Turn(turn) => {
                let fresh = self
                    .report
                    .turns
                    .insert((thread.to_owned(), turn.turn_id.to_owned()));
                if fresh && query.scope() == &HistoryScope::Turns {
                    self.queue_items(query, turn.turn_id);
                }
            }
            HistoryEntry::Item(item) => {
                self.report.items.insert(ItemKey {
                    namespace_id: self.namespace_id,
                    thread_id: thread.to_owned(),
                    turn_id: item.turn_id.to_owned(),
                    item_id: item.item_id.to_owned(),
                });
            }
        }
    }

    fn queue_items(&mut self, query: &HistoryQuery, turn_id: &str) {
        match HistoryQuery::items(query.identity().clone(), turn_id.into(), self.limit, None) {
            Ok(items) => self.queue.push_back(items),
            Err(error) => self
                .report
                .failed_pages
                .push((query.identity().thread_id().to_owned(), error.kind)),
        }
    }
}

fn add_coverage(total: &mut HistoryCoverage, page: HistoryCoverage) {
    total.undiscovered_entries += page.undiscovered_entries;
    total.uncertain_entry_semantics += page.uncertain_entry_semantics;
    total.uncertain_metadata |= page.uncertain_metadata;
    total.ephemeral |= page.ephemeral;
}
