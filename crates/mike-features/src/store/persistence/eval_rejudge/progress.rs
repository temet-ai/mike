use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RejudgeWindow {
    pub bundle_id: i64,
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct RejudgeProgress {
    pub eligible_total: i64,
    pub ineligible_total: i64,
    pub judged: i64,
    pub permanent_failures: i64,
    pub remaining: i64,
    pub calls_today: i64,
    pub cost_usd_so_far: f64,
    pub ineligible_reprojected: i64,
}

impl RejudgeProgress {
    pub fn cap_left_today(&self, daily_cap: i32) -> i64 {
        (i64::from(daily_cap) - self.calls_today).max(0)
    }

    pub fn days_at_cap(&self, daily_cap: i32) -> i64 {
        let cap = i64::from(daily_cap.max(1));
        (self.remaining + cap - 1) / cap
    }
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn rejudge_progress(
        self,
        window: &RejudgeWindow,
    ) -> Result<RejudgeProgress, PersistError> {
        crate::metrics::time_db("rejudge_progress", async move {
            Ok(sqlx::query_file_as!(
                RejudgeProgress,
                "src/store/persistence/eval_rejudge/progress.sql",
                window.bundle_id,
                window.since,
                window.until,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn rejudge_mean_cost_usd(self, bundle_id: i64) -> Result<Option<f64>, PersistError> {
        crate::metrics::time_db("rejudge_mean_cost_usd", async move {
            let from_gate = sqlx::query_file_scalar!(
                "src/store/persistence/eval_rejudge/gate_cost.sql",
                bundle_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            if let Some(Some(cost)) = from_gate {
                return Ok(Some(cost));
            }
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/eval_rejudge/verdict_cost.sql",
                bundle_id,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn passing_holdout_gate(self, bundle_id: i64) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("passing_holdout_gate", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/eval_rejudge/passing_gate.sql",
                bundle_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use super::RejudgeProgress;

    fn progress(calls_today: i64, remaining: i64) -> RejudgeProgress {
        RejudgeProgress {
            eligible_total: 0,
            ineligible_total: 0,
            judged: 0,
            permanent_failures: 0,
            remaining,
            calls_today,
            cost_usd_so_far: 0.0,
            ineligible_reprojected: 0,
        }
    }

    #[test]
    fn cap_left_never_goes_negative() {
        assert_eq!(progress(320, 0).cap_left_today(300), 0);
        assert_eq!(progress(120, 0).cap_left_today(300), 180);
    }

    #[test]
    fn days_at_cap_rounds_up() {
        assert_eq!(progress(0, 3900).days_at_cap(300), 13);
        assert_eq!(progress(0, 301).days_at_cap(300), 2);
        assert_eq!(progress(0, 0).days_at_cap(300), 0);
    }
}
