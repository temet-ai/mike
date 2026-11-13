use chrono::{DateTime, Utc};

use super::PersistError;

#[derive(Debug, Clone, Copy)]
pub struct JudgeAttemptKey<'a> {
    pub session_db_id: i64,
    pub judge_model: &'a str,
    pub rubric_version: i32,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn skip_repeated_digest(
        self,
        key: &JudgeAttemptKey<'_>,
        digest: &[u8],
        at: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("skip_repeated_digest", async move {
            let done = sqlx::query!(
                "UPDATE session_judges.attempts
                    SET last_attempt_at = GREATEST(last_attempt_at, $5)
                  WHERE session_id = $1 AND judge_model = $2 AND rubric_version = $3
                    AND last_digest_sha256 = $4",
                key.session_db_id,
                key.judge_model,
                key.rubric_version,
                digest,
                at,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected() > 0)
        })
        .await
    }

    pub async fn record_judged_digest(
        self,
        key: &JudgeAttemptKey<'_>,
        digest: &[u8],
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("record_judged_digest", async move {
            sqlx::query!(
                "UPDATE session_judges.attempts
                    SET last_digest_sha256 = $4
                  WHERE session_id = $1 AND judge_model = $2 AND rubric_version = $3",
                key.session_db_id,
                key.judge_model,
                key.rubric_version,
                digest,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
