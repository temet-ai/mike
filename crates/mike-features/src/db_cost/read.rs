use crate::store::persistence::state_repo::StatePersist as _;
use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::snapshot::{Counters, Snapshot};
use crate::store::persistence::db_cost_reads::StatementCounters;
use crate::store::persistence::{PersistError, PersistHandle};

pub enum Reading {
    Missing,
    Taken(Snapshot),
}

fn counters_of(rows: Vec<StatementCounters>) -> HashMap<i64, Counters> {
    rows.into_iter()
        .map(|row| {
            (
                row.queryid,
                Counters {
                    calls: row.calls,
                    generic_plan_calls: row.generic_plan_calls,
                    custom_plan_calls: row.custom_plan_calls,
                    exec_ms: row.exec_ms,
                    blks_hit: row.blks_hit,
                    blks_read: row.blks_read,
                    blks_dirtied: row.blks_dirtied,
                    blks_written: row.blks_written,
                },
            )
        })
        .collect()
}

pub async fn read_snapshot(persist: &PersistHandle) -> Result<Reading, PersistError> {
    Ok(match persist.state().read_statement_stats().await? {
        None => Reading::Missing,
        Some(reading) => Reading::Taken(Snapshot {
            taken_at: reading.taken_at,
            stats_reset: reading.stats_reset,
            statements: counters_of(reading.statements),
        }),
    })
}

pub async fn read_texts(
    persist: &PersistHandle,
    queryids: &[i64],
) -> Result<HashMap<i64, String>, PersistError> {
    persist.state().read_statement_texts(queryids).await
}

pub async fn count_requests(
    persist: &PersistHandle,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<i64, PersistError> {
    persist.state().count_requests_between(from, to).await
}

#[cfg(test)]
#[path = "read_db_tests.rs"]
pub mod read_db_tests;
