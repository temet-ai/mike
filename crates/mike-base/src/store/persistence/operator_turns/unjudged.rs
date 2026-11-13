use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnjudgedBacklog {
    pub stale_turns: i64,
    pub oldest_closed_at: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub async fn unjudged_closed_turn_backlog(
        &self,
        closed_before: DateTime<Utc>,
    ) -> Result<UnjudgedBacklog, PersistError> {
        crate::metrics::time_db("unjudged_closed_turn_backlog", async move {
            let row = sqlx::query!(
                r#"SELECT COUNT(*)::bigint AS "stale_turns!", MIN(closed_at) AS oldest_closed_at
                     FROM memory.operator_turns
                    WHERE closed_at   IS NOT NULL
                      AND outcome_id  = 1
                      AND judge_model IS NULL
                      AND judge_failure_reason IS DISTINCT FROM 'permanent'
                      AND (opened_by IS NULL OR opened_by = 'operator')
                      AND closed_at   < $1"#,
                closed_before,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(UnjudgedBacklog {
                stale_turns: row.stale_turns,
                oldest_closed_at: row.oldest_closed_at,
            })
        })
        .await
    }
}

#[cfg(test)]
pub mod tests;
