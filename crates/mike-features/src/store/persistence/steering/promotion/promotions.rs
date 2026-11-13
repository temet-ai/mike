use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct PromotionRow {
    pub id: i64,
    pub policy_id: i64,
    pub question: String,
    pub from_mode: String,
    pub to_mode: String,
    pub action: String,
    pub decided_via: String,
    pub decided_at: DateTime<Utc>,
    pub gate_passed: bool,
    pub gate_json: serde_json::Value,
    pub override_reason: Option<String>,
    pub replaces_steerer_id: Option<i64>,
    pub restored_steerer_id: Option<i64>,
    pub change_log_id: Option<i64>,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn question_promotions(
        self,
        steerer_id: i64,
    ) -> Result<Vec<PromotionRow>, PersistError> {
        crate::metrics::time_db("question_promotions", async move {
            Ok(sqlx::query_as!(
                PromotionRow,
                "SELECT pr.id, pr.policy_id, p.question, pr.from_mode, pr.to_mode, pr.action,
                        pr.decided_via, pr.decided_at, pr.gate_passed, pr.gate_json,
                        pr.override_reason, pr.replaces_steerer_id, pr.restored_steerer_id,
                        pr.change_log_id
                   FROM steering.question_promotions pr
                   JOIN steering.question_policies p ON p.id = pr.policy_id
                  WHERE p.steerer_id = $1
                  ORDER BY pr.decided_at DESC, pr.id DESC",
                steerer_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
