use crate::store::persistence::PersistError;

pub(crate) const ATTRIBUTE_BATCH: i64 = 250;
pub(crate) const ATTRIBUTE_WITHIN_SECS: f64 = 86_400.0;
pub(crate) const TOOL_CALL_LEAD_SECS: f64 = 60.0;

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub(crate) async fn attribute_search_callers(self) -> Result<u64, PersistError> {
        self.attribute_search_callers_within(ATTRIBUTE_BATCH, ATTRIBUTE_WITHIN_SECS)
            .await
    }

    pub(super) async fn attribute_search_callers_within(
        self,
        batch: i64,
        within_secs: f64,
    ) -> Result<u64, PersistError> {
        let rows = sqlx::query_file_scalar!(
            "src/store/persistence/search_events/attribute_callers.sql",
            batch,
            within_secs,
            TOOL_CALL_LEAD_SECS,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.len() as u64)
    }
}
