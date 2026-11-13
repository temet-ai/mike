use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

pub struct NewSearchEvent<'a> {
    pub session: Option<Uuid>,
    pub tool: &'a str,
    pub query: &'a str,
    pub kind_filter: Option<&'a str>,
    pub returned: &'a Value,
    pub rerank_state: &'a str,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn record_search_event(
        self,
        event: &NewSearchEvent<'_>,
    ) -> Result<i64, PersistError> {
        let id = sqlx::query_file_scalar!(
            "src/store/persistence/search_events/record.sql",
            event.session,
            event.tool,
            event.query,
            event.kind_filter,
            event.returned,
            event.rerank_state,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(id)
    }
}
