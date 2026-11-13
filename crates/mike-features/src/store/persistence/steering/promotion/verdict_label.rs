use serde_json::json;

use super::super::super::PersistError;
use super::super::super::eval_labels::assign_turn_split;

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn label_steer_evaluation(
        self,
        evaluation: uuid::Uuid,
        label: &str,
        note: Option<&str>,
        holdout_percent: i64,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("label_steer_evaluation", async move {
            let mut tx = self.pool.begin().await?;
            let turn_id = sqlx::query_scalar!(
                "SELECT operator_turn_id FROM steering.evaluations WHERE uuid = $1",
                evaluation,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(turn_id) = turn_id else {
                return Ok(None);
            };
            let split = assign_turn_split(&mut tx, turn_id, holdout_percent).await?;
            let provenance = json!({"via": "steer_verdict_control"});
            let id = sqlx::query_scalar!(
                r#"INSERT INTO eval.labels
                       (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis,
                        label, tier, method, split, source, note, provenance)
                   SELECT 'evaluation', e.id, e.operator_turn_id, e.session_id, s.session_uuid,
                          'steer_valid', $2, 'gold', 'operator_click', $5, 'spa', $3, $4
                     FROM steering.evaluations e
                     JOIN sessions.sessions s ON s.id = e.session_id
                    WHERE e.uuid = $1
                   RETURNING id AS "id!""#,
                evaluation,
                label,
                note,
                &provenance,
                &split,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(label_id) = id else {
                return Ok(None);
            };
            let evidence = json!({"gold_label_id": label_id, "note": note});
            sqlx::query!(
                "UPDATE steering.evaluations
                    SET outcome_label = $2, label_source = 'operator', labelled_at = now(),
                        label_evidence = $3
                  WHERE uuid = $1 AND status <> 'dispatched'",
                evaluation,
                label,
                &evidence,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(Some(label_id))
        })
        .await
    }
}
