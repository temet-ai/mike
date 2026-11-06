use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenancePass {
    Shape,
    Placement,
}

impl MaintenancePass {
    pub fn code(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Placement => "placement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    DailyBudget,
    ProviderLimit,
}

impl RefusalReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::DailyBudget => "daily_budget",
            Self::ProviderLimit => "provider_limit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceRefusal {
    pub pass: String,
    pub reason: String,
    pub cap: i64,
    pub refused_at: DateTime<Utc>,
}

impl PersistHandle {
    pub async fn reserve_lesson_maintenance_call(&self, cap: i64) -> Result<bool, PersistError> {
        if cap <= 0 {
            return Ok(false);
        }
        crate::metrics::time_db("reserve_lesson_maintenance_call", async move {
            let consumed: Option<i64> = sqlx::query_scalar!(
                "INSERT INTO memory.lesson_proposer_daily_budget (day, consumed, maintenance_consumed)
                 VALUES ((NOW() AT TIME ZONE 'UTC')::date, 0, 1)
                 ON CONFLICT (day) DO UPDATE
                    SET maintenance_consumed = memory.lesson_proposer_daily_budget.maintenance_consumed + 1
                  WHERE memory.lesson_proposer_daily_budget.maintenance_consumed < $1
                 RETURNING maintenance_consumed",
                cap
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(consumed.is_some())
        })
        .await
    }

    pub async fn record_maintenance_refusal(
        &self,
        pass: MaintenancePass,
        reason: RefusalReason,
        cap: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("record_maintenance_refusal", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO memory.lesson_maintenance_refusals AS r (pass, reason, cap)
                   VALUES ($1, $2, $3)
                   ON CONFLICT (pass) DO UPDATE
                      SET reason = EXCLUDED.reason,
                          cap = EXCLUDED.cap,
                          refused_at = now(),
                          logged_at = CASE WHEN r.logged_at <= now() - interval '1 hour'
                                           THEN now() ELSE r.logged_at END
                   RETURNING (r.logged_at = now()) AS "logged!""#,
                pass.code(),
                reason.code(),
                cap
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn recent_maintenance_refusals(
        &self,
    ) -> Result<Vec<MaintenanceRefusal>, PersistError> {
        crate::metrics::time_db("recent_maintenance_refusals", async move {
            Ok(sqlx::query_as!(
                MaintenanceRefusal,
                "SELECT pass, reason, cap, refused_at
                   FROM memory.lesson_maintenance_refusals
                  WHERE refused_at > now() - interval '2 hours'
                    AND (refused_at AT TIME ZONE 'UTC')::date = (now() AT TIME ZONE 'UTC')::date
                  ORDER BY pass"
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
