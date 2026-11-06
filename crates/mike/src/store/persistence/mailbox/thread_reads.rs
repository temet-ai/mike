use sqlx::{Postgres, QueryBuilder};

use crate::store::persistence::{PersistError, PersistHandle};

use super::thread_types::{
    ParticipantIdentityRow, ThreadMessageRow, ThreadSummaryFilter, ThreadSummaryRow,
};

const SUMMARIES_TAIL: &str = include_str!("thread_summaries_tail.sql");

fn thread_summaries_builder(filter: &ThreadSummaryFilter) -> QueryBuilder<Postgres> {
    let mut builder = QueryBuilder::new("WITH selected AS (SELECT t.id FROM mailbox.threads t");
    let mut keyword = " WHERE ";
    if let Some(thread_uuid) = filter.thread_uuid {
        builder
            .push(keyword)
            .push("t.thread_uuid = ")
            .push_bind(thread_uuid);
        keyword = " AND ";
    }
    if let Some(participant) = filter.participant_session_uuid {
        builder
            .push(keyword)
            .push(
                "t.id IN (SELECT pm.thread_id FROM mailbox.messages pm \
                 JOIN sessions.sessions ps ON ps.session_uuid = ",
            )
            .push_bind(participant)
            .push(
                " WHERE pm.thread_id IS NOT NULL AND pm.to_session_id IS NOT NULL \
                 AND (pm.from_session_id = ps.id OR pm.to_session_id = ps.id))",
            );
        keyword = " AND ";
    }
    if let Some(project_ids) = &filter.project_ids {
        builder
            .push(keyword)
            .push("t.project_id = ANY(")
            .push_bind(project_ids)
            .push(")");
        keyword = " AND ";
    }
    if let Some(resolved) = filter.resolved {
        builder
            .push(keyword)
            .push("(t.resolved_at IS NOT NULL) = ")
            .push_bind(resolved);
        keyword = " AND ";
    }
    if let Some(tag) = &filter.tag {
        builder
            .push(keyword)
            .push(
                "EXISTS (SELECT 1 FROM mailbox.thread_tags ft \
                 JOIN mailbox.tags fg ON fg.id = ft.tag_id \
                 WHERE ft.thread_id = t.id AND fg.code = ",
            )
            .push_bind(tag)
            .push(")");
    }
    builder.push(") ").push(SUMMARIES_TAIL);
    builder
}

impl PersistHandle {
    pub(crate) async fn fetch_thread_messages(
        &self,
        thread_id: i64,
    ) -> Result<Vec<ThreadMessageRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_thread_messages", async move {
            let rows = sqlx::query_as!(
                ThreadMessageRow,
                r#"SELECT
                       m.message_uuid,
                       recip.session_uuid  AS "to_session_uuid?",
                       sender.session_uuid AS "from_session_uuid?",
                       m.body,
                       m.queued_at,
                       m.claimed_at,
                       m.sent_request_id,
                       m.sent_at,
                       m.consumed_at,
                       m.dead_lettered_at,
                       m.channel_delivered_at,
                       m.delivery_attempts,
                       th.thread_uuid,
                       parent.message_uuid AS "in_reply_to_uuid?",
                       COALESCE(ca.items, '[]'::jsonb)
                           AS "context_items!: super::types::ContextItems"
                   FROM mailbox.messages m
                   LEFT JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                   JOIN mailbox.threads th ON th.id = m.thread_id
                   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
                   LEFT JOIN LATERAL (
                       SELECT jsonb_agg(
                                  jsonb_build_object('kind', ci.kind, 'value', ci.value, 'label', ci.label)
                                  ORDER BY ci.id
                              ) AS items
                         FROM mailbox.message_context_items ci
                        WHERE ci.message_id = m.id
                   ) ca ON true
                  WHERE m.thread_id = $1
                  ORDER BY m.id ASC"#,
                thread_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn fetch_thread_participants(
        &self,
        thread_id: i64,
    ) -> Result<Vec<uuid::Uuid>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_thread_participants", async move {
            let rows = sqlx::query_scalar!(
                r#"SELECT DISTINCT s.session_uuid AS "session_uuid!"
                     FROM (
                         SELECT from_session_id AS session_id FROM mailbox.messages
                          WHERE thread_id = $1 AND from_session_id IS NOT NULL
                         UNION
                         SELECT to_session_id AS session_id FROM mailbox.messages
                          WHERE thread_id = $1 AND to_session_id IS NOT NULL
                         UNION
                         SELECT created_by_session_id AS session_id FROM mailbox.threads
                          WHERE id = $1 AND created_by_session_id IS NOT NULL
                     ) ids
                     JOIN sessions.sessions s ON s.id = ids.session_id"#,
                thread_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn fetch_participant_identities(
        &self,
        session_uuids: &[uuid::Uuid],
    ) -> Result<Vec<ParticipantIdentityRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_participant_identities", async move {
            let rows = sqlx::query_as!(
                ParticipantIdentityRow,
                r#"WITH target AS (
                       SELECT s.id, s.session_uuid, s.title
                         FROM sessions.sessions s
                        WHERE s.session_uuid = ANY($1)
                   ),
                   subagent AS (
                       SELECT DISTINCT ON (ai.session_id) ai.session_id, ag.name AS agent_type
                         FROM agents.instances ai
                         JOIN agents.agents ag ON ag.id = ai.agent_id
                        WHERE ai.session_id IN (SELECT id FROM target)
                        ORDER BY ai.session_id, ai.created_at DESC
                   )
                   SELECT
                       target.session_uuid AS "session_uuid!",
                       target.title,
                       sg.role,
                       sg.display_name,
                       subagent.agent_type
                     FROM target
                     LEFT JOIN sessions.session_goals sg ON sg.session_id = target.id
                     LEFT JOIN subagent ON subagent.session_id = target.id"#,
                session_uuids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn fetch_thread_summaries(
        &self,
        filter: &ThreadSummaryFilter,
    ) -> Result<Vec<ThreadSummaryRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_thread_summaries", async move {
            let rows = thread_summaries_builder(filter)
                .build_query_as::<ThreadSummaryRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }
}
