use crate::store::persistence::PersistError;

use super::ThreadResolution;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn attach_reply(
        self,
        message_uuid: uuid::Uuid,
        parent_uuid: uuid::Uuid,
        recipient: uuid::Uuid,
        sender: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        let Some((_, recipient_project_id)) = self.resolve_session_core(recipient).await? else {
            return Ok(false);
        };
        let created_by = self.resolve_session_core(sender).await?.map(|(id, _)| id);
        let resolution = self
            .resolve_thread_for_send(
                None,
                Some(parent_uuid),
                None,
                recipient_project_id,
                created_by,
            )
            .await?;
        let ThreadResolution::Resolved {
            thread_id,
            in_reply_to_id,
            ..
        } = resolution
        else {
            return Ok(false);
        };
        crate::metrics::time_db("mailbox_attach_reply", async move {
            let attached = sqlx::query!(
                "UPDATE mailbox.messages
                    SET thread_id = $1, in_reply_to_id = $2
                  WHERE message_uuid = $3
                    AND thread_id IS NULL
                    AND in_reply_to_id IS NULL",
                thread_id,
                in_reply_to_id,
                message_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(attached.rows_affected() > 0)
        })
        .await
    }
}
