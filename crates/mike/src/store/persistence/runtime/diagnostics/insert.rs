use sqlx::PgConnection;

use super::{DiagnosticExchange, DiagnosticLimits, DiagnosticOutcome, EvidenceStatus};
use crate::store::persistence::{PersistError, PersistHandle};

const MAX_METADATA_ROWS: i64 = 100_000;
const MAX_SESSION_ROWS: i64 = 10_000;
const MAX_METADATA_BYTES: usize = 65_536;

fn payload_bytes(exchange: &DiagnosticExchange) -> usize {
    let forwarded = exchange
        .forwarded_body
        .as_ref()
        .filter(|body| Some(*body) != exchange.request_body.as_ref());
    exchange
        .request_body
        .as_ref()
        .map_or(0, Vec::len)
        .saturating_add(forwarded.map_or(0, Vec::len))
        .saturating_add(exchange.response_body.as_ref().map_or(0, Vec::len))
}

fn metadata_bytes(exchange: &DiagnosticExchange) -> Result<i64, PersistError> {
    let bytes = serde_json::to_vec(exchange)?.len();
    if bytes > MAX_METADATA_BYTES {
        return Err(PersistError::DataInvariant(
            "diagnostic metadata exceeds 64 KiB".into(),
        ));
    }
    Ok(bytes as i64 + 1024)
}

pub(super) async fn record(
    persist: &PersistHandle,
    exchange: &DiagnosticExchange,
    limits: &DiagnosticLimits,
) -> Result<DiagnosticOutcome, PersistError> {
    metadata_bytes(exchange)?;
    let mut tx = persist.pool().begin().await?;
    sqlx::query!(
        "SELECT retained_rows FROM runtime.storage_accounting WHERE kind = 'diagnostics' FOR UPDATE"
    )
    .fetch_one(&mut *tx)
    .await?;
    let session = sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
        exchange.session_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    let mut status = if limits.enabled {
        exchange.evidence_status
    } else {
        EvidenceStatus::NotCollected
    };
    if status == EvidenceStatus::Retained && payload_bytes(exchange) > limits.max_record_bytes {
        status = EvidenceStatus::BudgetRefused;
    }
    let id = insert(
        &mut tx,
        exchange,
        session,
        status,
        limits.ttl_seconds.clamp(1, 86_400),
    )
    .await?;
    if !admitted(&mut tx, session, limits).await? && status == EvidenceStatus::Retained {
        status = EvidenceStatus::BudgetRefused;
        sqlx::query!(
            "UPDATE runtime.responses_exchanges SET request_body = NULL, forwarded_body = NULL,
             response_body = NULL, evidence_status = 'budget_refused' WHERE id = $1",
            id
        )
        .execute(&mut *tx)
        .await?;
    }
    if !admitted(&mut tx, session, limits).await? {
        tx.rollback().await?;
        return Ok(DiagnosticOutcome {
            exchange_id: None,
            evidence_status: EvidenceStatus::BudgetRefused,
        });
    }
    tx.commit().await?;
    Ok(DiagnosticOutcome {
        exchange_id: Some(id),
        evidence_status: status,
    })
}

async fn admitted(
    connection: &mut PgConnection,
    session: Option<i64>,
    limits: &DiagnosticLimits,
) -> Result<bool, sqlx::Error> {
    let total = sqlx::query!(
        "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'diagnostics'"
    ).fetch_one(&mut *connection).await?;
    let used = sqlx::query!(
        r#"SELECT count(*) AS "rows!", COALESCE(sum(retained_bytes), 0)::bigint AS "bytes!"
           FROM runtime.responses_exchanges WHERE session_id IS NOT DISTINCT FROM $1"#,
        session
    )
    .fetch_one(connection)
    .await?;
    Ok(total.retained_rows <= MAX_METADATA_ROWS
        && used.rows <= MAX_SESSION_ROWS
        && total.retained_bytes <= limits.max_total_bytes
        && used.bytes <= limits.max_session_bytes)
}

async fn insert(
    connection: &mut PgConnection,
    e: &DiagnosticExchange,
    session: Option<i64>,
    status: EvidenceStatus,
    ttl: i64,
) -> Result<i64, sqlx::Error> {
    let keep = status == EvidenceStatus::Retained;
    let request = keep.then_some(e.request_body.as_deref()).flatten();
    let forwarded = keep
        .then_some(e.forwarded_body.as_deref())
        .flatten()
        .filter(|body| Some(*body) != request);
    let response = keep.then_some(e.response_body.as_deref()).flatten();
    sqlx::query_file_scalar!(
        "src/store/persistence/runtime/diagnostics/insert.sql",
        session,
        e.provider_session_id,
        e.provider_thread_id,
        e.provider_turn_id,
        e.request_kind,
        e.correlation,
        e.auth_route,
        e.transport,
        e.method,
        e.path,
        e.upstream_url,
        e.request_headers,
        e.forwarded_headers,
        request,
        forwarded,
        e.request_body_encoding,
        e.model,
        e.response_status,
        e.response_headers,
        response,
        e.upstream_request_id,
        e.response_id,
        e.response_model,
        e.response_state,
        e.usage,
        e.stream_events,
        e.error,
        e.status,
        e.created_at,
        status.as_str(),
        ttl as f64
    )
    .fetch_one(connection)
    .await
}
