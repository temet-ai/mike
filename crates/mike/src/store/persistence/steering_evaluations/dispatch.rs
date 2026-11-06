use super::super::{PersistError, PersistHandle};
use super::NewEvaluation;

impl PersistHandle {
    pub async fn insert_evaluations_dispatched(
        &self,
        new: &NewEvaluation<'_>,
        questions: &[Option<String>],
    ) -> Result<Vec<(uuid::Uuid, Option<String>)>, PersistError> {
        if questions.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("insert_evaluations_dispatched", async move {
            let rows = sqlx::query!(
                r#"INSERT INTO steering.evaluations
                           (steerer_id, session_id, operator_turn_id, status, mode, bundle_id,
                            question, trigger_point, trigger_request_id)
                       SELECT $1, $2, $3, 'dispatched', $4, $5, q.question, $6, $7
                         FROM unnest($8::text[]) AS q(question)
                       RETURNING uuid AS "uuid!", question AS "question?""#,
                new.steerer_id,
                new.session_id,
                new.operator_turn_id,
                new.mode.code(),
                new.bundle_id,
                new.trigger_point,
                new.trigger_request_id,
                questions as &[Option<String>],
            )
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| (r.uuid, r.question))
            .collect();
            Ok(rows)
        })
        .await
    }

    pub async fn insert_evaluation_dispatched(
        &self,
        new: &NewEvaluation<'_>,
    ) -> Result<uuid::Uuid, PersistError> {
        let rows = self.insert_evaluations_dispatched(new, &[None]).await?;
        rows.into_iter()
            .next()
            .map(|(uuid, _)| uuid)
            .ok_or_else(|| {
                PersistError::DataInvariant("evaluation insert returned no row".to_owned())
            })
    }
}
