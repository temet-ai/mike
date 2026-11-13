use sqlx::PgConnection;

use super::{LaunchSurfaceRefs, NewRun};

pub async fn close_run(connection: &mut PgConnection, run_id: &str) -> Result<bool, sqlx::Error> {
    let closed = sqlx::query_file_scalar!("src/store/persistence/launch/close_run.sql", run_id,)
        .fetch_optional(connection)
        .await?;
    Ok(closed.is_some())
}

pub async fn record_launch_opened(
    connection: &mut PgConnection,
    launch_id: i64,
    refs: &LaunchSurfaceRefs,
) -> Result<bool, sqlx::Error> {
    let recorded = sqlx::query_file_scalar!(
        "src/store/persistence/launch/record_launch_opened.sql",
        launch_id,
        refs.workspace_ref,
        refs.tab_ref,
        refs.surface_ref,
        refs.pid,
        refs.actual_placement,
    )
    .fetch_optional(connection)
    .await?;
    Ok(recorded.is_some())
}

pub async fn upsert_run(connection: &mut PgConnection, run: &NewRun) -> Result<(), sqlx::Error> {
    sqlx::query_file!(
        "src/store/persistence/launch/upsert_run.sql",
        run.run_id,
        run.project_path,
        run.backend,
        run.workspace_ref,
        run.tab_ref,
    )
    .execute(connection)
    .await?;
    Ok(())
}
