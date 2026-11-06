use chrono::Duration;
use sqlx::PgConnection;

use super::super::PersistError;
use super::rollup::RollupCtx;

const TRAILING_DAYS: i32 = 30;

pub(crate) async fn insert_window(
    conn: &mut PgConnection,
    ctx: &RollupCtx,
    start: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<u64, PersistError> {
    let days = start.map(|s| {
        i32::try_from((ctx.window_end - s).num_days())
            .unwrap_or(i32::MAX)
            .max(1)
    });
    Ok(sqlx::query_file!(
        "src/store/persistence/eval_metrics/accuracy.sql",
        ctx.labels_as_of,
        ctx.window_end,
        start,
        ctx.rollup_id,
        days,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

pub(crate) async fn insert(conn: &mut PgConnection, ctx: &RollupCtx) -> Result<u64, PersistError> {
    let trailing_start = ctx.window_end - Duration::days(i64::from(TRAILING_DAYS));
    Ok(insert_window(conn, ctx, None).await?
        + insert_window(conn, ctx, Some(trailing_start)).await?)
}
