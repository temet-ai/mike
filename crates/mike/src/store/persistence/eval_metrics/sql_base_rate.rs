use sqlx::PgConnection;

use super::super::PersistError;
use super::rollup::RollupCtx;

const WINDOWS: [i32; 2] = [30, 90];

pub(crate) async fn insert(conn: &mut PgConnection, ctx: &RollupCtx) -> Result<u64, PersistError> {
    Ok(sqlx::query_file!(
        "src/store/persistence/eval_metrics/base_rate.sql",
        ctx.labels_as_of,
        ctx.window_end,
        &WINDOWS[..],
        ctx.rollup_id,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected())
}
