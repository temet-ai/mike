use crate::store::persistence::PersistHandle;

impl PersistHandle {
    pub(super) async fn audit_closed_recipient(&self, from: Option<i64>, to: Option<i64>) {
        let (Some(from), Some(to)) = (from, to) else {
            return;
        };
        let result = sqlx::query!(
            "INSERT INTO mailbox.failed_sends (attempted_by_session_id, attempted_to, reason)
             SELECT $1, session_uuid::text, 'recipient_ended'
               FROM sessions.sessions WHERE id = $2",
            from,
            to,
        )
        .execute(&self.pool)
        .await;
        if let Err(err) = result {
            tracing::warn!(error = %err, from, to, "mailbox: closed recipient was refused but its audit could not be saved");
        }
    }
}
