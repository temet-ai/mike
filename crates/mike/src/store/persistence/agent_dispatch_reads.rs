use super::{PersistError, PersistHandle};
use sqlx::{Postgres, QueryBuilder};

#[derive(sqlx::FromRow)]
pub(crate) struct AgentDispatchRow {
    pub(crate) parent_request_id: i64,
    pub(crate) session_id: String,
    pub(crate) project_id: i64,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) tool_id: String,
    pub(crate) subagent_type: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) brief_char_len: i64,
    pub(crate) brief_sha256: Option<String>,
    pub(crate) ord: i32,
}

pub(crate) struct AgentDispatchFilter {
    pub(crate) project: Option<String>,
    pub(crate) since: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) until: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) order: crate::types::SortOrder,
}

const DISPATCH_SELECT: &str = "SELECT tu.request_id                       AS parent_request_id,
        s.session_uuid::text AS session_id,
        rq.project_id,
        tu.created_at                       AS created_at,
        tu.wire_tool_use_id                 AS tool_id,
        a.name                              AS subagent_type,
        tu.input->>'description'            AS description,
        COALESCE(LENGTH(tu.input->>'prompt'), 0)::bigint AS brief_char_len,
        CASE WHEN tu.input ? 'prompt'
             THEN encode(digest(tu.input->>'prompt', 'sha256'), 'hex')
             ELSE NULL
        END                                 AS brief_sha256,
        tu.ord
   FROM agents.dispatches ad
   JOIN tools.uses        tu ON tu.id = ad.tool_use_id
   JOIN requests.requests         rq ON rq.id = tu.request_id
   JOIN sessions.sessions         s  ON s.id = rq.session_id
   JOIN agents.agents           a  ON a.id = ad.agent_id";

impl PersistHandle {
    pub(crate) async fn list_agent_dispatches_filtered(
        &self,
        filter: &AgentDispatchFilter,
    ) -> Result<Vec<AgentDispatchRow>, PersistError> {
        crate::metrics::time_db("list_agent_dispatches_filtered", async {
            let mut query = QueryBuilder::<Postgres>::new(DISPATCH_SELECT);
            if filter.project.is_some() {
                query.push(" JOIN projects.projects p ON p.id = rq.project_id");
            }
            query.push(" WHERE TRUE");
            if let Some(project) = &filter.project {
                query.push(" AND p.project_path = ").push_bind(project);
            }
            if let Some(since) = filter.since {
                query.push(" AND tu.created_at >= ").push_bind(since);
            }
            if let Some(until) = filter.until {
                query.push(" AND tu.created_at < ").push_bind(until);
            }
            query
                .push(" ORDER BY tu.created_at ")
                .push(filter.order.sql())
                .push(", tu.request_id ")
                .push(filter.order.sql())
                .push(", tu.ord ")
                .push(filter.order.sql_reversed())
                .push(" LIMIT ")
                .push_bind(filter.limit)
                .push(" OFFSET ")
                .push_bind(filter.offset);
            Ok(query
                .build_query_as::<AgentDispatchRow>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }
}
