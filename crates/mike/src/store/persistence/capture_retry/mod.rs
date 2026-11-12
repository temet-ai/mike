mod late;
mod ledger;

pub(crate) use late::record_late;
pub(crate) use ledger::{CaptureRetryHealth, defer, health, is_deferred, note_unrecorded};

use super::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WriteFailure {
    Temporary(String),
    Refused(String),
    Other(String),
}

impl WriteFailure {
    pub(crate) fn reason(&self) -> &str {
        match self {
            WriteFailure::Temporary(r) | WriteFailure::Refused(r) | WriteFailure::Other(r) => r,
        }
    }
}

pub(crate) fn classify(error: &PersistError) -> WriteFailure {
    let source = match error {
        PersistError::Database(e) | PersistError::Connect { source: e, .. } => e,
        other => return WriteFailure::Other(other.to_string()),
    };
    match source {
        sqlx::Error::PoolTimedOut => {
            WriteFailure::Temporary("pool timed out waiting for an open connection".to_string())
        }
        sqlx::Error::Io(e) => WriteFailure::Temporary(format!("database connection error: {e}")),
        sqlx::Error::PoolClosed => WriteFailure::Temporary("database pool closed".to_string()),
        sqlx::Error::Database(db) => classify_code(db.code().as_deref(), db.message()),
        other => WriteFailure::Other(other.to_string()),
    }
}

fn classify_code(code: Option<&str>, message: &str) -> WriteFailure {
    let Some(code) = code else {
        return WriteFailure::Other(message.to_string());
    };
    let named = |what: &str| format!("{what} ({code}): {message}");
    match code {
        "57014" => WriteFailure::Temporary(named("statement timeout")),
        "55P03" => WriteFailure::Temporary(named("lock timeout")),
        "40001" | "40P01" => WriteFailure::Temporary(named("transaction conflict")),
        "53300" | "57P01" | "57P02" | "57P03" => {
            WriteFailure::Temporary(named("database unavailable"))
        }
        c if c.starts_with("08") => WriteFailure::Temporary(named("database connection lost")),
        c if c.starts_with("22") => WriteFailure::Refused(named("value refused by the database")),
        _ => WriteFailure::Other(named("database error")),
    }
}

#[cfg(test)]
#[path = "tests/db.rs"]
mod db_tests;
#[cfg(test)]
mod tests;
