use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct ArmWrite {
    pub evaluation: uuid::Uuid,
    pub arm: &'static str,
    pub draw: f64,
    pub anchor_turn_id: i64,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct AnchorFire {
    pub wake_state: Option<String>,
    pub delivered: bool,
}

impl PersistHandle {
    pub async fn record_arm(&self, w: &ArmWrite) -> Result<bool, PersistError> {
        crate::metrics::time_db("record_arm", async move {
            let r = sqlx::query!(
                "UPDATE steering.evaluations
                    SET arm = $2, holdback_draw = $3, arm_anchor_turn_id = $4, due_at = $5
                  WHERE uuid = $1",
                w.evaluation,
                w.arm,
                w.draw,
                w.anchor_turn_id,
                w.due_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected() > 0)
        })
        .await
    }

    pub async fn anchor_history(
        &self,
        steerer_id: i64,
        question: &str,
        anchor_turn_id: i64,
    ) -> Result<Vec<AnchorFire>, PersistError> {
        crate::metrics::time_db("anchor_history", async move {
            Ok(sqlx::query_as!(
                AnchorFire,
                r#"SELECT i.wake_state AS "wake_state?",
                          (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL) AS "delivered!"
                     FROM steering.evaluations e
                     JOIN steering.interventions i ON i.uuid = e.intervention_uuid
                    WHERE e.steerer_id = $1 AND e.question = $2 AND e.arm = 'treated'
                      AND e.arm_anchor_turn_id = $3
                    ORDER BY e.dispatched_at"#,
                steerer_id,
                question,
                anchor_turn_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn same_change_fires(
        &self,
        steerer_id: i64,
        question: &str,
        session_id: i64,
        change_use_id: i64,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("same_change_fires", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT count(*)::bigint AS "count!"
                     FROM steering.evaluations e
                     JOIN steering.interventions i ON i.uuid = e.intervention_uuid
                    WHERE e.steerer_id = $1 AND e.question = $2 AND e.session_id = $3
                      AND e.arm = 'treated'
                      AND e.verdict_json -> 'facts' -> 'observation' ->> 'last_change_use_id' = $4
                      AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL
                           OR i.wake_state = 'pending')"#,
                steerer_id,
                question,
                session_id,
                change_use_id.to_string(),
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
