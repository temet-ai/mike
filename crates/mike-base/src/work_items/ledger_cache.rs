use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};

use super::attention::{self, Clock};
use super::ledger::{self, GitOutputRow, Ledger, WorkItemRow};

const REREAD_OVERLAP_MINUTES: i64 = 10;

#[derive(Debug, Clone)]
pub struct ScopeRow {
    pub session_db_id: i64,
    pub session_uuid: String,
    pub item: WorkItemRow,
}

#[derive(Debug, Clone)]
pub struct ActivityRow {
    pub session_db_id: i64,
    pub agent_id: Option<String>,
    pub last_request_at: DateTime<Utc>,
    pub main_last_request_at: Option<DateTime<Utc>>,
    pub last_edit_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct GitDeltaRow {
    pub result_id: i64,
    pub session_db_id: i64,
    pub row: GitOutputRow,
}

#[derive(Debug, Default)]
pub struct SessionReads {
    pub activity: Vec<ActivityRow>,
    pub git: Vec<GitDeltaRow>,
}

#[derive(Debug, Default, Clone, Copy)]
struct AgentActivity {
    last_request_at: Option<DateTime<Utc>>,
    last_edit_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct CachedSession {
    session_uuid: String,
    items: Vec<WorkItemRow>,
    agents: HashMap<String, AgentActivity>,
    main_last_request_at: Option<DateTime<Utc>>,
    git: BTreeMap<(DateTime<Utc>, i64), GitOutputRow>,
    read_up_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub struct LedgerCache {
    sessions: HashMap<i64, CachedSession>,
}

fn later(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    a.max(b)
}

impl CachedSession {
    fn new(session_uuid: String) -> Self {
        Self {
            session_uuid,
            items: Vec::new(),
            agents: HashMap::new(),
            main_last_request_at: None,
            git: BTreeMap::new(),
            read_up_to: None,
        }
    }

    fn forget_reads(&mut self) {
        self.agents.clear();
        self.main_last_request_at = None;
        self.git.clear();
        self.read_up_to = None;
    }

    fn apply_activity(&mut self, row: &ActivityRow) {
        if let Some(agent) = &row.agent_id {
            let entry = self.agents.entry(agent.clone()).or_default();
            entry.last_request_at = later(entry.last_request_at, Some(row.last_request_at));
            entry.last_edit_at = later(entry.last_edit_at, row.last_edit_at);
        }
        self.main_last_request_at = later(self.main_last_request_at, row.main_last_request_at);
        self.read_up_to = later(self.read_up_to, Some(row.last_request_at));
    }

    fn rows(&self) -> Vec<WorkItemRow> {
        self.items
            .iter()
            .map(|item| {
                let activity = self.agents.get(&item.task_id).copied().unwrap_or_default();
                WorkItemRow {
                    last_request_at: activity.last_request_at,
                    last_edit_at: activity.last_edit_at,
                    ..item.clone()
                }
            })
            .collect()
    }

    fn ledger(&self, now: DateTime<Utc>) -> (Ledger, Clock) {
        let git: Vec<GitOutputRow> = self.git.values().cloned().collect();
        let mut built = ledger::build_ledger(&self.session_uuid, &self.rows(), &git);
        let clock = Clock {
            now,
            main_last_request_at: self.main_last_request_at,
        };
        built.attention = attention::assess(&built, &clock);
        (built, clock)
    }
}

impl LedgerCache {
    pub fn apply_scope(&mut self, rows: Vec<ScopeRow>, asked: Option<&[String]>) {
        let mut found: HashMap<i64, (String, Vec<WorkItemRow>)> = HashMap::new();
        for row in rows {
            found
                .entry(row.session_db_id)
                .or_insert_with(|| (row.session_uuid, Vec::new()))
                .1
                .push(row.item);
        }
        match asked {
            None => self.sessions.retain(|id, _| found.contains_key(id)),
            Some(uuids) => {
                let asked: HashSet<&str> = uuids.iter().map(String::as_str).collect();
                self.sessions.retain(|id, cached| {
                    found.contains_key(id) || !asked.contains(cached.session_uuid.as_str())
                });
            }
        }
        for (id, (uuid, items)) in found {
            self.sessions
                .entry(id)
                .or_insert_with(|| CachedSession::new(uuid))
                .items = items;
        }
    }

    pub fn ids_of(&self, uuids: &[String]) -> Vec<i64> {
        let wanted: HashSet<&str> = uuids.iter().map(String::as_str).collect();
        let mut ids: Vec<i64> = self
            .sessions
            .iter()
            .filter(|(_, cached)| wanted.contains(cached.session_uuid.as_str()))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.sessions.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub fn session_uuid(&self, id: i64) -> Option<&str> {
        self.sessions.get(&id).map(|c| c.session_uuid.as_str())
    }

    pub fn reads_after(&self, ids: &[i64]) -> Vec<(i64, DateTime<Utc>)> {
        ids.iter()
            .filter_map(|id| {
                let cached = self.sessions.get(id)?;
                let after = cached.read_up_to.map_or(DateTime::<Utc>::UNIX_EPOCH, |at| {
                    at - Duration::minutes(REREAD_OVERLAP_MINUTES)
                });
                Some((*id, after))
            })
            .collect()
    }

    pub fn apply_reads(&mut self, reads: &SessionReads) {
        for row in &reads.activity {
            if let Some(cached) = self.sessions.get_mut(&row.session_db_id) {
                cached.apply_activity(row);
            }
        }
        for row in &reads.git {
            if let Some(cached) = self.sessions.get_mut(&row.session_db_id) {
                cached
                    .git
                    .insert((row.row.at, row.result_id), row.row.clone());
            }
        }
    }

    pub fn forget_reads(&mut self, id: i64) {
        if let Some(cached) = self.sessions.get_mut(&id) {
            cached.forget_reads();
        }
    }

    pub fn ledger(&self, id: i64, now: DateTime<Utc>) -> Option<Ledger> {
        self.sessions.get(&id).map(|cached| cached.ledger(now).0)
    }

    pub fn next_change(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.sessions
            .values()
            .filter_map(|cached| {
                let (built, clock) = cached.ledger(now);
                attention::next_change(&built, &clock)
            })
            .min()
    }
}
