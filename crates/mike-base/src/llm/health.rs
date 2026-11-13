use std::sync::Arc;
use std::sync::OnceLock;

use arc_swap::ArcSwap;

use super::error::LlmError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmFailureDetail {
    pub status: Option<u16>,
    pub message: String,
    pub at: chrono::DateTime<chrono::Utc>,
}

impl From<&LlmError> for LlmFailureDetail {
    fn from(e: &LlmError) -> Self {
        let status = match e {
            LlmError::Http { status } => Some(*status),
            _ => None,
        };
        Self {
            status,
            message: e.to_string(),
            at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmHealthSnapshot {
    pub consecutive_failures: u32,
    pub last_failure: Option<LlmFailureDetail>,
}

static HEALTH: OnceLock<ArcSwap<LlmHealthSnapshot>> = OnceLock::new();

fn cell() -> &'static ArcSwap<LlmHealthSnapshot> {
    HEALTH.get_or_init(|| ArcSwap::from_pointee(LlmHealthSnapshot::default()))
}

pub fn record_success() {
    cell().store(Arc::new(LlmHealthSnapshot::default()));
}

pub fn record_failure(detail: LlmFailureDetail) {
    cell().rcu(|old| {
        Arc::new(LlmHealthSnapshot {
            consecutive_failures: old.consecutive_failures.saturating_add(1),
            last_failure: Some(detail.clone()),
        })
    });
}

pub fn snapshot() -> LlmHealthSnapshot {
    (**cell().load()).clone()
}

#[cfg(any(test, feature = "test-seams"))]
pub fn reset_for_test() {
    cell().store(Arc::new(LlmHealthSnapshot::default()));
}

#[cfg(any(test, feature = "test-seams"))]
pub fn test_detail(status: Option<u16>, message: &str) -> LlmFailureDetail {
    LlmFailureDetail {
        status,
        message: message.to_owned(),
        at: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_reports_zero_failures_and_no_detail() {
        reset_for_test();
        let snap = snapshot();
        assert_eq!(snap.consecutive_failures, 0);
        assert_eq!(snap.last_failure, None);
    }

    #[test]
    fn record_failure_increments_streak_and_keeps_latest_detail() -> anyhow::Result<()> {
        reset_for_test();
        record_failure(test_detail(Some(429), "rate limited"));
        record_failure(test_detail(Some(402), "payment required"));

        let snap = snapshot();
        assert_eq!(snap.consecutive_failures, 2);
        let last = snap
            .last_failure
            .ok_or_else(|| anyhow::anyhow!("a failure was just recorded"))?;
        assert_eq!(last.status, Some(402));
        assert_eq!(last.message, "payment required");
        Ok(())
    }

    #[test]
    fn record_success_clears_streak_and_detail() {
        reset_for_test();
        record_failure(test_detail(Some(402), "payment required"));
        assert_eq!(snapshot().consecutive_failures, 1);

        record_success();

        let snap = snapshot();
        assert_eq!(snap.consecutive_failures, 0);
        assert_eq!(snap.last_failure, None);
    }

    #[test]
    fn failure_detail_from_http_error_carries_status() {
        let detail = LlmFailureDetail::from(&LlmError::Http { status: 402 });
        assert_eq!(detail.status, Some(402));
    }

    #[test]
    fn failure_detail_from_timeout_has_no_status() {
        let detail = LlmFailureDetail::from(&LlmError::Timeout);
        assert_eq!(detail.status, None);
    }
}
