use super::super::PersistError;

pub async fn attribute_inherited_requests(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    request_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!("SET LOCAL jit = off")
        .execute(&mut **tx)
        .await?;
    sqlx::query_file!(
        "src/store/persistence/turn_lifecycle/inheritance/attribute.sql",
        &[request_id],
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(())
}

pub async fn inherited_request_turn(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    request_id: i64,
) -> Result<Option<i64>, PersistError> {
    Ok(sqlx::query_scalar(
        "SELECT r.operator_turn_id FROM requests.requests r
         JOIN memory.operator_turns t ON t.id = r.operator_turn_id
         JOIN requests.turn_origins o ON o.id = r.turn_origin_id
         WHERE r.id = $1 AND o.code = 'subagent'",
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await?)
}

#[cfg(test)]
pub mod call_count_tests;
#[cfg(test)]
pub mod tests;

#[cfg(test)]
thread_local! {
    static RECONCILE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub async fn reconcile_dispatch_results(
    tx: &mut sqlx::PgConnection,
    wire_ids: &[String],
) -> Result<(), PersistError> {
    #[cfg(test)]
    RECONCILE_CALLS.with(|calls| calls.set(calls.get() + 1));
    if wire_ids.is_empty() {
        return Ok(());
    }
    sqlx::query!("SET LOCAL jit = off")
        .execute(&mut *tx)
        .await?;
    let ids: Vec<i64> = sqlx::query_scalar(include_str!("inheritance/reconcile.sql"))
        .bind(wire_ids)
        .fetch_all(&mut *tx)
        .await?;
    if !ids.is_empty() {
        let turns: Vec<i64> = sqlx::query_scalar(include_str!("inheritance/attribute.sql"))
            .bind(&ids)
            .fetch_all(&mut *tx)
            .await?;
        sqlx::query!(
            "SELECT 1 AS refreshed FROM (SELECT memory.refresh_inherited_turns($1)) refresh",
            &turns,
        )
        .fetch_one(&mut *tx)
        .await?;
    }
    Ok(())
}
