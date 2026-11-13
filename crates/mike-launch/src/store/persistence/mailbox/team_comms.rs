use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn fetch_team_comms_dispatches(
        self,
        session_db_id: i64,
    ) -> Result<Vec<TeamCommsDispatchRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_team_comms_dispatches", async move {
            let rows = sqlx::query_as!(
                TeamCommsDispatchRow,
                r#"SELECT
                       COALESCE(a.name, r.subagent_persona) AS "agent_label!",
                       COALESCE(
                           (ARRAY_AGG(tu.input ->> 'description' ORDER BY tu.created_at)
                                FILTER (WHERE tu.tool_name = 'Agent'
                                          AND tu.input ->> 'description' IS NOT NULL)
                           )[1],
                           ''
                       ) AS "description!",
                       MIN(tu.created_at) AS "first_seen!",
                       MAX(tu.created_at) AS "last_seen!",
                       COUNT(DISTINCT tu.request_id) AS "request_count!"
                   FROM tools.uses tu
                   JOIN requests.requests r ON r.id = tu.request_id
                   LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
                   LEFT JOIN agents.agents a ON a.id = ai.agent_id
                  WHERE tu.session_id = $1
                    AND (a.name IS NOT NULL OR r.subagent_persona IS NOT NULL)
                  GROUP BY COALESCE(a.name, r.subagent_persona)
                  ORDER BY MIN(tu.created_at) ASC"#,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_team_comms_messages(
        self,
        session_db_id: i64,
    ) -> Result<Vec<TeamCommsMessageRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_team_comms_messages", async move {
            let rows = sqlx::query_as!(
                TeamCommsMessageRow,
                r#"SELECT
                       tu.created_at AS ts,
                       CASE WHEN a.name IS NULL AND r.subagent_persona IS NULL
                            THEN 'sent' ELSE 'received' END AS "direction!",
                       CASE WHEN a.name IS NULL AND r.subagent_persona IS NULL
                            THEN COALESCE(tu.input ->> 'to', '')
                            ELSE COALESCE(a.name, r.subagent_persona, '')
                       END AS "counterpart!",
                       tu.input ->> 'summary' AS summary,
                       COALESCE(tu.input ->> 'message', '') AS "body!"
                   FROM tools.uses tu
                   JOIN requests.requests r ON r.id = tu.request_id
                   LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
                   LEFT JOIN agents.agents a ON a.id = ai.agent_id
                  WHERE tu.session_id = $1
                    AND tu.tool_name = 'SendMessage'
                    AND (
                        (a.name IS NULL AND r.subagent_persona IS NULL)
                        OR (tu.input ->> 'to' = 'main')
                    )
                  ORDER BY tu.created_at ASC"#,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}

#[derive(Debug, Clone)]
pub struct TeamCommsDispatchRow {
    pub agent_label: String,
    pub description: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub request_count: i64,
}

#[derive(Debug, Clone)]
pub struct TeamCommsMessageRow {
    pub ts: DateTime<Utc>,
    pub direction: String,
    pub counterpart: String,
    pub summary: Option<String>,
    pub body: String,
}
