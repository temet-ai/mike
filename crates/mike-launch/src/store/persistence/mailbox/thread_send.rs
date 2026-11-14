use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

use super::thread_types::ThreadResolution;
use super::types::{ContextItem, InsertedMessage, ThreadedMessageInsert};

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn resolve_thread_for_send(
        self,
        thread_uuid: Option<uuid::Uuid>,
        in_reply_to_message_uuid: Option<uuid::Uuid>,
        subject: Option<&str>,
        recipient_project_id: i64,
        created_by_session_id: Option<i64>,
    ) -> Result<ThreadResolution, PersistError> {
        if let Some(tu) = thread_uuid {
            return Ok(match self.fetch_thread_core_by_uuid(tu).await? {
                Some(core) => {
                    self.touch_thread(core.id).await?;
                    ThreadResolution::Resolved {
                        thread_id: core.id,
                        thread_uuid: core.thread_uuid,
                        subject: core.subject,
                        in_reply_to_id: None,
                    }
                }
                None => ThreadResolution::ThreadNotFound,
            });
        }

        if let Some(parent_uuid) = in_reply_to_message_uuid {
            let Some(parent) = self.fetch_message_core_by_uuid(parent_uuid).await? else {
                return Ok(ThreadResolution::ParentMessageNotFound);
            };
            let thread_core = match parent.thread_id {
                Some(tid) => {
                    self.touch_thread(tid).await?;
                    match self.fetch_thread_core_by_id(tid).await? {
                        Some(core) => core,
                        None => {
                            return Err(PersistError::DataInvariant(format!(
                                "resolve_thread_for_send: message {parent_uuid} points at \
                                 thread id {tid} with no matching mailbox.threads row"
                            )));
                        }
                    }
                }
                None => {
                    let subject: String = parent.body.chars().take(80).collect();
                    let created = self
                        .create_thread(
                            uuid::Uuid::new_v4(),
                            &subject,
                            Some(recipient_project_id),
                            created_by_session_id,
                        )
                        .await?;
                    self.adopt_message_into_thread(parent.id, created.id)
                        .await?;
                    created
                }
            };
            return Ok(ThreadResolution::Resolved {
                thread_id: thread_core.id,
                thread_uuid: thread_core.thread_uuid,
                subject: thread_core.subject,
                in_reply_to_id: Some(parent.id),
            });
        }

        if let Some(subject) = subject {
            let trimmed = subject.trim();
            if !trimmed.is_empty() {
                let created = self
                    .create_thread(
                        uuid::Uuid::new_v4(),
                        trimmed,
                        Some(recipient_project_id),
                        created_by_session_id,
                    )
                    .await?;
                return Ok(ThreadResolution::Resolved {
                    thread_id: created.id,
                    thread_uuid: created.thread_uuid,
                    subject: created.subject,
                    in_reply_to_id: None,
                });
            }
        }

        Ok(ThreadResolution::None)
    }

    async fn insert_context_items<'e, E>(
        executor: E,
        message_id: i64,
        items: &[ContextItem],
    ) -> Result<(), PersistError>
    where
        E: sqlx::PgExecutor<'e>,
    {
        if items.is_empty() {
            return Ok(());
        }
        let kinds: Vec<&str> = items.iter().map(|i| i.kind.as_str()).collect();
        let values: Vec<&str> = items.iter().map(|i| i.value.as_str()).collect();
        let labels: Vec<Option<&str>> = items.iter().map(|i| i.label.as_deref()).collect();
        sqlx::query!(
            "INSERT INTO mailbox.message_context_items (message_id, kind, value, label)
             SELECT $1, u.kind, u.value, u.label
               FROM UNNEST($2::text[], $3::text[], $4::text[]) AS u(kind, value, label)",
            message_id,
            &kinds as &[&str],
            &values as &[&str],
            &labels as &[Option<&str>],
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn insert_message_with_context(
        self,
        message_uuid: uuid::Uuid,
        to_session_uuid: uuid::Uuid,
        from_session_uuid: Option<uuid::Uuid>,
        body: &str,
        context: &[ContextItem],
    ) -> Result<Option<InsertedMessage>, PersistError> {
        crate::metrics::time_db("mailbox_insert_message_with_context", async move {
            let from_session_id: Option<i64> = match from_session_uuid {
                Some(uuid) => {
                    let id: Option<i64> = self.session_row_id(uuid).await?;
                    match id {
                        Some(id) => Some(id),
                        None => {
                            return Err(PersistError::DataInvariant(format!(
                                "insert_message_with_context: from_session_uuid {uuid} not found"
                            )));
                        }
                    }
                }
                None => None,
            };

            let Some(to_session_id) = self.session_row_id(to_session_uuid).await? else {
                return Ok(None);
            };
            let queued_at = self
                .insert_threaded_message_with_context(
                    ThreadedMessageInsert {
                        message_uuid,
                        to_session_id: Some(to_session_id),
                        from_session_id,
                        body,
                        thread_id: None,
                        in_reply_to_id: None,
                        held: false,
                    },
                    context,
                )
                .await?;
            Ok(queued_at.map(|queued_at| InsertedMessage {
                message_uuid,
                from_session_uuid,
                queued_at,
            }))
        })
        .await
    }

    pub async fn insert_threaded_message_with_context(
        self,
        params: ThreadedMessageInsert<'_>,
        context: &[ContextItem],
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("mailbox_insert_threaded_message_with_context", async move {
            let queued = if context.is_empty() {
                Self::insert_threaded_row(&self.pool, &params)
                    .await?
                    .map(|row| row.1)
            } else {
                self.insert_threaded_with_items(&params, context).await?
            };
            if queued.is_none() {
                self.audit_closed_recipient(params.from_session_id, params.to_session_id)
                    .await;
            }
            Ok(queued)
        })
        .await
    }

    async fn insert_threaded_with_items(
        self,
        params: &ThreadedMessageInsert<'_>,
        context: &[ContextItem],
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        let mut tx = self.pool.begin().await?;
        let row = Self::insert_threaded_row(&mut *tx, params).await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(None);
        };
        Self::insert_context_items(&mut *tx, row.0, context).await?;
        tx.commit().await?;
        Ok(Some(row.1))
    }

    async fn insert_threaded_row<'e, E>(
        executor: E,
        params: &ThreadedMessageInsert<'_>,
    ) -> Result<Option<(i64, DateTime<Utc>)>, PersistError>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let row = sqlx::query_file!(
            "src/store/persistence/mailbox/insert_threaded_message.sql",
            params.message_uuid,
            params.to_session_id,
            params.from_session_id,
            params.body,
            params.thread_id,
            params.in_reply_to_id,
            params.held,
        )
        .fetch_optional(executor)
        .await?;
        Ok(row.map(|row| (row.id, row.queued_at)))
    }
}
