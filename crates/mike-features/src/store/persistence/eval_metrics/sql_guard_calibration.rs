use sqlx::PgConnection;

use super::super::PersistError;
use super::rollup::RollupCtx;

pub async fn insert(conn: &mut PgConnection, ctx: &RollupCtx) -> Result<u64, PersistError> {
    let mut rows = sqlx::query_file!(
        "src/store/persistence/eval_metrics/candidate_bins.sql",
        ctx.window_end,
        ctx.rollup_id,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    rows += sqlx::query_file!(
        "src/store/persistence/eval_metrics/guard_fact.sql",
        ctx.window_end,
        ctx.rollup_id,
        ctx.labels_as_of,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows)
}
