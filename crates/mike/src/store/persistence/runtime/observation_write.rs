use sqlx::PgConnection;

use crate::runtime::{SessionObservation, UsageSnapshot};
use crate::store::persistence::PersistError;

pub(super) async fn apply(
    connection: &mut PgConnection,
    session: i64,
    observation: &SessionObservation,
) -> Result<(), PersistError> {
    let existing = sqlx::query_scalar!(
        "SELECT runtime FROM runtime.session_facts WHERE session_id = $1",
        session
    )
    .fetch_optional(&mut *connection)
    .await?;
    if existing
        .as_ref()
        .is_some_and(|kind| kind != &observation.runtime)
    {
        return Err(PersistError::DataInvariant(
            "session runtime cannot change".into(),
        ));
    }
    upsert_session(connection, session, observation).await?;
    upsert_call(connection, session, observation).await?;
    upsert_item(connection, session, observation).await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET last_seen = GREATEST(last_seen, $2) WHERE id = $1",
        session,
        observation.observed_at
    )
    .execute(connection)
    .await?;
    Ok(())
}

async fn upsert_item(
    connection: &mut PgConnection,
    session: i64,
    o: &SessionObservation,
) -> Result<(), sqlx::Error> {
    let Some(item) = &o.item else {
        return Ok(());
    };
    sqlx::query!(
        "INSERT INTO runtime.session_items AS i
             (session_id, item_id, turn_id, kind, name, status, success, exit_code, source,
              fidelity, completeness, first_seen_at, observed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
         ON CONFLICT (session_id, item_id) DO UPDATE SET
             turn_id = COALESCE(EXCLUDED.turn_id, i.turn_id), kind = EXCLUDED.kind,
             name = COALESCE(EXCLUDED.name, i.name), status = COALESCE(EXCLUDED.status, i.status),
             success = COALESCE(EXCLUDED.success, i.success), exit_code = COALESCE(EXCLUDED.exit_code, i.exit_code),
             source = EXCLUDED.source, fidelity = EXCLUDED.fidelity, completeness = EXCLUDED.completeness,
             observed_at = EXCLUDED.observed_at
         WHERE EXCLUDED.observed_at >= i.observed_at",
        session, item.item_id, item.turn_id, item.kind, item.name, item.status, item.success,
        item.exit_code, o.source.kind, o.source.fidelity, o.source.completeness, o.observed_at
    ).execute(connection).await?;
    Ok(())
}

async fn upsert_session(
    connection: &mut PgConnection,
    session: i64,
    o: &SessionObservation,
) -> Result<(), sqlx::Error> {
    let empty = UsageSnapshot::default();
    let u = o.usage.as_ref().unwrap_or(&empty);
    let scope = o.usage.as_ref().map(|u| u.scope.as_str());
    let basis = o.usage.as_ref().map(|u| u.basis.as_str());
    sqlx::query_file!(
        "src/store/persistence/runtime/upsert_session.sql",
        session,
        o.runtime,
        o.source.kind,
        o.source.version,
        o.source.fidelity,
        o.source.completeness,
        o.observed_at,
        o.sequence,
        o.lifecycle.map(|s| s.as_str()),
        o.requested_model,
        o.observed_model,
        o.control.map(|c| c.as_str()),
        scope,
        basis,
        u.input_tokens,
        u.cached_input_tokens,
        u.output_tokens,
        u.reasoning_output_tokens,
        u.total_tokens,
        u.context_window
    )
    .execute(connection)
    .await?;
    Ok(())
}

async fn upsert_call(
    connection: &mut PgConnection,
    session: i64,
    o: &SessionObservation,
) -> Result<(), sqlx::Error> {
    let Some(call) = &o.model_call else {
        return Ok(());
    };
    let empty = UsageSnapshot::default();
    let u = call.usage.as_ref().unwrap_or(&empty);
    let scope = call.usage.as_ref().map(|u| u.scope.as_str());
    let basis = call.usage.as_ref().map(|u| u.basis.as_str());
    sqlx::query_file!(
        "src/store/persistence/runtime/upsert_model_call.sql",
        session,
        call.call_id,
        o.observed_at,
        o.source.kind,
        o.source.version,
        o.source.fidelity,
        o.source.completeness,
        o.requested_model,
        o.observed_model,
        call.status,
        call.duration_ms,
        call.http_status,
        scope,
        basis,
        u.input_tokens,
        u.cached_input_tokens,
        u.output_tokens,
        u.reasoning_output_tokens,
        u.total_tokens,
        u.context_window
    )
    .execute(connection)
    .await?;
    Ok(())
}
