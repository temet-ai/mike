use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct CorrectionLabelRow {
    pub label_id: i64,
    pub tier: String,
    pub split: String,
    pub created_at: DateTime<Utc>,
    pub corrected_turn_id: i64,
    pub corrected_created_at: DateTime<Utc>,
    pub corrected_prompt: String,
}

impl PersistHandle {
    pub async fn correction_labels_as_of(
        &self,
        session_uuid: uuid::Uuid,
        label_bound: DateTime<Utc>,
        tiers: &[String],
    ) -> Result<Vec<CorrectionLabelRow>, PersistError> {
        crate::metrics::time_db("correction_labels_as_of", async move {
            let rows = sqlx::query_as!(
                CorrectionLabelRow,
                r#"SELECT l.id AS "label_id!", l.tier AS "tier!", l.split AS "split!",
                          l.created_at AS "created_at!",
                          l.operator_turn_id AS "corrected_turn_id!",
                          t.created_at AS "corrected_created_at!",
                          COALESCE(t.user_prompt, '') AS "corrected_prompt!"
                     FROM eval.labels_as_of($2) l
                     JOIN memory.operator_turns t ON t.id = l.operator_turn_id
                    WHERE l.session_uuid = $1
                      AND l.subject_kind = 'turn'
                      AND l.axis = 'reaction'
                      AND l.label = 'corrected'
                      AND l.tier = ANY($3)
                    ORDER BY l.created_at, l.id"#,
                session_uuid,
                label_bound,
                tiers,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
