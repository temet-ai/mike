use super::types::{QuestionRow, STATE_OPEN, SettleKind, SettleOutcome, SettledQuestion};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn settle_question(
        &self,
        settled: &SettledQuestion<'_>,
    ) -> Result<SettleOutcome, PersistError> {
        let answer = settled.answer.map(str::trim).filter(|a| !a.is_empty());
        if settled.kind == SettleKind::Answered && answer.is_none() {
            return Err(PersistError::DataInvariant(
                "settle_question: an answer is empty".to_owned(),
            ));
        }
        crate::metrics::time_db("questions_settle", async move {
            let done = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/settle.sql",
                settled.question,
                settled.kind.state(),
                answer,
                settled.by,
            )
            .fetch_optional(&self.pool)
            .await?;
            if let Some(row) = done {
                return Ok(SettleOutcome::Settled(Box::new(row)));
            }
            match self.question_by_uuid(settled.question).await? {
                None => Ok(SettleOutcome::NotFound),
                Some(row) if row.state == STATE_OPEN => Err(PersistError::DataInvariant(format!(
                    "settle_question: settler session {} not found",
                    settled.by
                ))),
                Some(row) => Ok(SettleOutcome::AlreadySettled(Box::new(row))),
            }
        })
        .await
    }
}
