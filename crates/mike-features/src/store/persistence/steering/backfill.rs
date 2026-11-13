use sqlx::PgConnection;

use crate::store::persistence::PersistError;

pub async fn backfill_sent_request_id(
    conn: &mut PgConnection,
    intervention_uuid: &str,
    request_id: i64,
) -> Result<bool, PersistError> {
    let rows = sqlx::query!(
        "UPDATE steering.interventions
            SET sent_request_id = $1
          WHERE uuid = $2::text::uuid
            AND sent_request_id IS NULL",
        request_id,
        intervention_uuid,
    )
    .execute(conn)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn backfill_trigger_request_id(
    conn: &mut PgConnection,
    intervention_uuid: &str,
    request_id: i64,
) -> Result<bool, PersistError> {
    let rows = sqlx::query!(
        "UPDATE steering.interventions
            SET trigger_request_id = $1
          WHERE uuid = $2::text::uuid
            AND trigger_request_id IS NULL",
        request_id,
        intervention_uuid,
    )
    .execute(conn)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn backfill_delivered_request_id(
    conn: &mut PgConnection,
    intervention_uuid: &str,
    request_id: i64,
) -> Result<bool, PersistError> {
    let rows = sqlx::query!(
        "UPDATE steering.interventions
            SET delivered_request_id = $1,
                processed_at = now()
          WHERE uuid = $2::text::uuid
            AND sent_request_id IS NOT NULL
            AND delivered_request_id IS NULL",
        request_id,
        intervention_uuid,
    )
    .execute(conn)
    .await?;
    Ok(rows.rows_affected() > 0)
}
