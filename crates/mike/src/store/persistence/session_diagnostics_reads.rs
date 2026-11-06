use super::rehydrate;
use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn aggregate_diagnostic_stats(
        &self,
        project: Option<&str>,
    ) -> Result<rehydrate::DiagnosticAggregateRows, PersistError> {
        crate::metrics::time_db(
            "aggregate_diagnostic_stats",
            rehydrate::aggregate_diagnostic_stats(&self.pool, project),
        )
        .await
    }
    pub(crate) async fn list_diagnostic_findings(
        &self,
        query: &rehydrate::FindingsQuery<'_>,
    ) -> Result<Vec<rehydrate::DiagnosticFindingRow>, PersistError> {
        crate::metrics::time_db(
            "list_diagnostic_findings",
            rehydrate::list_diagnostic_findings(&self.pool, query),
        )
        .await
    }
    pub(crate) async fn count_diagnostic_findings(
        &self,
        category: Option<&str>,
        analyzer: Option<&str>,
        session_id: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db(
            "count_diagnostic_findings",
            rehydrate::count_diagnostic_findings(
                &self.pool, category, analyzer, session_id, project,
            ),
        )
        .await
    }
    pub(crate) async fn fetch_session_diagnostic_hit_counts(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Vec<(String, i64)>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_diagnostic_hit_counts",
            rehydrate::fetch_session_diagnostic_hit_counts(&self.pool, session_uuid),
        )
        .await
    }
    pub(crate) async fn fetch_intercepted_count_for_session(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db(
            "fetch_intercepted_count_for_session",
            rehydrate::fetch_intercepted_count_for_session(&self.pool, session_uuid),
        )
        .await
    }
}
