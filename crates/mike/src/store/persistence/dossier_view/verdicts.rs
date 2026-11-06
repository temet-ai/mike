use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct LiveVerdictRow {
    pub verdict_id: i64,
    pub turn_id: i64,
    pub reaction: String,
    pub delivery: Option<String>,
    pub legacy_label: Option<String>,
    pub confidence: f32,
    pub rationale: String,
    pub created_at: DateTime<Utc>,
    pub bundle_id: Option<i64>,
}

impl PersistHandle {
    pub async fn fetch_live_verdicts_as_of(
        &self,
        session_db_id: i64,
        at: DateTime<Utc>,
        reconstructed: bool,
    ) -> Result<Vec<LiveVerdictRow>, PersistError> {
        crate::metrics::time_db("fetch_live_verdicts_as_of", async move {
            let rows = sqlx::query_as!(
                LiveVerdictRow,
                r#"SELECT DISTINCT ON (v.operator_turn_id)
                          v.id AS verdict_id, v.operator_turn_id AS turn_id,
                          v.reaction AS "reaction!", v.delivery, v.legacy_label,
                          v.confidence, v.rationale, v.created_at, v.bundle_id
                     FROM memory.turn_verdicts v
                     JOIN memory.operator_turns t ON t.id = v.operator_turn_id
                    WHERE v.session_id = $1
                      AND v.mode = 'live'
                      AND v.reaction IS NOT NULL
                      AND t.closed_at <= $2
                      AND (v.created_at <= $2 OR $3)
                    ORDER BY v.operator_turn_id, v.created_at DESC, v.id DESC"#,
                session_db_id,
                at,
                reconstructed,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
