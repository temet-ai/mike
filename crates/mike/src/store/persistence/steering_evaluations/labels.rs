use super::super::{PersistError, PersistHandle};
use super::EvaluationLabel;

impl PersistHandle {
    pub async fn apply_evaluation_labels(
        &self,
        labels: &[EvaluationLabel],
    ) -> Result<u64, PersistError> {
        if labels.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(labels)?;
        crate::metrics::time_db("apply_evaluation_labels", async move {
            let result = sqlx::query!(
                "UPDATE steering.evaluations e
                    SET outcome_label = x.label,
                        label_source = x.source,
                        labelled_at = NOW(),
                        label_evidence = x.evidence
                   FROM jsonb_to_recordset($1::jsonb) AS x(uuid uuid, label text, source text,
                                                           evidence jsonb)
                  WHERE e.uuid = x.uuid
                    AND e.status <> 'dispatched'
                    AND (e.outcome_label IS NULL
                         OR (x.source = 'operator' AND e.label_source IS DISTINCT FROM 'operator')
                         OR (x.source = 'session_reply' AND e.label_source = 'session_reply'
                             AND e.label_evidence->>'labeller_version'
                                 IS DISTINCT FROM x.evidence->>'labeller_version'))",
                payload,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }
}
