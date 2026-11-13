use sqlx::PgPool;

use super::session_aggregates::SessionSummaryRow;
use super::session_summary::listing_by_ids_query;
use crate::store::persistence::PersistError;

pub async fn list_sessions_by_ids(
    pool: &PgPool,
    ids: &[uuid::Uuid],
) -> Result<Vec<SessionSummaryRow>, PersistError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = listing_by_ids_query(ids)
        .build_query_as::<SessionSummaryRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows)
}
