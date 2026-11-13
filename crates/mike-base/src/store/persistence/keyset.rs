use chrono::{DateTime, Utc};

pub const COUNT_CAP: i64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysetCursor {
    pub at: DateTime<Utc>,
    pub id: i64,
}

impl KeysetCursor {
    pub fn encode(&self) -> String {
        format!("{}.{}", self.at.timestamp_micros(), self.id)
    }

    pub fn decode(raw: &str) -> Option<Self> {
        let (micros, id) = raw.split_once('.')?;
        let at = DateTime::from_timestamp_micros(micros.parse().ok()?)?;
        Some(Self {
            at,
            id: id.parse().ok()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CappedCount {
    pub count: i64,
    pub capped: bool,
}

impl CappedCount {
    pub fn from_probe(probed: i64) -> Self {
        Self {
            count: probed.min(COUNT_CAP),
            capped: probed > COUNT_CAP,
        }
    }
}

pub fn probe_limit(limit: i64) -> i64 {
    limit.saturating_add(1)
}

pub fn split_page<T>(
    mut rows: Vec<T>,
    limit: i64,
    key: impl Fn(&T) -> KeysetCursor,
) -> (Vec<T>, Option<KeysetCursor>) {
    let keep = usize::try_from(limit).unwrap_or(0);
    if rows.len() <= keep {
        return (rows, None);
    }
    let _probe = rows.split_off(keep);
    let next = rows.last().map(key);
    (rows, next)
}

#[cfg(test)]
#[path = "keyset_tests.rs"]
pub mod keyset_tests;
