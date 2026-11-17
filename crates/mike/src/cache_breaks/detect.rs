use chrono::{DateTime, Utc};

pub(crate) const MIN_REWRITE_TOKENS: i64 = 5_000;
const HOUR_SECS: i64 = 3_600;
const FIVE_MINUTES_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage {
    pub(crate) cache_read: i64,
    pub(crate) cache_creation: i64,
    pub(crate) input: i64,
    pub(crate) creation_1h: i64,
    pub(crate) creation_5m: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub(crate) request_id: i64,
    pub(crate) session_id: i64,
    pub(crate) model: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) woken_by: String,
    pub(crate) usage: Usage,
    pub(crate) prev_request_id: i64,
    pub(crate) prev_model: Option<String>,
    pub(crate) prev_done_at: DateTime<Utc>,
    pub(crate) prev_usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Break,
    Expiry,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Break => "break",
            Self::Expiry => "expiry",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Verdict {
    pub(crate) kind: Kind,
    pub(crate) ttl_seconds: i64,
    pub(crate) gap_seconds: i64,
    pub(crate) expected_read: i64,
    pub(crate) cache_read: i64,
    pub(crate) tokens_rewritten: i64,
}

pub(crate) fn ttl_seconds(prev: &Usage, cur: &Usage) -> i64 {
    if prev.creation_1h > 0 {
        return HOUR_SECS;
    }
    if prev.creation_5m > 0 {
        return FIVE_MINUTES_SECS;
    }
    if cur.creation_5m > 0 && cur.creation_1h == 0 {
        return FIVE_MINUTES_SECS;
    }
    HOUR_SECS
}

fn cached_after(usage: &Usage) -> i64 {
    usage.cache_read + usage.cache_creation
}

fn went_cold(usage: &Usage) -> bool {
    let context = usage.cache_read + usage.cache_creation + usage.input;
    context > 0 && usage.cache_read * 5 < context
}

fn read_clearly_less(expected: i64, read: i64) -> bool {
    expected - read >= MIN_REWRITE_TOKENS.max(expected / 5)
}

pub(crate) fn judge(c: &Candidate) -> Option<Verdict> {
    if c.model != c.prev_model || c.usage.cache_creation <= MIN_REWRITE_TOKENS {
        return None;
    }
    let ttl = ttl_seconds(&c.prev_usage, &c.usage);
    let gap = (c.created_at - c.prev_done_at).num_seconds().max(0);
    let expected = cached_after(&c.prev_usage);
    let kind = if gap >= ttl {
        went_cold(&c.usage).then_some(Kind::Expiry)?
    } else {
        read_clearly_less(expected, c.usage.cache_read).then_some(Kind::Break)?
    };
    Some(Verdict {
        kind,
        ttl_seconds: ttl,
        gap_seconds: gap,
        expected_read: expected,
        cache_read: c.usage.cache_read,
        tokens_rewritten: c.usage.cache_creation,
    })
}

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
