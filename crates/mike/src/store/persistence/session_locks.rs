use super::PersistError;

pub(super) async fn lock_agent_instance_binding(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(
                    hashtextextended('agent_instance_binding:' || $1::bigint::text, 0)
                ) AS locked",
        session_db_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

pub(super) async fn lock_request_numbering(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(
                    hashtextextended('request_numbering:' || $1::bigint::text, 0)
                ) AS locked",
        session_db_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}
