use super::file_writes;
use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn summarise_file_writes(
        &self,
        filter: &file_writes::FileWriteFilter<'_>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<file_writes::FileWriteSummaryRow>, PersistError> {
        crate::metrics::time_db(
            "summarise_file_writes",
            file_writes::summarise_file_writes(&self.pool, filter, offset, limit),
        )
        .await
    }
    pub(crate) async fn count_file_writes(
        &self,
        filter: &file_writes::FileWriteFilter<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db(
            "count_file_writes",
            file_writes::count_file_writes(&self.pool, filter),
        )
        .await
    }
    pub(crate) async fn fetch_writes_for_paths(
        &self,
        paths: &[String],
        filter: &file_writes::FileWriteFilter<'_>,
    ) -> Result<Vec<file_writes::FileWriteRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_writes_for_paths",
            file_writes::fetch_writes_for_paths(&self.pool, paths, filter),
        )
        .await
    }
}
