use super::super::{PersistError, PersistHandle};
use super::{EvaluationVerdict, UNRECORDED_ABSTENTION};

fn verdict_columns(
    fired: bool,
    verdict: Option<EvaluationVerdict>,
) -> (Option<serde_json::Value>, Option<f64>, Option<String>) {
    match verdict {
        Some(v) => (Some(v.verdict_json), v.confidence, v.reason),
        None if !fired => (
            Some(serde_json::json!({
                "judged": false,
                "abstention": UNRECORDED_ABSTENTION,
            })),
            None,
            Some("not judged: the caller recorded no judgement".to_owned()),
        ),
        None => (None, None, None),
    }
}

impl PersistHandle {
    pub async fn resolve_evaluation(
        &self,
        evaluation_uuid: uuid::Uuid,
        fired: bool,
        intervention_uuid: Option<uuid::Uuid>,
        verdict: Option<EvaluationVerdict>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("resolve_evaluation", async move {
            let status = if fired {
                "resolved_correction"
            } else {
                "resolved_pass"
            };
            let (verdict_json, confidence, reason) = verdict_columns(fired, verdict);
            let rows = sqlx::query!(
                "UPDATE steering.evaluations
                    SET status = $1,
                        intervention_uuid = $2,
                        resolved_at = NOW(),
                        verdict_json = $4,
                        confidence = $5,
                        reason = $6
                  WHERE uuid = $3
                    AND status = 'dispatched'",
                status,
                intervention_uuid,
                evaluation_uuid,
                verdict_json,
                confidence,
                reason,
            )
            .execute(&self.pool)
            .await?;
            Ok(rows.rows_affected() > 0)
        })
        .await
    }
}
