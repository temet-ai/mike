use uuid::Uuid;

use super::PersistError;

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub(crate) async fn insert_response_intercept(
        self,
        session_uuid: Uuid,
        request_id: Option<i64>,
        outcome: &str,
        subject: Option<&str>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("insert_response_intercept", async move {
            let done = sqlx::query!(
                "INSERT INTO requests.response_intercepts (session_id, request_id, outcome, subject)
                 SELECT s.id, $2, $3, $4
                   FROM sessions.sessions s
                  WHERE s.session_uuid = $1",
                session_uuid,
                request_id,
                outcome,
                subject,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}
